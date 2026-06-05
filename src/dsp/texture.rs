use nih_plug::prelude::*;

use crate::params::TextureParams;

use super::{
    chain::{sanitize_sample, ModuleCore},
    dry_wet::DryWet,
    smoothing::LinearSmoother,
};

const MAX_CHANNELS: usize = 2;
const MAX_DELAY_SECONDS: f32 = 0.08;
const BASE_DELAY_MS: f32 = 18.0;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureMode {
    #[id = "off"]
    Off,

    #[id = "wow-flutter"]
    #[name = "Wow/Flutter"]
    WowFlutter,

    #[id = "noise"]
    #[name = "Noise"]
    Noise,
}

#[derive(Debug, Clone)]
pub struct Texture {
    core: ModuleCore,
    delay: StereoDelay,
    sample_rate: f32,
    wow_phase: f32,
    flutter_phase: f32,
    drift: [DriftGenerator; MAX_CHANNELS],
    noise: [NoiseGenerator; MAX_CHANNELS],
    current_mode: TextureMode,
    mode_crossfade: LinearSmoother,
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
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
}

impl Default for Texture {
    fn default() -> Self {
        let mut texture = Self {
            core: ModuleCore::default(),
            delay: StereoDelay::default(),
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
            current_mode: TextureMode::Off,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
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

    pub fn next_frame(&mut self, params: &TextureParams) -> TextureFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let wow_depth = params.wow_depth.smoothed.next().clamp(0.0, 1.0);
        let wow_rate_hz = params.wow_rate.smoothed.next().clamp(0.1, 2.0);
        let flutter_depth = params.flutter_depth.smoothed.next().clamp(0.0, 1.0);
        let flutter_rate_hz = params.flutter_rate.smoothed.next().clamp(3.0, 20.0);
        let random_drift = params.random_drift.smoothed.next().clamp(0.0, 1.0);
        let noise_amount = params.noise_amount.smoothed.next().clamp(0.0, 1.0);
        let noise_color = params.noise_color.smoothed.next().clamp(0.0, 1.0);
        let degrade = params.degrade.smoothed.next().clamp(0.0, 1.0);
        let stereo_spread = params.stereo_spread.smoothed.next().clamp(0.0, 1.0);
        let mix = params.mix.smoothed.next().clamp(0.0, 1.0);
        let module_frame = self
            .core
            .next_frame(params.bypass.value(), wow_depth, mix, 0.0);

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
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params);
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
            TextureMode::Off => dry,
            TextureMode::WowFlutter => self.process_wow_flutter(index, dry, frame),
            TextureMode::Noise => self.process_noise(index, dry, frame),
        };

        let mixed = if frame.mode == TextureMode::Off {
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

    fn process_wow_flutter(&mut self, channel: usize, sample: f32, frame: &TextureFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let stereo_phase = if index == 0 { 0.0 } else { 0.17 };
        let wow = sine_lfo((frame.wow_phase + stereo_phase).fract()) * frame.wow_depth * 8.0;
        let flutter = sine_lfo((frame.flutter_phase + (stereo_phase * 2.0)).fract())
            * frame.flutter_depth
            * 2.2;
        let drift = self.drift[index].next_value(frame.random_drift) * 6.0;
        let delay_ms = (BASE_DELAY_MS + wow + flutter + drift).clamp(2.0, 60.0);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;

        self.delay.process(index, sample, delay_samples)
    }

    fn process_noise(&mut self, channel: usize, sample: f32, frame: &TextureFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let shared_noise = self.noise[0].next_colored(frame.noise_color);
        let local_noise = if index == 0 {
            shared_noise
        } else {
            self.noise[index].next_colored(frame.noise_color)
        };
        let stereo_noise =
            (shared_noise * (1.0 - frame.stereo_spread)) + (local_noise * frame.stereo_spread);
        let noise_gain = noise_amount_to_gain(frame.noise_amount);
        let noisy = sample + (stereo_noise * noise_gain);

        apply_degradation(noisy, frame.degrade, stereo_noise)
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
        }
    }

    fn reset(&mut self) {
        self.low_state = 0.0;
    }

    fn next_colored(&mut self, color: f32) -> f32 {
        let white = self.random_bipolar();
        let color = color.clamp(0.0, 1.0);
        let alpha = 0.015 + (color * color * 0.45);
        self.low_state += alpha * (white - self.low_state);
        self.low_state = sanitize_sample(self.low_state);

        let high = white - self.low_state;
        let dark_weight = 1.0 - color;
        sanitize_sample((self.low_state * dark_weight) + (high * color * 0.65))
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
fn noise_amount_to_gain(amount: f32) -> f32 {
    amount.clamp(0.0, 1.0).powf(2.2) * 0.045
}

#[inline]
fn apply_degradation(sample: f32, degrade: f32, noise: f32) -> f32 {
    let degrade = degrade.clamp(0.0, 1.0);
    if degrade <= 0.000_001 {
        return sanitize_sample(sample);
    }

    let drive = 1.0 + (degrade * 2.5);
    let bias = noise * degrade * 0.025;
    let saturated = ((sample + bias) * drive).tanh() / drive.tanh();
    let softened = (sample * (1.0 - degrade * 0.35)) + (saturated * degrade * 0.35);

    sanitize_sample(softened)
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
        advance_phase, apply_degradation, noise_amount_to_gain, read_interpolated, DriftGenerator,
        NoiseGenerator, StereoDelay, Texture, TextureFrame, TextureMode,
    };

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
    fn wow_flutter_output_stays_finite() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let frame = test_frame(TextureMode::WowFlutter);

        let mut sample = 0.5;
        for _ in 0..2_000 {
            sample = texture.process_sample_for_channel(0, sample, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn noise_gain_is_calibrated_low() {
        assert_eq!(noise_amount_to_gain(0.0), 0.0);
        assert!(noise_amount_to_gain(0.25) < 0.003);
        assert!(noise_amount_to_gain(1.0) <= 0.045);
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

    #[test]
    fn degradation_is_finite_and_subtle_at_low_amount() {
        let dry = 0.25;
        let degraded = apply_degradation(dry, 0.1, 0.2);

        assert!(degraded.is_finite());
        assert!((degraded - dry).abs() < 0.05);
    }

    #[test]
    fn noise_mode_output_stays_finite() {
        let mut texture = Texture::default();
        texture.prepare(48_000.0);
        let mut frame = test_frame(TextureMode::Noise);
        frame.noise_amount = 1.0;
        frame.noise_color = 0.8;
        frame.degrade = 1.0;
        frame.stereo_spread = 1.0;

        let mut sample = 0.2;
        for _ in 0..2_000 {
            sample = texture.process_sample_for_channel(1, sample, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }
}
