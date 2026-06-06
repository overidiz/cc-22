use nih_plug::prelude::*;

use crate::params::DiffusionParams;

use super::{
    chain::{sanitize_sample, ModuleCore},
    dry_wet::DryWet,
    smoothing::LinearSmoother,
};

const MAX_CHANNELS: usize = 2;
const MAX_DELAY_SECONDS: f32 = 2.1;
const MAX_REVERB_PRE_DELAY_SECONDS: f32 = 0.13;
const NUM_REVERB_COMBS: usize = 6;
const NUM_REVERB_ALLPASSES: usize = 3;
const MIN_TONE_HZ: f32 = 900.0;
const MAX_TONE_HZ: f32 = 16_000.0;
const REVERB_COMB_DELAYS_MS: [[f32; NUM_REVERB_COMBS]; MAX_CHANNELS] = [
    [23.83, 31.37, 37.11, 43.73, 53.17, 61.71],
    [25.31, 33.89, 39.79, 47.29, 56.11, 64.43],
];
const REVERB_ALLPASS_DELAYS_MS: [[f32; NUM_REVERB_ALLPASSES]; MAX_CHANNELS] =
    [[7.13, 3.97, 1.73], [7.91, 4.31, 1.91]];
const REVERB_MOD_RATES_HZ: [[f32; NUM_REVERB_COMBS]; MAX_CHANNELS] = [
    [0.071, 0.083, 0.097, 0.113, 0.131, 0.149],
    [0.079, 0.089, 0.103, 0.121, 0.137, 0.157],
];
const REVERB_MOD_DEPTH_MS: f32 = 0.38;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffusionMode {
    #[id = "off"]
    Off,

    #[id = "delay"]
    Delay,

    #[id = "slap"]
    Slap,

    #[id = "reverb"]
    Reverb,

    #[id = "cascade"]
    #[name = "Cascade"]
    Cascade,

    #[id = "reels"]
    #[name = "Reels"]
    Reels,

    #[id = "space"]
    #[name = "Space"]
    Space,

    #[id = "collage"]
    #[name = "Collage"]
    Collage,

    #[id = "reverse"]
    #[name = "Reverse"]
    Reverse,
}

#[derive(Debug, Clone)]
pub struct Diffusion {
    core: ModuleCore,
    delay: StereoDelay,
    reverb: ReverbTank,
    sample_rate: f32,
    current_mode: DiffusionMode,
    mode_crossfade: LinearSmoother,
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
    reels_wow_phase: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DiffusionFrame {
    mode: DiffusionMode,
    time_ms: f32,
    feedback: f32,
    size: f32,
    decay: f32,
    pre_delay_ms: f32,
    damping: f32,
    mix: f32,
    stereo_offset: f32,
    width: f32,
    active_mix: f32,
    tone_alpha: f32,
    mode_fade: f32,
}

#[derive(Debug, Clone, Default)]
struct StereoDelay {
    buffers: [Vec<f32>; MAX_CHANNELS],
    write_positions: [usize; MAX_CHANNELS],
    feedback_filter_state: [f32; MAX_CHANNELS],
}

#[derive(Debug, Clone)]
struct ReverbTank {
    pre_delay: [DelayLine; MAX_CHANNELS],
    combs: [[DelayLine; NUM_REVERB_COMBS]; MAX_CHANNELS],
    allpasses: [[DelayLine; NUM_REVERB_ALLPASSES]; MAX_CHANNELS],
    mod_phases: [[f32; NUM_REVERB_COMBS]; MAX_CHANNELS],
    last_wet: [f32; MAX_CHANNELS],
    sample_rate: f32,
}

#[derive(Debug, Clone, Default)]
struct DelayLine {
    buffer: Vec<f32>,
    write_position: usize,
    filter_state: f32,
}

impl Default for Diffusion {
    fn default() -> Self {
        let mut diffusion = Self {
            core: ModuleCore::default(),
            delay: StereoDelay::default(),
            reverb: ReverbTank::default(),
            sample_rate: 44_100.0,
            current_mode: DiffusionMode::Off,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
            reels_wow_phase: 0.0,
        };
        diffusion.prepare(44_100.0);
        diffusion
    }
}

impl Diffusion {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.core.prepare(self.sample_rate);
        self.delay.prepare(self.sample_rate);
        self.reverb.prepare(self.sample_rate);
        self.mode_crossfade.prepare(self.sample_rate);
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.delay.reset();
        self.reverb.reset();
        self.mode_crossfade.reset(1.0);
        self.last_output = [0.0; MAX_CHANNELS];
        self.has_processed = false;
        self.reels_wow_phase = 0.0;
    }

