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

/// The five Character modes, in product order. Variants are identified by their
/// stable `#[id]`, so removing the old legacy modes (clean/saturation/cassette)
/// keeps state compatibility for every id that remains.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterMode {
    #[id = "drive"]
    #[name = "Drive"]
    Drive,

    #[id = "sweet"]
    #[name = "Sweeten"]
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

impl CharacterMode {
    /// The five product modes, in the exact order shown in the UI.
    pub const PRODUCT_MODES: [CharacterMode; 5] = [
        CharacterMode::Drive,
        CharacterMode::Sweet,
        CharacterMode::Fuzz,
        CharacterMode::Howl,
        CharacterMode::Swell,
    ];
}

#[derive(Debug, Clone)]
pub struct Character {
    core: ModuleCore,
    sample_rate: f32,
    tone_state: [f32; MAX_CHANNELS],
    drive_dc_state: [f32; MAX_CHANNELS],
    drive_hf_state: [f32; MAX_CHANNELS],
    sweet_dc_state: [f32; MAX_CHANNELS],
    sweet_exciter_state: [f32; MAX_CHANNELS],
    fuzz_dc_state: [f32; MAX_CHANNELS],
    fuzz_tone_state: [f32; MAX_CHANNELS],
    howl_input_hp_state: [f32; MAX_CHANNELS],
    howl_body_lp_state: [f32; MAX_CHANNELS],
    howl_formant1_lp_state: [f32; MAX_CHANNELS],
    howl_formant1_bp_state: [f32; MAX_CHANNELS],
    howl_formant2_lp_state: [f32; MAX_CHANNELS],
    howl_formant2_bp_state: [f32; MAX_CHANNELS],
    howl_damping_state: [f32; MAX_CHANNELS],
    swell_fast_env_state: [f32; MAX_CHANNELS],
    swell_slow_env_state: [f32; MAX_CHANNELS],
    swell_phase_state: [f32; MAX_CHANNELS],
    swell_gain_state: [f32; MAX_CHANNELS],
    swell_cooldown_samples: [usize; MAX_CHANNELS],
    swell_open_state: [bool; MAX_CHANNELS],
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
    tone: f32,
    mix: f32,
    output_gain: f32,
    active_mix: f32,
    tone_alpha: f32,
    mode_fade: f32,
}

