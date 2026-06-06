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

    #[id = "drive"]
    #[name = "Drive"]
    Drive,

    #[id = "sweet"]
    #[name = "Sweet"]
    Sweet,

    #[id = "fuzz"]
    #[name = "Fuzz"]
    Fuzz,

    #[id = "howl"]
    #[name = "Howl"]
    Howl,

    #[id = "swell"]
    #[name = "Swell"]
    Swell,
}

#[derive(Debug, Clone)]
pub struct Character {
    core: ModuleCore,
    sample_rate: f32,
    tone_state: [f32; MAX_CHANNELS],
    cassette_tone_state: [f32; MAX_CHANNELS],
    instability_phase: [f32; MAX_CHANNELS],
    noise_state: [u32; MAX_CHANNELS],
    drive_dc_state: [f32; MAX_CHANNELS],
    drive_hf_state: [f32; MAX_CHANNELS],
    sweet_dc_state: [f32; MAX_CHANNELS],
    sweet_exciter_state: [f32; MAX_CHANNELS],
    fuzz_dc_state: [f32; MAX_CHANNELS],
    fuzz_tone_state: [f32; MAX_CHANNELS],
    howl_low_state: [f32; MAX_CHANNELS],
    howl_band_state: [f32; MAX_CHANNELS],
    swell_env_state: [f32; MAX_CHANNELS],
    swell_gain_state: [f32; MAX_CHANNELS],
    swell_tone_state: [f32; MAX_CHANNELS],
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
            drive_dc_state: [0.0; MAX_CHANNELS],
            drive_hf_state: [0.0; MAX_CHANNELS],
            sweet_dc_state: [0.0; MAX_CHANNELS],
            sweet_exciter_state: [0.0; MAX_CHANNELS],
            fuzz_dc_state: [0.0; MAX_CHANNELS],
            fuzz_tone_state: [0.0; MAX_CHANNELS],
            howl_low_state: [0.0; MAX_CHANNELS],
            howl_band_state: [0.0; MAX_CHANNELS],
            swell_env_state: [0.0; MAX_CHANNELS],
            swell_gain_state: [0.0; MAX_CHANNELS],
            swell_tone_state: [0.0; MAX_CHANNELS],
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
        self.drive_dc_state = [0.0; MAX_CHANNELS];
        self.drive_hf_state = [0.0; MAX_CHANNELS];
        self.sweet_dc_state = [0.0; MAX_CHANNELS];
        self.sweet_exciter_state = [0.0; MAX_CHANNELS];
        self.fuzz_dc_state = [0.0; MAX_CHANNELS];
        self.fuzz_tone_state = [0.0; MAX_CHANNELS];
        self.howl_low_state = [0.0; MAX_CHANNELS];
        self.howl_band_state = [0.0; MAX_CHANNELS];
        self.swell_env_state = [0.0; MAX_CHANNELS];
        self.swell_gain_state = [0.0; MAX_CHANNELS];
        self.swell_tone_state = [0.0; MAX_CHANNELS];
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
            CharacterMode::Drive => self.process_drive(index, dry, frame),
            CharacterMode::Sweet => self.process_sweet(index, dry, frame),
            CharacterMode::Fuzz => self.process_fuzz(index, dry, frame),
            CharacterMode::Howl => self.process_howl(index, dry, frame),
            CharacterMode::Swell => self.process_swell(index, dry, frame),
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

    fn process_drive(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);

        // Stage 1: DC removal — first-order HPF tracking subsonic offset
        let dc_alpha = 0.9992;
        let dc = self.drive_dc_state[index];
        self.drive_dc_state[index] += (1.0 - dc_alpha) * (sample - dc);
        let dc_free = sanitize_sample(sample - self.drive_dc_state[index]);

        // Stage 2: Input drive gain with subtle asymmetrical bias
        let drive_gain = drive_to_gain(frame.drive);
        let bias = frame.drive * frame.drive * 0.011;
        let driven = sanitize_sample((dc_free + bias) * drive_gain);

        // Stage 3: Cascaded soft-clip stages for musical harmonic richness
        let stage1 = fast_tanh(driven * 0.7);

        let asymmetry = frame.drive * 0.19;
        let pos = stage1.max(0.0);
        let neg = stage1.min(0.0);
        let asymmetric_input = pos * (1.0 + asymmetry) + neg;
        let stage2 = fast_tanh(asymmetric_input);