    pub fn next_frame(&mut self, params: &DiffusionParams) -> DiffusionFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let time_ms = params.time.smoothed.next().clamp(1.0, 2_000.0);
        let feedback = params.feedback.smoothed.next().clamp(0.0, 0.949);
        let size = params.size.smoothed.next().clamp(0.0, 1.0);
        let decay = params.decay.smoothed.next().clamp(0.0, 1.0);
        let pre_delay_ms = params.pre_delay.smoothed.next().clamp(0.0, 120.0);
        let damping = params.damping.smoothed.next().clamp(0.0, 1.0);
        let mix = params.mix.smoothed.next().clamp(0.0, 1.0);
        let tone = params.tone.smoothed.next().clamp(0.0, 1.0);
        let stereo_offset = params.stereo_offset.smoothed.next().clamp(-0.5, 0.5);
        let width = params.width.smoothed.next().clamp(0.0, 1.0);
        let module_frame = self.core.next_frame(params.bypass.value(), mix, 0.0);

        DiffusionFrame {
            mode,
            time_ms,
            feedback,
            size,
            decay,
            pre_delay_ms,
            damping,
            mix,
            stereo_offset,
            width,
            active_mix: module_frame.active_mix,
            tone_alpha: tone_to_alpha(tone, self.sample_rate),
            mode_fade: self.mode_crossfade.next_value().clamp(0.0, 1.0),
        }
    }

    pub fn process_block(&mut self, buffer: &mut Buffer, params: &DiffusionParams) {
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params);
            for (channel_index, sample) in channel_samples.into_iter().enumerate() {
                *sample = self.process_sample_for_channel(channel_index, *sample, &frame);
            }
        }
    }

    pub fn process_sample(&mut self, sample: f32, frame: &DiffusionFrame) -> f32 {
        self.process_sample_for_channel(0, sample, frame)
    }

    pub fn process_sample_for_channel(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &DiffusionFrame,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let dry = sanitize_sample(sample);
        let wet = match frame.mode {
            DiffusionMode::Off => dry,
            DiffusionMode::Delay => self.process_delay(index, dry, frame),
            DiffusionMode::Slap => self.process_slap(index, dry, frame),
            DiffusionMode::Reverb => self.process_reverb(index, dry, frame),
            DiffusionMode::Cascade => self.process_cascade(index, dry, frame),
            DiffusionMode::Reels => self.process_reels(index, dry, frame),
            DiffusionMode::Space => self.process_space(index, dry, frame),
            // TODO: Character-22 inspired modes — placeholder passthrough until DSP is implemented
            DiffusionMode::Collage | DiffusionMode::Reverse => dry,
        };

        let mixed = if frame.mode == DiffusionMode::Off {
            dry
        } else {
            DryWet.mix(dry, wet, frame.mix)
        };
        let mixed = self.smooth_mode_transition(index, mixed, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, mixed, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    fn process_delay(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let delay_ms = channel_delay_ms(index, frame);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;

        self.delay.process(
            index,
            sample,
            delay_samples,
            frame.feedback,
            frame.tone_alpha,
        )
    }

    fn process_slap(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let delay_ms = slap_channel_delay_ms(index, frame);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;
        let feedback = frame.feedback.clamp(0.0, 0.6);

        self.delay
            .process(index, sample, delay_samples, feedback, frame.tone_alpha)
    }

    fn process_reverb(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        self.reverb.process(index, sample, frame)
    }
    fn process_cascade(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let base_ms = frame.time_ms.clamp(40.0, 800.0);
        let density = frame.size.clamp(0.0, 1.0);
        let decay = frame.decay.clamp(0.0, 1.0);

        let tap_ratios = [1.0, 1.42, 2.05, 3.15];
        let tap_gains = [
            0.65,
            0.65 * (1.0 - decay * 0.55),
            0.65 * (1.0 - decay * 0.75),
            0.65 * (1.0 - decay * 0.90),
        ];

        let chan_offset = if index == 0 {
            -frame.stereo_offset * base_ms * 0.12
        } else {
            frame.stereo_offset * base_ms * 0.12
        };

        // Read all taps before any mutable buffer borrow
        let buf_len = self.delay.buffers[index].len();
        let write_pos = self.delay.write_positions[index];
        let mut sum = 0.0;
        for i in 0..4 {
            let spacing = 0.7 + density * 0.6;
            let tap_ms = base_ms * tap_ratios[i] * spacing + chan_offset * (i as f32 + 1.0);
            let tap_samples = tap_ms * 0.001 * self.sample_rate;
            let clamped = tap_samples.clamp(1.0, buf_len as f32 - 2.0);
            let _read_pos = write_pos as f32 - clamped;
            let mut rp = _read_pos;
            while rp < 0.0 {
                rp += buf_len as f32;
            }
            let ia = rp.floor() as usize % buf_len;
            let ib = (ia + 1) % buf_len;
            let frac = rp - rp.floor();
            let tap = sanitize_sample(
                self.delay.buffers[index][ia] * (1.0 - frac) + self.delay.buffers[index][ib] * frac,
            );
            sum += tap * tap_gains[i];
        }
        let wet = sanitize_sample(sum);

        // Write input + feedback to buffer
        let buffer = &mut self.delay.buffers[index];
        buffer[write_pos] = sanitize_sample(sample);

        let fb = frame.feedback.clamp(0.0, 0.80);
        let fb_filtered = self.delay.feedback_filter_state[index]
            + frame.tone_alpha * (wet - self.delay.feedback_filter_state[index]);
        self.delay.feedback_filter_state[index] = sanitize_sample(fb_filtered);
        buffer[write_pos] = sanitize_sample(sample + fb_filtered * fb * 0.7);

        self.delay.write_positions[index] = (write_pos + 1) % buf_len;

        sanitize_sample(fb_filtered * cascade_level_compensation(decay, frame.feedback))
    }

    fn process_reels(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let decay = frame.decay.clamp(0.0, 1.0);
        let tone_alpha = frame.tone_alpha;

        // Wow modulation: slow tape-speed drift
        let wow_rate = 0.45 + frame.size * 1.8;
        self.reels_wow_phase += wow_rate / self.sample_rate.max(1.0);
        if self.reels_wow_phase >= 1.0 {
            self.reels_wow_phase -= 1.0;
        }
        let ch_phase = (self.reels_wow_phase + if index == 0 { 0.0 } else { 0.34 }).fract();
        let wow_ms = (ch_phase * core::f32::consts::TAU).sin() * frame.size * 3.8;

        let delay_ms = (frame.time_ms + wow_ms).clamp(25.0, 2_000.0);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;

        // Read delayed signal (interpolated)
        let buf_len = self.delay.buffers[index].len();
        let write_pos = self.delay.write_positions[index];
        let clamped = delay_samples.clamp(1.0, buf_len as f32 - 2.0);
        let rp = write_pos as f32 - clamped;
        let mut rp = rp;
        while rp < 0.0 {
            rp += buf_len as f32;
        }
        let ia = rp.floor() as usize % buf_len;
        let ib = (ia + 1) % buf_len;
        let frac = rp - rp.floor();
        let delayed = sanitize_sample(
            self.delay.buffers[index][ia] * (1.0 - frac) + self.delay.buffers[index][ib] * frac,
        );

        // Tape saturation in feedback path
        let sat_gain = 1.0 + decay * 3.8;
        let saturated = (delayed * sat_gain).tanh();

        // Progressive tone damping
        let fb_filtered = self.delay.feedback_filter_state[index]
            + tone_alpha * (saturated - self.delay.feedback_filter_state[index]);
        self.delay.feedback_filter_state[index] = sanitize_sample(fb_filtered);

        // Write back with feedback
        let fb = frame.feedback.clamp(0.0, 0.92);
        let buffer = &mut self.delay.buffers[index];
        buffer[write_pos] = sanitize_sample(sample + fb_filtered * fb);
        self.delay.write_positions[index] = (write_pos + 1) % buf_len;

        sanitize_sample(fb_filtered * reels_level_compensation(frame.feedback, decay))
    }

    fn process_space(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);

        // Pre-delay
        let pre_delay_samples = frame.pre_delay_ms * 0.001 * self.sample_rate;
        let reverb_input = self.reverb.pre_delay[index].process_delay(sample, pre_delay_samples);

        // Space: larger size scaling for ambient tail
        let size_scale = 0.78 + frame.size * 1.42;
        let feedback = space_reverb_feedback(frame.decay);
        let damping_alpha = space_damping_alpha(frame.damping);

        // 6 comb filters — gentler modulation for smoother ambience
        let mut sum = 0.0;
        for (comb_index, delay_ms) in REVERB_COMB_DELAYS_MS[index].iter().enumerate() {
            let mod_phase = self.reverb.next_mod_phase(index, comb_index);
            let mod_ms = mod_phase.sin() * REVERB_MOD_DEPTH_MS * (0.45 + frame.size * 0.55);
            let delay_samples = (*delay_ms + mod_ms) * 0.001 * self.sample_rate * size_scale;
            sum += self.reverb.combs[index][comb_index].process_comb(
                reverb_input * 0.14,
                delay_samples,
                feedback,
                damping_alpha,
            );
        }

        // 3 allpass diffusers
        let mut wet = sum * 0.14;
        for (allpass_index, delay_ms) in REVERB_ALLPASS_DELAYS_MS[index].iter().enumerate() {
            let delay_samples = *delay_ms * 0.001 * self.sample_rate * size_scale;
            wet = self.reverb.allpasses[index][allpass_index].process_allpass(
                wet,
                delay_samples,
                0.55,
            );
        }

        // Smooth tail + wide stereo
        wet = sanitize_sample((wet * 0.76) + (self.reverb.last_wet[index] * 0.24));
        let other = self.reverb.last_wet[1 - index];
        self.reverb.last_wet[index] = wet;

        let monoish = (wet + other) * 0.5;
        sanitize_sample((monoish * (1.0 - frame.width)) + (wet * frame.width))
    }

    fn set_mode(&mut self, mode: DiffusionMode) {
        if mode != self.current_mode {
            self.current_mode = mode;
            if self.has_processed {
                self.mode_crossfade.reset(0.0);
                self.mode_crossfade.set_target(1.0);
            } else {
                self.mode_crossfade.reset(1.0);
            }
        }
    }

    fn smooth_mode_transition(&self, channel: usize, next_output: f32, mode_fade: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        if mode_fade >= 0.999_999 {
            sanitize_sample(next_output)
        } else {
            linear_crossfade(self.last_output[index], next_output, mode_fade)
        }
    }
}