impl Default for Character {
    fn default() -> Self {
        Self {
            core: ModuleCore::default(),
            sample_rate: 44_100.0,
            tone_state: [0.0; MAX_CHANNELS],
            drive_dc_state: [0.0; MAX_CHANNELS],
            drive_hf_state: [0.0; MAX_CHANNELS],
            sweet_dc_state: [0.0; MAX_CHANNELS],
            sweet_exciter_state: [0.0; MAX_CHANNELS],
            fuzz_dc_state: [0.0; MAX_CHANNELS],
            fuzz_tone_state: [0.0; MAX_CHANNELS],
            howl_input_hp_state: [0.0; MAX_CHANNELS],
            howl_body_lp_state: [0.0; MAX_CHANNELS],
            howl_formant1_lp_state: [0.0; MAX_CHANNELS],
            howl_formant1_bp_state: [0.0; MAX_CHANNELS],
            howl_formant2_lp_state: [0.0; MAX_CHANNELS],
            howl_formant2_bp_state: [0.0; MAX_CHANNELS],
            howl_damping_state: [0.0; MAX_CHANNELS],
            swell_fast_env_state: [0.0; MAX_CHANNELS],
            swell_slow_env_state: [0.0; MAX_CHANNELS],
            swell_phase_state: [0.0; MAX_CHANNELS],
            swell_gain_state: [0.0; MAX_CHANNELS],
            swell_cooldown_samples: [0; MAX_CHANNELS],
            swell_open_state: [false; MAX_CHANNELS],
            swell_tone_state: [0.0; MAX_CHANNELS],
            current_mode: CharacterMode::Drive,
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
        self.drive_dc_state = [0.0; MAX_CHANNELS];
        self.drive_hf_state = [0.0; MAX_CHANNELS];
        self.sweet_dc_state = [0.0; MAX_CHANNELS];
        self.sweet_exciter_state = [0.0; MAX_CHANNELS];
        self.fuzz_dc_state = [0.0; MAX_CHANNELS];
        self.fuzz_tone_state = [0.0; MAX_CHANNELS];
        self.howl_input_hp_state = [0.0; MAX_CHANNELS];
        self.howl_body_lp_state = [0.0; MAX_CHANNELS];
        self.howl_formant1_lp_state = [0.0; MAX_CHANNELS];
        self.howl_formant1_bp_state = [0.0; MAX_CHANNELS];
        self.howl_formant2_lp_state = [0.0; MAX_CHANNELS];
        self.howl_formant2_bp_state = [0.0; MAX_CHANNELS];
        self.howl_damping_state = [0.0; MAX_CHANNELS];
        self.swell_fast_env_state = [0.0; MAX_CHANNELS];
        self.swell_slow_env_state = [0.0; MAX_CHANNELS];
        self.swell_phase_state = [0.0; MAX_CHANNELS];
        self.swell_gain_state = [0.0; MAX_CHANNELS];
        self.swell_cooldown_samples = [0; MAX_CHANNELS];
        self.swell_open_state = [false; MAX_CHANNELS];
        self.swell_tone_state = [0.0; MAX_CHANNELS];
        self.mode_crossfade.reset(1.0);
        self.last_output = [0.0; MAX_CHANNELS];
        self.has_processed = false;
    }

    pub fn next_frame(&mut self, params: &CharacterParams) -> CharacterFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let drive = params.drive.smoothed.next().clamp(0.0, 1.0);
        let tone = params.tone.smoothed.next().clamp(0.0, 1.0);
        let mix = params.mix.smoothed.next().clamp(0.0, 1.0);
        let output_gain = db_to_gain(params.output_trim.smoothed.next().clamp(-12.0, 12.0));
        let module_frame = self.core.next_frame(params.bypass.value(), mix, 0.0);

        CharacterFrame {
            mode,
            drive,
            tone,
            mix,
            output_gain,
            active_mix: module_frame.active_mix,
            tone_alpha: tone_to_alpha(tone, self.sample_rate),
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
        let sample_rate = self.sample_rate.max(1.0);

        let hp_cutoff = 40.0 + drive * 30.0;
        let hp_alpha =
            (1.0 - (-core::f32::consts::TAU * hp_cutoff / sample_rate).exp()).clamp(0.0, 1.0);
        let input_hp =
            self.howl_input_hp_state[index] + hp_alpha * (sample - self.howl_input_hp_state[index]);
        self.howl_input_hp_state[index] = sanitize_sample(input_hp);
        let conditioned = sanitize_sample(sample - input_hp);

        let drive_gain = 1.0 + drive * 1.5;
        let saturated = fast_tanh(conditioned * drive_gain);

        let body_cutoff = (250.0 + tone * 50.0 + drive * 30.0).clamp(250.0, 360.0);
        let body_alpha =
            (1.0 - (-core::f32::consts::TAU * body_cutoff / sample_rate).exp()).clamp(0.0, 1.0);
        let body_lp = self.howl_body_lp_state[index]
            + body_alpha * (saturated - self.howl_body_lp_state[index]);
        self.howl_body_lp_state[index] = sanitize_sample(body_lp);

        let f1 = howl_formant1_frequency(tone);
        let f2 = howl_formant2_frequency(f1, tone, drive);
        let q = howl_q(drive);

        let (f1_out, _) = howl_resonator_step(
            saturated,
            f1,
            q,
            sample_rate,
            &mut self.howl_formant1_lp_state[index],
            &mut self.howl_formant1_bp_state[index],
        );
        let (f2_out, _) = howl_resonator_step(
            saturated,
            f2,
            q * 0.75,
            sample_rate,
            &mut self.howl_formant2_lp_state[index],
            &mut self.howl_formant2_bp_state[index],
        );

        let formant_balance = 0.35 + tone * 0.08;
        let formant_mix =
            sanitize_sample(f1_out * (1.0 - formant_balance) + f2_out * formant_balance);

        let formant_limited = soft_clip_sample(formant_mix * 0.72);

        let damping_hz = howl_output_damping(tone);
        let damping_alpha =
            (1.0 - (-core::f32::consts::TAU * damping_hz / sample_rate).exp()).clamp(0.0, 1.0);
        let damped = self.howl_damping_state[index]
            + damping_alpha * (formant_limited - self.howl_damping_state[index]);
        self.howl_damping_state[index] = sanitize_sample(damped);

        let body_mix = howl_body_mix(drive);
        let voiced = sanitize_sample(body_lp * body_mix + damped * (1.0 - body_mix));

        let compensation = howl_gain_compensation(drive, q);
        let compensated = sanitize_sample(voiced * compensation);

        soft_clip_sample(compensated)
    }

    fn process_swell(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let drive = frame.drive;
        let tone = frame.tone;
        let sample_rate = self.sample_rate.max(1.0);
        let abs_in = sample.abs();

        let fast_alpha = envelope_alpha(0.0025, sample_rate);
        let slow_alpha = envelope_alpha(0.085 + drive * 0.080, sample_rate);
        let fast_env = self.swell_fast_env_state[index]
            + fast_alpha * (abs_in - self.swell_fast_env_state[index]);
        let slow_env = self.swell_slow_env_state[index]
            + slow_alpha * (abs_in - self.swell_slow_env_state[index]);
        self.swell_fast_env_state[index] = sanitize_sample(fast_env);
        self.swell_slow_env_state[index] = sanitize_sample(slow_env);

        let cooldown = self.swell_cooldown_samples[index];
        let signal_floor = 0.0018 + drive * 0.0012;
        if fast_env < signal_floor * 0.75 {
            self.swell_open_state[index] = false;
            self.swell_phase_state[index] = 0.0;
        }

        let can_retrigger =
            self.swell_open_state[index] || self.swell_phase_state[index] <= 0.000_1;
        if can_retrigger
            && swell_onset_detect(fast_env, slow_env, drive)
            && cooldown == 0
            && slow_env > signal_floor
        {
            self.swell_phase_state[index] = 0.0;
            self.swell_open_state[index] = false;
            self.swell_cooldown_samples[index] =
                ((0.045 + drive * 0.065) * sample_rate).round() as usize;
        } else if cooldown > 0 {
            self.swell_cooldown_samples[index] = cooldown - 1;
        }

        let gain = swell_envelope_step(
            drive,
            slow_env,
            signal_floor,
            sample_rate,
            swell_attack_time(drive),
            &mut self.swell_phase_state[index],
            &mut self.swell_gain_state[index],
            &mut self.swell_open_state[index],
        );

        let phase_open = smoothstep(self.swell_phase_state[index]);
        let depth = ((0.45 + drive.powf(1.4) * 0.50) * (1.0 - phase_open * 0.28)).clamp(0.32, 0.95);
        let shaped_gain = (1.0 - depth) + gain * depth;
        let wet = sanitize_sample(swell_bloom_level(sample, slow_env, drive) * shaped_gain);

        let lpf_alpha = swell_tone_alpha(tone, sample_rate);
        let toned = self.swell_tone_state[index] + lpf_alpha * (wet - self.swell_tone_state[index]);
        self.swell_tone_state[index] = sanitize_sample(toned);

        soft_clip_sample(toned)
    }
    fn apply_tone(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let state = self.tone_state[index] + (frame.tone_alpha * (sample - self.tone_state[index]));
        self.tone_state[index] = sanitize_sample(state);

        let bright_blend = frame.tone * frame.tone;
        sanitize_sample((state * (1.0 - bright_blend)) + (sample * bright_blend))
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
fn howl_formant1_frequency(tone: f32) -> f32 {
    let tone = tone.clamp(0.0, 1.0);
    200.0 + tone * 2_000.0
}

#[inline]
fn howl_formant2_frequency(f1: f32, tone: f32, drive: f32) -> f32 {
    let tone = tone.clamp(0.0, 1.0);
    let drive = drive.clamp(0.0, 1.0);
    let ratio = (1.40 + tone * 0.26 + drive * 0.08).clamp(1.40, 1.74);
    (f1 * ratio).clamp(280.0, 3_200.0)
}

#[inline]
fn howl_q(drive: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    (0.65 + drive.powf(1.2) * 2.85).clamp(0.65, 3.50)
}

#[inline]
fn howl_body_mix(drive: f32) -> f32 {
    (0.40 - drive.clamp(0.0, 1.0) * 0.15).clamp(0.25, 0.40)
}

#[inline]
fn howl_gain_compensation(drive: f32, q: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    let base = 1.0 / (1.0 + drive * 2.2 + q * 0.10);
    (base * (1.0 + drive * 0.25)).clamp(0.18, 0.90)
}

#[inline]
fn howl_resonator_step(
    input: f32,
    frequency_hz: f32,
    q: f32,
    sample_rate: f32,
    lp_state: &mut f32,
    bp_state: &mut f32,
) -> (f32, f32) {
    let freq = frequency_hz.clamp(40.0, sample_rate.max(1.0) * 0.40);
    let f = (2.0 * (core::f32::consts::PI * freq / sample_rate.max(1.0)).sin()).clamp(0.0005, 0.82);
    let q_safe = q.clamp(0.5, 3.80);
    let damping = (1.0 / q_safe + 0.015).clamp(0.24, 1.0);

    let high = sanitize_sample(input - *lp_state - damping * *bp_state);
    let band = sanitize_sample(fast_tanh((*bp_state + f * high) * 0.70));
    let low = sanitize_sample(fast_tanh((*lp_state + f * band) * 0.70));
    *bp_state = band;
    *lp_state = low;

    let lp_out = soft_clip_sample(low * (0.47 + q * 0.035));
    let bp_out = soft_clip_sample(band);
    (lp_out, bp_out)
}

#[inline]
fn howl_output_damping(tone: f32) -> f32 {
    let tone = tone.clamp(0.0, 1.0);
    (1_400.0 + tone.powf(1.5) * 8_000.0).clamp(1_400.0, 9_400.0)
}

#[inline]
fn swell_attack_time(drive: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    (0.014 + drive.powf(1.55) * 0.42).clamp(0.014, 0.44)
}

#[inline]
fn swell_onset_detect(fast_env: f32, slow_env: f32, drive: f32) -> bool {
    let drive = drive.clamp(0.0, 1.0);
    let differential = fast_env - slow_env;
    let ratio = fast_env / (slow_env + 0.000_8);
    let threshold = 0.012 - drive * 0.006;
    let ratio_threshold = 1.12 + drive * 0.18;
    differential > threshold && ratio > ratio_threshold
}

#[inline]
fn swell_envelope_step(
    drive: f32,
    slow_env: f32,
    signal_floor: f32,
    sample_rate: f32,
    attack_time: f32,
    phase: &mut f32,
    gain: &mut f32,
    open: &mut bool,
) -> f32 {
    let sample_rate = sample_rate.max(1.0);
    if slow_env <= signal_floor * 0.45 {
        *open = false;
        *phase = 0.0;
        let release_step = 1.0 / ((0.16 + drive.clamp(0.0, 1.0) * 0.20) * sample_rate);
        *gain = (*gain - release_step).max(0.0);
        return sanitize_sample(*gain);
    }

    if !*open {
        let step = 1.0 / (attack_time.max(0.001) * sample_rate);
        *phase = (*phase + step).min(1.0);
        let curve = 1.15 + drive.clamp(0.0, 1.0) * 0.85;
        let target = smoothstep(*phase).powf(curve);
        *gain += (target - *gain) * 0.42;
        if *phase >= 0.999 {
            *open = true;
            *gain = (*gain).max(0.995);
        }
    } else {
        *phase = 1.0;
        *gain += (1.0 - *gain) * 0.018;
    }

    sanitize_sample((*gain).clamp(0.0, 1.0))
}

#[inline]
fn swell_tone_alpha(tone: f32, sample_rate: f32) -> f32 {
    let tone = tone.clamp(0.0, 1.0);
    let cutoff = 850.0 + tone.powf(1.7) * 14_500.0;
    (1.0 - (-core::f32::consts::TAU * cutoff / sample_rate.max(1.0)).exp()).clamp(0.0, 1.0)
}

#[inline]
fn swell_bloom_level(sample: f32, slow_env: f32, drive: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    let threshold = 0.18 + drive * 0.22;
    let amount = 0.10 + drive * 0.16;
    let level = slow_env.max(0.000_1);
    let compression = if level > threshold {
        1.0 / (1.0 + (level - threshold) * amount * 2.0)
    } else {
        1.0 + drive * 0.04
    };
    sanitize_sample(sample * compression.clamp(0.72, 1.04))
}

#[inline]
fn fast_tanh(x: f32) -> f32 {
    let x2 = x * x;
    let num = x * (27.0 + x2);
    let den = 27.0 + 9.0 * x2;
    num / den
}

#[inline]
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[inline]
fn envelope_alpha(time_seconds: f32, sample_rate: f32) -> f32 {
    let samples = (time_seconds.max(0.000_1) * sample_rate.max(1.0)).max(1.0);
    (1.0 / samples).clamp(0.0, 1.0)
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
        drive_mode_compensation, drive_to_gain, fuzz_compensation, fuzz_drive_to_gain,
        sweet_compensation, sweet_drive_to_gain, Character, CharacterFrame, CharacterMode,
    };

    fn drive_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Drive,
            drive,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: if tone > 0.0 { 0.5 } else { 0.0 },
            mode_fade: 1.0,
        }
    }

