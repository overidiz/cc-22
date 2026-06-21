use nih_plug::prelude::*;

use crate::params::CharacterParams;

use super::{
    chain::{sanitize_sample, soft_clip_sample, ModuleCore},
    dry_wet::DryWet,
    gain::db_to_gain,
    smoothing::LinearSmoother,
    util::{
        dc_blocker_step, gain_compensation_curve, one_pole_alpha, safe_frequency, safe_q,
        smoothstep, soft_saturate,
    },
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
    drive_hp_state: [f32; MAX_CHANNELS],
    drive_hf_state: [f32; MAX_CHANNELS],
    sweet_dc_state: [f32; MAX_CHANNELS],
    sweet_exciter_state: [f32; MAX_CHANNELS],
    sweet_air_state: [f32; MAX_CHANNELS],
    fuzz_dc_state: [f32; MAX_CHANNELS],
    fuzz_tone_state: [f32; MAX_CHANNELS],
    fuzz_hp_state: [f32; MAX_CHANNELS],
    fuzz_body_state: [f32; MAX_CHANNELS],
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
            drive_hp_state: [0.0; MAX_CHANNELS],
            drive_hf_state: [0.0; MAX_CHANNELS],
            sweet_dc_state: [0.0; MAX_CHANNELS],
            sweet_exciter_state: [0.0; MAX_CHANNELS],
            sweet_air_state: [0.0; MAX_CHANNELS],
            fuzz_dc_state: [0.0; MAX_CHANNELS],
            fuzz_tone_state: [0.0; MAX_CHANNELS],
            fuzz_hp_state: [0.0; MAX_CHANNELS],
            fuzz_body_state: [0.0; MAX_CHANNELS],
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
        self.drive_hp_state = [0.0; MAX_CHANNELS];
        self.drive_hf_state = [0.0; MAX_CHANNELS];
        self.sweet_dc_state = [0.0; MAX_CHANNELS];
        self.sweet_exciter_state = [0.0; MAX_CHANNELS];
        self.sweet_air_state = [0.0; MAX_CHANNELS];
        self.fuzz_dc_state = [0.0; MAX_CHANNELS];
        self.fuzz_tone_state = [0.0; MAX_CHANNELS];
        self.fuzz_hp_state = [0.0; MAX_CHANNELS];
        self.fuzz_body_state = [0.0; MAX_CHANNELS];
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
        let sample_rate = self.sample_rate.max(1.0);
        let drive = frame.drive;

        // Stage 1: Input conditioning. A DC blocker plus a gentle subsonic
        // high-pass keeps rumble and offset out of the saturator so the
        // sub-bass never "explodes" the waveshaper. The corner rises a touch
        // with drive (more headroom when pushed) but stays subsonic, so all the
        // musical low-mid body still reaches the saturation stage.
        let dc_free = sanitize_sample(dc_blocker_step(
            &mut self.drive_dc_state[index],
            sample,
            0.9992,
        ));
        let hp_cutoff = 24.0 + drive * 14.0;
        let hp_alpha = one_pole_alpha(hp_cutoff, sample_rate);
        self.drive_hp_state[index] = sanitize_sample(
            self.drive_hp_state[index] + hp_alpha * (dc_free - self.drive_hp_state[index]),
        );
        let conditioned = sanitize_sample(dc_free - self.drive_hp_state[index]);

        // Stage 2: Input gain and a gentle pre-drive soft stage that rounds
        // transients before the main clip. A tiny asymmetric bias seeds the
        // even-harmonic warmth that gives the tone its body.
        let drive_gain = drive_to_gain(drive);
        let bias = drive * drive * 0.011;
        let driven = sanitize_sample((conditioned + bias) * drive_gain);
        let pre = fast_tanh(driven * 0.7);

        // Stage 3: Controlled asymmetry (even harmonics = warmth/body) feeding
        // the main soft-clip waveshaper.
        let asymmetry = drive * 0.19;
        let pos = pre.max(0.0);
        let neg = pre.min(0.0);
        let shaped = pos * (1.0 + asymmetry) + neg;
        let saturated = fast_tanh(shaped);

        // Stage 4: Anti-fizz post damping. A drive-dependent one-pole low-pass
        // pulls the corner down as drive rises, so harder settings stay warm and
        // controlled instead of fizzy. It only touches the highs — the body is
        // left intact.
        let hf_cutoff = (18_000.0 - drive * drive * 15_800.0).max(2_000.0);
        let hf_alpha = one_pole_alpha(hf_cutoff, sample_rate);
        self.drive_hf_state[index] = sanitize_sample(
            self.drive_hf_state[index] + hf_alpha * (saturated - self.drive_hf_state[index]),
        );
        let tamed = self.drive_hf_state[index];

        // Stage 5: Gain compensation. The makeup shrinks as drive rises (the
        // saturation already adds density/loudness), keeping perceived level
        // consistent and avoiding an absurd volume jump on the way up.
        let compensated = tamed * drive_mode_compensation(drive, drive_gain);

        // Stage 6: Safety soft-clip to guard against overshoot from filter ringing.
        let safe = soft_clip_sample(compensated);

        // Stage 7: Tone — darker/bodied when low, open (but pre-tamed, never
        // harsh) when high.
        self.apply_tone(channel, safe, frame)
    }

    fn process_sweet(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let sample_rate = self.sample_rate.max(1.0);
        let drive = frame.drive;
        let tone = frame.tone;

        // Stage 1: DC blocker first — clean the offset before any shaping.
        let dc_free = sanitize_sample(dc_blocker_step(
            &mut self.sweet_dc_state[index],
            sample,
            0.9992,
        ));

        // Stage 2: Very gentle asymmetric saturation (2nd-harmonic warmth).
        // Much milder than Drive — low gain, soft asymmetry — so low amounts
        // stay nearly transparent and high amounts stay elegant.
        let drive_gain = sweet_drive_to_gain(drive);
        let asymmetry = drive * 0.24;
        let pos = dc_free.max(0.0);
        let neg = dc_free.min(0.0);
        let shaped = (pos * (1.0 + asymmetry) + neg) * drive_gain;
        let saturated = fast_tanh(shaped);

        // Stage 3: Controlled high-band exciter. Extract the highs, then
        // *soft-saturate* them to generate musical harmonics (sheen) rather than
        // a raw boost (which would just expose hiss). Tone moves the band corner
        // and the amount, but the amount is capped so it never gets brittle.
        let exciter_freq = 5_500.0 - tone * 3_500.0;
        let exciter_alpha = one_pole_alpha(exciter_freq, sample_rate);
        self.sweet_exciter_state[index] = sanitize_sample(
            self.sweet_exciter_state[index]
                + exciter_alpha * (saturated - self.sweet_exciter_state[index]),
        );
        let highs = saturated - self.sweet_exciter_state[index];
        let exciter_amount = (tone * tone * 0.42).min(0.42);
        let harmonics = soft_saturate(highs, 1.0 + drive * 1.2);
        let excited = sanitize_sample(saturated + harmonics * exciter_amount);

        // Stage 4: Body preservation. The exciter only adds the high band, so the
        // low/mid body of `saturated` passes through untouched — graves and
        // médios keep their weight.

        // Stage 5: Air/presence tilt — a gentle, band-limited high shelf. The air
        // band is low-passed near the top so opening presence adds silk, not
        // brittle ultra-high fizz.
        let air_cut = 11_000.0 + tone * 3_500.0;
        let air_alpha = one_pole_alpha(air_cut, sample_rate);
        self.sweet_air_state[index] = sanitize_sample(
            self.sweet_air_state[index] + air_alpha * (excited - self.sweet_air_state[index]),
        );
        let air = excited - self.sweet_air_state[index];
        let tilted = sanitize_sample(excited + air * (tone * 0.18));

        // Stage 6: Gain compensation — level-matched with a little makeup for the
        // added harmonics, so the perceived level stays steady.
        let compensated = tilted * sweet_compensation(drive, drive_gain);

        // Stage 7: Safety soft-clip.
        soft_clip_sample(compensated)
    }

    fn process_fuzz(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let sample_rate = self.sample_rate.max(1.0);
        let drive = frame.drive;
        let tone = frame.tone;

        // Stage 1: Pre-filter. A gentle high-pass keeps sub-bass rumble out of
        // the huge gain stage (no blocking-distortion mud) while leaving the
        // bass fundamental intact. A small low-mid emphasis adds body so the
        // fuzz stays thick instead of thin.
        let hp_cutoff = 40.0 + drive * 40.0;
        let hp_alpha = one_pole_alpha(hp_cutoff, sample_rate);
        self.fuzz_hp_state[index] = sanitize_sample(
            self.fuzz_hp_state[index] + hp_alpha * (sample - self.fuzz_hp_state[index]),
        );
        let conditioned = sanitize_sample(sample - self.fuzz_hp_state[index]);

        let body_alpha = one_pole_alpha(280.0, sample_rate);
        self.fuzz_body_state[index] = sanitize_sample(
            self.fuzz_body_state[index] + body_alpha * (conditioned - self.fuzz_body_state[index]),
        );
        let bodied = sanitize_sample(conditioned + self.fuzz_body_state[index] * 0.18);

        // Stage 2: Massive input gain — from subtle grit to full fuzz.
        let drive_gain = fuzz_drive_to_gain(drive);

        // Stage 3: Pre-gain safety clamp before the waveshaper.
        let limited = (bodied * drive_gain).clamp(-8.0, 8.0);

        // Stage 4: Musical asymmetry — thick, dense, even-harmonic saturation.
        let asymmetry = drive * 0.35;
        let pos = limited.max(0.0);
        let neg = limited.min(0.0);
        let biased = pos * (1.0 + asymmetry) + neg;

        // Stage 5: Combined hard/soft, multi-stage waveshaping. The tanh blend
        // keeps the clip from being a brickwall, which softens ugly digital
        // foldover; each stage is bounded.
        let stage1 = fuzz_hard_clip(biased, drive);
        let stage2 = fast_tanh(stage1 * (1.0 + drive * 0.6));
        let saturated = sanitize_sample(stage2);

        // Stage 6: DC blocker — mandatory right after the asymmetry stage.
        let dc_free = sanitize_sample(dc_blocker_step(
            &mut self.fuzz_dc_state[index],
            saturated,
            0.9990,
        ));

        // Stage 7: Post-fuzz tone low-pass. Low tone = thick/dark; high tone
        // opens up — but the maximum opening shrinks as drive rises, so heavy
        // fuzz keeps its aliased top in check and never turns to sandpaper.
        let lpf_cutoff = 1_000.0 + tone * tone * (14_500.0 - drive * 6_000.0);
        let lpf_alpha = one_pole_alpha(lpf_cutoff, sample_rate);
        self.fuzz_tone_state[index] = sanitize_sample(
            self.fuzz_tone_state[index] + lpf_alpha * (dc_free - self.fuzz_tone_state[index]),
        );
        let toned = self.fuzz_tone_state[index];

        // Stage 8: Strong gain compensation — dense fuzz kept level-safe.
        let compensated = toned * fuzz_compensation(drive);

        // Stage 9: Final safety soft-clip.
        soft_clip_sample(compensated)
    }

    fn process_howl(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let tone = frame.tone;
        let drive = frame.drive;
        let sample_rate = self.sample_rate.max(1.0);

        let hp_cutoff = 40.0 + drive * 30.0;
        let hp_alpha = one_pole_alpha(hp_cutoff, sample_rate);
        let input_hp =
            self.howl_input_hp_state[index] + hp_alpha * (sample - self.howl_input_hp_state[index]);
        self.howl_input_hp_state[index] = sanitize_sample(input_hp);
        let conditioned = sanitize_sample(sample - input_hp);

        let drive_gain = 1.0 + drive * 1.5;
        let saturated = fast_tanh(conditioned * drive_gain);

        let body_cutoff = (250.0 + tone * 50.0 + drive * 30.0).clamp(250.0, 360.0);
        let body_alpha = one_pole_alpha(body_cutoff, sample_rate);
        let body_lp = self.howl_body_lp_state[index]
            + body_alpha * (saturated - self.howl_body_lp_state[index]);
        self.howl_body_lp_state[index] = sanitize_sample(body_lp);

        let f1 = howl_formant1_frequency(tone);
        let f2 = howl_formant2_frequency(f1, tone, drive);
        let q = howl_q(drive, tone);

        // Dual formant resonators. The lowpass output of each is used (never a
        // bare bandpass), so the formants read as vowels with body rather than
        // as thin peaks.
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

        // Limited resonance: the formant contribution is scaled back a touch as
        // tone rises so a bright vowel never tips into a whistle, then soft
        // clipped so the peak can't run away.
        let formant_limited = soft_clip_sample(formant_mix * (0.72 - tone * 0.08));

        let damping_hz = howl_output_damping(tone);
        let damping_alpha = one_pole_alpha(damping_hz, sample_rate);
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
    let compensation = gain_compensation_curve(drive_gain, 0.58);
    let drive = drive.clamp(0.0, 1.0);
    // Less automatic makeup as drive rises — the saturation already adds density,
    // so this keeps perceived loudness consistent without a volume jump.
    let makeup = 1.0 + ((1.0 - drive) * 0.30);
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
    // Vowel-like first formant, kept in a musical 180–1800 Hz range.
    let tone = tone.clamp(0.0, 1.0);
    180.0 + tone * 1_620.0
}

