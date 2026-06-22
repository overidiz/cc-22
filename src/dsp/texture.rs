use nih_plug::prelude::*;

use crate::params::TextureParams;

use super::{
    chain::{sanitize_sample, soft_clip_sample, ModuleCore},
    dry_wet::DryWet,
    gain::db_to_gain,
    smoothing::LinearSmoother,
    transport::{hz_for_division, samples_for_division, TransportFrame},
    util::{dc_blocker_step, one_pole_alpha, smoothstep, soft_saturate},
};

const MAX_CHANNELS: usize = 2;
const MAX_DELAY_SECONDS: f32 = 0.08;
const BASE_DELAY_MS: f32 = 18.0;

/// The five Texture modes, in product order. Variants are identified by their
/// stable `#[id]`, so dropping the legacy modes (off/wow-flutter/noise/tape)
/// keeps state compatibility for every id that remains.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureMode {
    #[id = "filter"]
    #[name = "Filter"]
    Filter,

    #[id = "squash"]
    #[name = "Squash"]
    Squash,

    #[id = "cassette"]
    #[name = "Cassette"]
    Cassette,

    #[id = "broken"]
    #[name = "Broken"]
    Broken,

    #[id = "interference"]
    #[name = "Interference"]
    Interference,
}

impl TextureMode {
    /// The five product modes, in the exact order shown in the UI.
    pub const PRODUCT_MODES: [TextureMode; 5] = [
        TextureMode::Filter,
        TextureMode::Squash,
        TextureMode::Cassette,
        TextureMode::Broken,
        TextureMode::Interference,
    ];
}

#[derive(Debug, Clone)]
pub struct Texture {
    core: ModuleCore,
    delay: StereoDelay,
    filter: TextureFilter,
    squash: SquashState,
    cassette: CassetteState,
    broken: BrokenState,
    interference: InterferenceState,
    sample_rate: f32,
    wow_phase: f32,
    flutter_phase: f32,
    drift: [DriftGenerator; MAX_CHANNELS],
    noise: [NoiseGenerator; MAX_CHANNELS],
    current_mode: TextureMode,
    mode_crossfade: LinearSmoother,
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
    /// Tempo-synced period in samples (set per block when sync is on), used by
    /// the Broken dropout gate and Interference pulse; `None` = free-running.
    sync_samples: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct TextureFrame {
    mode: TextureMode,
    wow_depth: f32,
    flutter_depth: f32,
    random_drift: f32,
    noise_amount: f32,
    noise_color: f32,
    degrade: f32,
    stereo_spread: f32,
    mix: f32,
    active_mix: f32,
    wow_phase: f32,
    flutter_phase: f32,
    mode_fade: f32,
}

#[derive(Debug, Clone, Default)]
struct StereoDelay {
    buffers: [Vec<f32>; MAX_CHANNELS],
    write_positions: [usize; MAX_CHANNELS],
}

#[derive(Debug, Clone, Default)]
struct TextureFilter {
    integrator_1: [f32; MAX_CHANNELS],
    integrator_2: [f32; MAX_CHANNELS],
    cutoff_state: [f32; MAX_CHANNELS],
}

#[derive(Debug, Clone, Default)]
struct SquashState {
    detector_state: [f32; MAX_CHANNELS],
    envelope: [f32; MAX_CHANNELS],
    linked_envelope: f32,
}

#[derive(Debug, Clone)]
struct CassetteState {
    lowpass_state: [f32; MAX_CHANNELS],
    highpass_state: [f32; MAX_CHANNELS],
    highpass_input: [f32; MAX_CHANNELS],
    instability_state: [f32; MAX_CHANNELS],
    bump_state: [f32; MAX_CHANNELS],
    hiss_env: [f32; MAX_CHANNELS],
    wear_state: [f32; MAX_CHANNELS],
    wear_rng: [u32; MAX_CHANNELS],
}

impl Default for CassetteState {
    fn default() -> Self {
        Self {
            lowpass_state: [0.0; MAX_CHANNELS],
            highpass_state: [0.0; MAX_CHANNELS],
            highpass_input: [0.0; MAX_CHANNELS],
            instability_state: [0.0; MAX_CHANNELS],
            bump_state: [0.0; MAX_CHANNELS],
            hiss_env: [0.0; MAX_CHANNELS],
            wear_state: [0.0; MAX_CHANNELS],
            // Distinct non-zero xorshift seeds per channel so the slow wear
            // wander is decorrelated L/R but fully deterministic.
            wear_rng: [0x68B5_2F1D, 0x1C3A_77E9],
        }
    }
}

#[derive(Debug, Clone)]
struct BrokenState {
    rng_state: [u32; MAX_CHANNELS],
    dropout_gain: [f32; MAX_CHANNELS],
    dropout_target: [f32; MAX_CHANNELS],
    dropout_remaining: [u32; MAX_CHANNELS],
    hold_counter: [u32; MAX_CHANNELS],
    held_sample: [f32; MAX_CHANNELS],
    post_filter_state: [f32; MAX_CHANNELS],
    dc_state: [f32; MAX_CHANNELS],
    sync_counter: [usize; MAX_CHANNELS],
}

#[derive(Debug, Clone, Default)]
struct InterferenceState {
    carrier_phase: [f32; MAX_CHANNELS],
    parasite_phase: [f32; MAX_CHANNELS],
    amplitude_state: [f32; MAX_CHANNELS],
    noise_band_hi: [f32; MAX_CHANNELS],
    noise_band_lo: [f32; MAX_CHANNELS],
    damp_state: [f32; MAX_CHANNELS],
    sync_counter: [usize; MAX_CHANNELS],
}

#[derive(Debug, Clone)]
struct DriftGenerator {
    rng_state: u32,
    smoother: LinearSmoother,
    sample_rate: f32,
}

#[derive(Debug, Clone)]
struct NoiseGenerator {
    rng_state: u32,
    low_state: f32,
    low2_state: f32,
}

impl Default for Texture {
    fn default() -> Self {
        let mut texture = Self {
            core: ModuleCore::default(),
            delay: StereoDelay::default(),
            filter: TextureFilter::default(),
            squash: SquashState::default(),
            cassette: CassetteState::default(),
            broken: BrokenState::default(),
            interference: InterferenceState::default(),
            sample_rate: 44_100.0,
            wow_phase: 0.0,
            flutter_phase: 0.25,
            drift: [
                DriftGenerator::new(0x1a2b_3c4d),
                DriftGenerator::new(0x5566_7788),
            ],
            noise: [
                NoiseGenerator::new(0x1020_3040),
                NoiseGenerator::new(0xa0b0_c0d0),
            ],
            current_mode: TextureMode::Filter,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
            sync_samples: None,
        };
        texture.prepare(44_100.0);
        texture
    }
}

impl Texture {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.core.prepare(self.sample_rate);
        self.delay.prepare(self.sample_rate);
        self.filter.reset();
        self.squash.reset();
        self.cassette.reset();
        self.broken.reset();
        self.interference.reset();
        self.mode_crossfade.prepare(self.sample_rate);
        for drift in &mut self.drift {
            drift.prepare(self.sample_rate);
        }
        for noise in &mut self.noise {
            noise.reset();
        }
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.delay.reset();
        self.filter.reset();
        self.squash.reset();
        self.cassette.reset();
        self.broken.reset();
        self.interference.reset();
        self.wow_phase = 0.0;
        self.flutter_phase = 0.25;
        for drift in &mut self.drift {
            drift.reset();
        }
        for noise in &mut self.noise {
            noise.reset();
        }
        self.mode_crossfade.reset(1.0);
        self.last_output = [0.0; MAX_CHANNELS];
        self.has_processed = false;
    }

    pub fn next_frame(
        &mut self,
        params: &TextureParams,
        transport: &TransportFrame,
    ) -> TextureFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let wow_depth = params.wow_depth.smoothed.next().clamp(0.0, 1.0);
        // Cassette wow can lock to tempo (kept subtle inside its slow range).
        // Filter/Squash don't use wow, so sync is a no-op for them.
        let manual_wow_rate = params.wow_rate.smoothed.next().clamp(0.1, 2.0);
        let wow_rate_hz = if params.sync_enabled.value() {
            hz_for_division(transport.bpm, params.sync_division.value()).clamp(0.1, 2.0)
        } else {
            manual_wow_rate
        };
        // Broken: tempo-locked dropout period (clamped so the glitch rate can't
        // get absurdly fast), or free-running random when sync is off.
        self.sync_samples = if params.sync_enabled.value() {
            let period = samples_for_division(
                transport.bpm,
                params.sync_division.value(),
                self.sample_rate,
            );
            Some(period.max((self.sample_rate * 0.04) as usize))
        } else {
            None
        };
        let flutter_depth = params.flutter_depth.smoothed.next().clamp(0.0, 1.0);
        let flutter_rate_hz = params.flutter_rate.smoothed.next().clamp(3.0, 20.0);
        let random_drift = params.random_drift.smoothed.next().clamp(0.0, 1.0);
        let noise_amount = params.noise_amount.smoothed.next().clamp(0.0, 1.0);
        let noise_color = params.noise_color.smoothed.next().clamp(0.0, 1.0);
        let degrade = params.degrade.smoothed.next().clamp(0.0, 1.0);
        let stereo_spread = params.stereo_spread.smoothed.next().clamp(0.0, 1.0);
        let mix = params.mix.smoothed.next().clamp(0.0, 1.0);
        let module_frame = self.core.next_frame(params.bypass.value(), mix, 0.0);

        let wow_phase = self.wow_phase;
        let flutter_phase = self.flutter_phase;
        self.advance_lfos(wow_rate_hz, flutter_rate_hz);

