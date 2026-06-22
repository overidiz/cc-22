use nih_plug::prelude::*;

use crate::params::DiffusionParams;

use super::{
    chain::{sanitize_sample, ModuleCore},
    dry_wet::DryWet,
    smoothing::LinearSmoother,
    transport::{ms_for_division, TransportFrame},
    util::{equal_power_crossfade, one_pole_alpha, safe_feedback, smoothstep},
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
const COLLAGE_SEEDS: [u32; MAX_CHANNELS] = [0x6d2b_79f5, 0x1f12_bb5d];

/// The five Diffusion modes, in product order. Variants are identified by their
/// stable `#[id]`, so dropping the legacy modes (off/delay/slap/reverb) keeps
/// state compatibility for every id that remains.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffusionMode {
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

impl DiffusionMode {
    /// The five product modes, in the exact order shown in the UI.
    pub const PRODUCT_MODES: [DiffusionMode; 5] = [
        DiffusionMode::Cascade,
        DiffusionMode::Reels,
        DiffusionMode::Space,
        DiffusionMode::Collage,
        DiffusionMode::Reverse,
    ];
}

#[derive(Debug, Clone)]
pub struct Diffusion {
    core: ModuleCore,
    delay: StereoDelay,
    reverb: ReverbTank,
    collage: CollageState,
    reverse: ReverseState,
    sample_rate: f32,
    current_mode: DiffusionMode,
    mode_crossfade: LinearSmoother,
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
    reels_delay_time_state: [f32; MAX_CHANNELS],
    reels_wow_phase: [f32; MAX_CHANNELS],
    reels_flutter_phase: [f32; MAX_CHANNELS],
    reels_drift_state: [f32; MAX_CHANNELS],
    reels_drift_target: [f32; MAX_CHANNELS],
    reels_drift_countdown: [usize; MAX_CHANNELS],
    reels_drift_rng: [u32; MAX_CHANNELS],
    reels_hp_state: [f32; MAX_CHANNELS],
    reels_lp_state: [f32; MAX_CHANNELS],
    reels_smear_state: [f32; MAX_CHANNELS],
    reels_feedback_level_state: [f32; MAX_CHANNELS],
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
    // Feedback-path tone filter state, used by the Cascade mode's colored echo.
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

#[derive(Debug, Clone)]
struct CollageState {
    rng_state: [u32; MAX_CHANNELS],
    fragment_delay_samples: [f32; MAX_CHANNELS],
    fade_from_delay_samples: [f32; MAX_CHANNELS],
    fragment_length_samples: [usize; MAX_CHANNELS],
    fragment_position_samples: [usize; MAX_CHANNELS],
    fragment_loop_index: [usize; MAX_CHANNELS],
    repeats_remaining: [usize; MAX_CHANNELS],
    crossfade_position_samples: [usize; MAX_CHANNELS],
    crossfade_length_samples: [usize; MAX_CHANNELS],
    filter_state: [f32; MAX_CHANNELS],
}

#[derive(Debug, Clone, Default)]
struct ReverseState {
    buffers: [Vec<f32>; MAX_CHANNELS],
    write_positions: [usize; MAX_CHANNELS],
    segment_starts: [usize; MAX_CHANNELS],
    segment_lengths: [usize; MAX_CHANNELS],
    playback_positions: [usize; MAX_CHANNELS],
    previous_segment_starts: [usize; MAX_CHANNELS],
    previous_segment_lengths: [usize; MAX_CHANNELS],
    previous_playback_positions: [usize; MAX_CHANNELS],
    crossfade_position_samples: [usize; MAX_CHANNELS],
    crossfade_length_samples: [usize; MAX_CHANNELS],
    filter_state: [f32; MAX_CHANNELS],
    feedback_filter_state: [f32; MAX_CHANNELS],
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
            collage: CollageState::default(),
            reverse: ReverseState::default(),
            sample_rate: 44_100.0,
            current_mode: DiffusionMode::Cascade,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
            reels_delay_time_state: [0.0; MAX_CHANNELS],
            reels_wow_phase: [0.0, 0.37],
            reels_flutter_phase: [0.0, 0.61],
            reels_drift_state: [0.0; MAX_CHANNELS],
            reels_drift_target: [0.0; MAX_CHANNELS],
            reels_drift_countdown: [0; MAX_CHANNELS],
            reels_drift_rng: [0x51f1_7eed, 0x7a9e_2d31],
            reels_hp_state: [0.0; MAX_CHANNELS],
            reels_lp_state: [0.0; MAX_CHANNELS],
            reels_smear_state: [0.0; MAX_CHANNELS],
            reels_feedback_level_state: [0.0; MAX_CHANNELS],
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
        self.collage.reset();
        self.reverse.prepare(self.sample_rate);
        self.mode_crossfade.prepare(self.sample_rate);
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.delay.reset();
        self.reverb.reset();
        self.collage.reset();
        self.reverse.reset();
        self.mode_crossfade.reset(1.0);
        self.last_output = [0.0; MAX_CHANNELS];
        self.has_processed = false;
        self.reels_delay_time_state = [0.0; MAX_CHANNELS];
        self.reels_wow_phase = [0.0, 0.37];
        self.reels_flutter_phase = [0.0, 0.61];
        self.reels_drift_state = [0.0; MAX_CHANNELS];
        self.reels_drift_target = [0.0; MAX_CHANNELS];
        self.reels_drift_countdown = [0; MAX_CHANNELS];
        self.reels_drift_rng = [0x51f1_7eed, 0x7a9e_2d31];
        self.reels_hp_state = [0.0; MAX_CHANNELS];
        self.reels_lp_state = [0.0; MAX_CHANNELS];
        self.reels_smear_state = [0.0; MAX_CHANNELS];
        self.reels_feedback_level_state = [0.0; MAX_CHANNELS];
    }

    pub fn next_frame(
        &mut self,
        params: &DiffusionParams,
        transport: &TransportFrame,
    ) -> DiffusionFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        // Tempo sync drives the base time, which every time-based mode (Cascade,
        // Reels, Reverse, Collage) derives from. Space ignores `time_ms` (its
        // tail comes from size/decay), so its reverb stays free as required.
        let manual_time = params.time.smoothed.next().clamp(1.0, 2_000.0);
        let time_ms = if params.sync_enabled.value() {
            ms_for_division(transport.bpm, params.sync_division.value()).clamp(1.0, 2_000.0)
        } else {
            manual_time
        };
        let feedback = safe_feedback(params.feedback.smoothed.next(), 0.949);
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
        let transport = TransportFrame::default();
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params, &transport);
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
            DiffusionMode::Cascade => self.process_cascade(index, dry, frame),
            DiffusionMode::Reels => self.process_reels(index, dry, frame),
            DiffusionMode::Space => self.process_space(index, dry, frame),
            DiffusionMode::Collage => self.process_collage(index, dry, frame),
            DiffusionMode::Reverse => self.process_reverse(index, dry, frame),
        };

        let mixed = DryWet.mix(dry, wet, frame.mix);
        let mixed = self.smooth_mode_transition(index, mixed, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, mixed, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    fn process_cascade(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let base_ms = frame.time_ms.clamp(40.0, 800.0);
        let density = frame.size.clamp(0.0, 1.0);
        let decay = frame.decay.clamp(0.0, 1.0);

        // Musical, non-harmonic tap spacing avoids a metallic flutter echo.
        let tap_ratios = [1.0, 1.42, 2.05, 3.15];
        // Each tap decays relative to the first, shaping the echo envelope.
        let tap_gains = [
            0.65,
            0.65 * (1.0 - decay * 0.55),
            0.65 * (1.0 - decay * 0.75),
            0.65 * (1.0 - decay * 0.90),
        ];
        // Normalize the tap sum by the total tap gain (capped at unity, never a
        // boost) so layering taps raises density rather than peak level. This is
        // headroom-safe even if every tap lines up, and keeps the perceived
        // level steady across the decay range.
        let gain_sum = tap_gains.iter().sum::<f32>().max(0.65);
        let tap_norm = (1.25 / gain_sum).clamp(0.55, 1.0);

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
        let wet = sanitize_sample(sum * tap_norm);

        // Write input + feedback to buffer
        let buffer = &mut self.delay.buffers[index];
        buffer[write_pos] = sanitize_sample(sample);

        let fb = safe_feedback(frame.feedback, 0.80);
        let fb_filtered = self.delay.feedback_filter_state[index]
            + frame.tone_alpha * (wet - self.delay.feedback_filter_state[index]);
        self.delay.feedback_filter_state[index] = sanitize_sample(fb_filtered);
        buffer[write_pos] = sanitize_sample(sample + fb_filtered * fb * 0.7);

        self.delay.write_positions[index] = (write_pos + 1) % buf_len;

        sanitize_sample(fb_filtered * cascade_level_compensation(decay, frame.feedback))
    }

    fn process_reels(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let sample_rate = self.sample_rate.max(1.0);
        let buf_len = self.delay.buffers[index].len();
        if buf_len < 4 {
            return sample;
        }

        let target_delay_ms = frame.time_ms.clamp(12.0, 2_000.0);
        if self.reels_delay_time_state[index] <= 0.0 {
            self.reels_delay_time_state[index] = target_delay_ms;
        }
        let time_alpha = (1.0 / (sample_rate * 0.080)).clamp(0.0, 1.0);
        self.reels_delay_time_state[index] +=
            (target_delay_ms - self.reels_delay_time_state[index]) * time_alpha;

        let drift = reels_drift_next(
            index,
            sample_rate,
            frame.size,
            &mut self.reels_drift_rng[index],
            &mut self.reels_drift_state[index],
            &mut self.reels_drift_target[index],
            &mut self.reels_drift_countdown[index],
        );
        let delay_ms = reels_modulated_delay_ms(
            index,
            frame,
            sample_rate,
            self.reels_delay_time_state[index],
            &mut self.reels_wow_phase[index],
            &mut self.reels_flutter_phase[index],
            drift,
        );
        let delay_samples = (delay_ms * 0.001 * sample_rate).clamp(1.0, buf_len as f32 - 2.0);

        let write_pos = self.delay.write_positions[index];
        let delayed = read_interpolated(&self.delay.buffers[index], write_pos, delay_samples);

        let colored = reels_feedback_color(
            delayed,
            frame,
            sample_rate,
            &mut self.reels_hp_state[index],
            &mut self.reels_lp_state[index],
            &mut self.reels_smear_state[index],
            &mut self.reels_feedback_level_state[index],
        );

        let other = self.reels_lp_state[1 - index];
        let width = frame.width.clamp(0.0, 1.0);
        let wet = sanitize_sample(((colored + other) * 0.5 * (1.0 - width)) + colored * width);

        let feedback_gain =
            safe_feedback(frame.feedback, 0.94) * (0.65 + frame.decay.clamp(0.0, 1.0) * 0.25);
        self.delay.buffers[index][write_pos] = sanitize_sample(sample + colored * feedback_gain);
        self.delay.write_positions[index] = (write_pos + 1) % buf_len;

        sanitize_sample(wet * reels_level_compensation(frame.feedback, frame.decay))
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

        // 6 comb filters. A little more delay modulation than a plain Schroeder
        // tank continuously detunes the comb resonances, breaking up the
        // standing waves that would otherwise read as metallic ringing.
        let mut sum = 0.0;
        for (comb_index, delay_ms) in REVERB_COMB_DELAYS_MS[index].iter().enumerate() {
            let mod_phase = self.reverb.next_mod_phase(index, comb_index);
            let mod_ms = mod_phase.sin() * REVERB_MOD_DEPTH_MS * (0.6 + frame.size * 0.9);
            let delay_samples = (*delay_ms + mod_ms) * 0.001 * self.sample_rate * size_scale;
            sum += self.reverb.combs[index][comb_index].process_comb(
                reverb_input * 0.14,
                delay_samples,
                feedback,
                damping_alpha,
            );
        }

        // 3 allpass diffusers. A higher diffusion coefficient packs the echoes
        // tighter, smoothing the early reflections into a denser, less grainy
        // ambience.
        let mut wet = sum * 0.14;
        for (allpass_index, delay_ms) in REVERB_ALLPASS_DELAYS_MS[index].iter().enumerate() {
            let delay_samples = *delay_ms * 0.001 * self.sample_rate * size_scale;
            wet = self.reverb.allpasses[index][allpass_index].process_allpass(
                wet,
                delay_samples,
                0.62,
            );
        }

        // Smooth tail + wide stereo
        wet = sanitize_sample((wet * 0.76) + (self.reverb.last_wet[index] * 0.24));
        let other = self.reverb.last_wet[1 - index];
        self.reverb.last_wet[index] = wet;

        let monoish = (wet + other) * 0.5;
        sanitize_sample((monoish * (1.0 - frame.width)) + (wet * frame.width))
    }

    fn process_collage(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let buf_len = self.delay.buffers[index].len();
        if buf_len < 4 {
            return sample;
        }

        if self.collage.fragment_length_samples[index] == 0 {
            self.choose_collage_fragment(index, frame, buf_len, false);
        }

        let write_pos = self.delay.write_positions[index];
        let current_delay = self.collage_current_delay(index, buf_len);
        let wet = {
            let buffer = &self.delay.buffers[index];
            let current = read_interpolated(buffer, write_pos, current_delay);
            let crossfade_len = self.collage.crossfade_length_samples[index];
            if crossfade_len == 0 {
                current
            } else {
                let fade_pos = self.collage.crossfade_position_samples[index];
                let fade_from_delay =
                    self.collage.fade_from_delay_samples[index].clamp(1.0, buf_len as f32 - 2.0);
                let previous = read_interpolated(buffer, write_pos, fade_from_delay);
                // Equal-power crossfade so the level stays constant when cutting
                // between two decorrelated fragments — no mid-fade dip or click.
                let amount = smoothstep(fade_pos as f32 / crossfade_len as f32);
                equal_power_crossfade(previous, current, amount)
            }
        };

        if self.collage.crossfade_length_samples[index] > 0 {
            self.collage.crossfade_position_samples[index] += 1;
            if self.collage.crossfade_position_samples[index]
                >= self.collage.crossfade_length_samples[index]
            {
                self.collage.crossfade_position_samples[index] = 0;
                self.collage.crossfade_length_samples[index] = 0;
            }
        }

        let tone_alpha = collage_tone_alpha(frame);
        let filtered = self.collage.filter_state[index]
            + tone_alpha * (wet - self.collage.filter_state[index]);
        self.collage.filter_state[index] = sanitize_sample(filtered);

        let feedback = collage_feedback_gain(frame.feedback, frame.decay);
        self.delay.buffers[index][write_pos] = sanitize_sample(sample + filtered * feedback);
        self.delay.write_positions[index] = (write_pos + 1) % buf_len;

        self.advance_collage_position(index, frame, buf_len);

        sanitize_sample(filtered * collage_level_compensation(frame.feedback, frame.decay))
    }

    fn advance_collage_position(&mut self, channel: usize, frame: &DiffusionFrame, buf_len: usize) {
        self.collage.fragment_position_samples[channel] += 1;
        let fragment_len = self.collage.fragment_length_samples[channel].max(1);
        if self.collage.fragment_position_samples[channel] < fragment_len {
            return;
        }

        let fade_from_delay = self.collage_current_delay(channel, buf_len);
        let change_probability = collage_change_probability(frame.feedback, frame.decay);
        let should_choose_new = self.collage.repeats_remaining[channel] == 0
            || self.collage.next_random_unit(channel) < change_probability;

        if should_choose_new {
            self.choose_collage_fragment(channel, frame, buf_len, true);
        } else {
            self.collage.repeats_remaining[channel] -= 1;
            self.collage.fragment_loop_index[channel] += 1;
            self.collage.fragment_position_samples[channel] = 0;
            self.start_collage_crossfade(channel, fade_from_delay, fragment_len);
        }
    }

    fn choose_collage_fragment(
        &mut self,
        channel: usize,
        frame: &DiffusionFrame,
        buf_len: usize,
        crossfade: bool,
    ) {
        let fade_from_delay = self.collage_current_delay(channel, buf_len);
        let fragment_len = collage_fragment_length_samples(frame, self.sample_rate, buf_len);
        let random_a = self.collage.next_random_unit(channel);
        let random_b = self.collage.next_random_unit(channel);
        let delay_samples = collage_fragment_delay_samples(
            channel,
            frame,
            self.sample_rate,
            buf_len,
            fragment_len,
            random_a,
            random_b,
        );
        let repeat_random = self.collage.next_random_unit(channel);

        self.collage.fragment_delay_samples[channel] = delay_samples;
        self.collage.fragment_length_samples[channel] = fragment_len;
        self.collage.fragment_position_samples[channel] = 0;
        self.collage.fragment_loop_index[channel] = 0;
        self.collage.repeats_remaining[channel] =
            collage_repeats_remaining(frame.feedback, frame.decay, repeat_random);

        if crossfade {
            self.start_collage_crossfade(channel, fade_from_delay, fragment_len);
        } else {
            self.collage.crossfade_position_samples[channel] = 0;
            self.collage.crossfade_length_samples[channel] = 0;
        }
    }

    fn start_collage_crossfade(
        &mut self,
        channel: usize,
        fade_from_delay: f32,
        fragment_len: usize,
    ) {
        self.collage.fade_from_delay_samples[channel] = fade_from_delay;
        self.collage.crossfade_position_samples[channel] = 0;
        self.collage.crossfade_length_samples[channel] =
            collage_crossfade_samples(fragment_len, self.sample_rate);
    }

    fn collage_current_delay(&self, channel: usize, buf_len: usize) -> f32 {
        let fragment_len = self.collage.fragment_length_samples[channel].max(1) as f32;
        let loop_offset = self.collage.fragment_loop_index[channel] as f32 * fragment_len;
        (self.collage.fragment_delay_samples[channel] + loop_offset)
            .clamp(1.0, buf_len as f32 - 2.0)
    }

    fn process_reverse(&mut self, channel: usize, sample: f32, frame: &DiffusionFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let buf_len = self.reverse.buffers[index].len();
        if buf_len < 8 {
            return sample;
        }

        if self.reverse.segment_lengths[index] == 0 {
            self.capture_reverse_segment(index, frame, buf_len, false);
        }

        let write_pos = self.reverse.write_positions[index];

        let current_pos = self.reverse.playback_positions[index];
        let current_len = self.reverse.segment_lengths[index];
        let current_sample = reverse_read_sample(
            &self.reverse.buffers[index],
            self.reverse.segment_starts[index],
            current_len,
            current_pos,
        );
        let current_env =
            reverse_playback_envelope(current_pos, current_len, frame.decay, self.sample_rate);
        let mut wet = sanitize_sample(current_sample * current_env);

        let crossfade_len = self.reverse.crossfade_length_samples[index];
        if crossfade_len > 0 {
            let prev_pos = self.reverse.previous_playback_positions[index];
            let prev_len = self.reverse.previous_segment_lengths[index];
            let prev_sample = reverse_read_sample(
                &self.reverse.buffers[index],
                self.reverse.previous_segment_starts[index],
                prev_len,
                prev_pos,
            );
            let prev_env =
                reverse_playback_envelope(prev_pos, prev_len, frame.decay, self.sample_rate);
            let prev_wet = sanitize_sample(prev_sample * prev_env);
            let amount =
                self.reverse.crossfade_position_samples[index] as f32 / crossfade_len as f32;
            wet = equal_power_crossfade(prev_wet, wet, amount);
        }

        if crossfade_len > 0 {
            self.reverse.previous_playback_positions[index] =
                (self.reverse.previous_playback_positions[index] + 1)
                    .min(self.reverse.previous_segment_lengths[index].saturating_sub(1));
            self.reverse.crossfade_position_samples[index] += 1;
            if self.reverse.crossfade_position_samples[index] >= crossfade_len {
                self.reverse.crossfade_position_samples[index] = 0;
                self.reverse.crossfade_length_samples[index] = 0;
            }
        }

        let filtered = reverse_tone_filter(
            wet,
            frame,
            self.sample_rate,
            &mut self.reverse.filter_state[index],
        );

        let other = self.reverse.filter_state[1 - index];
        let width = frame.width.clamp(0.0, 1.0);
        let stereo_wet =
            sanitize_sample(((filtered + other) * 0.5 * (1.0 - width)) + filtered * width);

        let feedback = reverse_feedback_gain(frame.feedback, frame.decay);
        let feedback_toned = reverse_tone_filter(
            stereo_wet,
            frame,
            self.sample_rate,
            &mut self.reverse.feedback_filter_state[index],
        );
        let fb_saturated = reverse_feedback_saturate(feedback_toned * feedback, frame.decay);
        self.reverse.buffers[index][write_pos] = sanitize_sample(sample + fb_saturated);
        self.reverse.write_positions[index] = (write_pos + 1) % buf_len;

        self.reverse.playback_positions[index] += 1;
        if self.reverse.playback_positions[index] >= self.reverse.segment_lengths[index].max(1) {
            self.capture_reverse_segment(index, frame, buf_len, true);
        }

        sanitize_sample(stereo_wet * reverse_level_compensation(frame.feedback, frame.decay))
    }

    fn capture_reverse_segment(
        &mut self,
        channel: usize,
        frame: &DiffusionFrame,
        buf_len: usize,
        crossfade: bool,
    ) {
        let segment_len = reverse_segment_length_samples(frame, self.sample_rate, buf_len);
        let window = reverse_window_samples(channel, frame, self.sample_rate, buf_len, segment_len);
        let write_pos = self.reverse.write_positions[channel];
        let start = wrap_sub(write_pos, window, buf_len);

        if crossfade {
            let prev_len = self.reverse.segment_lengths[channel];
            if prev_len > 0 {
                let xfade_len =
                    reverse_crossfade_samples(segment_len.min(prev_len), self.sample_rate)
                        .min(segment_len)
                        .min(prev_len)
                        .max(1);

                self.reverse.previous_segment_starts[channel] =
                    self.reverse.segment_starts[channel];
                self.reverse.previous_segment_lengths[channel] = prev_len;
                self.reverse.previous_playback_positions[channel] =
                    prev_len.saturating_sub(xfade_len.max(1));
                self.reverse.crossfade_position_samples[channel] = 0;
                self.reverse.crossfade_length_samples[channel] = xfade_len;
            }
        } else {
            self.reverse.crossfade_position_samples[channel] = 0;
            self.reverse.crossfade_length_samples[channel] = 0;
        }

        self.reverse.segment_starts[channel] = start;
        self.reverse.segment_lengths[channel] = segment_len;
        self.reverse.playback_positions[channel] = 0;
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

impl Default for CollageState {
    fn default() -> Self {
        Self {
            rng_state: COLLAGE_SEEDS,
            fragment_delay_samples: [1.0; MAX_CHANNELS],
            fade_from_delay_samples: [1.0; MAX_CHANNELS],
            fragment_length_samples: [0; MAX_CHANNELS],
            fragment_position_samples: [0; MAX_CHANNELS],
            fragment_loop_index: [0; MAX_CHANNELS],
            repeats_remaining: [0; MAX_CHANNELS],
            crossfade_position_samples: [0; MAX_CHANNELS],
            crossfade_length_samples: [0; MAX_CHANNELS],
            filter_state: [0.0; MAX_CHANNELS],
        }
    }
}

impl CollageState {
    fn reset(&mut self) {
        self.rng_state = COLLAGE_SEEDS;
        self.fragment_delay_samples = [1.0; MAX_CHANNELS];
        self.fade_from_delay_samples = [1.0; MAX_CHANNELS];
        self.fragment_length_samples = [0; MAX_CHANNELS];
        self.fragment_position_samples = [0; MAX_CHANNELS];
        self.fragment_loop_index = [0; MAX_CHANNELS];
        self.repeats_remaining = [0; MAX_CHANNELS];
        self.crossfade_position_samples = [0; MAX_CHANNELS];
        self.crossfade_length_samples = [0; MAX_CHANNELS];
        self.filter_state = [0.0; MAX_CHANNELS];
    }

    fn next_random_unit(&mut self, channel: usize) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let mut state = self.rng_state[index];
        if state == 0 {
            state = COLLAGE_SEEDS[index];
        }
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.rng_state[index] = state;
        ((state >> 8) as f32) * (1.0 / 16_777_216.0)
    }
}

impl ReverseState {
    fn prepare(&mut self, sample_rate: f32) {
        let samples = ((sample_rate.max(1.0) * MAX_DELAY_SECONDS).ceil() as usize).max(4);
        for buffer in &mut self.buffers {
            buffer.resize(samples, 0.0);
            buffer.fill(0.0);
        }
        self.reset_positions();
    }

    fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        self.reset_positions();
    }

    fn reset_positions(&mut self) {
        self.write_positions = [0; MAX_CHANNELS];
        self.segment_starts = [0; MAX_CHANNELS];
        self.segment_lengths = [0; MAX_CHANNELS];
        self.playback_positions = [0; MAX_CHANNELS];
        self.previous_segment_starts = [0; MAX_CHANNELS];
        self.previous_segment_lengths = [0; MAX_CHANNELS];
        self.previous_playback_positions = [0; MAX_CHANNELS];
        self.crossfade_position_samples = [0; MAX_CHANNELS];
        self.crossfade_length_samples = [0; MAX_CHANNELS];
        self.filter_state = [0.0; MAX_CHANNELS];
        self.feedback_filter_state = [0.0; MAX_CHANNELS];
    }
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
            sanitize_sample(input + (self.filter_state * safe_feedback(feedback, 0.92)));
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
fn reverse_read_sample(
    buffer: &[f32],
    segment_start: usize,
    segment_len: usize,
    playback_pos: usize,
) -> f32 {
    if buffer.is_empty() || segment_len == 0 {
        return 0.0;
    }

    let len = buffer.len();
    let pos = playback_pos.min(segment_len - 1);
    let reverse_offset = segment_len - 1 - pos;
    sanitize_sample(buffer[(segment_start + reverse_offset) % len])
}

#[inline]
fn wrap_sub(position: usize, amount: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (position + len - (amount % len)) % len
    }
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
    (1.0 - (feedback * 0.16) - (decay * 0.03)).clamp(0.72, 1.0)
}