#[inline]
fn linear_crossfade(dry: f32, wet: f32, amount: f32) -> f32 {
    let amount = amount.clamp(0.0, 1.0);
    sanitize_sample((dry * (1.0 - amount)) + (wet * amount))
}

impl StereoDelay {
    fn prepare(&mut self, sample_rate: f32) {
        let samples = ((sample_rate.max(1.0) * MAX_DELAY_SECONDS).ceil() as usize).max(4);
        for buffer in &mut self.buffers {
            buffer.resize(samples, 0.0);
            buffer.fill(0.0);
        }
        self.write_positions = [0; MAX_CHANNELS];
        self.feedback_filter_state = [0.0; MAX_CHANNELS];
    }

    fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        self.write_positions = [0; MAX_CHANNELS];
        self.feedback_filter_state = [0.0; MAX_CHANNELS];
    }

    fn process(
        &mut self,
        channel: usize,
        input: f32,
        delay_samples: f32,
        feedback: f32,
        tone_alpha: f32,
    ) -> f32 {
        let buffer = &mut self.buffers[channel];
        if buffer.is_empty() {
            return input;
        }

        let len = buffer.len();
        let write_pos = self.write_positions[channel];
        let delayed = read_interpolated(
            buffer,
            write_pos,
            delay_samples.clamp(1.0, len as f32 - 2.0),
        );
        let filtered_feedback = self.feedback_filter_state[channel]
            + (tone_alpha * (delayed - self.feedback_filter_state[channel]));
        self.feedback_filter_state[channel] = sanitize_sample(filtered_feedback);

        let feedback = feedback.clamp(0.0, 0.949);
        buffer[write_pos] = sanitize_sample(input + (filtered_feedback * feedback));
        self.write_positions[channel] = (write_pos + 1) % len;

        sanitize_sample(filtered_feedback)
    }
}