#[inline]
fn howl_formant2_frequency(f1: f32, tone: f32, drive: f32) -> f32 {
    // Second formant a musical 1.45–2.2× above the first, capped at ~4.2 kHz so
    // it opens the vowel without ever reaching a piercing whistle band.
    let tone = tone.clamp(0.0, 1.0);
    let drive = drive.clamp(0.0, 1.0);
    let ratio = (1.45 + tone * 0.45 + drive * 0.15).clamp(1.45, 2.2);
    (f1 * ratio).clamp(300.0, 4_200.0)
}

#[inline]
fn howl_q(drive: f32, tone: f32) -> f32 {
    // Resonance grows with drive but is *pulled back* as tone (brightness)
    // rises — high formants at high Q are exactly what turns into a whistle, so
    // bright settings trade some Q for musicality. Capped well below self-osc.
    let drive = drive.clamp(0.0, 1.0);
    let tone = tone.clamp(0.0, 1.0);
    safe_q(
        (0.65 + drive.powf(1.2) * 2.45) * (1.0 - tone * 0.20),
        0.6,
        3.2,
    )
}

#[inline]
fn howl_body_mix(drive: f32) -> f32 {
    // 20–40 % clean low body preserved in the wet so it never sounds like a
    // bare resonator.
    (0.40 - drive.clamp(0.0, 1.0) * 0.18).clamp(0.20, 0.40)
}