#[inline]
fn reels_modulated_delay_ms(
    channel: usize,
    frame: &DiffusionFrame,
    sample_rate: f32,
    base_delay_ms: f32,
    wow_phase: &mut f32,
    flutter_phase: &mut f32,
    drift: f32,
) -> f32 {
    let size = frame.size.clamp(0.0, 1.0);
    let width = frame.width.clamp(0.0, 1.0);
    let stereo_offset = frame.stereo_offset.clamp(-0.5, 0.5) * width;
    let stereo_sign = if channel == 0 { -1.0 } else { 1.0 };
    let channel_offset_ms = base_delay_ms * stereo_offset * stereo_sign * 0.10;

    let wow_rate_1 = 0.17 + size * 0.45;
    *wow_phase = (*wow_phase + wow_rate_1 / sample_rate.max(1.0)).fract();
    let wow_angle = *wow_phase * core::f32::consts::TAU;
    let wow1 = wow_angle.sin() * size * 3.4;
    let wow2 = (wow_angle * 2.0 + channel as f32 * 1.2).sin() * size * 1.3;

    let flutter_rate = 5.2 + size * 4.8 + channel as f32 * 0.71;
    *flutter_phase = (*flutter_phase + flutter_rate / sample_rate.max(1.0)).fract();
    let flutter = (*flutter_phase * core::f32::consts::TAU).sin() * size * 0.48;

    (base_delay_ms + channel_offset_ms + wow1 + wow2 + flutter + drift).clamp(8.0, 2_000.0)
}