        TextureFrame {
            mode,
            wow_depth,
            flutter_depth,
            random_drift,
            noise_amount,
            noise_color,
            degrade,
            stereo_spread,
            mix,
            active_mix: module_frame.active_mix,
            wow_phase,
            flutter_phase,
            mode_fade: self.mode_crossfade.next_value().clamp(0.0, 1.0),
        }
    }

    pub fn process_block(&mut self, buffer: &mut Buffer, params: &TextureParams) {
        let transport = TransportFrame::default();
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params, &transport);
            for (channel_index, sample) in channel_samples.into_iter().enumerate() {
                *sample = self.process_sample_for_channel(channel_index, *sample, &frame);
            }
        }
    }

    pub fn process_sample(&mut self, sample: f32, frame: &TextureFrame) -> f32 {
        self.process_sample_for_channel(0, sample, frame)
    }

    pub fn process_sample_for_channel(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &TextureFrame,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let dry = sanitize_sample(sample);
        let wet = match frame.mode {
            TextureMode::Filter => self.process_filter(index, dry, frame),
            TextureMode::Squash => self.process_squash(index, dry, frame),
            TextureMode::Cassette => self.process_cassette(index, dry, frame),
            TextureMode::Broken => self.process_broken(index, dry, frame),
            TextureMode::Interference => self.process_interference(index, dry, frame),
        };

        let mixed = DryWet.mix(dry, wet, frame.mix);
        let mixed = self.smooth_mode_transition(index, mixed, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, mixed, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    fn process_filter(&mut self, channel: usize, sample: f32, frame: &TextureFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let driven = apply_filter_drive(sample, frame.degrade);
        let cutoff_hz = texture_filter_cutoff_hz(
            frame.noise_color,
            frame.stereo_spread,
            index,
            self.sample_rate,
        );
        let resonance = texture_filter_resonance(frame.degrade);
        let filtered = self
            .filter
            .process(index, driven, cutoff_hz, resonance, self.sample_rate);
        let shaped = texture_filter_blend(sample, filtered.low, filtered.high, frame.noise_color);

        sanitize_sample(shaped * texture_filter_level_compensation(frame.degrade))
    }

    fn process_squash(&mut self, channel: usize, sample: f32, frame: &TextureFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let degrade = frame.degrade.clamp(0.0, 1.0);
        if degrade <= 0.000_001 {
            return sample;
        }

        let detector = self.squash.detector(index, sample, frame.noise_color);
        let attack = squash_attack_coeff(degrade, self.sample_rate);
        let release = squash_release_coeff(degrade, frame.noise_color, self.sample_rate);
        let envelope = self.squash.follow(index, detector, attack, release);
        let linked = self.squash.follow_linked(envelope, attack, release);
        let link_amount = 1.0 - frame.stereo_spread.clamp(0.0, 1.0);
        let detector_envelope =
            (envelope * (1.0 - link_amount)) + (envelope.max(linked) * link_amount);

        let gain = squash_compression_gain(detector_envelope, degrade);
        let makeup = squash_makeup_gain(degrade);
        let squashed = sample * gain * makeup;
        squash_soft_clip(squashed, degrade)
    }

    fn process_cassette(&mut self, channel: usize, sample: f32, frame: &TextureFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let stereo_phase = if index == 0 {
            0.0
        } else {
            0.21 * frame.stereo_spread
        };
        let wow = sine_lfo((frame.wow_phase + stereo_phase).fract()) * frame.wow_depth * 9.5;
        let flutter = sine_lfo((frame.flutter_phase + stereo_phase * 2.7).fract())
            * frame.flutter_depth
            * 2.8;
        let drift = self.drift[index].next_value(frame.random_drift) * 7.0;
        let instability = self
            .cassette
            .next_instability(index, drift, frame.stereo_spread);
        let delay_ms = (BASE_DELAY_MS + wow + flutter + instability).clamp(2.0, 65.0);
        let delayed = self
            .delay
            .process(index, sample, delay_ms * 0.001 * self.sample_rate);

        let band_limited = self
            .cassette
            .process_bandwidth(index, delayed, frame, self.sample_rate);
        let wear = self.cassette.next_wear_gain(
            index,
            frame.degrade,
            frame.random_drift,
            self.sample_rate,
        );
        let worn = band_limited * wear;
        let gate = self.cassette.hiss_gate(index, sample);
        let hiss = self.cassette_hiss(index, frame, gate);
        let with_hiss = sanitize_sample(worn + hiss);
        let degraded = apply_cassette_degradation(with_hiss, frame.degrade, hiss);

        sanitize_sample(degraded * cassette_level_compensation(frame.noise_amount, frame.degrade))
    }

    fn cassette_hiss(&mut self, channel: usize, frame: &TextureFrame, gate: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let shared = self.noise[0].next_colored(frame.noise_color);
        let local = if index == 0 {
            shared
        } else {
            self.noise[index].next_colored(frame.noise_color)
        };
        let spread = frame.stereo_spread.clamp(0.0, 1.0);
        let hiss = (shared * (1.0 - spread)) + (local * spread);

        sanitize_sample(hiss * cassette_noise_gain(frame.noise_amount, frame.degrade) * gate)
    }

    fn process_broken(&mut self, channel: usize, sample: f32, frame: &TextureFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let shared_noise = self.noise[0].next_colored(frame.noise_color);
        let local_noise = if index == 0 {
            shared_noise
        } else {
            self.noise[index].next_colored(frame.noise_color)
        };
        let spread = frame.stereo_spread.clamp(0.0, 1.0);
        let artifact_noise = (shared_noise * (1.0 - spread)) + (local_noise * spread);

        let dropout_gain = self.broken.next_dropout_gain(
            index,
            frame.degrade,
            frame.random_drift,
            self.sample_rate,
            self.sync_samples,
        );
        let held = self.broken.next_held_sample(
            index,
            sample,
            artifact_noise,
            frame.degrade,
            frame.random_drift,
            frame.stereo_spread,
        );
        let reduced = linear_crossfade(sample, held, broken_rate_reduce_mix(frame.degrade));
        let crushed = broken_bitcrush(reduced, frame.degrade);
        let noisy = sanitize_sample(
            crushed + artifact_noise * broken_artifact_gain(frame.noise_amount, frame.degrade),
        );
        let dropped = noisy * dropout_gain;
        let filtered = self.broken.post_filter(
            index,
            dropped,
            frame.noise_color,
            frame.degrade,
            self.sample_rate,
        );
        let blocked = self.broken.block_dc(index, filtered, self.sample_rate);
        let compensated = blocked * broken_level_compensation(frame.noise_amount, frame.degrade);

        // Final safety limiter: crush + artifacts + dropout transitions can stack,
        // so a soft clip guarantees a musical ceiling instead of a hard edge.
        soft_clip_sample(compensated)
    }

    fn process_interference(&mut self, channel: usize, sample: f32, frame: &TextureFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let amount = frame.noise_amount.clamp(0.0, 1.0);
        if amount <= 0.000_001 {
            return sample;
        }

        let drift = self.drift[index].next_value(frame.random_drift);
        let base_frequency = interference_frequency_hz(
            frame.noise_color,
            frame.stereo_spread,
            index,
            self.sample_rate,
        );
        let drift_range = 0.025 + frame.random_drift.clamp(0.0, 1.0) * 0.28;
        let carrier_frequency =
            (base_frequency * (1.0 + drift * drift_range)).clamp(18.0, self.sample_rate * 0.42);
        let parasite_frequency = interference_parasite_frequency_hz(
            carrier_frequency,
            frame.noise_color,
            index,
            self.sample_rate,
        );
        let carrier = self
            .interference
            .next_carrier(index, carrier_frequency, self.sample_rate);
        let parasite = self
            .interference
            .next_parasite(index, parasite_frequency, self.sample_rate);
        let amplitude_wobble = self
            .interference
            .smooth_amplitude(index, 1.0 + drift * frame.random_drift * 0.45);

        let shared_noise = self.noise[0].next_colored(frame.noise_color);
        let local_noise = if index == 0 {
            shared_noise
        } else {
            self.noise[index].next_colored(frame.noise_color)
        };
        let spread = frame.stereo_spread.clamp(0.0, 1.0);
        let electric_noise = (shared_noise * (1.0 - spread)) + (local_noise * spread);
        let band_noise = self.interference.bandpass_noise(
            index,
            electric_noise,
            carrier_frequency,
            self.sample_rate,
        );

        // Tempo sync: swell the interference rhythmically with a smooth pulse
        // (never fully gone, so it breathes on the beat). Off = steady.
        let intensity = match self.sync_samples {
            Some(period) => 0.25 + 0.75 * self.interference.next_sync_pulse(index, period),
            None => 1.0,
        };
        let am_depth = interference_am_depth(amount, frame.degrade) * amplitude_wobble * intensity;
        let ring_depth = interference_ring_depth(amount, frame.degrade);
        let modulated = sample * (1.0 + carrier * am_depth);
        let ringed = sample * carrier;
        let tonal = (carrier * 0.72 + parasite * 0.28)
            * interference_tone_gain(amount, frame.degrade)
            * amplitude_wobble;
        let noisy = band_noise * interference_noise_gain(amount, frame.degrade);
        let wet = linear_crossfade(modulated, (ringed + tonal + noisy) * intensity, ring_depth);
        let damped = self.interference.damp(
            index,
            wet,
            frame.noise_color,
            frame.degrade,
            self.sample_rate,
        );
        let clipped = interference_soft_clip(damped, frame.degrade, amount);

        sanitize_sample(clipped * interference_level_compensation(amount, frame.degrade))
    }

    fn advance_lfos(&mut self, wow_rate_hz: f32, flutter_rate_hz: f32) {
        self.wow_phase = advance_phase(self.wow_phase, wow_rate_hz, self.sample_rate);
        self.flutter_phase = advance_phase(self.flutter_phase, flutter_rate_hz, self.sample_rate);
    }

    fn set_mode(&mut self, mode: TextureMode) {
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

#[derive(Debug, Clone, Copy)]
struct FilterOutput {
    low: f32,
    high: f32,
}

impl TextureFilter {
    fn reset(&mut self) {
        self.integrator_1 = [0.0; MAX_CHANNELS];
        self.integrator_2 = [0.0; MAX_CHANNELS];
        self.cutoff_state = [0.0; MAX_CHANNELS];
    }

    fn process(
        &mut self,
        channel: usize,
        input: f32,
        cutoff_hz: f32,
        resonance: f32,
        sample_rate: f32,
    ) -> FilterOutput {
        let index = channel.min(MAX_CHANNELS - 1);
        let sample_rate = sample_rate.max(1.0);
        // Glide the cutoff (~4 ms one-pole) so moving Color quickly can't produce
        // zipper noise. State starts at the target to avoid an opening sweep.
        let target_cutoff = cutoff_hz.clamp(20.0, sample_rate * 0.45);
        if self.cutoff_state[index] <= 0.0 {
            self.cutoff_state[index] = target_cutoff;
        }
        let smooth_alpha = (1.0 / (sample_rate * 0.004)).clamp(0.0, 1.0);
        self.cutoff_state[index] += smooth_alpha * (target_cutoff - self.cutoff_state[index]);
        let cutoff_hz = self.cutoff_state[index].clamp(20.0, sample_rate * 0.45);
        let g = (core::f32::consts::PI * cutoff_hz / sample_rate).tan();
        let k = (2.0 - resonance.clamp(0.0, 1.0) * 1.25).clamp(0.70, 2.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let ic1 = self.integrator_1[index];
        let ic2 = self.integrator_2[index];
        let v3 = sanitize_sample(input - ic2);
        let band = sanitize_sample((a1 * ic1) + (a2 * v3));
        let low = sanitize_sample(ic2 + (a2 * ic1) + (a3 * v3));
        let high = sanitize_sample(input - (k * band) - low);

        self.integrator_1[index] = sanitize_sample((2.0 * band) - ic1);
        self.integrator_2[index] = sanitize_sample((2.0 * low) - ic2);

        FilterOutput { low, high }
    }
}

impl SquashState {
    fn reset(&mut self) {
        self.detector_state = [0.0; MAX_CHANNELS];
        self.envelope = [0.0; MAX_CHANNELS];
        self.linked_envelope = 0.0;
    }

    fn detector(&mut self, channel: usize, sample: f32, noise_color: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let rectified = sample.abs();
        let color = noise_color.clamp(0.0, 1.0);
        let alpha = 0.015 + color * color * 0.50;
        let state = self.detector_state[index] + alpha * (rectified - self.detector_state[index]);
        self.detector_state[index] = sanitize_sample(state);
        let transient_weight = color * 0.55;

        linear_crossfade(self.detector_state[index], rectified, transient_weight)
    }

    fn follow(&mut self, channel: usize, detector: f32, attack: f32, release: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let envelope = self.envelope[index];
        let coeff = if detector > envelope { attack } else { release };
        let next = (coeff * envelope) + ((1.0 - coeff) * detector);
        self.envelope[index] = sanitize_sample(next);
        self.envelope[index]
    }

    fn follow_linked(&mut self, envelope: f32, attack: f32, release: f32) -> f32 {
        let coeff = if envelope > self.linked_envelope {
            attack
        } else {
            release
        };
        self.linked_envelope =
            sanitize_sample((coeff * self.linked_envelope) + ((1.0 - coeff) * envelope));
        self.linked_envelope
    }
}

impl CassetteState {
    fn reset(&mut self) {
        self.lowpass_state = [0.0; MAX_CHANNELS];
        self.highpass_state = [0.0; MAX_CHANNELS];
        self.highpass_input = [0.0; MAX_CHANNELS];
        self.instability_state = [0.0; MAX_CHANNELS];
        self.bump_state = [0.0; MAX_CHANNELS];
        self.hiss_env = [0.0; MAX_CHANNELS];
        self.wear_state = [0.0; MAX_CHANNELS];
        self.wear_rng = [0x68B5_2F1D, 0x1C3A_77E9];
    }

    fn next_instability(&mut self, channel: usize, drift: f32, stereo_spread: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let amount = 0.20 + stereo_spread.clamp(0.0, 1.0) * 0.80;
        self.instability_state[index] += 0.004 * ((drift * amount) - self.instability_state[index]);
        sanitize_sample(self.instability_state[index])
    }

    /// Smooth, slow level sag emulating tape wear / mild dropout. Only engages
    /// when both `degrade` and `random_drift` allow it, never sags below 0.55,
    /// and wanders at sub-Hz speed so it can never click.
    fn next_wear_gain(
        &mut self,
        channel: usize,
        degrade: f32,
        random_drift: f32,
        sample_rate: f32,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let mut state = self.wear_rng[index];
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.wear_rng[index] = state;
        let target = state as f32 / u32::MAX as f32; // [0, 1]
        let coeff = one_pole_alpha(0.8, sample_rate);
        self.wear_state[index] += coeff * (target - self.wear_state[index]);
        self.wear_state[index] = sanitize_sample(self.wear_state[index].clamp(0.0, 1.0));
        let depth = degrade.clamp(0.0, 1.0) * random_drift.clamp(0.0, 1.0) * 0.30;
        (1.0 - self.wear_state[index] * depth).clamp(0.55, 1.0)
    }

    /// Program-dependent hiss gate: hiss ducks toward a low floor during silence
    /// (so pauses stay clean) and opens up under signal, where it is masked. The
    /// gate only scales the already-capped hiss, so effective hiss never exceeds
    /// the documented noise gain.
    fn hiss_gate(&mut self, channel: usize, program: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let target = program.abs();
        let env = self.hiss_env[index];
        let coeff = if target > env { 0.02 } else { 0.000_8 };
        self.hiss_env[index] = sanitize_sample(env + coeff * (target - env));
        0.4 + 0.6 * smoothstep(self.hiss_env[index] * 9.0)
    }

    fn process_bandwidth(
        &mut self,
        channel: usize,
        input: f32,
        frame: &TextureFrame,
        sample_rate: f32,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let highpass_hz = cassette_highpass_hz(frame.degrade, frame.noise_color);
        let highpass_alpha = (-core::f32::consts::TAU * highpass_hz / sample_rate.max(1.0)).exp();
        let highpassed =
            highpass_alpha * (self.highpass_state[index] + input - self.highpass_input[index]);
        self.highpass_state[index] = sanitize_sample(highpassed);
        self.highpass_input[index] = input;

        let cutoff = cassette_lowpass_cutoff_hz(
            frame.noise_color,
            frame.degrade,
            frame.stereo_spread,
            index,
            sample_rate,
        );
        let lowpass_alpha = one_pole_alpha(cutoff, sample_rate);
        self.lowpass_state[index] += lowpass_alpha * (highpassed - self.lowpass_state[index]);
        self.lowpass_state[index] = sanitize_sample(self.lowpass_state[index]);

        // Subtle tape "head bump": a low-frequency resonance lift derived from
        // the DC-blocked (highpassed) signal, so it warms the low-mids without
        // introducing any DC or subsonic energy.
        let bump_alpha = one_pole_alpha(cassette_head_bump_hz(), sample_rate);
        self.bump_state[index] += bump_alpha * (highpassed - self.bump_state[index]);
        self.bump_state[index] = sanitize_sample(self.bump_state[index]);
        let bump = self.bump_state[index] * cassette_head_bump_gain(frame.degrade);

        sanitize_sample(self.lowpass_state[index] + bump)
    }
}

impl Default for BrokenState {
    fn default() -> Self {
        Self {
            rng_state: [0x3141_5926, 0x2718_2818],
            dropout_gain: [1.0; MAX_CHANNELS],
            dropout_target: [1.0; MAX_CHANNELS],
            dropout_remaining: [0; MAX_CHANNELS],
            hold_counter: [0; MAX_CHANNELS],
            held_sample: [0.0; MAX_CHANNELS],
            post_filter_state: [0.0; MAX_CHANNELS],
            dc_state: [0.0; MAX_CHANNELS],
            sync_counter: [0; MAX_CHANNELS],
        }
    }
}

impl BrokenState {
    fn reset(&mut self) {
        self.dropout_gain = [1.0; MAX_CHANNELS];
        self.dropout_target = [1.0; MAX_CHANNELS];
        self.dropout_remaining = [0; MAX_CHANNELS];
        self.hold_counter = [0; MAX_CHANNELS];
        self.held_sample = [0.0; MAX_CHANNELS];
        self.post_filter_state = [0.0; MAX_CHANNELS];
        self.dc_state = [0.0; MAX_CHANNELS];
        self.sync_counter = [0; MAX_CHANNELS];
    }

    /// Remove any DC offset the bit-crush / sample-and-hold rounding can bias
    /// into the signal, so "degrade" never parks the waveform off-center.
    fn block_dc(&mut self, channel: usize, input: f32, sample_rate: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let alpha = 1.0 - one_pole_alpha(18.0, sample_rate);
        sanitize_sample(dc_blocker_step(&mut self.dc_state[index], input, alpha))
    }

    fn next_dropout_gain(
        &mut self,
        channel: usize,
        degrade: f32,
        random_drift: f32,
        sample_rate: f32,
        sync_period: Option<usize>,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let degrade = degrade.clamp(0.0, 1.0);
        let drift = random_drift.clamp(0.0, 1.0);
        let sample_rate = sample_rate.max(1.0);

        if let Some(period) = sync_period {
            // Tempo-locked gate: drop out for ~40% of each division, on the beat.
            // The same mini-fade smoothing below keeps it click-free (no hard cut).
            let period = period.max(1);
            self.sync_counter[index] += 1;
            if self.dropout_remaining[index] > 0 {
                self.dropout_remaining[index] -= 1;
                if self.dropout_remaining[index] == 0 {
                    self.dropout_target[index] = 1.0;
                }
            }
            if self.sync_counter[index] >= period {
                self.sync_counter[index] = 0;
                self.dropout_remaining[index] = ((period as f32) * 0.4).round().max(1.0) as u32;
                self.dropout_target[index] = 0.08 + (1.0 - degrade) * 0.10;
            }
        } else {
            self.sync_counter[index] = 0;
            if self.dropout_remaining[index] == 0 {
                self.dropout_target[index] = 1.0;
                let events_per_second = broken_dropout_events_per_second(degrade, drift);
                if self.random_unit(index) < events_per_second / sample_rate {
                    let duration_ms = 4.0 + self.random_unit(index) * (10.0 + drift * 28.0);
                    self.dropout_remaining[index] =
                        ((duration_ms * 0.001 * sample_rate).round() as u32).max(1);
                    self.dropout_target[index] =
                        0.08 + self.random_unit(index) * (0.50 - degrade * 0.30);
                }
            } else {
                self.dropout_remaining[index] -= 1;
                if self.dropout_remaining[index] == 0 {
                    self.dropout_target[index] = 1.0;
                }
            }
        }

        let coeff = broken_dropout_smoothing_coeff(self.dropout_target[index], sample_rate);
        let current = self.dropout_gain[index];
        self.dropout_gain[index] =
            sanitize_sample(current + coeff * (self.dropout_target[index] - current));
        self.dropout_gain[index].clamp(0.0, 1.0)
    }

    fn next_held_sample(
        &mut self,
        channel: usize,
        input: f32,
        artifact_noise: f32,
        degrade: f32,
        random_drift: f32,
        stereo_spread: f32,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        if self.hold_counter[index] == 0 {
            let period = broken_hold_period(degrade, random_drift, stereo_spread, index);
            self.hold_counter[index] = period.saturating_sub(1);
            self.held_sample[index] = sanitize_sample(input + artifact_noise * degrade * 0.010);
        } else {
            self.hold_counter[index] -= 1;
        }

        self.held_sample[index]
    }

    fn post_filter(
        &mut self,
        channel: usize,
        input: f32,
        noise_color: f32,
        degrade: f32,
        sample_rate: f32,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let cutoff = broken_post_filter_cutoff_hz(noise_color, degrade, sample_rate);
        let alpha = one_pole_alpha(cutoff, sample_rate);
        self.post_filter_state[index] += alpha * (input - self.post_filter_state[index]);
        self.post_filter_state[index] = sanitize_sample(self.post_filter_state[index]);
        self.post_filter_state[index]
    }

    fn random_unit(&mut self, channel: usize) -> f32 {
        self.next_u32(channel) as f32 / u32::MAX as f32
    }

    fn next_u32(&mut self, channel: usize) -> u32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let mut state = self.rng_state[index];
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.rng_state[index] = state;
        state
    }
}

impl InterferenceState {
    fn reset(&mut self) {
        self.carrier_phase = [0.0; MAX_CHANNELS];
        self.parasite_phase = [0.37, 0.61];
        self.amplitude_state = [1.0; MAX_CHANNELS];
        self.noise_band_hi = [0.0; MAX_CHANNELS];
        self.noise_band_lo = [0.0; MAX_CHANNELS];
        self.damp_state = [0.0; MAX_CHANNELS];
        self.sync_counter = [0; MAX_CHANNELS];
    }

    /// Tempo-locked pulse envelope (0..1) that swells once per division — a
    /// raised cosine, so it's smooth (no clicks). Drives a rhythmic radio-pulse.
    fn next_sync_pulse(&mut self, channel: usize, period: usize) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let period = period.max(1);
        self.sync_counter[index] += 1;
        if self.sync_counter[index] >= period {
            self.sync_counter[index] = 0;
        }
        let phase = self.sync_counter[index] as f32 / period as f32;
        0.5 - 0.5 * (phase * core::f32::consts::TAU).cos()
    }

    /// Band-limit the electric noise around the interference frequency (a
    /// difference-of-low-pass band-pass), so it reads as tonal radio static
    /// instead of broadband white hiss.
    fn bandpass_noise(
        &mut self,
        channel: usize,
        input: f32,
        carrier_hz: f32,
        sample_rate: f32,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let nyquist_guard = sample_rate.max(1.0) * 0.45;
        let hi = (carrier_hz * 2.0).clamp(120.0, nyquist_guard);
        let lo = (carrier_hz * 0.5).clamp(40.0, nyquist_guard);
        let a_hi = one_pole_alpha(hi, sample_rate);
        let a_lo = one_pole_alpha(lo, sample_rate);
        self.noise_band_hi[index] += a_hi * (input - self.noise_band_hi[index]);
        self.noise_band_lo[index] += a_lo * (input - self.noise_band_lo[index]);
        self.noise_band_hi[index] = sanitize_sample(self.noise_band_hi[index]);
        self.noise_band_lo[index] = sanitize_sample(self.noise_band_lo[index]);
        sanitize_sample(self.noise_band_hi[index] - self.noise_band_lo[index])
    }

    /// Tone / damping stage: a one-pole low-pass on the wet interference whose
    /// cutoff opens with `noise_color` and closes with `degrade`. This is the
    /// harshness control — it tames the bright ring-mod / static edge.
    fn damp(
        &mut self,
        channel: usize,
        input: f32,
        noise_color: f32,
        degrade: f32,
        sample_rate: f32,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let cutoff = interference_damping_hz(noise_color, degrade, sample_rate);
        let alpha = one_pole_alpha(cutoff, sample_rate);
        self.damp_state[index] += alpha * (input - self.damp_state[index]);
        self.damp_state[index] = sanitize_sample(self.damp_state[index]);
        self.damp_state[index]
    }

    fn next_carrier(&mut self, channel: usize, frequency_hz: f32, sample_rate: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        self.carrier_phase[index] =
            advance_phase(self.carrier_phase[index], frequency_hz, sample_rate);
        sine_lfo(self.carrier_phase[index])
    }

    fn next_parasite(&mut self, channel: usize, frequency_hz: f32, sample_rate: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        self.parasite_phase[index] =
            advance_phase(self.parasite_phase[index], frequency_hz, sample_rate);
        sine_lfo(self.parasite_phase[index])
    }

    fn smooth_amplitude(&mut self, channel: usize, target: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let target = target.clamp(0.45, 1.55);
        self.amplitude_state[index] += 0.0025 * (target - self.amplitude_state[index]);
        sanitize_sample(self.amplitude_state[index]).clamp(0.45, 1.55)
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
    }

    fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        self.write_positions = [0; MAX_CHANNELS];
    }

    fn process(&mut self, channel: usize, input: f32, delay_samples: f32) -> f32 {
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
        buffer[write_pos] = sanitize_sample(input);
        self.write_positions[channel] = (write_pos + 1) % len;

        sanitize_sample(delayed)
    }
}

impl DriftGenerator {
    fn new(seed: u32) -> Self {
        Self {
            rng_state: seed,
            smoother: LinearSmoother::new(320.0, 0.0),
            sample_rate: 44_100.0,
        }
    }

    fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.smoother.prepare(self.sample_rate);
    }

    fn reset(&mut self) {
        self.smoother.reset(0.0);
    }

    fn next_value(&mut self, amount: f32) -> f32 {
        let current = self.smoother.next_value();
        let events_per_second = 24.0 + (amount.clamp(0.0, 1.0) * 48.0);
        let target_threshold = (events_per_second / self.sample_rate).clamp(0.0, 0.02);
        if self.random_unit() < target_threshold {
            let target = self.random_bipolar() * amount;
            self.smoother.set_target(target);
        }

        sanitize_sample(current)
    }

    fn random_unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    fn random_bipolar(&mut self) -> f32 {
        (self.random_unit() * 2.0) - 1.0
    }

    fn next_u32(&mut self) -> u32 {
        let mut state = self.rng_state;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.rng_state = state;
        state
    }
}