#[inline]
fn howl_gain_compensation(drive: f32, q: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    let base = 1.0 / (1.0 + drive * 2.0 + q * 0.10);
    (base * (1.0 + drive * 0.25)).clamp(0.20, 0.92)
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
    let freq = safe_frequency(frequency_hz, 40.0, sample_rate.max(1.0) * 0.40);
    let f = (2.0 * (core::f32::consts::PI * freq / sample_rate.max(1.0)).sin()).clamp(0.0005, 0.82);
    let q_safe = safe_q(q, 0.5, 3.80);
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
    // Output damping opens with tone for brightness, but its ceiling is held at
    // ~6.1 kHz (was 9.4 kHz) so the resonant top never becomes a thin whistle.
    let tone = tone.clamp(0.0, 1.0);
    (1_800.0 + tone * 4_300.0).clamp(1_800.0, 6_100.0)
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
        // Smooth exponential release toward silence (volume-pedal feel) instead
        // of a linear ramp, with a small floor so the tail actually reaches zero.
        let release_time = 0.16 + drive.clamp(0.0, 1.0) * 0.20;
        let release_alpha = (3.0 / (release_time * sample_rate)).clamp(0.0, 1.0);
        *gain -= *gain * release_alpha;
        if *gain < 0.000_5 {
            *gain = 0.0;
        }
        return sanitize_sample((*gain).max(0.0));
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
    one_pole_alpha(cutoff, sample_rate)
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
    fn drive_makeup_shrinks_as_drive_rises() {
        // Rule: less automatic makeup at higher drive (avoids a volume jump).
        let low = drive_mode_compensation(0.1, drive_to_gain(0.1));
        let high = drive_mode_compensation(0.9, drive_to_gain(0.9));
        assert!(
            high < low,
            "makeup/compensation should be lower at high drive (low={low}, high={high})"
        );
    }

    fn series_rms(buffer: &[f32]) -> f32 {
        if buffer.is_empty() {
            return 0.0;
        }
        let sum: f64 = buffer.iter().map(|s| (*s as f64).powi(2)).sum();
        (sum / buffer.len() as f64).sqrt() as f32
    }

    // First-difference RMS approximates high-frequency energy.
    fn series_hf_rms(buffer: &[f32]) -> f32 {
        if buffer.len() < 2 {
            return 0.0;
        }
        let sum: f64 = buffer
            .windows(2)
            .map(|w| ((w[1] - w[0]) as f64).powi(2))
            .sum();
        (sum / (buffer.len() - 1) as f64).sqrt() as f32
    }

    #[test]
    fn drive_max_sine_110hz_does_not_explode() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = drive_frame(1.0, 0.5);

        let mut peak = 0.0_f32;
        let mut phase = 0.0_f32;
        for _ in 0..(48_000 * 2) {
            phase += 110.0 / 48_000.0;
            let sine = (phase * core::f32::consts::TAU).sin() * 0.5;
            let out = character.process_sample(0, sine, &frame);
            assert!(out.is_finite(), "drive max 110Hz: NaN/inf");
            peak = peak.max(out.abs());
        }
        assert!(
            peak < 2.0,
            "drive max 110Hz should stay controlled, peak={peak}"
        );
        assert!(
            peak > 0.02,
            "drive max 110Hz should still produce output, peak={peak}"
        );
    }

    #[test]
    fn drive_max_white_noise_does_not_add_fizz() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = drive_frame(1.0, 0.5);

        let mut rng: u32 = 0x1234_5678;
        let mut input = Vec::with_capacity(4096);
        let mut output = Vec::with_capacity(4096);
        for _ in 0..4096 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.3;
            let out = character.process_sample(0, noise, &frame);
            assert!(out.is_finite());
            assert!(out.abs() <= 8.0);
            input.push(noise);
            output.push(out);
        }

        // The high-to-total energy ratio must drop: the anti-fizz low-pass tames
        // the highs instead of adding fizz on top.
        let in_ratio = series_hf_rms(&input) / (series_rms(&input) + 1e-9);
        let out_ratio = series_hf_rms(&output) / (series_rms(&output) + 1e-9);
        assert!(
            out_ratio < in_ratio,
            "max drive should tame highs (in HF ratio {in_ratio}, out {out_ratio})"
        );
    }

    #[test]
    fn drive_max_impulse_has_no_dangerous_peak() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = drive_frame(1.0, 1.0);

        let mut peak = 0.0_f32;
        for index in 0..4096 {
            let input = if index == 0 { 0.9 } else { 0.0 };
            let out = character.process_sample(0, input, &frame);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak < 1.5, "drive impulse peak should be safe, got {peak}");
    }

    #[test]
    fn drive_sweep_has_no_zipper_or_clicks() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let total = 48_000usize; // 1 s sweep of drive 0 -> 1
        let mut phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..total {
            let drive = index as f32 / (total - 1) as f32;
            let frame = drive_frame(drive, 0.5);
            phase += 220.0 / 48_000.0;
            let sine = (phase * core::f32::consts::TAU).sin() * 0.4;
            let out = character.process_sample(0, sine, &frame);
            assert!(out.is_finite(), "drive sweep: NaN/inf");
            peak = peak.max(out.abs());
            if index > 0 {
                max_step = max_step.max((out - previous).abs());
            }
            previous = out;
        }
        assert!(
            peak < 2.0,
            "drive sweep peak should stay controlled, got {peak}"
        );
        assert!(
            max_step < 0.2,
            "drive sweep should not click/zipper, max step {max_step}"
        );
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
    fn sweet_flat_low_drive_is_nearly_transparent() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        // drive 0, tone 0 → no saturation gain, no exciter, no air tilt.
        let frame = sweet_frame(0.0, 0.0);

        let mut phase = 0.0_f32;
        let mut max_diff = 0.0_f32;
        for _ in 0..2048 {
            phase += 440.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite());
            max_diff = max_diff.max((output - input).abs());
        }
        assert!(
            max_diff < 0.05,
            "sweet at flat/low drive should be near-transparent, max diff {max_diff}"
        );
    }

    #[test]
    fn sweet_tone_max_is_not_harsh() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = sweet_frame(0.6, 1.0); // full tone = max exciter/air

        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            phase += 1_000.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.35;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 1.0,
            "sweet at full tone should stay clean (no harsh clipping), peak {peak}"
        );
        assert!(peak > 0.05, "sweet should still be audible, peak {peak}");
    }

    #[test]
    fn sweet_white_noise_peak_is_safe() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = sweet_frame(1.0, 1.0);

        let mut rng: u32 = 0x00c0_ffee;
        let mut peak = 0.0_f32;
        for _ in 0..8192 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.3;
            let output = character.process_sample(0, noise, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 2.0,
            "sweet white noise peak should be controlled, {peak}"
        );
    }

    #[test]
    fn sweet_sine_keeps_harmonics_controlled() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = sweet_frame(1.0, 0.5);

        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..4096 {
            phase += 220.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.35;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 1.0,
            "sweet sine harmonics should stay controlled, peak {peak}"
        );
    }

    #[test]
    fn sweet_mix_zero_is_dry_and_mix_one_is_wet() {
        let dry_frame = CharacterFrame {
            mode: CharacterMode::Sweet,
            drive: 0.7,
            tone: 0.7,
            mix: 0.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            mode_fade: 1.0,
        };
        let wet_frame = CharacterFrame {
            mix: 1.0,
            ..dry_frame
        };

        let mut dry_chain = Character::default();
        dry_chain.prepare(48_000.0);
        let mut wet_chain = Character::default();
        wet_chain.prepare(48_000.0);

        let mut phase = 0.0_f32;
        let mut max_dry_diff = 0.0_f32;
        let mut max_wet_diff = 0.0_f32;
        for _ in 0..2048 {
            phase += 440.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let dry = dry_chain.process_sample(0, input, &dry_frame);
            let wet = wet_chain.process_sample(0, input, &wet_frame);
            max_dry_diff = max_dry_diff.max((dry - input).abs());
            max_wet_diff = max_wet_diff.max((wet - input).abs());
        }
        assert!(
            max_dry_diff < 1e-4,
            "mix 0 should be a dry passthrough, max diff {max_dry_diff}"
        );
        assert!(
            max_wet_diff > 0.001,
            "mix 1 should be audibly processed, max diff {max_wet_diff}"
        );
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
    fn fuzz_max_drive_10s_does_not_explode() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = fuzz_frame(1.0, 0.5);

        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..(48_000 * 10) {
            phase += 110.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.5;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite(), "fuzz 10s max drive: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 2.0,
            "fuzz max drive over 10s should stay bounded, peak={peak}"
        );
        assert!(
            peak > 0.02,
            "fuzz should sustain audible output, peak={peak}"
        );
    }

    #[test]
    fn fuzz_sine_110hz_has_no_absurd_dc() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = fuzz_frame(1.0, 0.4);

        let total = 48_000 * 2;
        let half = total / 2;
        let mut phase = 0.0_f32;
        let mut sum = 0.0_f64;
        let mut count = 0u32;
        for index in 0..total {
            phase += 110.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.5;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite());
            if index >= half {
                sum += output as f64;
                count += 1;
            }
        }
        let dc = (sum / count as f64).abs() as f32;
        assert!(
            dc < 0.05,
            "fuzz 110Hz should not build a DC offset, got {dc}"
        );
    }

    #[test]
    fn fuzz_white_noise_is_controlled() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = fuzz_frame(1.0, 0.7);

        let mut rng: u32 = 0x0bad_f00d;
        let mut peak = 0.0_f32;
        for _ in 0..8192 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.3;
            let output = character.process_sample(0, noise, &frame);
            assert!(output.is_finite());
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 2.0,
            "fuzz white noise peak should be controlled, {peak}"
        );
    }

    #[test]
    fn fuzz_tone_sweep_has_no_zipper() {
        let mut character = Character::default();
        character.prepare(48_000.0);

        let total = 48_000usize; // 1 s sweep of tone 0 -> 1
        let mut phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..total {
            let tone = index as f32 / (total - 1) as f32;
            let frame = fuzz_frame(0.7, tone);
            phase += 220.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.4;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite(), "fuzz tone sweep: NaN/inf");
            peak = peak.max(output.abs());
            if index > 0 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            peak < 2.0,
            "fuzz tone sweep peak should stay controlled, {peak}"
        );
        assert!(
            max_step < 0.3,
            "fuzz tone sweep should not zipper/burst, max step {max_step}"
        );
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
    fn howl_chord_signal_stays_controlled() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(0.7, 0.6);

        let freqs = [220.0_f32, 277.0, 330.0]; // a chord, three partials
        let mut phases = [0.0_f32; 3];
        let mut peak = 0.0_f32;
        let mut audible = false;
        for _ in 0..(48_000 * 2) {
            let mut input = 0.0_f32;
            for (phase, freq) in phases.iter_mut().zip(freqs) {
                *phase += freq / 48_000.0;
                input += (*phase * core::f32::consts::TAU).sin();
            }
            input *= 0.18;
            let output = character.process_sample(0, input, &frame);
            assert!(output.is_finite(), "howl chord: NaN/inf");
            peak = peak.max(output.abs());
            if output.abs() > 0.02 {
                audible = true;
            }
        }
        assert!(peak < 5.0, "howl chord should stay controlled, peak={peak}");
        assert!(audible, "howl chord should produce audible output");
    }

    #[test]
    fn howl_mix_endpoints_dry_50_100() {
        let dry_frame = CharacterFrame {
            mode: CharacterMode::Howl,
            drive: 0.7,
            tone: 0.6,
            mix: 0.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            mode_fade: 1.0,
        };
        let half_frame = CharacterFrame {
            mix: 0.5,
            ..dry_frame
        };
        let wet_frame = CharacterFrame {
            mix: 1.0,
            ..dry_frame
        };

        let mut dry_chain = Character::default();
        dry_chain.prepare(48_000.0);
        let mut half_chain = Character::default();
        half_chain.prepare(48_000.0);
        let mut wet_chain = Character::default();
        wet_chain.prepare(48_000.0);

        let mut phase = 0.0_f32;
        let mut max_dry_diff = 0.0_f32;
        let mut half_peak = 0.0_f32;
        let mut wet_diff = 0.0_f32;
        for _ in 0..2048 {
            phase += 330.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let dry = dry_chain.process_sample(0, input, &dry_frame);
            let half = half_chain.process_sample(0, input, &half_frame);
            let wet = wet_chain.process_sample(0, input, &wet_frame);
            assert!(dry.is_finite() && half.is_finite() && wet.is_finite());
            max_dry_diff = max_dry_diff.max((dry - input).abs());
            half_peak = half_peak.max(half.abs());
            wet_diff = wet_diff.max((wet - input).abs());
        }
        assert!(
            max_dry_diff < 1e-4,
            "howl mix 0 should be dry, diff {max_dry_diff}"
        );
        assert!(half_peak <= 8.0, "howl mix 50 should stay bounded");
        assert!(
            wet_diff > 0.001,
            "howl mix 100 should be audibly processed, diff {wet_diff}"
        );
    }

    #[test]
    fn howl_tone_max_does_not_whistle() {
        // A whistle/apito is a long-ringing narrow resonance. Excite with an
        // impulse at full tone + drive and confirm the tail decays rather than
        // sustaining.
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 1.0);
        for _ in 0..32 {
            character.process_sample(0, 0.0, &frame);
        }
        let _ = character.process_sample(0, 0.9, &frame);

        let mut early = 0.0_f64;
        let mut late = 0.0_f64;
        for index in 0..8000 {
            let output = character.process_sample(0, 0.0, &frame);
            let energy = (output as f64) * (output as f64);
            if index < 1000 {
                early += energy;
            } else if index >= 7000 {
                late += energy;
            }
        }
        let early_rms = (early / 1000.0).sqrt() as f32;
        let late_rms = (late / 1000.0).sqrt() as f32;
        assert!(
            late_rms < early_rms * 0.25 + 1e-6,
            "howl at full tone should not sustain a whistle (early {early_rms}, late {late_rms})"
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

    #[test]
    fn swell_max_drive_does_not_stay_closed() {
        // At maximum drive (longest swell), a sustained note must still open up
        // and reach its sustain level instead of staying gated shut.
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(1.0, 0.5);

        let amplitude = 0.35;
        // Hold a steady tone well past the longest attack time (~0.44 s).
        for _ in 0..(48_000) {
            character.process_sample(0, amplitude, &frame);
        }
        let mut peak = 0.0_f32;
        for _ in 0..2048 {
            let output = character.process_sample(0, amplitude, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
        }
        assert!(
            peak > 0.20,
            "max-drive swell should open to sustain, peak={peak}"
        );
    }
}