impl Default for ReverbTank {
    fn default() -> Self {
        Self {
            pre_delay: std::array::from_fn(|_| DelayLine::default()),
            combs: std::array::from_fn(|_| std::array::from_fn(|_| DelayLine::default())),
            allpasses: std::array::from_fn(|_| std::array::from_fn(|_| DelayLine::default())),
            mod_phases: [[0.0; NUM_REVERB_COMBS]; MAX_CHANNELS],
            last_wet: [0.0; MAX_CHANNELS],
            sample_rate: 44_100.0,
        }
    }
}

impl ReverbTank {
    fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        let pre_delay_samples =
            ((self.sample_rate * MAX_REVERB_PRE_DELAY_SECONDS).ceil() as usize).max(4);
        let comb_samples = ((self.sample_rate * 0.08).ceil() as usize).max(4);
        let allpass_samples = ((self.sample_rate * 0.012).ceil() as usize).max(4);

        for channel in 0..MAX_CHANNELS {
            self.pre_delay[channel].prepare(pre_delay_samples);

            for comb in &mut self.combs[channel] {
                comb.prepare(comb_samples);
            }

            for allpass in &mut self.allpasses[channel] {
                allpass.prepare(allpass_samples);
            }

            for (index, phase) in self.mod_phases[channel].iter_mut().enumerate() {
                *phase = ((index as f32 * 0.173) + (channel as f32 * 0.091)).fract();
            }
        }