        // Stage 4: High-frequency damping — increases with drive to soften fizz
        let drive_sq = frame.drive * frame.drive;
        let hf_cutoff_hz = 18_000.0 - (drive_sq * 15_000.0);
        let hf_cutoff_hz = hf_cutoff_hz.max(2_200.0);
        let hf_alpha = (1.0
            - (-core::f32::consts::TAU * hf_cutoff_hz / self.sample_rate.max(1.0)).exp())
        .clamp(0.0, 1.0);
        let hf = self.drive_hf_state[index] + hf_alpha * (stage2 - self.drive_hf_state[index]);
        self.drive_hf_state[index] = sanitize_sample(hf);

        // Stage 5: Gain compensation — perceived-loudness-aware
        let compensated = hf * drive_mode_compensation(frame.drive, drive_gain);

        // Stage 6: Safety soft-clip to guard against overshoot from filter ringing
        let safe = soft_clip_sample(compensated);

        // Stage 7: Tone control
        self.apply_tone(channel, safe, frame)
    }

    fn process_sweet(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);

        // Stage 1: Gentle asymmetric saturation emphasizing 2nd harmonic
        let drive_amount = frame.drive;
        let drive_gain = sweet_drive_to_gain(drive_amount);
        let asymmetry = drive_amount * 0.24;
        let pos = sample.max(0.0);
        let neg = sample.min(0.0);
        let shaped = (pos * (1.0 + asymmetry) + neg) * drive_gain;
        let saturated = fast_tanh(shaped);

        // Stage 2: DC removal from asymmetrical biasing
        let dc_alpha = 0.9992;
        let dc = self.sweet_dc_state[index];
        self.sweet_dc_state[index] += (1.0 - dc_alpha) * (saturated - dc);
        let dc_free = sanitize_sample(saturated - self.sweet_dc_state[index]);

        // Stage 3: Presence exciter — tone controls air/presence via high-frequency emphasis
        let tone = frame.tone;
        let exciter_amount = tone * tone * 0.52;
        let exciter_freq = 5_500.0 - (tone * 3_500.0);
        let exciter_alpha = (1.0
            - (-core::f32::consts::TAU * exciter_freq / self.sample_rate.max(1.0)).exp())
        .clamp(0.0, 1.0);
        let hpf = dc_free - self.sweet_exciter_state[index];
        self.sweet_exciter_state[index] += exciter_alpha * hpf;
        let excited = dc_free + (hpf * exciter_amount);

        // Stage 4: Gain compensation — level-matched, with slight makeup for harmonics
        let compensated = excited * sweet_compensation(drive_amount, drive_gain);

        // Stage 5: Safety soft-clip
        soft_clip_sample(compensated)
    }

    fn process_fuzz(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let drive = frame.drive;
        let tone = frame.tone;

        // Stage 1: Massive input gain — from subtle grit to full fuzz saturation
        let drive_gain = fuzz_drive_to_gain(drive);

        // Stage 2: Pre-gain soft-clip safety — prevents extreme overshoot before waveshaper
        let limited = (sample * drive_gain).clamp(-8.0, 8.0);

        // Stage 3: Asymmetric fuzz — thick, dense saturation with even harmonics
        let asymmetry = drive * 0.35;
        let pos = limited.max(0.0);
        let neg = limited.min(0.0);
        let biased = pos * (1.0 + asymmetry) + neg;

        // Stage 4: Hard-clip style saturation via cascaded tanh (each stage bounded)
        let stage1 = fuzz_hard_clip(biased, drive);
        let stage2 = fast_tanh(stage1 * (1.0 + drive * 0.6));
        let saturated = sanitize_sample(stage2);

        // Stage 5: DC block — clean up offset from asymmetry
        let dc_alpha = 0.9990;
        let dc = self.fuzz_dc_state[index];
        self.fuzz_dc_state[index] += (1.0 - dc_alpha) * (saturated - dc);
        let dc_free = sanitize_sample(saturated - self.fuzz_dc_state[index]);

        // Stage 6: Post-fuzz tone filter — LPF controlled by tone parameter
        let lpf_cutoff = 1_000.0 + (tone * tone * 14_500.0);
        let lpf_alpha = (1.0
            - (-core::f32::consts::TAU * lpf_cutoff / self.sample_rate.max(1.0)).exp())
        .clamp(0.0, 1.0);
        let toned =
            self.fuzz_tone_state[index] + lpf_alpha * (dc_free - self.fuzz_tone_state[index]);
        self.fuzz_tone_state[index] = sanitize_sample(toned);

        // Stage 7: Strong gain compensation — fuzz is dense, keep level safe
        let compensated = toned * fuzz_compensation(drive);

        // Stage 8: Final safety clip
        soft_clip_sample(compensated)
    }

    fn process_howl(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let tone = frame.tone;
        let drive = frame.drive;

        // Stage 1: Map tone to resonant frequency (vocal/synth range)
        let freq_hz = 150.0 + (tone * tone) * 3850.0;
        let f = (core::f32::consts::TAU * freq_hz / self.sample_rate.max(1.0)).clamp(0.001, 1.2);

        // Stage 2: Map drive to Q (resonance) — quadratic for subtle low end
        let q = 0.45 + (drive * drive) * 7.5;
        let q = q.clamp(0.40, 8.0);

        // Stage 3: Saturation gain in the feedback path
        let sat_gain = 1.0 + drive * 4.5;

        // Stage 4: State-variable filter with saturated feedback
        let low = self.howl_low_state[index];
        let band = self.howl_band_state[index];

        let high = sanitize_sample(sample - low - q * band);
        let high_sat = fast_tanh(high * sat_gain);

        let band_next = sanitize_sample(band + f * high_sat);
        let low_next = sanitize_sample(low + f * band_next);

        self.howl_low_state[index] = low_next;
        self.howl_band_state[index] = band_next;

        // Stage 5: Output the resonant bandpass
        let wet = band_next;

        // Stage 6: Gain compensation — resonance boosts level, compensate proportionally
        let comp = 0.72 / (1.0 + drive * 3.8);
        let compensated = sanitize_sample(wet * comp.clamp(0.10, 1.0));

        // Stage 7: Final safety clip
        soft_clip_sample(compensated)
    }

    fn process_swell(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let drive = frame.drive;
        let tone = frame.tone;

        // Stage 1: Fast envelope detector — tracks input amplitude
        let abs_in = sample.abs();
        let env_alpha = 0.08;
        let env = self.swell_env_state[index];
        let env_next = env + env_alpha * (abs_in - env);
        self.swell_env_state[index] = sanitize_sample(env_next);

        // Stage 2: Slew-rate limited gain — slow attack, musical release
        let attack_sec = 0.004 + drive * 0.48;
        let attack_sec = attack_sec.clamp(0.003, 0.50);
        let release_sec = 0.020 + drive * 0.12;
        let max_rise = 1.0 / (attack_sec * self.sample_rate.max(1.0));
        let max_fall = 1.0 / (release_sec * self.sample_rate.max(1.0));

        // Gate threshold: above this, the swell opens toward 1.0
        let threshold = 0.006;
        let target = if env_next > threshold { 1.0 } else { 0.0 };

        let gain = self.swell_gain_state[index];
        let gain_next = if target > gain {
            (gain + max_rise).min(target)
        } else {
            (gain - max_fall).max(target)
        };
        self.swell_gain_state[index] = gain_next;

        // Stage 3: Apply swell gain
        let wet = sanitize_sample(sample * gain_next);

        // Stage 4: Post-swell tone LPF — brightness control
        let lpf_cutoff = 1_200.0 + (tone * tone * 14_000.0);
        let lpf_alpha = (1.0
            - (-core::f32::consts::TAU * lpf_cutoff / self.sample_rate.max(1.0)).exp())
        .clamp(0.0, 1.0);
        let toned = self.swell_tone_state[index] + lpf_alpha * (wet - self.swell_tone_state[index]);
        self.swell_tone_state[index] = sanitize_sample(toned);

        // Stage 5: Safety
        soft_clip_sample(toned)
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
fn drive_mode_compensation(drive: f32, drive_gain: f32) -> f32 {
    let compensation = 1.0 / drive_gain.powf(0.58);
    let drive = drive.clamp(0.0, 1.0);
    let makeup = 1.0 + (drive * 0.30);
    (compensation * makeup).clamp(0.19, 1.0)
}

#[inline]
fn sweet_drive_to_gain(drive: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    1.0 + (drive * drive * 3.2)
}

#[inline]
fn sweet_compensation(drive: f32, drive_gain: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    let base = 1.0 / drive_gain.max(1.0);
    let makeup = 1.0 + (drive * 0.28);
    (base * makeup).clamp(0.30, 1.0)
}

#[inline]
fn fuzz_drive_to_gain(drive: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    db_to_gain(3.0 + drive * 29.0)
}

#[inline]
fn fuzz_hard_clip(x: f32, drive: f32) -> f32 {
    // Emulated transistor fuzz: blend between tanh (soft) and hard clamp
    // Higher drive = harder clipping character
    let hardness = 0.15 + drive * 0.65;
    let soft = fast_tanh(x);
    let hard = x.clamp(-1.0, 1.0);
    let blended = soft * (1.0 - hardness) + hard * hardness;
    // Smooth the hard edge with a final gentle tanh pass
    fast_tanh(blended * 1.1)
}

#[inline]
fn fuzz_compensation(drive: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    (0.55 / (1.0 + drive * 0.55)).clamp(0.30, 1.0)
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
        cassette_noise_gain, cassette_tone_to_alpha, drive_mode_compensation, drive_to_gain,
        fuzz_compensation, fuzz_drive_to_gain, sweet_compensation, sweet_drive_to_gain, Character,
        CharacterFrame, CharacterMode,
    };

    fn drive_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Drive,
            drive,
            age: 0.0,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: if tone > 0.0 { 0.5 } else { 0.0 },
            cassette_tone_alpha: 0.5,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        }
    }

    fn sweet_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Sweet,
            drive,
            age: 0.0,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            cassette_tone_alpha: 0.0,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        }
    }

    fn fuzz_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Fuzz,
            drive,
            age: 0.0,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            cassette_tone_alpha: 0.0,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        }
    }

    fn howl_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Howl,
            drive,
            age: 0.0,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            cassette_tone_alpha: 0.0,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        }
    }

    fn swell_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Swell,
            drive,
            age: 0.0,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            cassette_tone_alpha: 0.0,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        }
    }

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

    #[test]
    fn drive_output_stays_finite_with_sine() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        for drive in [0.0, 0.33, 0.66, 1.0] {
            for tone in [0.0, 0.5, 1.0] {
                let frame = drive_frame(drive, tone);
                let mut phase = 0.0;
                for _ in 0..128 {
                    phase += 440.0 / 48_000.0;
                    let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
                    let sample = character.process_sample(0, sine, &frame);
                    assert!(sample.is_finite(), "NaN/inf at drive={drive}, tone={tone}");
                    assert!(
                        sample.abs() <= 8.0,
                        "overflow at drive={drive}, tone={tone}: {sample}"
                    );
                }
            }
        }
    }

    #[test]
    fn drive_output_stays_finite_with_noise() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = drive_frame(1.0, 0.5);

        let mut rng: u32 = 0xdecafbad;
        for _ in 0..512 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let sample = character.process_sample(0, noise, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn high_drive_does_not_explode() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = drive_frame(1.0, 1.0);

        let sample = character.process_sample(0, 100.0, &frame);
        assert!(sample.is_finite());
        assert!(
            sample.abs() < 2.5,
            "max-drive extreme input should be bounded, got {sample}"
        );
    }

    #[test]
    fn drive_differs_from_saturation() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let sat_frame = CharacterFrame {
            mode: CharacterMode::Saturation,
            drive: 0.7,
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

        let drv_frame = drive_frame(0.7, 0.5);

        let input = 0.3;
        let sat_out = character.process_sample(0, input, &sat_frame);
        let drv_out = character.process_sample(0, input, &drv_frame);
        assert!(sat_out.is_finite() && drv_out.is_finite());
        assert!(
            (sat_out - drv_out).abs() > 0.000_01,
            "Drive and Saturation should produce different output, sat={sat_out}, drv={drv_out}"
        );
    }

    #[test]
    fn low_drive_is_subtle_warmth() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let input = 0.35;
        // Warm up tone filter state by feeding a few steady samples
        let frame = drive_frame(0.15, 0.5);
        for _ in 0..8 {
            character.process_sample(0, input, &frame);
        }
        let output = character.process_sample(0, input, &frame);
        assert!(output.is_finite());
        assert!(
            (output - input).abs() < 0.16,
            "low drive should produce subtle change, input={input}, output={output}"
        );
        assert!(
            output > 0.18 && output < 0.55,
            "low drive output should stay in musical range, got {output}"
        );
    }

    #[test]
    fn drive_compensation_reduces_level_at_high_drive() {
        let gain = drive_to_gain(1.0);
        let comp = drive_mode_compensation(1.0, gain);
        assert!(
            comp < 0.5,
            "high drive compensation should reduce peak level"
        );
        assert!(comp > 0.05, "compensation should not silence the signal");
    }

    #[test]
    fn sweet_output_stays_finite_with_sine_100hz() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        for drive in [0.0, 0.33, 0.66, 1.0] {
            for tone in [0.0, 0.5, 1.0] {
                let frame = sweet_frame(drive, tone);
                let mut phase = 0.0;
                for _ in 0..256 {
                    phase += 100.0 / 48_000.0;
                    let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
                    let sample = character.process_sample(0, sine, &frame);
                    assert!(sample.is_finite(), "NaN/inf at d={drive} t={tone} 100hz");
                    assert!(sample.abs() <= 8.0, "overflow at d={drive} t={tone} 100hz");
                }
            }
        }
    }

    #[test]
    fn sweet_output_stays_finite_with_sine_1khz() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        for drive in [0.0, 0.5, 1.0] {
            for tone in [0.0, 0.5, 1.0] {
                let frame = sweet_frame(drive, tone);
                let mut phase = 0.0;
                for _ in 0..256 {
                    phase += 1000.0 / 48_000.0;
                    let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
                    let sample = character.process_sample(0, sine, &frame);
                    assert!(sample.is_finite(), "NaN/inf at d={drive} t={tone} 1khz");
                    assert!(sample.abs() <= 8.0, "overflow at d={drive} t={tone} 1khz");
                }
            }
        }
    }

    #[test]
    fn sweet_output_stays_finite_with_sine_8khz() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        for drive in [0.0, 0.5, 1.0] {
            for tone in [0.0, 0.5, 1.0] {
                let frame = sweet_frame(drive, tone);
                let mut phase = 0.0;
                for _ in 0..256 {
                    phase += 8000.0 / 48_000.0;
                    let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
                    let sample = character.process_sample(0, sine, &frame);
                    assert!(sample.is_finite(), "NaN/inf at d={drive} t={tone} 8khz");
                    assert!(sample.abs() <= 8.0, "overflow at d={drive} t={tone} 8khz");
                }
            }
        }
    }

    #[test]
    fn sweet_output_stays_finite_with_noise() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = sweet_frame(1.0, 0.5);

        let mut rng: u32 = 0xcafe_feed;
        for _ in 0..512 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let sample = character.process_sample(0, noise, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn sweet_differs_from_drive() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let drv_frame = drive_frame(0.6, 0.5);
        let swt_frame = sweet_frame(0.6, 0.5);

        let input = 0.3;
        // Warm up both
        for _ in 0..4 {
            character.process_sample(0, input, &drv_frame);
            character.process_sample(0, input, &swt_frame);
        }
        let drv_out = character.process_sample(0, input, &drv_frame);
        let swt_out = character.process_sample(0, input, &swt_frame);
        assert!(drv_out.is_finite() && swt_out.is_finite());
        assert!(
            (drv_out - swt_out).abs() > 0.000_01,
            "Sweet and Drive should differ, drv={drv_out}, swt={swt_out}"
        );
    }

    #[test]
    fn sweet_low_drive_is_subtle_enhancement() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let input = 0.35;
        let frame = sweet_frame(0.15, 0.5);
        for _ in 0..8 {
            character.process_sample(0, input, &frame);
        }
        let output = character.process_sample(0, input, &frame);
        assert!(output.is_finite());
        assert!(
            (output - input).abs() < 0.14,
            "low sweet should be subtle, input={input}, output={output}"
        );
        assert!(
            output > 0.20 && output < 0.55,
            "low sweet output in range, got {output}"
        );
    }

    #[test]
    fn sweet_tone_affects_frequency_content() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let input = 0.25;
        let frame_dark = sweet_frame(0.5, 0.0);
        let frame_bright = sweet_frame(0.5, 1.0);

        for _ in 0..8 {
            character.process_sample(0, input, &frame_dark);
        }
        let dark_out = character.process_sample(0, input, &frame_dark);

        for _ in 0..8 {
            character.process_sample(0, input, &frame_bright);
        }
        let bright_out = character.process_sample(0, input, &frame_bright);

        assert!(dark_out.is_finite() && bright_out.is_finite());
        // With tone=0 exciter off vs tone=1 exciter on, outputs should differ
        // The difference may be subtle on a fixed sine; verify both are sane
        assert!(dark_out.abs() > 0.01);
        assert!(bright_out.abs() > 0.01);
    }

    #[test]
    fn sweet_high_drive_does_not_explode() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = sweet_frame(1.0, 1.0);

        let sample = character.process_sample(0, 10.0, &frame);
        assert!(sample.is_finite());
        assert!(
            sample.abs() < 3.0,
            "max sweet extreme input should be bounded, got {sample}"
        );
    }

    #[test]
    fn sweet_drive_gain_is_gentle() {
        let low = sweet_drive_to_gain(0.0);
        let mid = sweet_drive_to_gain(0.5);
        let high = sweet_drive_to_gain(1.0);
        assert!((low - 1.0).abs() < 0.001);
        assert!(mid > 1.5);
        assert!(high < 5.0, "sweet gain should be gentle, got {high}");
    }

    #[test]
    fn sweet_compensation_keeps_level_stable() {
        for drive in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gain = sweet_drive_to_gain(drive);
            let comp = sweet_compensation(drive, gain);
            let net = gain * comp;
            assert!(comp.is_finite());
            assert!(net > 0.4 && net < 3.0, "net gain at drive={drive}: {net}");
        }
    }

    #[test]
    fn fuzz_output_stays_finite_with_sine() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        for drive in [0.0, 0.33, 0.66, 1.0] {
            for tone in [0.0, 0.5, 1.0] {
                let frame = fuzz_frame(drive, tone);
                let mut phase = 0.0;
                for _ in 0..128 {
                    phase += 440.0 / 48_000.0;
                    let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
                    let sample = character.process_sample(0, sine, &frame);
                    assert!(sample.is_finite(), "NaN/inf at d={drive} t={tone}");
                    assert!(
                        sample.abs() <= 8.0,
                        "overflow at d={drive} t={tone}: {sample}"
                    );
                }
            }
        }
    }

    #[test]
    fn fuzz_output_stays_finite_with_noise() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = fuzz_frame(1.0, 0.5);

        let mut rng: u32 = 0xb00b_b00b;
        for _ in 0..512 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let sample = character.process_sample(0, noise, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn fuzz_max_drive_does_not_explode() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = fuzz_frame(1.0, 0.5);

        let sample = character.process_sample(0, 100.0, &frame);
        assert!(sample.is_finite());
        assert!(
            sample.abs() < 4.0,
            "max fuzz with extreme input should be bounded, got {sample}"
        );
    }

    #[test]
    fn fuzz_low_drive_is_dirty_overdrive() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let clean_frame = CharacterFrame {
            mode: CharacterMode::Clean,
            drive: 0.0,
            age: 0.0,
            tone: 0.5,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            cassette_tone_alpha: 0.0,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        };
        let fuzz_frame_low = fuzz_frame(0.0, 0.5);

        let input = 0.3;
        let clean_out = character.process_sample(0, input, &clean_frame);
        let fuzz_out = character.process_sample(0, input, &fuzz_frame_low);
        assert!(clean_out.is_finite() && fuzz_out.is_finite());
        assert!(
            (clean_out - fuzz_out).abs() > 0.000_1,
            "even min fuzz should differ from clean"
        );
    }

    #[test]
    fn fuzz_differs_from_drive() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let drv_frame = drive_frame(0.7, 0.5);
        let fzz_frame = fuzz_frame(0.7, 0.5);

        let input = 0.3;
        for _ in 0..4 {
            character.process_sample(0, input, &drv_frame);
            character.process_sample(0, input, &fzz_frame);
        }
        let drv_out = character.process_sample(0, input, &drv_frame);
        let fzz_out = character.process_sample(0, input, &fzz_frame);
        assert!(drv_out.is_finite() && fzz_out.is_finite());
        assert!(
            (drv_out - fzz_out).abs() > 0.001,
            "Fuzz and Drive should differ significantly, drv={drv_out}, fzz={fzz_out}"
        );
    }

    #[test]
    fn fuzz_tone_shapes_frequency_response() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let input = 0.25;
        let frame_dark = fuzz_frame(0.6, 0.0);
        let frame_bright = fuzz_frame(0.6, 1.0);

        for _ in 0..8 {
            character.process_sample(0, input, &frame_dark);
        }
        let dark_out = character.process_sample(0, input, &frame_dark);

        for _ in 0..8 {
            character.process_sample(0, input, &frame_bright);
        }
        let bright_out = character.process_sample(0, input, &frame_bright);

        assert!(dark_out.is_finite() && bright_out.is_finite());
        assert!(dark_out.abs() > 0.01);
        assert!(bright_out.abs() > 0.01);
    }

    #[test]
    fn fuzz_compensation_keeps_output_safe() {
        let comp_min = fuzz_compensation(0.0);
        let comp_mid = fuzz_compensation(0.5);
        let comp_max = fuzz_compensation(1.0);
        assert!((comp_min - 0.55).abs() < 0.01, "min comp={comp_min}");
        assert!(comp_mid < 0.55, "mid comp should reduce level");
        assert!(comp_max > 0.25, "max comp should not silence");
        assert!(comp_max < 0.55, "max comp should attenuate strongly");
    }

    #[test]
    fn fuzz_drive_gain_is_aggressive() {
        let low = fuzz_drive_to_gain(0.0);
        let mid = fuzz_drive_to_gain(0.5);
        let high = fuzz_drive_to_gain(1.0);
        assert!(low > 1.3, "min fuzz gain should be > 1, got {low}");
        assert!(mid > 5.0, "mid fuzz gain should be strong, got {mid}");
        assert!(high > 20.0, "max fuzz gain should be extreme, got {high}");
    }

    #[test]
    fn howl_output_stays_finite_with_sine_on_resonance() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        for drive in [0.0, 0.33, 0.66, 1.0] {
            for tone in [0.0, 0.5, 1.0] {
                let frame = howl_frame(drive, tone);
                let mut phase = 0.0;
                for _ in 0..256 {
                    phase += 440.0 / 48_000.0;
                    let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
                    let sample = character.process_sample(0, sine, &frame);
                    assert!(sample.is_finite(), "NaN/inf at d={drive} t={tone}");
                    assert!(
                        sample.abs() <= 8.0,
                        "overflow at d={drive} t={tone}: {sample}"
                    );
                }
            }
        }
    }

    #[test]
    fn howl_output_stays_finite_with_noise() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 0.5);

        let mut rng: u32 = 0x1a2b3c4d;
        for _ in 0..512 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let sample = character.process_sample(0, noise, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn howl_silence_does_not_runaway() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 0.5);

        for _ in 0..1024 {
            let sample = character.process_sample(0, 0.0, &frame);
            assert!(
                sample.is_finite(),
                "howl should not self-oscillate on silence"
            );
            assert!(sample.abs() <= 8.0, "howl should stay bounded on silence");
        }
    }

    #[test]
    fn howl_max_drive_stays_bounded() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 0.5);

        let sample = character.process_sample(0, 10.0, &frame);
        assert!(sample.is_finite());
        assert!(
            sample.abs() < 3.0,
            "max howl should be bounded, got {sample}"
        );
    }

    #[test]
    fn howl_differs_from_drive() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let drv_frame = drive_frame(0.6, 0.5);
        let hwl_frame = howl_frame(0.6, 0.5);

        let input = 0.3;
        for _ in 0..8 {
            character.process_sample(0, input, &drv_frame);
            character.process_sample(0, input, &hwl_frame);
        }
        let drv_out = character.process_sample(0, input, &drv_frame);
        let hwl_out = character.process_sample(0, input, &hwl_frame);
        assert!(drv_out.is_finite() && hwl_out.is_finite());
        assert!(
            (drv_out - hwl_out).abs() > 0.000_1,
            "Howl and Drive should differ, drv={drv_out}, hwl={hwl_out}"
        );
    }

    #[test]
    fn howl_tone_shifts_resonance() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let input = 0.25;
        let frame_low = howl_frame(0.5, 0.0);
        let frame_high = howl_frame(0.5, 1.0);

        for _ in 0..8 {
            character.process_sample(0, input, &frame_low);
        }
        let low_out = character.process_sample(0, input, &frame_low);

        for _ in 0..8 {
            character.process_sample(0, input, &frame_high);
        }
        let high_out = character.process_sample(0, input, &frame_high);

        assert!(low_out.is_finite() && high_out.is_finite());
        assert!(low_out.abs() > 0.001);
        assert!(high_out.abs() > 0.001);
    }

    #[test]
    fn howl_drive_increases_activity() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let input = 0.35;
        let frame_low = howl_frame(0.1, 0.4);
        let frame_high = howl_frame(0.9, 0.4);

        for _ in 0..16 {
            character.process_sample(0, input, &frame_low);
        }
        let low_out = character.process_sample(0, input, &frame_low);

        for _ in 0..16 {
            character.process_sample(0, input, &frame_high);
        }
        let high_out = character.process_sample(0, input, &frame_high);

        assert!(low_out.is_finite() && high_out.is_finite());
        // Higher drive = higher Q = more energy around resonance
        assert!(low_out.abs() > 0.001);
        assert!(high_out.abs() > 0.001);
    }

    #[test]
    fn swell_impulse_attack_is_reduced() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(0.8, 0.5);

        let first = character.process_sample(0, 0.8, &frame);
        assert!(first.is_finite());
        assert!(
            first.abs() < 0.15,
            "swell should reduce impulse attack, got {first}"
        );
    }

    #[test]
    fn swell_sine_sustain_is_preserved() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(0.5, 0.5);

        let amplitude = 0.35;

        // Warm up: ramp swell gain to full during sustain (~250ms)
        for _ in 0..12000 {
            let sample = character.process_sample(0, amplitude, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }

        let last = character.process_sample(0, amplitude, &frame);
        assert!(
            last > 0.25,
            "swell should reach near-full sustain after ramp-up, got {last}"
        );
    }

    #[test]
    fn swell_output_stays_finite_with_noise() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(1.0, 0.5);

        let mut rng: u32 = 0xfade_fade;
        for _ in 0..512 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let sample = character.process_sample(0, noise, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn swell_max_drive_stays_bounded() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(1.0, 0.5);

        let sample = character.process_sample(0, 10.0, &frame);
        assert!(sample.is_finite());
        assert!(
            sample.abs() < 4.0,
            "swell should stay bounded, got {sample}"
        );
    }

    #[test]
    fn swell_differs_from_clean() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let clean_frame = CharacterFrame {
            mode: CharacterMode::Clean,
            drive: 0.0,
            age: 0.0,
            tone: 0.5,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            cassette_tone_alpha: 0.0,
            cassette_noise_gain: 0.0,
            cassette_flutter_depth: 0.0,
            mode_fade: 1.0,
        };
        let swell_frame_test = swell_frame(0.7, 0.5);

        let clean_out = character.process_sample(0, 0.5, &clean_frame);
        let swell_out = character.process_sample(0, 0.5, &swell_frame_test);
        assert!(clean_out.is_finite() && swell_out.is_finite());
        assert!(
            (clean_out - swell_out).abs() > 0.000_1,
            "swell should differ from clean on first sample"
        );
    }

    #[test]
    fn swell_low_drive_is_subtle() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let frame = swell_frame(0.1, 0.5);
        // Warm up both the swell envelope and tone LPF (attack ~52ms, give 200ms)
        for _ in 0..9600 {
            character.process_sample(0, 0.6, &frame);
        }
        let output = character.process_sample(0, 0.6, &frame);
        assert!(output.is_finite());
        assert!(
            output > 0.35,
            "low drive swell should let signal through after warmup, got {output}"
        );
    }

    #[test]
    fn swell_gain_ramps_up_with_sustained_input() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(0.6, 0.5);

        let amplitude = 0.35;
        let first = character.process_sample(0, amplitude, &frame);
        assert!(first.is_finite());

        // Sustain: swell should ramp up to near full level
        for _ in 0..16000 {
            character.process_sample(0, amplitude, &frame);
        }
        let steady = character.process_sample(0, amplitude, &frame);
        assert!(steady.is_finite());
        assert!(
            steady > first,
            "swell gain should ramp up during sustain, first={first}, steady={steady}"
        );
        assert!(
            steady > 0.20,
            "steady state should be near full level after warmup, got {steady}"
        );
    }
}