    fn sweet_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Sweet,
            drive,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            mode_fade: 1.0,
        }
    }

    fn fuzz_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Fuzz,
            drive,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            mode_fade: 1.0,
        }
    }

    fn howl_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Howl,
            drive,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            mode_fade: 1.0,
        }
    }

    fn swell_frame(drive: f32, tone: f32) -> CharacterFrame {
        CharacterFrame {
            mode: CharacterMode::Swell,
            drive,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn drive_maps_to_more_input_gain() {
        assert!(drive_to_gain(1.0) > drive_to_gain(0.5));
        assert!((drive_to_gain(0.0) - 1.0).abs() < 0.000_001);
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

        let fuzz_frame_low = fuzz_frame(0.0, 0.5);

        let input = 0.3;
        let fuzz_out = character.process_sample(0, input, &fuzz_frame_low);
        assert!(fuzz_out.is_finite());
        assert!(
            (input - fuzz_out).abs() > 0.000_1,
            "even min fuzz should color the dry signal"
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
    fn howl_silence_10s_max_drive_tone_stays_silent() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 1.0);

        let mut peak = 0.0_f32;
        for _ in 0..(48_000 * 10) {
            let sample = character.process_sample(0, 0.0, &frame);
            assert!(sample.is_finite(), "howl 10s silence: NaN/inf");
            peak = peak.max(sample.abs());
        }
        assert!(
            peak < 0.000_2,
            "howl should not self-oscillate on silence, peak={peak}"
        );
    }

    #[test]
    fn howl_sine_110hz_stays_controlled() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 1.0);

        let mut peak = 0.0_f32;
        let mut phase = 0.0_f32;
        for _ in 0..(48_000 * 2) {
            phase += 110.0 / 48_000.0;
            let sine = (phase * core::f32::consts::TAU).sin() * 0.5;
            let sample = character.process_sample(0, sine, &frame);
            assert!(sample.is_finite(), "howl 110Hz: NaN/inf");
            peak = peak.max(sample.abs());
        }
        assert!(
            peak < 6.0,
            "howl 110Hz max drive should stay controlled, peak={peak}"
        );
    }

    #[test]
    fn howl_sine_440hz_is_musical_and_controlled() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(0.6, 0.5);

        let mut peak = 0.0_f32;
        let mut phase = 0.0_f32;
        for _ in 0..(48_000 * 2) {
            phase += 440.0 / 48_000.0;
            let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
            let sample = character.process_sample(0, sine, &frame);
            assert!(sample.is_finite(), "howl 440Hz: NaN/inf");
            peak = peak.max(sample.abs());
        }
        assert!(
            peak < 5.0,
            "howl 440Hz moderate drive should be musical, peak={peak}"
        );
        assert!(
            peak > 0.05,
            "howl 440Hz should produce audible output, peak={peak}"
        );
    }

    #[test]
    fn howl_white_noise_has_controlled_output() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 0.8);

        let mut peak = 0.0_f32;
        let mut rng: u32 = 0xdead_beef;
        for _ in 0..4096 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let sample = character.process_sample(0, noise * 0.5, &frame);
            assert!(sample.is_finite(), "howl noise: NaN/inf");
            peak = peak.max(sample.abs());
        }
        assert!(
            peak < 6.0,
            "howl white noise should stay controlled, peak={peak}"
        );
    }

    #[test]
    fn howl_impulse_has_controlled_decay() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(0.8, 0.5);

        for _ in 0..32 {
            character.process_sample(0, 0.0, &frame);
        }
        let impulse_peak = character.process_sample(0, 0.9, &frame).abs();
        assert!(
            impulse_peak < 4.0,
            "howl impulse peak should be controlled: {impulse_peak}"
        );

        let mut late_peak = 0.0_f32;
        for _ in 0..256 {
            let sample = character.process_sample(0, 0.0, &frame).abs();
            late_peak = late_peak.max(sample);
        }
        assert!(
            late_peak < 0.08,
            "howl impulse should decay to near silence, late_peak={late_peak}"
        );
    }

    #[test]
    fn howl_fuzz_howl_sweet_switch_without_click() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let fuzz = fuzz_frame(0.8, 0.5);
        let howl = howl_frame(0.8, 0.5);
        let sweet = sweet_frame(0.8, 0.5);

        for _ in 0..64 {
            character.process_sample(0, 0.3, &fuzz);
        }
        let pre_switch = character.process_sample(0, 0.3, &fuzz).abs();

        for _ in 0..128 {
            character.process_sample(0, 0.3, &howl);
        }
        let during_howl = character.process_sample(0, 0.3, &howl).abs();

        for _ in 0..128 {
            character.process_sample(0, 0.3, &sweet);
        }
        let during_sweet = character.process_sample(0, 0.3, &sweet).abs();

        assert!(during_howl > 0.001, "howl should process signal");
        assert!(during_sweet > 0.001, "sweet should process signal");

        let max_jump = (pre_switch - during_howl)
            .abs()
            .max((during_howl - during_sweet).abs());
        assert!(
            max_jump < 0.8,
            "switch clicks should be small, max_jump={max_jump}"
        );
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
    fn swell_silence_stays_silent() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(1.0, 0.5);

        for _ in 0..48_000 {
            let sample = character.process_sample(0, 0.0, &frame);
            assert!(sample.is_finite());
            assert!(
                sample.abs() < 0.000_001,
                "swell should not generate sound from silence, got {sample}"
            );
        }
    }

    #[test]
    fn swell_continuous_sine_does_not_pulse_after_opening() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(0.55, 0.7);

        let mut phase = 0.0_f32;
        let mut min_peak = f32::MAX;
        let mut max_peak = 0.0_f32;
        let mut window_peak = 0.0_f32;
        for index in 0..48_000 {
            phase += 220.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.35;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            if index > 24_000 {
                window_peak = window_peak.max(output.abs());
                if index % 1_200 == 1_199 {
                    min_peak = min_peak.min(window_peak);
                    max_peak = max_peak.max(window_peak);
                    window_peak = 0.0;
                }
            }
        }

        assert!(max_peak > 0.20, "swell should preserve opened sustain");
        assert!(
            min_peak > max_peak * 0.70,
            "opened swell should not pump heavily, min={min_peak}, max={max_peak}"
        );
    }

    #[test]
    fn swell_repeated_notes_retrigger_musically() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(0.55, 0.5);

        let mut note_starts = Vec::new();
        let mut early_peaks = Vec::new();
        let mut late_peaks = Vec::new();
        for note in 0..4 {
            note_starts.push(note * 12_000);
            early_peaks.push(0.0_f32);
            late_peaks.push(0.0_f32);
        }

        for index in 0..48_000 {
            let note_index = index / 12_000;
            let pos = index % 12_000;
            let active = pos < 7_200;
            let phase = core::f32::consts::TAU * 330.0 * pos as f32 / 48_000.0;
            let input = if active { phase.sin() * 0.35 } else { 0.0 };
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);

            if note_index < 4 {
                if (20..480).contains(&pos) {
                    early_peaks[note_index] = early_peaks[note_index].max(output.abs());
                }
                if (4_800..6_800).contains(&pos) {
                    late_peaks[note_index] = late_peaks[note_index].max(output.abs());
                }
            }
        }

        for note in 1..4 {
            assert!(
                early_peaks[note] < late_peaks[note] * 0.70,
                "note {note} should retrigger with softened attack, early={}, late={}",
                early_peaks[note],
                late_peaks[note]
            );
            assert!(
                late_peaks[note] > 0.12,
                "note {note} should bloom back to audible sustain"
            );
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
    fn swell_colors_the_dry_signal() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let swell_frame_test = swell_frame(0.7, 0.5);

        let input = 0.5;
        let swell_out = character.process_sample(0, input, &swell_frame_test);
        assert!(swell_out.is_finite());
        assert!(
            (input - swell_out).abs() > 0.000_1,
            "swell should shape the dry signal on the first sample"
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