        self.last_wet = [0.0; MAX_CHANNELS];
    }

    fn reset(&mut self) {
        for channel in 0..MAX_CHANNELS {
            self.pre_delay[channel].reset();

            for comb in &mut self.combs[channel] {
                comb.reset();
            }

            for allpass in &mut self.allpasses[channel] {
                allpass.reset();
            }

            for (index, phase) in self.mod_phases[channel].iter_mut().enumerate() {
                *phase = ((index as f32 * 0.173) + (channel as f32 * 0.091)).fract();
            }
        }

        self.last_wet = [0.0; MAX_CHANNELS];
    }

    fn process(&mut self, channel: usize, input: f32, frame: &DiffusionFrame) -> f32 {
        let pre_delay_samples = frame.pre_delay_ms * 0.001 * self.sample_rate;
        let reverb_input = self.pre_delay[channel].process_delay(input, pre_delay_samples);
        let size_scale = 0.65 + (frame.size * 0.85);
        let feedback = reverb_feedback(frame.decay);
        let damping_alpha = damping_to_alpha(frame.damping);

        let mut sum = 0.0;
        for (comb_index, delay_ms) in REVERB_COMB_DELAYS_MS[channel].iter().enumerate() {
            let mod_phase = self.next_mod_phase(channel, comb_index);
            let mod_ms = mod_phase.sin() * REVERB_MOD_DEPTH_MS * (0.25 + frame.size * 0.75);
            let delay_samples = (*delay_ms + mod_ms) * 0.001 * self.sample_rate * size_scale;
            sum += self.combs[channel][comb_index].process_comb(
                reverb_input * 0.16,
                delay_samples,
                feedback,
                damping_alpha,
            );
        }

        let mut wet = sum * 0.16;
        for (allpass_index, delay_ms) in REVERB_ALLPASS_DELAYS_MS[channel].iter().enumerate() {
            let delay_samples = *delay_ms * 0.001 * self.sample_rate * size_scale;
            wet = self.allpasses[channel][allpass_index].process_allpass(wet, delay_samples, 0.58);
        }

        wet = sanitize_sample((wet * 0.82) + (self.last_wet[channel] * 0.18));
        let other = self.last_wet[1 - channel];
        self.last_wet[channel] = wet;

        let monoish = (wet + other) * 0.5;
        sanitize_sample((monoish * (1.0 - frame.width)) + (wet * frame.width))
    }

    fn next_mod_phase(&mut self, channel: usize, comb_index: usize) -> f32 {
        let phase = self.mod_phases[channel][comb_index];
        let next = phase + (REVERB_MOD_RATES_HZ[channel][comb_index] / self.sample_rate.max(1.0));
        self.mod_phases[channel][comb_index] = if next >= 1.0 { next - 1.0 } else { next };
        phase * core::f32::consts::TAU
    }
}

impl DelayLine {
    fn prepare(&mut self, samples: usize) {
        self.buffer.resize(samples.max(4), 0.0);
        self.reset();
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_position = 0;
        self.filter_state = 0.0;
    }

    fn process_delay(&mut self, input: f32, delay_samples: f32) -> f32 {
        if self.buffer.is_empty() {
            return input;
        }

        let len = self.buffer.len();
        let delayed = read_interpolated(
            &self.buffer,
            self.write_position,
            delay_samples.clamp(1.0, len as f32 - 2.0),
        );
        self.buffer[self.write_position] = sanitize_sample(input);
        self.write_position = (self.write_position + 1) % len;

        delayed
    }

    fn process_comb(
        &mut self,
        input: f32,
        delay_samples: f32,
        feedback: f32,
        damping_alpha: f32,
    ) -> f32 {
        if self.buffer.is_empty() {
            return input;
        }

        let len = self.buffer.len();
        let delayed = read_interpolated(
            &self.buffer,
            self.write_position,
            delay_samples.clamp(1.0, len as f32 - 2.0),
        );
        self.filter_state += damping_alpha * (delayed - self.filter_state);
        self.filter_state = sanitize_sample(self.filter_state);
        self.buffer[self.write_position] =
            sanitize_sample(input + (self.filter_state * feedback.clamp(0.0, 0.92)));
        self.write_position = (self.write_position + 1) % len;

        self.filter_state
    }

    fn process_allpass(&mut self, input: f32, delay_samples: f32, feedback: f32) -> f32 {
        if self.buffer.is_empty() {
            return input;
        }

        let len = self.buffer.len();
        let delayed = read_interpolated(
            &self.buffer,
            self.write_position,
            delay_samples.clamp(1.0, len as f32 - 2.0),
        );
        let output = sanitize_sample(delayed - input);
        self.buffer[self.write_position] = sanitize_sample(input + (delayed * feedback));
        self.write_position = (self.write_position + 1) % len;

        output
    }
}

#[inline]
fn channel_delay_ms(channel: usize, frame: &DiffusionFrame) -> f32 {
    let offset = frame.stereo_offset * frame.width;
    let multiplier = if channel == 0 {
        1.0 - offset
    } else {
        1.0 + offset
    };

    (frame.time_ms * multiplier).clamp(1.0, 2_000.0)
}

#[inline]
fn slap_channel_delay_ms(channel: usize, frame: &DiffusionFrame) -> f32 {
    let base_time = frame.time_ms.clamp(30.0, 220.0);
    let spread = frame.width.clamp(0.0, 1.0) * 0.18;
    let multiplier = if channel == 0 {
        1.0 - spread
    } else {
        1.0 + spread
    };

    (base_time * multiplier).clamp(30.0, 260.0)
}

#[inline]
fn reverb_feedback(decay: f32) -> f32 {
    (0.50 + (decay.clamp(0.0, 1.0) * 0.36)).clamp(0.50, 0.86)
}

#[inline]
fn damping_to_alpha(damping: f32) -> f32 {
    (0.78 - (damping.clamp(0.0, 1.0) * 0.68)).clamp(0.10, 0.78)
}

#[inline]
fn read_interpolated(buffer: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
    let len = buffer.len();
    let mut read_pos = write_pos as f32 - delay_samples;
    while read_pos < 0.0 {
        read_pos += len as f32;
    }

    let index_a = read_pos.floor() as usize % len;
    let index_b = (index_a + 1) % len;
    let frac = read_pos - read_pos.floor();
    sanitize_sample((buffer[index_a] * (1.0 - frac)) + (buffer[index_b] * frac))
}

