use nih_plug::prelude::*;

use crate::params::CharacterParams;

use super::{
    chain::{sanitize_sample, soft_clip_sample, ModuleCore},
    dry_wet::DryWet,
    gain::db_to_gain,
    smoothing::LinearSmoother,
};

const MAX_CHANNELS: usize = 2;
const MIN_TONE_HZ: f32 = 700.0;
const MAX_TONE_HZ: f32 = 18_000.0;
const MIN_CASSETTE_TONE_HZ: f32 = 1_800.0;
const MAX_CASSETTE_TONE_HZ: f32 = 16_000.0;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterMode {
    #[id = "clean"]
    Clean,

    #[id = "saturation"]
    Saturation,

    #[id = "cassette"]
    Cassette,
}

#[derive(Debug, Clone)]
pub struct Character {
    core: ModuleCore,
    sample_rate: f32,
    tone_state: [f32; MAX_CHANNELS],
    cassette_tone_state: [f32; MAX_CHANNELS],
    instability_phase: [f32; MAX_CHANNELS],
    noise_state: [u32; MAX_CHANNELS],
    current_mode: CharacterMode,
    mode_crossfade: LinearSmoother,
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CharacterFrame {
    mode: CharacterMode,
    drive: f32,
    age: f32,
    tone: f32,
    mix: f32,
    output_gain: f32,
    active_mix: f32,
    tone_alpha: f32,
    cassette_tone_alpha: f32,
    cassette_noise_gain: f32,
    cassette_flutter_depth: f32,
    mode_fade: f32,
}

impl Default for Character {
    fn default() -> Self {
        Self {
            core: ModuleCore::default(),
            sample_rate: 44_100.0,
            tone_state: [0.0; MAX_CHANNELS],
            cassette_tone_state: [0.0; MAX_CHANNELS],
            instability_phase: [0.0; MAX_CHANNELS],
            noise_state: [0x1234_5678, 0x8765_4321],
            current_mode: CharacterMode::Clean,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
        }
    }
}

impl Character {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.core.prepare(self.sample_rate);
        self.mode_crossfade.prepare(self.sample_rate);
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.tone_state = [0.0; MAX_CHANNELS];
        self.cassette_tone_state = [0.0; MAX_CHANNELS];
        self.instability_phase = [0.0; MAX_CHANNELS];
        self.noise_state = [0x1234_5678, 0x8765_4321];
        self.mode_crossfade.reset(1.0);
        self.last_output = [0.0; MAX_CHANNELS];
        self.has_processed = false;
    }

    pub fn next_frame(&mut self, params: &CharacterParams) -> CharacterFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let drive = params.drive.smoothed.next().clamp(0.0, 1.0);
        let age = params.age.smoothed.next().clamp(0.0, 1.0);
        let tone = params.tone.smoothed.next().clamp(0.0, 1.0);
        let noise = params.noise.smoothed.next().clamp(0.0, 1.0);
        let mix = params.mix.smoothed.next().clamp(0.0, 1.0);
        let output_gain = db_to_gain(params.output_trim.smoothed.next().clamp(-12.0, 12.0));
        let module_frame = self.core.next_frame(params.bypass.value(), mix, 0.0);

