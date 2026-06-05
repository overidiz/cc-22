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
const NUM_REVERB_COMBS: usize = 4;
const NUM_REVERB_ALLPASSES: usize = 2;
const MIN_TONE_HZ: f32 = 900.0;
const MAX_TONE_HZ: f32 = 16_000.0;
const REVERB_COMB_DELAYS_MS: [[f32; NUM_REVERB_COMBS]; MAX_CHANNELS] =
    [[29.7, 37.1, 41.1, 43.7], [30.9, 33.3, 39.5, 45.1]];
const REVERB_ALLPASS_DELAYS_MS: [[f32; NUM_REVERB_ALLPASSES]; MAX_CHANNELS] =
    [[5.0, 1.7], [5.6, 1.9]];

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
        let module_frame = self
            .core
            .next_frame(params.bypass.value(), feedback, mix, 0.0);

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
            let delay_samples = *delay_ms * 0.001 * self.sample_rate * size_scale;
            sum += self.combs[channel][comb_index].process_comb(
                reverb_input * 0.28,
                delay_samples,
                feedback,
                damping_alpha,
            );
        }

        let mut wet = sum * 0.22;
        for (allpass_index, delay_ms) in REVERB_ALLPASS_DELAYS_MS[channel].iter().enumerate() {
            let delay_samples = *delay_ms * 0.001 * self.sample_rate * size_scale;
            wet = self.allpasses[channel][allpass_index].process_allpass(wet, delay_samples, 0.5);
        }

        wet = sanitize_sample(wet);
        let other = self.last_wet[1 - channel];
        self.last_wet[channel] = wet;

        let monoish = (wet + other) * 0.5;
        sanitize_sample((monoish * (1.0 - frame.width)) + (wet * frame.width))
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
    (0.55 + (decay.clamp(0.0, 1.0) * 0.36)).clamp(0.55, 0.91)
}

#[inline]
fn damping_to_alpha(damping: f32) -> f32 {
    (1.0 - (damping.clamp(0.0, 1.0) * 0.9)).clamp(0.1, 1.0)
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

#[cfg(test)]
mod tests {
    use super::{
        channel_delay_ms, damping_to_alpha, read_interpolated, reverb_feedback,
        slap_channel_delay_ms, Diffusion, DiffusionFrame, DiffusionMode, StereoDelay,
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

        let mut sample = 1.0;
        for _ in 0..8_000 {
            sample = delay.process(0, sample, 480.0, 0.949, 0.25);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
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
}