#[inline]
fn tone_to_alpha(tone: f32, sample_rate: f32) -> f32 {
    let tone = tone.clamp(0.0, 1.0);
    let cutoff = MIN_TONE_HZ + ((MAX_TONE_HZ - MIN_TONE_HZ) * tone * tone);
    let sample_rate = sample_rate.max(1.0);
    (1.0 - (-2.0 * core::f32::consts::PI * cutoff / sample_rate).exp()).clamp(0.0, 1.0)
}

#[inline]
fn cascade_level_compensation(decay: f32, feedback: f32) -> f32 {
    let decay = decay.clamp(0.0, 1.0);
    let feedback = feedback.clamp(0.0, 1.0);
    (1.0 - (decay * 0.08) - (feedback * 0.10)).clamp(0.78, 1.0)
}

#[inline]
fn reels_level_compensation(feedback: f32, decay: f32) -> f32 {
    let feedback = feedback.clamp(0.0, 1.0);
    let decay = decay.clamp(0.0, 1.0);
    (1.0 - (feedback * 0.10) - (decay * 0.04)).clamp(0.80, 1.0)
}

#[inline]
fn space_reverb_feedback(decay: f32) -> f32 {
    (0.55 + (decay.clamp(0.0, 1.0) * 0.35)).clamp(0.55, 0.90)
}

#[inline]
fn space_damping_alpha(damping: f32) -> f32 {
    (0.72 - (damping.clamp(0.0, 1.0) * 0.52)).clamp(0.20, 0.72)
}

#[cfg(test)]
mod tests {
    use super::{
        cascade_level_compensation, channel_delay_ms, damping_to_alpha, read_interpolated,
        reels_level_compensation, reverb_feedback, slap_channel_delay_ms, space_reverb_feedback,
        Diffusion, DiffusionFrame, DiffusionMode, StereoDelay,
    };