impl NoiseGenerator {
    fn new(seed: u32) -> Self {
        Self {
            rng_state: seed,
            low_state: 0.0,
            low2_state: 0.0,
        }
    }

    fn reset(&mut self) {
        self.low_state = 0.0;
        self.low2_state = 0.0;
    }

    fn next_colored(&mut self, color: f32) -> f32 {
        let white = self.random_bipolar();
        let color = color.clamp(0.0, 1.0);
        let alpha = 0.03 + (color * color * 0.55);
        self.low_state += alpha * (white - self.low_state);
        self.low_state = sanitize_sample(self.low_state);
        self.low2_state += alpha * (self.low_state - self.low2_state);
        self.low2_state = sanitize_sample(self.low2_state);

        let high = white - self.low2_state;
        let dark_weight = 1.0 - color;
        sanitize_sample((self.low2_state * dark_weight) + (high * color * 0.7))
    }

    fn random_bipolar(&mut self) -> f32 {
        let mut state = self.rng_state;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.rng_state = state;

        (state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[inline]
fn cassette_lowpass_cutoff_hz(
    noise_color: f32,
    degrade: f32,
    stereo_spread: f32,
    channel: usize,
    sample_rate: f32,
) -> f32 {
    let color = noise_color.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    let bright_hz = 6_000.0 + color * 10_000.0;
    let worn_loss = 1.0 - degrade * 0.62;
    let spread = stereo_spread.clamp(0.0, 1.0) * 0.07;
    let channel_offset = if channel == 0 { -spread } else { spread };
    let cutoff = bright_hz * worn_loss * (1.0 + channel_offset);

    cutoff.clamp(1_800.0, 16_000.0_f32.min(sample_rate.max(1.0) * 0.45))
}

#[inline]
fn cassette_head_bump_hz() -> f32 {
    95.0
}

#[inline]
fn cassette_head_bump_gain(degrade: f32) -> f32 {
    // A gentle low-mid lift that grows as the tape wears; capped so it stays a
    // warming "bump", never a boomy resonance.
    (0.06 + degrade.clamp(0.0, 1.0) * 0.16).clamp(0.0, 0.22)
}

#[inline]
fn cassette_highpass_hz(degrade: f32, noise_color: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    let color = noise_color.clamp(0.0, 1.0);
    (24.0 + degrade * 90.0 + color * 24.0).clamp(20.0, 150.0)
}

#[inline]
fn cassette_noise_gain(noise_amount: f32, degrade: f32) -> f32 {
    let noise_amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (noise_amount.powf(1.65) * (0.010 + degrade * 0.020)).clamp(0.0, 0.030)
}

#[inline]
fn apply_cassette_degradation(sample: f32, degrade: f32, hiss: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    if degrade <= 0.000_001 {
        return sanitize_sample(sample);
    }

    let drive = 1.0 + degrade * 1.35;
    let bias = hiss * degrade * 0.18;
    let saturated = soft_saturate(sample + bias, drive);
    let blend = degrade * 0.24;

    sanitize_sample((sample * (1.0 - blend)) + (saturated * blend))
}

#[inline]
fn cassette_level_compensation(noise_amount: f32, degrade: f32) -> f32 {
    let noise_amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (1.0 - noise_amount * 0.035 - degrade * 0.055).clamp(0.90, 1.0)
}

#[inline]
fn broken_rate_reduce_mix(degrade: f32) -> f32 {
    degrade.clamp(0.0, 1.0).powf(1.25) * 0.72
}

#[inline]
fn broken_hold_period(degrade: f32, random_drift: f32, stereo_spread: f32, channel: usize) -> u32 {
    let degrade = degrade.clamp(0.0, 1.0);
    let drift = random_drift.clamp(0.0, 1.0);
    let spread = stereo_spread.clamp(0.0, 1.0);
    let max_period = 2.0 + degrade * degrade * 20.0 + drift * 10.0;
    let offset = if channel == 0 { -0.35 } else { 0.65 } * spread * drift;

    ((1.0 + (max_period * (0.35 + degrade * 0.65) * (1.0 + offset))).round() as u32).clamp(1, 48)
}

#[inline]
fn broken_bitcrush(sample: f32, degrade: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    if degrade <= 0.000_001 {
        return sanitize_sample(sample);
    }

    let bits = (16.0 - degrade * 8.5).round().clamp(7.0, 16.0);
    let levels = 2.0_f32.powf(bits - 1.0);
    let crushed = (sample * levels).round() / levels;
    linear_crossfade(sample, crushed, degrade * 0.78)
}

#[inline]
fn broken_artifact_gain(noise_amount: f32, degrade: f32) -> f32 {
    let noise_amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (noise_amount.powf(1.45) * (0.006 + degrade * 0.024)).clamp(0.0, 0.030)
}

#[inline]
fn broken_dropout_events_per_second(degrade: f32, random_drift: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    let drift = random_drift.clamp(0.0, 1.0);
    drift * drift * (0.30 + degrade * 11.0)
}

#[inline]
fn broken_dropout_smoothing_coeff(target: f32, sample_rate: f32) -> f32 {
    let fade_ms = if target < 1.0 { 1.6 } else { 3.8 };
    (1.0 - (-1.0 / (fade_ms * 0.001 * sample_rate.max(1.0))).exp()).clamp(0.0, 1.0)
}

#[inline]
fn broken_post_filter_cutoff_hz(noise_color: f32, degrade: f32, sample_rate: f32) -> f32 {
    let color = noise_color.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    let cutoff = 13_500.0 - degrade * 5_200.0 + color * 3_500.0;
    cutoff.clamp(4_000.0, 17_000.0_f32.min(sample_rate.max(1.0) * 0.45))
}

#[inline]
fn broken_level_compensation(noise_amount: f32, degrade: f32) -> f32 {
    let noise_amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (1.0 - noise_amount * 0.025 - degrade * 0.060).clamp(0.88, 1.0)
}

#[inline]
fn interference_frequency_hz(
    noise_color: f32,
    stereo_spread: f32,
    channel: usize,
    sample_rate: f32,
) -> f32 {
    let color = noise_color.clamp(0.0, 1.0);
    let min_hz = 35.0_f32;
    let max_hz = 6_800.0_f32.min(sample_rate.max(1.0) * 0.32);
    let base = min_hz * (max_hz / min_hz).powf(color);
    let spread = stereo_spread.clamp(0.0, 1.0) * 0.13;
    let offset = if channel == 0 { -spread } else { spread };

    (base * (1.0 + offset)).clamp(min_hz, max_hz)
}

#[inline]
fn interference_parasite_frequency_hz(
    carrier_frequency: f32,
    noise_color: f32,
    channel: usize,
    sample_rate: f32,
) -> f32 {
    let color = noise_color.clamp(0.0, 1.0);
    let ratio = if channel == 0 { 1.997 } else { 2.013 } + color * 0.71;
    let fold_limit = 8_500.0_f32.min(sample_rate.max(1.0) * 0.38);
    let mut frequency = carrier_frequency * ratio + 17.0 + color * 43.0;
    while frequency > fold_limit {
        frequency *= 0.5;
    }

    frequency.clamp(45.0, fold_limit)
}

#[inline]
fn interference_am_depth(noise_amount: f32, degrade: f32) -> f32 {
    let amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (amount * (0.08 + degrade * 0.34)).clamp(0.0, 0.42)
}

#[inline]
fn interference_ring_depth(noise_amount: f32, degrade: f32) -> f32 {
    let amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (amount.powf(1.35) * (0.18 + degrade * 0.45)).clamp(0.0, 0.63)
}

#[inline]
fn interference_tone_gain(noise_amount: f32, degrade: f32) -> f32 {
    let amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (amount.powf(1.55) * (0.005 + degrade * 0.018)).clamp(0.0, 0.023)
}

#[inline]
fn interference_noise_gain(noise_amount: f32, degrade: f32) -> f32 {
    let amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (amount.powf(1.35) * (0.006 + degrade * 0.020)).clamp(0.0, 0.026)
}

#[inline]
fn interference_soft_clip(sample: f32, degrade: f32, noise_amount: f32) -> f32 {
    let drive = 1.0 + degrade.clamp(0.0, 1.0) * 1.25 + noise_amount.clamp(0.0, 1.0) * 0.45;
    soft_saturate(sample, drive)
}

#[inline]
fn interference_level_compensation(noise_amount: f32, degrade: f32) -> f32 {
    let amount = noise_amount.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    (1.0 - amount * 0.025 - degrade * 0.035).clamp(0.90, 1.0)
}

#[inline]
fn interference_damping_hz(noise_color: f32, degrade: f32, sample_rate: f32) -> f32 {
    let color = noise_color.clamp(0.0, 1.0);
    let degrade = degrade.clamp(0.0, 1.0);
    // Brighter with color, darker (more damped) as degrade climbs — this is what
    // keeps an aggressive setting from turning into a harsh fizz.
    let bright = 3_000.0 + color * 9_000.0;
    let damped = bright * (1.0 - degrade * 0.55);
    damped.clamp(1_800.0, 16_000.0_f32.min(sample_rate.max(1.0) * 0.45))
}

#[inline]
fn squash_attack_coeff(degrade: f32, sample_rate: f32) -> f32 {
    let attack_ms = (8.0 - degrade.clamp(0.0, 1.0) * 7.2).clamp(0.6, 8.0);
    one_pole_coeff(attack_ms, sample_rate)
}

#[inline]
fn squash_release_coeff(degrade: f32, noise_color: f32, sample_rate: f32) -> f32 {
    let release_ms =
        130.0 - degrade.clamp(0.0, 1.0) * 75.0 + (1.0 - noise_color.clamp(0.0, 1.0)) * 60.0;
    one_pole_coeff(release_ms.clamp(45.0, 220.0), sample_rate)
}

#[inline]
fn one_pole_coeff(time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (time_ms.max(0.001) * 0.001 * sample_rate.max(1.0))).exp()
}

#[inline]
fn squash_compression_gain(envelope: f32, degrade: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    if degrade <= 0.000_001 {
        return 1.0;
    }

    let threshold_db = -4.0 - degrade * 30.0;
    let ratio = 1.0 + degrade * degrade * 17.0;
    let knee_db = 6.0;
    let level_db = amplitude_to_db(envelope);
    let over = level_db - threshold_db;
    let slope = 1.0 / ratio - 1.0; // gain reduction (dB) per dB over threshold

    // Soft knee: no reduction below the knee, the full ratio above it, and a
    // smooth quadratic blend across the ±3 dB knee so compression engages
    // gradually instead of snapping on — that's what keeps it from pumping.
    let gain_db = if over <= -knee_db * 0.5 {
        0.0
    } else if over >= knee_db * 0.5 {
        slope * over
    } else {
        let knee_pos = over + knee_db * 0.5;
        slope * knee_pos * knee_pos / (2.0 * knee_db)
    };
    db_to_gain(gain_db.clamp(-36.0, 0.0))
}

#[inline]
fn amplitude_to_db(value: f32) -> f32 {
    20.0 * value.max(0.000_001).log10()
}

#[inline]
fn squash_makeup_gain(degrade: f32) -> f32 {
    db_to_gain(degrade.clamp(0.0, 1.0) * 5.5).clamp(1.0, 1.90)
}

#[inline]
fn squash_soft_clip(sample: f32, degrade: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    let drive = 1.0 + degrade * 1.8;
    let clipped = soft_saturate(sample, drive);
    let blend = degrade * 0.55;

    sanitize_sample((sample * (1.0 - blend)) + (clipped * blend))
}

#[inline]
fn apply_filter_drive(sample: f32, degrade: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    if degrade <= 0.000_001 {
        return sanitize_sample(sample);
    }

    let drive = 1.0 + degrade * 2.2;
    let driven = soft_saturate(sample, drive);
    sanitize_sample((sample * (1.0 - degrade * 0.28)) + (driven * degrade * 0.28))
}

#[inline]
fn texture_filter_cutoff_hz(
    noise_color: f32,
    stereo_spread: f32,
    channel: usize,
    sample_rate: f32,
) -> f32 {
    let color = noise_color.clamp(0.0, 1.0);
    let min_hz = 80.0_f32;
    let max_hz = 16_000.0_f32.min(sample_rate.max(1.0) * 0.45);
    let base = min_hz * (max_hz / min_hz).powf(color);
    let spread = stereo_spread.clamp(0.0, 1.0) * 0.10;
    let offset = if channel == 0 { -spread } else { spread };

    (base * (1.0 + offset)).clamp(min_hz, max_hz)
}

#[inline]
fn texture_filter_resonance(degrade: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    (0.05 + degrade * degrade * 0.58).clamp(0.05, 0.63)
}

#[inline]
fn texture_filter_blend(dry: f32, low: f32, high: f32, noise_color: f32) -> f32 {
    let color = noise_color.clamp(0.0, 1.0);
    if color < 0.5 {
        let amount = smoothstep((0.5 - color) * 2.0);
        linear_crossfade(dry, low, amount)
    } else {
        let amount = smoothstep((color - 0.5) * 2.0);
        let tilt = sanitize_sample(dry + high * amount * 0.22);
        linear_crossfade(tilt, high, amount * 0.78)
    }
}

#[inline]
fn texture_filter_level_compensation(degrade: f32) -> f32 {
    (1.0 - degrade.clamp(0.0, 1.0) * 0.08).clamp(0.90, 1.0)
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
fn advance_phase(phase: f32, rate_hz: f32, sample_rate: f32) -> f32 {
    let mut phase = phase + (rate_hz / sample_rate.max(1.0));
    if phase >= 1.0 {
        phase -= phase.floor();
    }
    phase
}

#[inline]
fn sine_lfo(phase: f32) -> f32 {
    (phase * core::f32::consts::TAU).sin()
}

#[cfg(test)]
mod tests {
    use super::{
        advance_phase, broken_artifact_gain, broken_bitcrush, broken_dropout_smoothing_coeff,
        broken_hold_period, broken_level_compensation, broken_rate_reduce_mix,
        cassette_head_bump_gain, cassette_level_compensation, cassette_lowpass_cutoff_hz,
        cassette_noise_gain, interference_am_depth, interference_damping_hz,
        interference_frequency_hz, interference_level_compensation, interference_noise_gain,
        interference_parasite_frequency_hz, interference_ring_depth, interference_tone_gain,
        read_interpolated, squash_compression_gain, squash_makeup_gain, texture_filter_cutoff_hz,
        texture_filter_resonance, BrokenState, DriftGenerator, NoiseGenerator, StereoDelay,
        Texture, TextureFrame, TextureMode,
    };
    use crate::dsp::transport::{NoteDivision, TransportFrame};
    use crate::params::TextureParams;
    use nih_plug::prelude::{BoolParam, EnumParam, FloatParam, FloatRange};

    #[test]
    fn texture_sync_interference_pulse_is_click_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let transport = TransportFrame {
            bpm: 120.0,
            ..TransportFrame::default()
        };
        let mut params = TextureParams::default();
        params.mode = EnumParam::new("m", TextureMode::Interference);
        params.bypass = BoolParam::new("b", false);
        params.noise_amount = FloatParam::new("n", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 });
        params.sync_enabled = BoolParam::new("s", true);
        params.sync_division = EnumParam::new("d", NoteDivision::Eighth);
        params.reset_smoothers();

        let mut prev = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut phase = 0.0_f32;
        for index in 0..48_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            let frame = texture.next_frame(&params, &transport);
            let input = (phase * core::f32::consts::TAU).sin() * 0.4;
            let out = texture.process_sample_for_channel(0, input, &frame);
            assert!(
                out.is_finite() && out.abs() <= 8.0,
                "interference sync {out}"
            );
            if index > 1_000 {
                max_step = max_step.max((out - prev).abs());
            }
            prev = out;
        }
        // The pulse is a raised cosine, so the swell never clicks.
        assert!(
            max_step < 0.6,
            "interference sync produced a click ({max_step})"
        );
    }