fn reels_feedback_color(
    delayed: f32,
    frame: &DiffusionFrame,
    sample_rate: f32,
    hp_state: &mut f32,
    lp_state: &mut f32,
    smear_state: &mut f32,
    feedback_level_state: &mut f32,
) -> f32 {
    let decay = frame.decay.clamp(0.0, 1.0);
    let size = frame.size.clamp(0.0, 1.0);
    let damping = frame.damping.clamp(0.0, 1.0);

    let saturated = reels_tape_saturate(delayed, decay);

    let level = saturated.abs();
    *feedback_level_state += (level - *feedback_level_state) * 0.003;
    let compression = 1.0 / (1.0 + (*feedback_level_state * (0.26 + decay * 0.88)));
    let compressed = saturated * compression.clamp(0.55, 1.0);

    let hp_cutoff = 34.0 + decay * 52.0;
    let hp_alpha = one_pole_alpha(hp_cutoff, sample_rate);
    *hp_state += hp_alpha * (compressed - *hp_state);
    let high_passed = sanitize_sample(compressed - *hp_state);

    let tone_brightness = frame.tone_alpha.clamp(0.0, 1.0);
    let lp_cutoff =
        (1_150.0 + tone_brightness * 10_350.0) * (1.0 - damping * 0.44) * (1.0 - decay * 0.38);
    let lp_cutoff = lp_cutoff.clamp(580.0, 13_000.0);
    let lp_alpha = one_pole_alpha(lp_cutoff, sample_rate);
    *lp_state += lp_alpha * (high_passed - *lp_state);
    let low_passed = sanitize_sample(*lp_state);

    let smear_amount = (0.14 + size * 0.18 + decay * 0.06).clamp(0.14, 0.38);
    let smeared = sanitize_sample(-smear_amount * low_passed + *smear_state);
    *smear_state = sanitize_sample(low_passed + smear_amount * smeared);

    sanitize_sample(smeared)
}