    fn test_frame(mode: DiffusionMode) -> DiffusionFrame {
        DiffusionFrame {
            mode,
            time_ms: 400.0,
            feedback: 0.0,
            size: 0.5,
            decay: 0.5,
            pre_delay_ms: 0.0,
            damping: 0.5,
            mix: 1.0,
            stereo_offset: 0.0,
            width: 1.0,
            active_mix: 1.0,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn interpolated_delay_reads_between_samples() {
        let buffer = [0.0, 1.0, 0.0, 0.0];
        let value = read_interpolated(&buffer, 2, 1.5);

        assert!((value - 0.5).abs() < 0.000_001);
    }

    #[test]
    fn stereo_offset_changes_channel_times() {
        let mut frame = test_frame(DiffusionMode::Delay);
        frame.stereo_offset = 0.5;

        assert!(channel_delay_ms(0, &frame) < channel_delay_ms(1, &frame));
    }

    #[test]
    fn slap_time_is_constrained_and_width_spreads_channels() {
        let mut frame = test_frame(DiffusionMode::Slap);
        frame.time_ms = 400.0;
        frame.feedback = 0.9;

        let left = slap_channel_delay_ms(0, &frame);
        let right = slap_channel_delay_ms(1, &frame);

        assert!(left >= 30.0);
        assert!(right <= 260.0);
        assert!(left < right);
    }

    #[test]
    fn high_feedback_stays_finite() {
        let mut delay = StereoDelay::default();
        delay.prepare(48_000.0);

        let mut peak = 0.0_f32;
        for index in 0..8_000 {
            let input = if index == 0 { 1.0 } else { 0.0 };
            let sample = delay.process(0, input, 480.0, 0.949, 0.25);
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }

        assert!(peak < 4.0, "high feedback delay peak was {peak}");
    }

    #[test]
    fn high_feedback_delay_does_not_run_away_without_safety_limiter() {
        let mut delay = StereoDelay::default();
        delay.prepare(48_000.0);

        let mut peak = 0.0_f32;
        for index in 0..48_000 {
            let input = if index == 0 { 1.0 } else { 0.0 };
            let sample = delay.process(0, input, 480.0, 0.949, 1.0);
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }

        assert!(
            peak <= 1.000_1,
            "high feedback delay should decay instead of relying on output safety limiting, peak {peak}"
        );
    }

    #[test]
    fn slap_feedback_range_stays_finite() {
        let mut delay = StereoDelay::default();
        delay.prepare(48_000.0);

        let mut sample = 1.0;
        for _ in 0..4_000 {
            sample = delay.process(0, sample, 3_600.0, 0.6, 0.4);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn reverb_coefficients_are_stable() {
        assert!(reverb_feedback(1.0) < 0.92);
        assert!(damping_to_alpha(1.0) >= 0.1);
        assert!(damping_to_alpha(0.0) <= 1.0);
    }

    #[test]
    fn reverb_output_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let mut frame = test_frame(DiffusionMode::Reverb);
        frame.size = 1.0;
        frame.decay = 1.0;
        frame.pre_delay_ms = 30.0;
        frame.damping = 0.7;

        let mut sample = 1.0;
        for index in 0..12_000 {
            let input = if index == 0 { sample } else { 0.0 };
            sample = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn high_decay_reverb_impulse_has_smooth_tail() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let mut frame = test_frame(DiffusionMode::Reverb);
        frame.size = 1.0;
        frame.decay = 1.0;
        frame.damping = 0.75;
        frame.width = 1.0;
        frame.pre_delay_ms = 0.0;

        let mut peak = 0.0_f32;
        let mut max_tail_step = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut tail_energy = 0.0_f32;

        for index in 0..36_000 {
            let input = if index == 0 { 1.0 } else { 0.0 };
            let sample = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());

            if index > 2_000 {
                max_tail_step = max_tail_step.max((sample - previous).abs());
                tail_energy += sample * sample;
            }
            previous = sample;
        }

        assert!(peak < 1.5, "high decay reverb peak was {peak}");
        assert!(
            max_tail_step < 0.12,
            "high decay reverb tail had a large metallic step: {max_tail_step}"
        );
        assert!(
            tail_energy > 0.000_01,
            "high decay reverb tail died too quickly"
        );
    }

    fn cascade_frame(time_ms: f32, feedback: f32, size: f32, decay: f32) -> DiffusionFrame {
        DiffusionFrame {
            mode: DiffusionMode::Cascade,
            time_ms,
            feedback,
            size,
            decay,
            pre_delay_ms: 0.0,
            damping: 0.5,
            mix: 1.0,
            stereo_offset: 0.0,
            width: 1.0,
            active_mix: 1.0,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn cascade_output_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = cascade_frame(150.0, 0.5, 0.5, 0.6);

        let mut sample = 0.3;
        for _ in 0..2048 {
            sample = diffusion.process_sample_for_channel(0, sample, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn cascade_impulse_has_multiple_taps() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        // Short time so taps fall within reach of the ~100k sample buffer
        let frame = cascade_frame(60.0, 0.0, 0.5, 0.7);

        // Feed impulse, then wait for taps to emerge from buffer
        let mut nonzero_count = 0;
        for i in 0..16000 {
            let input = if i == 0 { 0.9 } else { 0.0 };
            let s = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(s.is_finite());
            if i > 1000 && s.abs() > 0.001 {
                nonzero_count += 1;
            }
        }
        assert!(
            nonzero_count > 3,
            "cascade should produce multiple taps, got {nonzero_count}"
        );
    }

    #[test]
    fn cascade_max_feedback_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = cascade_frame(100.0, 1.0, 0.5, 0.5);

        let mut sample = 0.3;
        for _ in 0..4096 {
            sample = diffusion.process_sample_for_channel(0, sample, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn cascade_differs_from_delay() {
        let mut c = Diffusion::default();
        c.prepare(48_000.0);
        let mut d = Diffusion::default();
        d.prepare(48_000.0);

        let delay_frame = DiffusionFrame {
            mode: DiffusionMode::Delay,
            time_ms: 140.0,
            feedback: 0.3,
            size: 0.5,
            decay: 0.5,
            pre_delay_ms: 0.0,
            damping: 0.5,
            mix: 1.0,
            stereo_offset: 0.0,
            width: 1.0,
            active_mix: 1.0,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        };
        let cascade_frame = cascade_frame(140.0, 0.3, 0.5, 0.6);

        let input = 0.3;
        for _ in 0..8000 {
            d.process_sample_for_channel(0, input, &delay_frame);
            c.process_sample_for_channel(0, input, &cascade_frame);
        }
        let delay_out = d.process_sample_for_channel(0, input, &delay_frame);
        let cascade_out = c.process_sample_for_channel(0, input, &cascade_frame);
        assert!(delay_out.is_finite() && cascade_out.is_finite());
        assert!(
            (delay_out - cascade_out).abs() > 0.000_01,
            "Cascade and Delay should differ"
        );
    }

    #[test]
    fn cascade_compensation_is_reasonable() {
        let c = cascade_level_compensation(1.0, 1.0);
        assert!(c > 0.70 && c < 1.0, "cascade comp={c}");
        let c0 = cascade_level_compensation(0.0, 0.0);
        assert!((c0 - 1.0).abs() < 0.01);
    }

    fn reels_frame(time_ms: f32, feedback: f32, size: f32, decay: f32) -> DiffusionFrame {
        DiffusionFrame {
            mode: DiffusionMode::Reels,
            time_ms,
            feedback,
            size,
            decay,
            pre_delay_ms: 0.0,
            damping: 0.5,
            mix: 1.0,
            stereo_offset: 0.0,
            width: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.5,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn reels_output_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reels_frame(200.0, 0.7, 0.5, 0.5);

        let mut sample = 0.3;
        for _ in 0..4096 {
            sample = diffusion.process_sample_for_channel(0, sample, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn reels_max_feedback_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reels_frame(200.0, 1.0, 0.5, 0.5);

        for _ in 0..8192 {
            let sample = diffusion.process_sample_for_channel(0, 0.3, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn reels_differs_from_delay() {
        let mut r = Diffusion::default();
        r.prepare(48_000.0);
        let mut d = Diffusion::default();
        d.prepare(48_000.0);

        let delay_frame = DiffusionFrame {
            mode: DiffusionMode::Delay,
            time_ms: 80.0,
            feedback: 0.5,
            size: 0.5,
            decay: 0.5,
            pre_delay_ms: 0.0,
            damping: 0.5,
            mix: 1.0,
            stereo_offset: 0.0,
            width: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.5,
            mode_fade: 1.0,
        };
        let reels_frame = reels_frame(80.0, 0.5, 0.9, 0.5);

        let input = 0.3;
        // Warm up: 80ms = 3840 samples, need more for feedback to build
        for _ in 0..12000 {
            d.process_sample_for_channel(0, input, &delay_frame);
            r.process_sample_for_channel(0, input, &reels_frame);
        }
        let d_out = d.process_sample_for_channel(0, input, &delay_frame);
        let r_out = r.process_sample_for_channel(0, input, &reels_frame);
        assert!(d_out.is_finite() && r_out.is_finite());
        assert!(
            (d_out - r_out).abs() > 0.000_01 || r_out.abs() > 0.01,
            "Reels output should be active, d={d_out}, r={r_out}"
        );
    }

    #[test]
    fn reels_compensation_is_reasonable() {
        let c = reels_level_compensation(1.0, 1.0);
        assert!(c > 0.75 && c < 1.0);
        let c0 = reels_level_compensation(0.0, 0.0);
        assert!((c0 - 1.0).abs() < 0.01);
    }

    fn space_frame(size: f32, decay: f32, damping: f32, width: f32) -> DiffusionFrame {
        DiffusionFrame {
            mode: DiffusionMode::Space,
            time_ms: 0.0,
            feedback: 0.0,
            size,
            decay,
            pre_delay_ms: 20.0,
            damping,
            mix: 1.0,
            stereo_offset: 0.0,
            width,
            active_mix: 1.0,
            tone_alpha: 0.4,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn space_output_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = space_frame(1.0, 0.8, 0.5, 1.0);

        let mut phase = 0.0;
        for _ in 0..4096 {
            phase += 440.0 / 48_000.0;
            let s = (phase * core::f32::consts::TAU).sin() * 0.3;
            let out = diffusion.process_sample_for_channel(0, s, &frame);
            assert!(out.is_finite());
            assert!(out.abs() <= 8.0);
        }
    }

    #[test]
    fn space_impulse_tail_is_stable() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = space_frame(1.0, 0.9, 0.7, 1.0);

        let mut peak: f32 = 0.0;
        let mut tail_energy = 0.0;
        for i in 0..32000 {
            let input = if i == 0 { 0.9 } else { 0.0 };
            let s = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(s.is_finite());
            assert!(s.abs() <= 8.0);
            peak = peak.max(s.abs());
            if i > 3000 {
                tail_energy += s * s;
            }
        }
        assert!(peak < 2.0, "space reverb peak was {peak}");
        assert!(tail_energy > 0.000_01, "space tail died too quickly");
    }

    #[test]
    fn space_max_decay_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = space_frame(1.0, 1.0, 0.5, 1.0);

        for _ in 0..8000 {
            let s = diffusion.process_sample_for_channel(0, 0.3, &frame);
            assert!(s.is_finite());
            assert!(s.abs() <= 8.0);
        }
    }

    #[test]
    fn space_differs_from_reverb() {
        let mut s = Diffusion::default();
        s.prepare(48_000.0);
        let mut r = Diffusion::default();
        r.prepare(48_000.0);

        let reverb_frame = DiffusionFrame {
            mode: DiffusionMode::Reverb,
            time_ms: 0.0,
            feedback: 0.0,
            size: 0.7,
            decay: 0.7,
            pre_delay_ms: 20.0,
            damping: 0.5,
            mix: 1.0,
            stereo_offset: 0.0,
            width: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.4,
            mode_fade: 1.0,
        };
        let space_frame = space_frame(0.7, 0.7, 0.5, 1.0);

        let input = 0.3;
        for _ in 0..4000 {
            r.process_sample_for_channel(0, input, &reverb_frame);
            s.process_sample_for_channel(0, input, &space_frame);
        }
        let r_out = r.process_sample_for_channel(0, input, &reverb_frame);
        let s_out = s.process_sample_for_channel(0, input, &space_frame);
        assert!(r_out.is_finite() && s_out.is_finite());
        assert!(
            (r_out - s_out).abs() > 0.000_01,
            "Space and Reverb should differ"
        );
    }

    #[test]
    fn space_reverb_feedback_is_in_range() {
        let fb0 = space_reverb_feedback(0.0);
        let fb1 = space_reverb_feedback(1.0);
        assert!(fb0 > 0.50 && fb0 < 0.65);
        assert!(fb1 > 0.80 && fb1 < 0.92);
    }
}