        CharacterFrame {
            mode,
            drive,
            age,
            tone,
            mix,
            output_gain,
            active_mix: module_frame.active_mix,
            tone_alpha: tone_to_alpha(tone, self.sample_rate),
            cassette_tone_alpha: cassette_tone_to_alpha(age, tone, self.sample_rate),
            cassette_noise_gain: cassette_noise_gain(age, noise),
            cassette_flutter_depth: cassette_flutter_depth(age),
            mode_fade: self.mode_crossfade.next_value().clamp(0.0, 1.0),
        }
    }

    pub fn process_block(&mut self, buffer: &mut Buffer, params: &CharacterParams) {
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params);

            for (channel_index, sample) in channel_samples.into_iter().enumerate() {
                *sample = self.process_sample(channel_index, *sample, &frame);
            }
        }
    }

    pub fn process_sample(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let dry = sanitize_sample(sample);
        let wet = match frame.mode {
            CharacterMode::Clean => dry,
            CharacterMode::Saturation => self.process_saturation(index, dry, frame),
            CharacterMode::Cassette => self.process_cassette(index, dry, frame),
        };

        let wet = sanitize_sample(wet * frame.output_gain);
        let mixed = DryWet.mix(dry, wet, frame.mix);
        let mode_mixed = self.smooth_mode_transition(index, mixed, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, mode_mixed, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    fn process_saturation(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let drive_gain = drive_to_gain(frame.drive);
        let bias = 0.008 * frame.drive;
        let driven = soft_clip_sample((sample + bias) * drive_gain);
        let saturated = fast_tanh(driven);
        let compensated = saturated * drive_compensation(frame.drive, drive_gain);
        self.apply_tone(channel, compensated, frame)
    }

    fn process_cassette(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let instability = self.next_instability(index, frame);
        let drive_gain = cassette_drive_to_gain(frame.drive, frame.age) * instability;
        let driven = soft_clip_sample(sample * drive_gain);
        let saturated = fast_tanh(driven + (0.015 * frame.age * driven * driven));
        let compressed = saturated * cassette_compensation(frame.drive, frame.age, drive_gain);
        let aged = self.apply_cassette_tone(index, compressed, frame);
        let noise = self.next_noise(index) * frame.cassette_noise_gain;

        sanitize_sample(aged + noise)
    }

    fn apply_tone(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let state = self.tone_state[index] + (frame.tone_alpha * (sample - self.tone_state[index]));
        self.tone_state[index] = sanitize_sample(state);

        let bright_blend = frame.tone * frame.tone;
        sanitize_sample((state * (1.0 - bright_blend)) + (sample * bright_blend))
    }

    fn apply_cassette_tone(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let state = self.cassette_tone_state[channel]
            + (frame.cassette_tone_alpha * (sample - self.cassette_tone_state[channel]));
        self.cassette_tone_state[channel] = sanitize_sample(state);
        sanitize_sample(state)
    }

    fn next_instability(&mut self, channel: usize, frame: &CharacterFrame) -> f32 {
        let flutter_hz = 0.35 + (frame.age * 4.0);
        let phase_step = (flutter_hz / self.sample_rate).clamp(0.0, 0.25);
        let phase = self.instability_phase[channel] + phase_step;
        self.instability_phase[channel] = if phase >= 1.0 { phase - 1.0 } else { phase };

        let wobble = (self.instability_phase[channel] * core::f32::consts::TAU).sin();
        1.0 + (wobble * frame.cassette_flutter_depth)
    }

    fn next_noise(&mut self, channel: usize) -> f32 {
        let mut state = self.noise_state[channel];
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.noise_state[channel] = state;

        let normalized = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        sanitize_sample(normalized)
    }

    fn set_mode(&mut self, mode: CharacterMode) {
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

#[inline]
fn drive_to_gain(drive: f32) -> f32 {
    db_to_gain(drive.clamp(0.0, 1.0) * 24.0)
}

#[inline]
fn cassette_drive_to_gain(drive: f32, age: f32) -> f32 {
    db_to_gain(2.0 + (drive.clamp(0.0, 1.0) * 18.0) + (age.clamp(0.0, 1.0) * 3.0))
}

#[inline]
fn drive_compensation(drive: f32, drive_gain: f32) -> f32 {
    let compensation = 1.0 / drive_gain.sqrt();
    let low_drive_makeup = 1.0 + (drive.clamp(0.0, 1.0) * 0.35);
    (compensation * low_drive_makeup).clamp(0.18, 1.0)
}

#[inline]
fn cassette_compensation(drive: f32, age: f32, drive_gain: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    let age = age.clamp(0.0, 1.0);
    let base = 1.0 / drive_gain.powf(0.42);
    let age_loss_makeup = 1.0 + (age * 0.12);
    let drive_makeup = 1.0 + (drive * 0.18);
    (base * age_loss_makeup * drive_makeup).clamp(0.16, 0.95)
}

#[inline]
fn fast_tanh(x: f32) -> f32 {
    let x2 = x * x;
    let num = x * (27.0 + x2);
    let den = 27.0 + 9.0 * x2;
    num / den
}

#[inline]
fn tone_to_alpha(tone: f32, sample_rate: f32) -> f32 {
    let tone = tone.clamp(0.0, 1.0);
    let cutoff = MIN_TONE_HZ + ((MAX_TONE_HZ - MIN_TONE_HZ) * tone * tone);
    let sample_rate = sample_rate.max(1.0);
    (1.0 - (-2.0 * core::f32::consts::PI * cutoff / sample_rate).exp()).clamp(0.0, 1.0)
}

#[inline]
fn cassette_tone_to_alpha(age: f32, tone: f32, sample_rate: f32) -> f32 {
    let age = age.clamp(0.0, 1.0);
    let tone = tone.clamp(0.0, 1.0);
    let age_darkening = 1.0 - (age * 0.78);
    let tone_brightness = 0.45 + (tone * 0.75);
    let cutoff = (MAX_CASSETTE_TONE_HZ * age_darkening * tone_brightness)
        .clamp(MIN_CASSETTE_TONE_HZ, MAX_CASSETTE_TONE_HZ);
    let sample_rate = sample_rate.max(1.0);
    (1.0 - (-2.0 * core::f32::consts::PI * cutoff / sample_rate).exp()).clamp(0.0, 1.0)
}

#[inline]
fn cassette_noise_gain(age: f32, noise: f32) -> f32 {
    let age_lift = 0.25 + (age.clamp(0.0, 1.0) * 0.75);
    noise.clamp(0.0, 1.0).powf(2.2) * age_lift * 0.000_45
}

#[inline]
fn cassette_flutter_depth(age: f32) -> f32 {
    age.clamp(0.0, 1.0).powf(1.5) * 0.018
}

#[cfg(test)]
mod tests {
    use super::{
        cassette_noise_gain, cassette_tone_to_alpha, drive_to_gain, Character, CharacterFrame,
        CharacterMode,
    };

    #[test]
    fn drive_maps_to_more_input_gain() {
        assert!(drive_to_gain(1.0) > drive_to_gain(0.5));
        assert!((drive_to_gain(0.0) - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn saturation_output_stays_finite() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = CharacterFrame {
            mode: CharacterMode::Saturation,
            drive: 1.0,
            age: 0.0,
            tone: 0.5,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.5,
            cassette_tone_alpha: 0.5,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        };

        let sample = character.process_sample(0, 100.0, &frame);
        assert!(sample.is_finite());
        assert!(sample.abs() <= 8.0);
    }

    #[test]
    fn high_drive_saturation_is_soft_limited() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = CharacterFrame {
            mode: CharacterMode::Saturation,
            drive: 1.0,
            age: 0.0,
            tone: 1.0,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 1.0,
            cassette_tone_alpha: 1.0,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        };

        let sample = character.process_sample(0, 1_000.0, &frame);
        assert!(sample.is_finite());
        assert!(
            sample.abs() < 2.0,
            "high drive saturation should be bounded by the waveshaper, got {sample}"
        );
    }

    #[test]
    fn cassette_output_stays_finite() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = CharacterFrame {
            mode: CharacterMode::Cassette,
            drive: 1.0,
            age: 1.0,
            tone: 0.25,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.5,
            cassette_tone_alpha: cassette_tone_to_alpha(1.0, 0.25, 48_000.0),
            cassette_noise_gain: cassette_noise_gain(1.0, 1.0),
            cassette_flutter_depth: 0.018,
            mode_fade: 1.0,
        };

        let sample = character.process_sample(1, 100.0, &frame);
        assert!(sample.is_finite());
        assert!(sample.abs() <= 8.0);
    }

    #[test]
    fn cassette_noise_scales_from_silent_to_subtle() {
        assert_eq!(cassette_noise_gain(1.0, 0.0), 0.0);
        assert!(cassette_noise_gain(1.0, 1.0) <= 0.000_45);
    }
}