#[inline]
fn reels_tape_saturate(sample: f32, decay: f32) -> f32 {
    let decay = decay.clamp(0.0, 1.0);
    let drive = 1.0 + decay * 2.9;
    let bias = sample * sample * decay * 0.022;
    let driven = (sample + bias) * drive;
    let saturated = driven.tanh() / (drive * (1.0 + decay * 0.04));
    // Blend toward the clean signal at low decay so subtle echoes stay
    // transparent, while heavy settings drive the full warm tape saturation.
    // The blend collapses quickly, so the well-tuned high-decay feedback
    // behaviour is essentially unchanged.
    let clean_amount = (1.0 - decay) * (1.0 - decay) * 0.5;
    sanitize_sample(sample * clean_amount + saturated * (1.0 - clean_amount))
}

fn reels_drift_next(
    channel: usize,
    sample_rate: f32,
    size: f32,
    rng: &mut u32,
    state: &mut f32,
    target: &mut f32,
    countdown: &mut usize,
) -> f32 {
    let size = size.clamp(0.0, 1.0);
    if *countdown == 0 {
        let random = next_reels_random(rng);
        *target = ((random * 2.0) - 1.0) * size * (2.2 + size * 4.0);
        let hold_ms = 130.0 + random * 290.0 + channel as f32 * 31.0;
        *countdown = (hold_ms * 0.001 * sample_rate.max(1.0)).round() as usize;
    } else {
        *countdown -= 1;
    }

    let alpha = (1.0 / (sample_rate.max(1.0) * 0.26)).clamp(0.0, 1.0);
    *state += (*target - *state) * alpha;
    sanitize_sample(*state)
}

#[inline]
fn next_reels_random(rng: &mut u32) -> f32 {
    let mut state = *rng;
    if state == 0 {
        state = 0x51f1_7eed;
    }
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    *rng = state;
    ((state >> 8) as f32) * (1.0 / 16_777_216.0)
}

#[inline]
fn space_reverb_feedback(decay: f32) -> f32 {
    (0.55 + (decay.clamp(0.0, 1.0) * 0.35)).clamp(0.55, 0.90)
}

#[inline]
fn space_damping_alpha(damping: f32) -> f32 {
    (0.72 - (damping.clamp(0.0, 1.0) * 0.52)).clamp(0.20, 0.72)
}

#[inline]
fn reverse_segment_length_samples(
    frame: &DiffusionFrame,
    sample_rate: f32,
    buf_len: usize,
) -> usize {
    let time_ms = frame.time_ms.clamp(60.0, 2_000.0);
    let size = frame.size.clamp(0.0, 1.0);
    let segment_ms = (time_ms * (0.35 + size * 0.55)).clamp(60.0, 1_600.0);
    let samples = (segment_ms * 0.001 * sample_rate.max(1.0)).round() as usize;
    let min_samples = (sample_rate.max(1.0) * 0.060).round() as usize;
    let max_samples = (sample_rate.max(1.0) * 1.600).round() as usize;
    samples.clamp(min_samples.max(8), max_samples.min((buf_len / 2).max(8)))
}

#[inline]
fn reverse_window_samples(
    channel: usize,
    frame: &DiffusionFrame,
    sample_rate: f32,
    buf_len: usize,
    segment_len: usize,
) -> usize {
    let offset = frame.stereo_offset.clamp(-0.5, 0.5) * frame.width.clamp(0.0, 1.0) * 0.35;
    let multiplier = if channel == 0 {
        1.0 - offset
    } else {
        1.0 + offset
    };
    let pre_delay_ms = frame.pre_delay_ms.clamp(0.0, 120.0) * 0.35;
    let window_ms =
        (frame.time_ms.clamp(60.0, 2_000.0) * multiplier + pre_delay_ms).clamp(60.0, 2_000.0);
    let samples = (window_ms * 0.001 * sample_rate.max(1.0)).round() as usize;
    let min_window = segment_len.saturating_add(2);
    samples.clamp(min_window, buf_len.saturating_sub(2).max(min_window))
}