    #[test]
    fn texture_sync_broken_dropout_is_periodic_and_click_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let transport = TransportFrame {
            bpm: 120.0,
            ..TransportFrame::default()
        };
        let mut params = TextureParams::default();
        params.mode = EnumParam::new("m", TextureMode::Broken);
        params.bypass = BoolParam::new("b", false);
        params.sync_enabled = BoolParam::new("s", true);
        params.sync_division = EnumParam::new("d", NoteDivision::Sixteenth);
        params.reset_smoothers();

        let mut prev = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut phase = 0.0_f32;
        for index in 0..48_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            let frame = texture.next_frame(&params, &transport);
            let input = (phase * core::f32::consts::TAU).sin() * 0.4;
            let out = texture.process_sample_for_channel(0, input, &frame);
            assert!(
                out.is_finite() && out.abs() <= 8.0,
                "broken sync output {out}"
            );
            if index > 1_000 {
                max_step = max_step.max((out - prev).abs());
            }
            prev = out;
        }
        // The tempo gate uses the smoothed mini-fades, so no hard click.
        assert!(
            max_step < 0.5,
            "broken sync produced a click (step {max_step})"
        );
    }

    #[test]
    fn texture_sync_cassette_wow_stays_finite_across_divisions() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let transport = TransportFrame {
            bpm: 120.0,
            ..TransportFrame::default()
        };
        for division in [
            NoteDivision::Bar1,
            NoteDivision::Half,
            NoteDivision::Quarter,
            NoteDivision::DottedQuarter,
        ] {
            let mut params = TextureParams::default();
            params.mode = EnumParam::new("m", TextureMode::Cassette);
            params.bypass = BoolParam::new("b", false);
            params.sync_enabled = BoolParam::new("s", true);
            params.sync_division = EnumParam::new("d", division);
            params.reset_smoothers();
            let mut phase = 0.0_f32;
            for _ in 0..6_000 {
                phase += 220.0 / 48_000.0;
                if phase >= 1.0 {
                    phase -= 1.0;
                }
                let frame = texture.next_frame(&params, &transport);
                let input = (phase * core::f32::consts::TAU).sin() * 0.3;
                let out = texture.process_sample_for_channel(0, input, &frame);
                assert!(
                    out.is_finite() && out.abs() <= 8.0,
                    "cassette sync {division:?} produced {out}"
                );
            }
        }
    }

    fn test_frame(mode: TextureMode) -> TextureFrame {
        TextureFrame {
            mode,
            wow_depth: 1.0,
            flutter_depth: 1.0,
            random_drift: 1.0,
            noise_amount: 0.0,
            noise_color: 0.5,
            degrade: 0.0,
            stereo_spread: 1.0,
            mix: 1.0,
            active_mix: 1.0,
            wow_phase: 0.08,
            flutter_phase: 0.54,
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
    fn phase_advances_independent_of_sample_rate() {
        let phase_a = advance_phase(0.0, 1.0, 48_000.0);
        let phase_b = advance_phase(0.0, 1.0, 96_000.0);

        assert!(phase_a > phase_b);
        assert!((phase_a - (1.0 / 48_000.0)).abs() < 0.000_001);
    }

    #[test]
    fn drift_is_smoothed() {
        let mut drift = DriftGenerator::new(0x1234_5678);
        drift.prepare(1_000.0);
        let first = drift.next_value(1.0);
        let second = drift.next_value(1.0);

        assert!((second - first).abs() < 0.1);
    }

    #[test]
    fn delay_output_stays_finite() {
        let mut delay = StereoDelay::default();
        delay.prepare(48_000.0);
        let mut sample = 1.0;

        for _ in 0..2_000 {
            sample = delay.process(0, sample, 900.0);
            assert!(sample.is_finite());
        }
    }

    #[test]
    fn noise_color_output_stays_finite() {
        let mut noise = NoiseGenerator::new(0x1234_5678);

        for index in 0..2_000 {
            let color = if index % 2 == 0 { 0.0 } else { 1.0 };
            let sample = noise.next_colored(color);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    fn cassette_frame(
        wow_depth: f32,
        flutter_depth: f32,
        random_drift: f32,
        noise_amount: f32,
        noise_color: f32,
        degrade: f32,
        stereo_spread: f32,
    ) -> TextureFrame {
        let mut frame = test_frame(TextureMode::Cassette);
        frame.wow_depth = wow_depth;
        frame.flutter_depth = flutter_depth;
        frame.random_drift = random_drift;
        frame.noise_amount = noise_amount;
        frame.noise_color = noise_color;
        frame.degrade = degrade;
        frame.stereo_spread = stereo_spread;
        frame
    }

    #[test]
    fn cassette_silence_has_controlled_hiss() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = cassette_frame(0.0, 0.0, 0.0, 1.0, 0.75, 0.65, 1.0);
        let mut peak = 0.0_f32;
        let mut sum = 0.0_f32;

        for _ in 0..8_000 {
            let output = texture.process_sample_for_channel(0, 0.0, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
            sum += output * output;
        }

        let rms = (sum / 8_000.0).sqrt();
        assert!(rms > 0.000_1, "cassette hiss should be audible, rms={rms}");
        assert!(
            peak < 0.08,
            "cassette hiss should stay controlled, peak={peak}"
        );
    }

    #[test]
    fn cassette_wow_flutter_modulates_sine() {
        let mut stable = Texture::default();
        let mut moving = Texture::default();
        stable.prepare(48_000.0);
        moving.prepare(48_000.0);
        let stable_frame = cassette_frame(0.0, 0.0, 0.0, 0.0, 0.55, 0.1, 0.0);
        let mut moving_frame = cassette_frame(1.0, 1.0, 0.5, 0.0, 0.55, 0.1, 0.8);

        let mut phase: f32 = 0.0;
        let mut diff_sum = 0.0_f32;
        let mut count = 0.0_f32;
        for index in 0..24_000 {
            phase += 440.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            moving_frame.wow_phase = advance_phase(moving_frame.wow_phase, 1.2, 48_000.0);
            moving_frame.flutter_phase = advance_phase(moving_frame.flutter_phase, 12.0, 48_000.0);
            let input = (phase * core::f32::consts::TAU).sin() * 0.35;
            let dryish = stable.process_sample_for_channel(0, input, &stable_frame);
            let modulated = moving.process_sample_for_channel(0, input, &moving_frame);
            assert!(dryish.is_finite() && modulated.is_finite());
            assert!(modulated.abs() <= 8.0);

            if index > 4_000 {
                let diff = modulated - dryish;
                diff_sum += diff * diff;
                count += 1.0;
            }
        }

        let diff_rms = (diff_sum / count).sqrt();
        assert!(
            diff_rms > 0.002,
            "cassette wow/flutter should audibly modulate sine, diff={diff_rms}"
        );
    }

    #[test]
    fn cassette_stereo_spread_creates_lr_instability() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let mut frame = cassette_frame(0.8, 0.8, 0.7, 0.2, 0.6, 0.4, 1.0);
        let mut phase: f32 = 0.0;
        let mut diff_sum = 0.0_f32;
        let mut count = 0.0_f32;

        for index in 0..16_000 {
            phase += 330.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            frame.wow_phase = advance_phase(frame.wow_phase, 0.9, 48_000.0);
            frame.flutter_phase = advance_phase(frame.flutter_phase, 9.0, 48_000.0);
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let left = texture.process_sample_for_channel(0, input, &frame);
            let right = texture.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());

            if index > 3_000 {
                let diff = left - right;
                diff_sum += diff * diff;
                count += 1.0;
            }
        }

        let diff_rms = (diff_sum / count).sqrt();
        assert!(
            diff_rms > 0.000_5,
            "cassette stereo spread should create L/R instability, diff={diff_rms}"
        );
    }

    #[test]
    fn cassette_helpers_are_bounded() {
        assert!(cassette_noise_gain(1.0, 1.0) <= 0.030);
        assert!(cassette_noise_gain(0.0, 1.0) == 0.0);
        assert!(cassette_level_compensation(1.0, 1.0) >= 0.90);
        assert!(cassette_lowpass_cutoff_hz(0.0, 1.0, 1.0, 0, 48_000.0) >= 1_800.0);
        assert!(cassette_lowpass_cutoff_hz(1.0, 0.0, 1.0, 1, 48_000.0) <= 16_000.0);
        // Head bump is always a subtle lift that grows with wear and stays capped.
        assert!(cassette_head_bump_gain(0.0) >= 0.0);
        assert!(cassette_head_bump_gain(1.0) > cassette_head_bump_gain(0.0));
        assert!(cassette_head_bump_gain(1.0) <= 0.22);
    }

    fn cassette_steady_output_rms(freq_hz: f32, degrade: f32, noise_color: f32) -> f32 {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // No hiss, no wow/flutter, no drift: isolate the tone-shaping path.
        let frame = cassette_frame(0.0, 0.0, 0.0, 0.0, noise_color, degrade, 0.0);
        let mut phase = 0.0_f32;
        let mut sum = 0.0_f32;
        let mut count = 0.0_f32;
        for index in 0..20_000 {
            phase += freq_hz / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 12_000 {
                sum += output * output;
                count += 1.0;
            }
        }
        (sum / count).sqrt()
    }

    #[test]
    fn cassette_tone_rolloff_attenuates_highs_more_than_lows() {
        // A worn cassette should pass low-mids while clearly rolling off highs.
        let low = cassette_steady_output_rms(300.0, 0.6, 0.25);
        let high = cassette_steady_output_rms(8_000.0, 0.6, 0.25);
        assert!(
            low > high * 1.5,
            "cassette should roll off highs musically (low={low}, high={high})"
        );
    }

    #[test]
    fn cassette_low_noise_silence_stays_subtle() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // Low noise amount, fully worn, on silence: must not become absurd.
        let frame = cassette_frame(0.0, 0.0, 0.0, 0.15, 0.6, 1.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..8_000 {
            let output = texture.process_sample_for_channel(0, 0.0, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 0.02,
            "cassette low-noise silence should stay subtle, peak={peak}"
        );
    }

    #[test]
    fn cassette_max_drive_wear_is_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // Everything maxed: drive, wear (degrade+drift), hiss, modulation.
        let mut frame = cassette_frame(1.0, 1.0, 1.0, 1.0, 0.9, 1.0, 1.0);
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..32_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            frame.wow_phase = advance_phase(frame.wow_phase, 1.4, 48_000.0);
            frame.flutter_phase = advance_phase(frame.flutter_phase, 11.0, 48_000.0);
            let transient = if index % 4_000 == 0 { 0.9 } else { 0.0 };
            let input = (phase * core::f32::consts::TAU).sin() * 0.7 + transient;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "cassette max: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 3.0,
            "cassette at max drive/wear should stay safe, peak={peak}"
        );
    }

    #[test]
    fn cassette_wow_flutter_has_no_clicks() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let mut frame = cassette_frame(1.0, 1.0, 0.6, 0.0, 0.5, 0.5, 0.6);
        let mut phase = 0.0_f32;
        let mut prev = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..40_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            frame.wow_phase = advance_phase(frame.wow_phase, 1.5, 48_000.0);
            frame.flutter_phase = advance_phase(frame.flutter_phase, 12.0, 48_000.0);
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let output = texture.process_sample_for_channel(0, input, &frame);
            if index > 2_000 {
                max_step = max_step.max((output - prev).abs());
            }
            prev = output;
        }
        // A click would show up as a sample-to-sample jump far larger than the
        // input slew; modulation must stay smooth (interpolated delay).
        assert!(
            max_step < 0.2,
            "cassette wow/flutter produced a click (max step={max_step})"
        );
    }

    #[test]
    fn cassette_mono_sum_preserves_signal() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // Strong stereo instability, but a mono fold-down must not cancel away.
        let mut frame = cassette_frame(0.5, 0.5, 0.5, 0.0, 0.5, 0.4, 1.0);
        let mut phase = 0.0_f32;
        let mut mono_sum = 0.0_f32;
        let mut count = 0.0_f32;
        for index in 0..16_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            frame.wow_phase = advance_phase(frame.wow_phase, 1.0, 48_000.0);
            frame.flutter_phase = advance_phase(frame.flutter_phase, 9.0, 48_000.0);
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let left = texture.process_sample_for_channel(0, input, &frame);
            let right = texture.process_sample_for_channel(1, input, &frame);
            let mono = 0.5 * (left + right);
            if index > 4_000 {
                mono_sum += mono * mono;
                count += 1.0;
            }
        }
        let mono_rms = (mono_sum / count).sqrt();
        assert!(
            mono_rms > 0.02,
            "cassette must stay mono-compatible (mono rms={mono_rms})"
        );
    }

    fn broken_frame(
        random_drift: f32,
        noise_amount: f32,
        noise_color: f32,
        degrade: f32,
        stereo_spread: f32,
    ) -> TextureFrame {
        let mut frame = test_frame(TextureMode::Broken);
        frame.random_drift = random_drift;
        frame.noise_amount = noise_amount;
        frame.noise_color = noise_color;
        frame.degrade = degrade;
        frame.stereo_spread = stereo_spread;
        frame
    }

    #[test]
    fn broken_silence_has_controlled_artifacts() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = broken_frame(0.9, 1.0, 0.65, 0.85, 1.0);
        let mut peak = 0.0_f32;
        let mut sum = 0.0_f32;

        for _ in 0..12_000 {
            let output = texture.process_sample_for_channel(0, 0.0, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
            sum += output * output;
        }

        let rms = (sum / 12_000.0).sqrt();
        assert!(
            rms > 0.000_05,
            "broken artifacts should be audible, rms={rms}"
        );
        assert!(peak < 0.08, "broken silence artifacts too hot, peak={peak}");
    }

    #[test]
    fn broken_sine_is_degraded_without_gain_blowup() {
        let mut clean = Texture::default();
        let mut broken = Texture::default();
        clean.prepare(48_000.0);
        broken.prepare(48_000.0);
        let clean_frame = broken_frame(0.0, 0.0, 0.5, 0.0, 0.0);
        let broken_frame = broken_frame(0.75, 0.35, 0.55, 0.75, 0.8);
        let mut phase: f32 = 0.0;
        let mut diff_sum = 0.0_f32;
        let mut count = 0.0_f32;
        let mut peak = 0.0_f32;

        for index in 0..18_000 {
            phase += 440.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let input = (phase * core::f32::consts::TAU).sin() * 0.32;
            let dryish = clean.process_sample_for_channel(0, input, &clean_frame);
            let output = broken.process_sample_for_channel(0, input, &broken_frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());

            if index > 2_000 {
                let diff = output - dryish;
                diff_sum += diff * diff;
                count += 1.0;
            }
        }

        let diff_rms = (diff_sum / count).sqrt();
        assert!(
            diff_rms > 0.001,
            "broken should alter sine, diff={diff_rms}"
        );
        assert!(
            peak < 0.70,
            "broken sine peak should stay controlled, peak={peak}"
        );
    }

    #[test]
    fn broken_drums_like_impulse_stays_click_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = broken_frame(1.0, 0.45, 0.8, 1.0, 1.0);
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut peak = 0.0_f32;

        for index in 0..16_000 {
            let input = if index % 1_200 == 0 {
                1.0
            } else if index % 1_200 < 80 {
                let decay = 1.0 - (index % 1_200) as f32 / 80.0;
                decay * 0.35
            } else {
                0.0
            };
            let output = texture.process_sample_for_channel(index % 2, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
            max_step = max_step.max((output - previous).abs());
            previous = output;
        }

        assert!(peak < 1.25, "broken impulse peak too high, peak={peak}");
        assert!(
            max_step < 1.05,
            "broken dropout/click steps should stay controlled, step={max_step}"
        );
    }

    #[test]
    fn broken_dropout_gain_uses_mini_fades() {
        let mut state = BrokenState::default();
        state.dropout_gain[0] = 1.0;
        state.dropout_target[0] = 0.08;
        state.dropout_remaining[0] = 128;
        let mut previous = 1.0_f32;
        let mut max_step = 0.0_f32;

        for _ in 0..128 {
            let gain = state.next_dropout_gain(0, 1.0, 1.0, 48_000.0, None);
            max_step = max_step.max((gain - previous).abs());
            previous = gain;
        }

        assert!(
            previous < 0.85,
            "dropout fade should move down, gain={previous}"
        );
        assert!(
            max_step <= broken_dropout_smoothing_coeff(0.08, 48_000.0),
            "dropout gain should be smoothed, step={max_step}"
        );
    }

    #[test]
    fn broken_helpers_are_bounded() {
        assert_eq!(broken_rate_reduce_mix(0.0), 0.0);
        assert!(broken_rate_reduce_mix(1.0) <= 0.72);
        assert!(broken_hold_period(1.0, 1.0, 1.0, 1) <= 50);
        assert!((broken_bitcrush(0.25, 0.0) - 0.25).abs() < 0.000_001);
        assert!(broken_artifact_gain(1.0, 1.0) <= 0.030);
        assert!(broken_level_compensation(1.0, 1.0) >= 0.88);
    }

    #[test]
    fn broken_seed_is_stable_across_runs() {
        // Same defaults + same input must give bit-identical output: the random
        // generators are seeded, not wall-clock, so presets recall exactly.
        let frame = broken_frame(0.9, 0.6, 0.6, 0.85, 0.7);
        let render = || {
            let mut texture = Texture::default();
            texture.prepare(48_000.0);
            let mut phase = 0.0_f32;
            let mut out = Vec::with_capacity(8_000);
            for _ in 0..8_000 {
                phase += 220.0 / 48_000.0;
                if phase >= 1.0 {
                    phase -= phase.floor();
                }
                let input = (phase * core::f32::consts::TAU).sin() * 0.3;
                out.push(texture.process_sample_for_channel(0, input, &frame));
            }
            out
        };
        assert_eq!(render(), render(), "broken must be deterministic per seed");
    }

    #[test]
    fn broken_removes_dc_offset() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // Heavy crush/hold can bias the waveform; feed a DC-heavy input and check
        // the running mean of the output stays centered.
        let frame = broken_frame(0.6, 0.4, 0.5, 0.9, 0.5);
        let mut phase = 0.0_f32;
        let mut sum = 0.0_f64;
        let mut count = 0.0_f64;
        for index in 0..40_000 {
            phase += 110.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            // Asymmetric input with a deliberate +0.3 DC pedestal.
            let input = (phase * core::f32::consts::TAU).sin() * 0.3 + 0.3;
            let output = texture.process_sample_for_channel(0, input, &frame);
            if index > 8_000 {
                sum += output as f64;
                count += 1.0;
            }
        }
        let mean = (sum / count).abs();
        assert!(
            mean < 0.02,
            "broken should not leave a DC offset, mean={mean}"
        );
    }

    #[test]
    fn broken_max_amount_is_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = broken_frame(1.0, 1.0, 0.9, 1.0, 1.0);
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..40_000 {
            phase += 330.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let transient = if index % 3_000 == 0 { 1.0 } else { 0.0 };
            let input = (phase * core::f32::consts::TAU).sin() * 0.8 + transient;
            let output = texture.process_sample_for_channel(index % 2, input, &frame);
            assert!(output.is_finite(), "broken max: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(peak < 2.0, "broken at max should stay safe, peak={peak}");
    }

    fn interference_frame(
        random_drift: f32,
        noise_amount: f32,
        noise_color: f32,
        degrade: f32,
        stereo_spread: f32,
    ) -> TextureFrame {
        let mut frame = test_frame(TextureMode::Interference);
        frame.random_drift = random_drift;
        frame.noise_amount = noise_amount;
        frame.noise_color = noise_color;
        frame.degrade = degrade;
        frame.stereo_spread = stereo_spread;
        frame
    }

    #[test]
    fn interference_silence_has_controlled_electricity() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = interference_frame(0.65, 1.0, 0.72, 0.7, 1.0);
        let mut peak = 0.0_f32;
        let mut sum = 0.0_f32;

        for _ in 0..12_000 {
            let output = texture.process_sample_for_channel(0, 0.0, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
            sum += output * output;
        }

        let rms = (sum / 12_000.0).sqrt();
        assert!(
            rms > 0.000_05,
            "interference should add audible electricity, rms={rms}"
        );
        assert!(
            peak < 0.08,
            "interference silence artifacts too hot, peak={peak}"
        );
    }

    #[test]
    fn interference_sine_is_modulated_without_gain_blowup() {
        let mut clean = Texture::default();
        let mut interference = Texture::default();
        clean.prepare(48_000.0);
        interference.prepare(48_000.0);
        let clean_frame = interference_frame(0.0, 0.0, 0.5, 0.0, 0.0);
        let active_frame = interference_frame(0.55, 0.65, 0.45, 0.45, 0.8);
        let mut phase: f32 = 0.0;
        let mut diff_sum = 0.0_f32;
        let mut count = 0.0_f32;
        let mut peak = 0.0_f32;

        for index in 0..18_000 {
            phase += 330.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let input = (phase * core::f32::consts::TAU).sin() * 0.30;
            let dryish = clean.process_sample_for_channel(0, input, &clean_frame);
            let output = interference.process_sample_for_channel(0, input, &active_frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());

            if index > 2_000 {
                let diff = output - dryish;
                diff_sum += diff * diff;
                count += 1.0;
            }
        }

        let diff_rms = (diff_sum / count).sqrt();
        assert!(
            diff_rms > 0.001,
            "interference should modulate sine, diff={diff_rms}"
        );
        assert!(
            peak < 0.75,
            "interference sine peak should stay controlled, peak={peak}"
        );
    }

    #[test]
    fn interference_stereo_spread_offsets_lr_modulation() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = interference_frame(0.4, 0.75, 0.35, 0.35, 1.0);
        let mut phase: f32 = 0.0;
        let mut diff_sum = 0.0_f32;
        let mut count = 0.0_f32;

        for index in 0..12_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let input = (phase * core::f32::consts::TAU).sin() * 0.25;
            let left = texture.process_sample_for_channel(0, input, &frame);
            let right = texture.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());

            if index > 2_000 {
                let diff = left - right;
                diff_sum += diff * diff;
                count += 1.0;
            }
        }

        let diff_rms = (diff_sum / count).sqrt();
        assert!(
            diff_rms > 0.000_5,
            "stereo spread should create L/R interference differences, diff={diff_rms}"
        );
    }

    #[test]
    fn interference_helpers_are_bounded_and_color_mapped() {
        let low = interference_frequency_hz(0.0, 0.0, 0, 48_000.0);
        let high = interference_frequency_hz(1.0, 0.0, 0, 48_000.0);
        assert!(low >= 35.0);
        assert!(high > low * 50.0);
        assert!(high <= 6_800.0);
        assert!(interference_parasite_frequency_hz(high, 1.0, 1, 48_000.0) <= 8_500.0);
        assert!(interference_am_depth(1.0, 1.0) <= 0.42);
        assert!(interference_ring_depth(1.0, 1.0) <= 0.63);
        assert!(interference_tone_gain(1.0, 1.0) <= 0.023);
        assert!(interference_noise_gain(1.0, 1.0) <= 0.026);
        assert!(interference_level_compensation(1.0, 1.0) >= 0.90);
        // Damping opens with color and closes with degrade (harshness control).
        let bright = interference_damping_hz(1.0, 0.0, 48_000.0);
        let dark = interference_damping_hz(0.0, 1.0, 48_000.0);
        assert!(
            bright > dark,
            "color should open damping (bright={bright}, dark={dark})"
        );
        assert!(dark >= 1_800.0);
        assert!(bright <= 16_000.0);
    }

    #[test]
    fn interference_max_amount_is_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = interference_frame(1.0, 1.0, 0.85, 1.0, 1.0);
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..40_000 {
            phase += 440.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let input = (phase * core::f32::consts::TAU).sin() * 0.8;
            let output = texture.process_sample_for_channel(index % 2, input, &frame);
            assert!(output.is_finite(), "interference max: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 2.0,
            "interference at max should stay safe, peak={peak}"
        );
    }

    #[test]
    fn interference_color_sweep_has_no_zipper() {
        // Sweeping the interference frequency (color) must not produce harsh
        // zipper steps: the oscillators and filters are continuous.
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let mut frame = interference_frame(0.3, 0.7, 0.0, 0.5, 0.4);
        let mut phase = 0.0_f32;
        let mut prev = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..48_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            frame.noise_color = (index as f32 / 48_000.0).clamp(0.0, 1.0);
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 2_000 {
                max_step = max_step.max((output - prev).abs());
            }
            prev = output;
        }
        assert!(
            max_step < 0.25,
            "interference color sweep should not zipper (max step={max_step})"
        );
    }

    #[test]
    fn interference_mono_sum_does_not_cancel() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // Full stereo spread, but a mono fold-down must not fully cancel.
        let frame = interference_frame(0.4, 0.6, 0.4, 0.4, 1.0);
        let mut phase = 0.0_f32;
        let mut mono_sum = 0.0_f32;
        let mut count = 0.0_f32;
        for index in 0..16_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let left = texture.process_sample_for_channel(0, input, &frame);
            let right = texture.process_sample_for_channel(1, input, &frame);
            let mono = 0.5 * (left + right);
            if index > 4_000 {
                mono_sum += mono * mono;
                count += 1.0;
            }
        }
        let mono_rms = (mono_sum / count).sqrt();
        assert!(
            mono_rms > 0.02,
            "interference must stay mono-compatible (mono rms={mono_rms})"
        );
    }

    fn squash_frame(degrade: f32, noise_color: f32, stereo_spread: f32) -> TextureFrame {
        let mut frame = test_frame(TextureMode::Squash);
        frame.degrade = degrade;
        frame.noise_color = noise_color;
        frame.stereo_spread = stereo_spread;
        frame
    }

    #[test]
    fn squash_impulse_stays_finite_and_limited() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = squash_frame(1.0, 0.55, 0.5);
        let mut peak = 0.0_f32;

        for index in 0..4_000 {
            let input = if index == 0 { 3.0 } else { 0.0 };
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
        }

        assert!(peak < 4.5, "squash impulse peak was {peak}");
    }

    #[test]
    fn squash_sine_stays_finite_and_adds_sustain() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = squash_frame(0.85, 0.45, 0.7);
        let mut phase: f32 = 0.0;
        let mut input_sum = 0.0;
        let mut output_sum = 0.0;
        let mut count = 0.0;

        for index in 0..12_000 {
            phase += 220.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let amp = if index < 6_000 { 0.18 } else { 0.045 };
            let input = (phase * core::f32::consts::TAU).sin() * amp;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            if index > 10_000 {
                input_sum += input * input;
                output_sum += output * output;
                count += 1.0;
            }
        }

        let input_rms = (input_sum / count).sqrt();
        let output_rms = (output_sum / count).sqrt();
        assert!(
            output_rms > input_rms * 1.15,
            "squash should lift quiet sustain, in={input_rms}, out={output_rms}"
        );
    }

    #[test]
    fn squash_noise_stays_finite() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = squash_frame(1.0, 0.9, 1.0);
        let mut rng = 0xdecafbad_u32;

        for _ in 0..12_000 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let input = ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.8;
            let output = texture.process_sample_for_channel(1, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
        }
    }

    #[test]
    fn squash_gain_helpers_are_bounded() {
        assert!(squash_compression_gain(1.0, 1.0) < 1.0);
        assert_eq!(squash_compression_gain(0.1, 0.0), 1.0);
        assert!(squash_makeup_gain(1.0) <= 1.90);
    }

    #[test]
    fn squash_soft_knee_engages_gradually() {
        // Just under the threshold the soft knee already applies a touch of
        // reduction, where a hard knee would still be exactly unity.
        let degrade = 0.6;
        let threshold_db = -4.0 - degrade * 30.0;
        let just_under = 10.0_f32.powf((threshold_db - 1.0) / 20.0);
        let gain = squash_compression_gain(just_under, degrade);
        assert!(
            gain < 1.0 && gain > 0.9,
            "soft knee should apply gentle reduction just under threshold, gain={gain}"
        );
    }

    #[test]
    fn squash_silence_stays_silent() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = squash_frame(1.0, 0.5, 0.0);
        let mut peak = 0.0_f32;
        for _ in 0..8192 {
            let output = texture.process_sample_for_channel(0, 0.0, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 1e-6,
            "squash should stay silent on silence, peak {peak}"
        );
    }

    #[test]
    fn squash_max_amount_is_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = squash_frame(1.0, 0.5, 0.0);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..16000 {
            phase += 440.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.6 + if index % 2000 == 0 { 0.9 } else { 0.0 };
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "squash max amount: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 3.0,
            "squash at max amount should stay safe, peak={peak}"
        );
    }

    fn filter_frame(noise_color: f32, degrade: f32, stereo_spread: f32) -> TextureFrame {
        let mut frame = test_frame(TextureMode::Filter);
        frame.noise_color = noise_color;
        frame.degrade = degrade;
        frame.stereo_spread = stereo_spread;
        frame
    }

    fn filter_rms(freq_hz: f32, noise_color: f32) -> f32 {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = filter_frame(noise_color, 0.25, 0.0);
        let mut phase = 0.0;
        let mut sum = 0.0;
        let mut count = 0.0;

        for index in 0..12_000 {
            phase += freq_hz / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let input = (phase * core::f32::consts::TAU).sin() * 0.35;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            if index > 2_000 {
                sum += output * output;
                count += 1.0;
            }
        }

        (sum / count).sqrt()
    }

    #[test]
    fn filter_extreme_cutoffs_stay_finite() {
        for noise_color in [0.0, 1.0] {
            let mut texture = Texture::default();
            texture.prepare(48_000.0);
            let frame = filter_frame(noise_color, 1.0, 1.0);

            let mut phase: f32 = 0.0;
            for _ in 0..4_000 {
                phase += 1_200.0 / 48_000.0;
                if phase >= 1.0 {
                    phase -= phase.floor();
                }
                let input = (phase * core::f32::consts::TAU).sin() * 0.35;
                let output = texture.process_sample_for_channel(0, input, &frame);
                assert!(output.is_finite());
                assert!(output.abs() <= 8.0);
            }
        }
    }

    #[test]
    fn filter_dark_setting_behaves_like_low_pass() {
        let low = filter_rms(120.0, 0.08);
        let high = filter_rms(8_000.0, 0.08);

        assert!(
            low > high * 2.0,
            "dark filter should keep lows above highs, low={low}, high={high}"
        );
    }

    #[test]
    fn filter_bright_setting_behaves_like_high_pass() {
        let low = filter_rms(200.0, 0.95);
        let high = filter_rms(18_000.0, 0.95);

        assert!(
            high > low * 2.0,
            "bright filter should keep highs above lows, low={low}, high={high}"
        );
    }

    #[test]
    fn filter_stereo_spread_offsets_cutoff_per_channel() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = filter_frame(0.30, 0.2, 1.0);
        let mut phase = 0.0;
        let mut left = 0.0;
        let mut right = 0.0;

        for _ in 0..4_000 {
            phase += 900.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.35;
            left = texture.process_sample_for_channel(0, input, &frame);
            right = texture.process_sample_for_channel(1, input, &frame);
        }

        assert!(left.is_finite() && right.is_finite());
        assert!(
            (left - right).abs() > 0.000_001,
            "stereo spread should offset filter response, l={left}, r={right}"
        );
    }

    #[test]
    fn filter_parameter_helpers_are_bounded() {
        assert!(texture_filter_cutoff_hz(0.0, 1.0, 0, 48_000.0) >= 80.0);
        assert!(texture_filter_cutoff_hz(1.0, 1.0, 1, 48_000.0) <= 16_000.0);
        assert!(texture_filter_resonance(1.0) < 0.70);
    }

    #[test]
    fn filter_color_sweep_has_no_strong_zipper() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let total = 48_000usize;
        let mut sig_phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..total {
            let color = index as f32 / (total - 1) as f32; // sweep Color 0 -> 1
            let frame = filter_frame(color, 0.4, 0.0);
            sig_phase += 220.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.35;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 2000 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            max_step < 0.1,
            "filter color sweep should not zipper, max step {max_step}"
        );
    }

    #[test]
    fn filter_max_resonance_does_not_explode() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // degrade = 1.0 maps to maximum resonance + drive.
        let frame = filter_frame(0.55, 1.0, 0.0);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            phase += 440.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.4;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "filter max resonance: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 2.0,
            "filter at max resonance should stay controlled, peak={peak}"
        );
    }

    #[test]
    fn filter_white_noise_peak_is_safe() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = filter_frame(0.7, 0.8, 1.0);

        let mut rng: u32 = 0xf117_e2a1;
        let mut peak = 0.0_f32;
        for _ in 0..8192 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.3;
            let output = texture.process_sample_for_channel(0, noise, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
        }
        assert!(peak < 1.5, "filter white noise peak should be safe, {peak}");
    }

    #[test]
    fn filter_default_color_is_transparent() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        // Color 0.5 (morph midpoint) + degrade 0 = pass-through.
        let frame = filter_frame(0.5, 0.0, 0.0);

        let tau = core::f32::consts::TAU;
        let mut phase = 0.0_f32;
        let mut max_diff = 0.0_f32;
        for _ in 0..4096 {
            phase += 440.0 / 48_000.0;
            let input = (phase * tau).sin() * 0.3;
            let output = texture.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            max_diff = max_diff.max((output - input).abs());
        }
        assert!(
            max_diff < 1e-3,
            "filter at default color should be transparent, max diff {max_diff}"
        );
    }
}