#[inline]
fn reverse_crossfade_samples(segment_len: usize, sample_rate: f32) -> usize {
    let min_samples = (sample_rate.max(1.0) * 0.008).round() as usize;
    let max_samples = (sample_rate.max(1.0) * 0.040).round() as usize;
    let max_for_segment = (segment_len / 2).max(1);
    (segment_len / 5)
        .clamp(min_samples.max(4), max_samples.max(4))
        .min(max_for_segment)
        .max(1)
}

#[inline]
fn reverse_playback_envelope(pos: usize, len: usize, decay: f32, sample_rate: f32) -> f32 {
    if len < 2 {
        return 1.0;
    }
    let pos = pos.min(len - 1);
    let progress = pos as f32 / (len - 1) as f32;
    let decay = decay.clamp(0.0, 1.0);

    let fade_samples = (sample_rate * 0.0035).round() as usize;
    let fade_ratio = (fade_samples as f32 / len as f32).clamp(0.0, 0.40);

    let fade_in = if progress < fade_ratio {
        smoothstep(progress / fade_ratio)
    } else {
        1.0
    };

    let swell_amount = 0.55 + decay * 0.45;
    let swell = smoothstep(progress).powf(1.0 / swell_amount.max(0.01));

    (swell * fade_in).clamp(0.0, 1.0)
}

#[inline]
fn reverse_tone_filter(
    input: f32,
    frame: &DiffusionFrame,
    sample_rate: f32,
    state: &mut f32,
) -> f32 {
    let tone = frame.tone_alpha.clamp(0.0, 1.0);
    let damping = frame.damping.clamp(0.0, 1.0);
    let decay = frame.decay.clamp(0.0, 1.0);
    let cutoff = (950.0 + tone * 12_000.0) * (1.0 - damping * 0.38) * (1.0 - decay * 0.22);
    let cutoff = cutoff.clamp(600.0, 13_500.0);
    let alpha = one_pole_alpha(cutoff, sample_rate);
    *state = sanitize_sample(*state + alpha * (input - *state));
    *state
}

#[inline]
fn reverse_feedback_saturate(sample: f32, decay: f32) -> f32 {
    let drive = 1.0 + decay.clamp(0.0, 1.0) * 0.7;
    sanitize_sample((sample * drive).tanh() / drive)
}

#[inline]
fn reverse_feedback_gain(feedback: f32, decay: f32) -> f32 {
    let feedback = feedback.clamp(0.0, 1.0);
    let decay = decay.clamp(0.0, 1.0);
    (feedback * (0.32 + decay * 0.20)).clamp(0.0, 0.54)
}

#[inline]
fn reverse_level_compensation(feedback: f32, decay: f32) -> f32 {
    let feedback = feedback.clamp(0.0, 1.0);
    let decay = decay.clamp(0.0, 1.0);
    (0.96 + decay * 0.04 - feedback * 0.12).clamp(0.72, 1.0)
}

#[inline]
fn collage_fragment_length_samples(
    frame: &DiffusionFrame,
    sample_rate: f32,
    buf_len: usize,
) -> usize {
    let time_ms = frame.time_ms.clamp(8.0, 600.0);
    let size = frame.size.clamp(0.0, 1.0);
    let decay = frame.decay.clamp(0.0, 1.0);
    let density_scale = 1.0 - decay * 0.35;
    let fragment_ms = (6.0 + time_ms * (0.10 + size * 0.42) * density_scale).clamp(6.0, 320.0);
    let samples = (fragment_ms * 0.001 * sample_rate.max(1.0)).round() as usize;
    samples.clamp(8, (buf_len / 4).max(8))
}

#[inline]
fn collage_fragment_delay_samples(
    channel: usize,
    frame: &DiffusionFrame,
    sample_rate: f32,
    buf_len: usize,
    fragment_len: usize,
    random_a: f32,
    random_b: f32,
) -> f32 {
    let sample_rate = sample_rate.max(1.0);
    let base_samples = frame.time_ms.clamp(12.0, 1_600.0) * 0.001 * sample_rate;
    let min_delay = (fragment_len as f32 + sample_rate * 0.006).clamp(2.0, buf_len as f32 - 2.0);
    let max_delay = (buf_len as f32 - 2.0 - fragment_len as f32 * 4.0)
        .max(min_delay + 1.0)
        .min(buf_len as f32 - 2.0);
    let spread = base_samples
        * (0.30 + frame.size.clamp(0.0, 1.0) * 1.40 + frame.decay.clamp(0.0, 1.0) * 0.80)
        * (0.35 + frame.width.clamp(0.0, 1.0) * 0.65);
    let random_offset = ((random_a.clamp(0.0, 0.999_999) * 2.0) - 1.0) * spread;
    let stereo_sign = if channel == 0 { -1.0 } else { 1.0 };
    let stereo_offset = stereo_sign
        * frame.stereo_offset.clamp(-0.5, 0.5)
        * frame.width.clamp(0.0, 1.0)
        * base_samples
        * 0.55;
    let snap =
        (fragment_len as f32 * (1.0 + (random_b.clamp(0.0, 0.999_999) * 4.0).floor())).max(16.0);
    let unsnapped = base_samples + random_offset + stereo_offset;
    let snapped = (unsnapped / snap).round() * snap;

    snapped.clamp(min_delay, max_delay)
}

#[inline]
fn collage_repeats_remaining(feedback: f32, decay: f32, random: f32) -> usize {
    let feedback = feedback.clamp(0.0, 1.0);
    let decay = decay.clamp(0.0, 1.0);
    let base = (feedback * 5.0).floor() as usize;
    let extra = if random < feedback * decay * 0.85 {
        1
    } else {
        0
    };
    (base + extra).min(6)
}

#[inline]
fn collage_change_probability(feedback: f32, decay: f32) -> f32 {
    (0.18 + decay.clamp(0.0, 1.0) * 0.58 - feedback.clamp(0.0, 1.0) * 0.22).clamp(0.08, 0.78)
}

#[inline]
fn collage_crossfade_samples(fragment_len: usize, sample_rate: f32) -> usize {
    let min_samples = (sample_rate.max(1.0) * 0.0015).round() as usize;
    let max_samples = (sample_rate.max(1.0) * 0.006).round() as usize;
    let max_for_fragment = (fragment_len / 2).max(1);
    (fragment_len / 4)
        .clamp(min_samples.max(4), max_samples.max(4))
        .min(max_for_fragment)
        .max(1)
}

#[inline]
fn collage_tone_alpha(frame: &DiffusionFrame) -> f32 {
    (frame.tone_alpha * (1.0 - frame.damping.clamp(0.0, 1.0) * 0.68)).clamp(0.03, 0.95)
}

#[inline]
fn collage_feedback_gain(feedback: f32, decay: f32) -> f32 {
    let feedback = feedback.clamp(0.0, 1.0);
    let decay = decay.clamp(0.0, 1.0);
    (feedback * (0.34 + decay * 0.22)).clamp(0.0, 0.58)
}

#[inline]
fn collage_level_compensation(feedback: f32, decay: f32) -> f32 {
    let feedback = feedback.clamp(0.0, 1.0);
    let decay = decay.clamp(0.0, 1.0);
    (0.86 + decay * 0.10 - feedback * 0.12).clamp(0.68, 0.96)
}

#[cfg(test)]
mod tests {
    use super::{
        cascade_level_compensation, collage_crossfade_samples, collage_feedback_gain,
        collage_fragment_length_samples, collage_level_compensation, read_interpolated,
        reels_level_compensation, reverse_crossfade_samples, reverse_feedback_gain,
        reverse_level_compensation, reverse_segment_length_samples, reverse_window_samples,
        space_reverb_feedback, Diffusion, DiffusionFrame, DiffusionMode,
    };
    use crate::dsp::transport::{NoteDivision, TransportFrame};
    use crate::params::DiffusionParams;
    use nih_plug::prelude::{BoolParam, EnumParam};

    #[test]
    fn diffusion_sync_locks_time_to_tempo() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let transport = TransportFrame {
            bpm: 120.0,
            ..TransportFrame::default()
        };
        // 120 BPM: 1/4 = 500 ms, 1/8 = 250 ms, 1 bar = 2000 ms.
        for (division, expected_ms) in [
            (NoteDivision::Quarter, 500.0),
            (NoteDivision::Eighth, 250.0),
            (NoteDivision::Bar1, 2_000.0),
        ] {
            let mut params = DiffusionParams::default();
            params.mode = EnumParam::new("m", DiffusionMode::Reels);
            params.sync_enabled = BoolParam::new("s", true);
            params.sync_division = EnumParam::new("d", division);
            params.reset_smoothers();
            let frame = diffusion.next_frame(&params, &transport);
            assert!(
                (frame.time_ms - expected_ms).abs() < 0.5,
                "synced {division:?} time {}",
                frame.time_ms
            );
        }

        // Sync off keeps the manual time, finite and in range.
        let mut params = DiffusionParams::default();
        params.sync_enabled = BoolParam::new("s", false);
        params.reset_smoothers();
        let frame = diffusion.next_frame(&params, &transport);
        assert!(frame.time_ms.is_finite() && frame.time_ms >= 1.0);
    }

    #[test]
    fn interpolated_delay_reads_between_samples() {
        let buffer = [0.0, 1.0, 0.0, 0.0];
        let value = read_interpolated(&buffer, 2, 1.5);

        assert!((value - 0.5).abs() < 0.000_001);
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
    fn cascade_compensation_is_reasonable() {
        let c = cascade_level_compensation(1.0, 1.0);
        assert!(c > 0.70 && c < 1.0, "cascade comp={c}");
        let c0 = cascade_level_compensation(0.0, 0.0);
        assert!((c0 - 1.0).abs() < 0.01);
    }

    #[test]
    fn cascade_impulse_taps_decrease() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = cascade_frame(60.0, 0.0, 0.5, 0.8); // no feedback, strong decay

        let mut windows = [0.0_f32; 4];
        for index in 0..16000 {
            let input = if index == 0 { 0.9 } else { 0.0 };
            let output = diffusion.process_sample_for_channel(0, input, &frame).abs();
            let window = (index / 4000).min(3);
            windows[window] = windows[window].max(output);
        }
        assert!(
            windows[0] > 0.01,
            "first taps should be audible, {windows:?}"
        );
        assert!(
            windows[2] < windows[0],
            "later taps should decay, {windows:?}"
        );
    }

    #[test]
    fn cascade_max_feedback_10s_does_not_explode() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = cascade_frame(120.0, 0.95, 0.6, 0.6);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..(48_000 * 10) {
            phase += 110.0 / 48_000.0;
            let input = if index < 4800 {
                (phase * tau).sin() * 0.3
            } else {
                0.0
            };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "cascade 10s feedback: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 4.0,
            "cascade max feedback over 10s should not explode, peak={peak}"
        );
    }

    #[test]
    fn cascade_time_sweep_has_no_strong_click() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let total = 48_000usize;
        let mut phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..total {
            let t = index as f32 / (total - 1) as f32;
            let frame = cascade_frame(60.0 + t * 400.0, 0.3, 0.5, 0.5);
            phase += 220.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 4096 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            max_step < 0.3,
            "cascade time sweep should not click strongly, max step {max_step}"
        );
    }

    #[test]
    fn cascade_stereo_offset_reads_stay_in_bounds() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        // Max time + max stereo offset pushes the tap reads to their extremes.
        let mut frame = cascade_frame(800.0, 0.4, 1.0, 0.5);
        frame.stereo_offset = 0.5;

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        for _ in 0..8192 {
            phase += 220.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let left = diffusion.process_sample_for_channel(0, input, &frame);
            let right = diffusion.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());
            assert!(left.abs() <= 8.0 && right.abs() <= 8.0);
        }
    }

    #[test]
    fn cascade_mix_zero_dry_mix_one_wet() {
        let mut dry_diffusion = Diffusion::default();
        dry_diffusion.prepare(48_000.0);
        let mut wet_diffusion = Diffusion::default();
        wet_diffusion.prepare(48_000.0);
        let mut dry_frame = cascade_frame(120.0, 0.3, 0.5, 0.5);
        dry_frame.mix = 0.0;
        let wet_frame = cascade_frame(120.0, 0.3, 0.5, 0.5);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut max_dry_diff = 0.0_f32;
        let mut wet_energy = 0.0_f64;
        let mut count = 0u32;
        for index in 0..8192 {
            phase += 220.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let dry = dry_diffusion.process_sample_for_channel(0, input, &dry_frame);
            let wet = wet_diffusion.process_sample_for_channel(0, input, &wet_frame);
            max_dry_diff = max_dry_diff.max((dry - input).abs());
            if index > 4000 {
                wet_energy += (wet as f64) * (wet as f64);
                count += 1;
            }
        }
        assert!(
            max_dry_diff < 1e-4,
            "cascade mix 0 should be dry, {max_dry_diff}"
        );
        let wet_rms = (wet_energy / count as f64).sqrt() as f32;
        assert!(
            wet_rms > 0.01,
            "cascade mix 1 should produce wet, {wet_rms}"
        );
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
    fn reels_compensation_is_reasonable() {
        let c = reels_level_compensation(1.0, 1.0);
        assert!(c > 0.75 && c < 1.0);
        let c0 = reels_level_compensation(0.0, 0.0);
        assert!((c0 - 1.0).abs() < 0.01);
    }

    #[test]
    fn reels_impulse_repeats_decay_without_runaway() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reels_frame(90.0, 0.78, 0.55, 0.65);

        let mut window_peaks = [0.0_f32; 5];
        for index in 0..32_000 {
            let input = if index == 0 { 0.9 } else { 0.0 };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            let repeat = index / 4_320;
            if repeat > 0 && repeat <= window_peaks.len() {
                window_peaks[repeat - 1] = window_peaks[repeat - 1].max(output.abs());
            }
        }

        assert!(window_peaks[0] > 0.05, "first repeat should be audible");
        assert!(
            window_peaks[3] < window_peaks[0],
            "later repeats should decay, peaks={window_peaks:?}"
        );
        assert!(
            window_peaks.iter().copied().fold(0.0, f32::max) < 1.2,
            "reels impulse repeats should stay controlled: {window_peaks:?}"
        );
    }

    #[test]
    fn reels_time_automation_stays_smooth_and_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let short_frame = reels_frame(70.0, 0.62, 0.7, 0.7);
        let long_frame = reels_frame(420.0, 0.62, 0.7, 0.7);

        let mut phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..48_000 {
            phase += 330.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.25;
            let frame = if (index / 6_000) % 2 == 0 {
                &short_frame
            } else {
                &long_frame
            };
            let output = diffusion.process_sample_for_channel(0, input, frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            if index > 6_000 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }

        assert!(
            max_step < 0.55,
            "reels time automation produced a click-sized step: {max_step}"
        );
    }

    #[test]
    fn reels_silence_has_no_dc_runaway() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reels_frame(160.0, 1.0, 1.0, 1.0);

        let mut peak = 0.0_f32;
        for _ in 0..480_000 {
            let output = diffusion.process_sample_for_channel(0, 0.0, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
        }

        assert!(
            peak < 0.000_001,
            "reels silence should stay silent, peak={peak}"
        );
    }

    #[test]
    fn reels_stereo_input_keeps_lr_difference() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = DiffusionFrame {
            mode: DiffusionMode::Reels,
            time_ms: 80.0,
            feedback: 0.58,
            size: 0.7,
            decay: 0.65,
            pre_delay_ms: 0.0,
            damping: 0.45,
            mix: 1.0,
            stereo_offset: 0.4,
            width: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.7,
            mode_fade: 1.0,
        };

        let mut left_energy = 0.0_f32;
        let mut right_energy = 0.0_f32;
        let mut diff_energy = 0.0_f32;
        for index in 0..24_000 {
            let left_in = (core::f32::consts::TAU * 220.0 * index as f32 / 48_000.0).sin() * 0.22;
            let right_in = (core::f32::consts::TAU * 330.0 * index as f32 / 48_000.0).sin() * 0.18;
            let left = diffusion.process_sample_for_channel(0, left_in, &frame);
            let right = diffusion.process_sample_for_channel(1, right_in, &frame);
            assert!(left.is_finite() && right.is_finite());
            assert!(left.abs() <= 8.0 && right.abs() <= 8.0);
            if index > 6_000 {
                left_energy += left * left;
                right_energy += right * right;
                diff_energy += (left - right) * (left - right);
            }
        }

        assert!(left_energy > 0.000_1 && right_energy > 0.000_1);
        assert!(
            diff_energy > (left_energy + right_energy) * 0.08,
            "reels should preserve stereo difference"
        );
    }

    #[test]
    fn reels_max_feedback_10s_does_not_explode() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reels_frame(180.0, 1.0, 0.6, 0.7);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..(48_000 * 10) {
            phase += 110.0 / 48_000.0;
            // Excite for the first 100 ms, then let the dub feedback ring.
            let input = if index < 4800 {
                (phase * tau).sin() * 0.3
            } else {
                0.0
            };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "reels 10s dub feedback: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 4.0,
            "reels max dub feedback over 10s should be safe, peak={peak}"
        );
    }

    #[test]
    fn reels_mix_zero_dry_mix_one_wet() {
        let mut dry_diffusion = Diffusion::default();
        dry_diffusion.prepare(48_000.0);
        let mut wet_diffusion = Diffusion::default();
        wet_diffusion.prepare(48_000.0);
        let mut dry_frame = reels_frame(150.0, 0.5, 0.5, 0.5);
        dry_frame.mix = 0.0;
        let wet_frame = reels_frame(150.0, 0.5, 0.5, 0.5);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut max_dry_diff = 0.0_f32;
        let mut wet_energy = 0.0_f64;
        let mut count = 0u32;
        for index in 0..12000 {
            phase += 220.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let dry = dry_diffusion.process_sample_for_channel(0, input, &dry_frame);
            let wet = wet_diffusion.process_sample_for_channel(0, input, &wet_frame);
            max_dry_diff = max_dry_diff.max((dry - input).abs());
            if index > 6000 {
                wet_energy += (wet as f64) * (wet as f64);
                count += 1;
            }
        }
        assert!(
            max_dry_diff < 1e-4,
            "reels mix 0 should be dry, {max_dry_diff}"
        );
        let wet_rms = (wet_energy / count as f64).sqrt() as f32;
        assert!(wet_rms > 0.01, "reels mix 1 should produce wet, {wet_rms}");
    }

    fn collage_frame(
        time_ms: f32,
        feedback: f32,
        size: f32,
        decay: f32,
        stereo_offset: f32,
        width: f32,
    ) -> DiffusionFrame {
        DiffusionFrame {
            mode: DiffusionMode::Collage,
            time_ms,
            feedback,
            size,
            decay,
            pre_delay_ms: 0.0,
            damping: 0.45,
            mix: 1.0,
            stereo_offset,
            width,
            active_mix: 1.0,
            tone_alpha: 0.65,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn collage_output_stays_finite_with_sine() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = collage_frame(45.0, 0.55, 0.35, 0.75, 0.25, 1.0);

        let mut phase = 0.0;
        let mut previous = 0.0;
        let mut peak = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..24_000 {
            phase += 440.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.25;
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            peak = peak.max(output.abs());
            if index > 6_000 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }

        assert!(peak > 0.001, "collage sine output should become audible");
        assert!(
            max_step < 0.35,
            "collage sine output had a click-sized step: {max_step}"
        );
    }

    #[test]
    fn collage_impulse_tail_stays_bounded_and_smooth() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = collage_frame(38.0, 0.70, 0.30, 0.85, -0.2, 1.0);

        let mut previous = 0.0;
        let mut peak = 0.0_f32;
        let mut tail_energy = 0.0_f32;
        let mut max_tail_step = 0.0_f32;
        for index in 0..32_000 {
            let input = if index == 0 { 0.9 } else { 0.0 };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            peak = peak.max(output.abs());
            if index > 4_000 {
                tail_energy += output * output;
                max_tail_step = max_tail_step.max((output - previous).abs());
            }
            previous = output;
        }

        assert!(peak < 1.2, "collage impulse peak was {peak}");
        assert!(
            tail_energy > 0.000_001,
            "collage impulse tail died too quickly"
        );
        assert!(
            max_tail_step < 0.45,
            "collage impulse tail had a click-sized step: {max_tail_step}"
        );
    }

    #[test]
    fn collage_uses_crossfade_at_fragment_boundaries() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = collage_frame(18.0, 0.8, 0.10, 0.35, 0.0, 1.0);

        diffusion.process_sample_for_channel(0, 0.2, &frame);
        let fragment_len = diffusion.collage.fragment_length_samples[0];
        assert!(fragment_len > 0);

        let mut saw_crossfade = false;
        for _ in 0..(fragment_len + 8) {
            diffusion.process_sample_for_channel(0, 0.2, &frame);
            saw_crossfade |= diffusion.collage.crossfade_length_samples[0] > 0;
        }

        assert!(
            saw_crossfade,
            "collage should crossfade after fragment wrap"
        );
    }

    #[test]
    fn collage_stereo_offset_spreads_fragment_choices() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = collage_frame(55.0, 0.4, 0.4, 0.6, 0.5, 1.0);

        diffusion.process_sample_for_channel(0, 0.2, &frame);
        diffusion.process_sample_for_channel(1, 0.2, &frame);

        let left = diffusion.collage.fragment_delay_samples[0];
        let right = diffusion.collage.fragment_delay_samples[1];
        assert!(
            (left - right).abs() > 1.0,
            "stereo offset should choose different fragment delays, l={left}, r={right}"
        );
    }

    #[test]
    fn collage_parameter_helpers_are_controlled() {
        let frame = collage_frame(40.0, 1.0, 0.5, 1.0, 0.0, 1.0);
        let len = collage_fragment_length_samples(&frame, 48_000.0, 100_800);
        let xfade = collage_crossfade_samples(len, 48_000.0);

        assert!(len >= 8);
        assert!(xfade > 0 && xfade <= len / 2);
        assert!(collage_feedback_gain(1.0, 1.0) <= 0.58);
        assert!(collage_level_compensation(1.0, 1.0) < 1.0);
    }

    #[test]
    fn collage_silence_stays_silent() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = collage_frame(120.0, 0.5, 0.5, 0.8, 0.0, 1.0);

        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            let output = diffusion.process_sample_for_channel(0, 0.0, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
        }
        // The randomized fragment chooser must never synthesize noise from
        // silence — reading a silent buffer stays silent.
        assert!(
            peak < 1e-6,
            "collage should stay silent on silence, peak {peak}"
        );
    }

    #[test]
    fn collage_max_feedback_10s_does_not_explode() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = collage_frame(120.0, 1.0, 0.6, 0.7, 0.2, 1.0);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..(48_000 * 10) {
            phase += 110.0 / 48_000.0;
            let input = if index < 4800 {
                (phase * tau).sin() * 0.3
            } else {
                0.0
            };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "collage 10s feedback: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 4.0,
            "collage max feedback over 10s should be safe, peak={peak}"
        );
    }

    #[test]
    fn collage_extreme_params_stay_in_bounds() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        // Extreme time, size, decay and stereo offset push the fragment reads to
        // their limits; clamped indexing must keep them inside the buffer.
        let frame = collage_frame(2000.0, 0.9, 1.0, 1.0, 0.5, 1.0);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        for _ in 0..16000 {
            phase += 220.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let left = diffusion.process_sample_for_channel(0, input, &frame);
            let right = diffusion.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());
            assert!(left.abs() <= 8.0 && right.abs() <= 8.0);
        }
    }

    fn reverse_frame(
        time_ms: f32,
        feedback: f32,
        size: f32,
        decay: f32,
        stereo_offset: f32,
        width: f32,
    ) -> DiffusionFrame {
        DiffusionFrame {
            mode: DiffusionMode::Reverse,
            time_ms,
            feedback,
            size,
            decay,
            pre_delay_ms: 0.0,
            damping: 0.45,
            mix: 1.0,
            stereo_offset,
            width,
            active_mix: 1.0,
            tone_alpha: 0.65,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn reverse_impulse_becomes_audible_and_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reverse_frame(32.0, 0.55, 0.65, 0.75, 0.0, 1.0);

        let mut peak = 0.0_f32;
        let mut tail_energy = 0.0_f32;
        let mut previous = 0.0;
        let mut max_tail_step = 0.0_f32;
        for index in 0..12_000 {
            let input = if index == 0 { 0.9 } else { 0.0 };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            peak = peak.max(output.abs());
            if index > 700 {
                tail_energy += output * output;
                max_tail_step = max_tail_step.max((output - previous).abs());
            }
            previous = output;
        }

        assert!(peak > 0.01, "reverse impulse should become audible");
        assert!(
            tail_energy > 0.000_01,
            "reverse impulse tail died too quickly"
        );
        assert!(
            max_tail_step < 0.55,
            "reverse impulse produced a click-sized step: {max_tail_step}"
        );
    }

    #[test]
    fn reverse_time_change_does_not_click_extremely() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let short_frame = reverse_frame(28.0, 0.45, 0.55, 0.60, 0.0, 1.0);
        let long_frame = reverse_frame(120.0, 0.45, 0.55, 0.60, 0.0, 1.0);

        let mut phase = 0.0;
        let mut previous = 0.0;
        let mut max_step_after_change = 0.0_f32;
        for index in 0..24_000 {
            phase += 330.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.28;
            let frame = if index < 8_000 {
                &short_frame
            } else {
                &long_frame
            };
            let output = diffusion.process_sample_for_channel(0, input, frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            if index > 8_000 {
                max_step_after_change = max_step_after_change.max((output - previous).abs());
            }
            previous = output;
        }

        assert!(
            max_step_after_change < 0.45,
            "reverse time change produced a click-sized step: {max_step_after_change}"
        );
    }

    #[test]
    fn reverse_uses_crossfade_between_segments() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reverse_frame(24.0, 0.5, 0.5, 0.6, 0.0, 1.0);

        diffusion.process_sample_for_channel(0, 0.2, &frame);
        let segment_len = diffusion.reverse.segment_lengths[0];
        assert!(segment_len > 0);

        let mut saw_crossfade = false;
        for _ in 0..(segment_len + 8) {
            diffusion.process_sample_for_channel(0, 0.2, &frame);
            saw_crossfade |= diffusion.reverse.crossfade_length_samples[0] > 0;
        }

        assert!(
            saw_crossfade,
            "reverse should crossfade between captured segments"
        );
    }

    #[test]
    fn reverse_stereo_offset_changes_capture_windows() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reverse_frame(60.0, 0.4, 0.5, 0.5, 0.5, 1.0);

        diffusion.process_sample_for_channel(0, 0.2, &frame);
        diffusion.process_sample_for_channel(1, 0.2, &frame);

        let left = diffusion.reverse.segment_starts[0];
        let right = diffusion.reverse.segment_starts[1];
        assert_ne!(
            left, right,
            "stereo offset should capture different reverse windows"
        );
    }

    #[test]
    fn reverse_parameter_helpers_are_controlled() {
        let frame = reverse_frame(80.0, 1.0, 1.0, 1.0, 0.0, 1.0);
        let len = reverse_segment_length_samples(&frame, 48_000.0, 100_800);
        let window = reverse_window_samples(0, &frame, 48_000.0, 100_800, len);
        let xfade = reverse_crossfade_samples(len, 48_000.0);

        assert!(len >= 8);
        assert!(window > len);
        assert!(xfade > 0 && xfade <= len / 2);
        assert!(reverse_feedback_gain(1.0, 1.0) <= 0.54);
        assert!(reverse_level_compensation(1.0, 1.0) < 1.0);
    }

    #[test]
    fn reverse_max_feedback_for_ten_seconds_does_not_runaway() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = reverse_frame(180.0, 1.0, 0.85, 1.0, 0.25, 1.0);

        let mut peak = 0.0_f32;
        let mut phase = 0.0_f32;
        for index in 0..480_000 {
            phase += 220.0 / 48_000.0;
            let input = if index < 240_000 {
                (phase * core::f32::consts::TAU).sin() * 0.24
            } else {
                0.0
            };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
        }

        assert!(peak < 1.8, "reverse max feedback peak was {peak}");
    }

    #[test]
    fn reverse_size_extremes_and_stereo_offset_stay_safe() {
        for (size, stereo_offset) in [(0.0, -0.5), (1.0, 0.5)] {
            let mut diffusion = Diffusion::default();
            diffusion.prepare(48_000.0);
            let frame = reverse_frame(120.0, 0.72, size, 0.75, stereo_offset, 1.0);

            let mut previous_l = 0.0_f32;
            let mut previous_r = 0.0_f32;
            let mut max_step = 0.0_f32;
            for index in 0..36_000 {
                let hit = index % 6_000 == 0;
                let left_in = if hit { 0.85 } else { 0.0 };
                let right_in = if hit { -0.55 } else { 0.0 };
                let left = diffusion.process_sample_for_channel(0, left_in, &frame);
                let right = diffusion.process_sample_for_channel(1, right_in, &frame);
                assert!(left.is_finite() && right.is_finite());
                assert!(left.abs() <= 8.0 && right.abs() <= 8.0);
                if index > 4_000 {
                    max_step = max_step
                        .max((left - previous_l).abs())
                        .max((right - previous_r).abs());
                }
                previous_l = left;
                previous_r = right;
            }

            assert!(
                max_step < 0.85,
                "reverse size/stereo extreme clicked, size={size}, stereo={stereo_offset}, step={max_step}"
            );
        }
    }

    #[test]
    fn reverse_time_and_size_automation_stays_finite() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let short_frame = reverse_frame(70.0, 0.52, 0.0, 0.35, -0.25, 1.0);
        let long_frame = reverse_frame(900.0, 0.52, 1.0, 0.85, 0.25, 1.0);

        let mut phase = 0.0_f32;
        for index in 0..72_000 {
            phase += 275.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.24;
            let frame = if (index / 12_000) % 2 == 0 {
                &short_frame
            } else {
                &long_frame
            };
            let output = diffusion.process_sample_for_channel(0, input, frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
        }
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
    fn space_reverb_feedback_is_in_range() {
        let fb0 = space_reverb_feedback(0.0);
        let fb1 = space_reverb_feedback(1.0);
        assert!(fb0 > 0.50 && fb0 < 0.65);
        assert!(fb1 > 0.80 && fb1 < 0.92);
    }

    #[test]
    fn space_impulse_tail_is_smooth() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = space_frame(1.0, 0.9, 0.6, 1.0);

        let mut previous = 0.0_f32;
        let mut max_tail_step = 0.0_f32;
        for index in 0..32000 {
            let input = if index == 0 { 0.9 } else { 0.0 };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 2000 {
                max_tail_step = max_tail_step.max((output - previous).abs());
            }
            previous = output;
        }
        // A smooth ambient tail has no large sample-to-sample jumps (a metallic
        // resonance or a click would spike this).
        assert!(
            max_tail_step < 0.1,
            "space tail should be smooth, max step {max_tail_step}"
        );
    }

    #[test]
    fn space_size_sweep_has_no_strong_click() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let total = 48_000usize;
        let mut phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..total {
            let t = index as f32 / (total - 1) as f32;
            let frame = space_frame(t, 0.7, 0.5, 1.0); // size 0 -> 1
            phase += 220.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 2000 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            max_step < 0.15,
            "space size sweep should not click, max step {max_step}"
        );
    }

    #[test]
    fn space_white_noise_tail_is_not_metallic() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = space_frame(0.9, 0.85, 0.5, 1.0);

        // Excite with a noise burst, then let the tail ring out.
        let mut rng: u32 = 0x51ee_d5a7;
        let mut peak = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_tail_step = 0.0_f32;
        for index in 0..48_000 {
            let input = if index < 4800 {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.25
            } else {
                0.0
            };
            let output = diffusion.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
            if index > 9600 {
                // long after the burst, only tail
                max_tail_step = max_tail_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            peak < 2.0,
            "space white-noise peak should be controlled, {peak}"
        );
        // Excessive metallic ringing would show up as large steps in the decayed
        // tail; a smooth ambient decay keeps them small.
        assert!(
            max_tail_step < 0.05,
            "space tail should decay smoothly, not ring metallically, max step {max_tail_step}"
        );
    }

    #[test]
    fn space_stereo_width_is_mono_compatible() {
        let mut diffusion = Diffusion::default();
        diffusion.prepare(48_000.0);
        let frame = space_frame(0.8, 0.7, 0.5, 1.0); // full width

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut in_sq = 0.0_f64;
        let mut mono_sq = 0.0_f64;
        let mut count = 0u32;
        for index in 0..24_000 {
            phase += 220.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let left = diffusion.process_sample_for_channel(0, input, &frame);
            let right = diffusion.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());
            if index > 8000 {
                let mono = (left + right) * 0.5;
                in_sq += (input as f64) * (input as f64);
                mono_sq += (mono as f64) * (mono as f64);
                count += 1;
            }
        }
        let in_rms = (in_sq / count as f64).sqrt() as f32;
        let mono_rms = (mono_sq / count as f64).sqrt() as f32;
        // The wide stereo tail must not vanish when summed to mono.
        assert!(
            mono_rms > in_rms * 0.1,
            "space mono sum should survive (in {in_rms}, mono {mono_rms})"
        );
    }
}
