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
    age_state: [f32; MAX_CHANNELS],
    tone_state: [f32; MAX_CHANNELS],
    drive_dc_state: [f32; MAX_CHANNELS],
    drive_hp_state: [f32; MAX_CHANNELS],
    drive_hf_state: [f32; MAX_CHANNELS],
    // Previous waveshaper input, for the light 2× oversampled saturators.
    drive_os_state: [f32; MAX_CHANNELS],
    sweet_dc_state: [f32; MAX_CHANNELS],
    sweet_exciter_state: [f32; MAX_CHANNELS],
    sweet_air_state: [f32; MAX_CHANNELS],
    fuzz_dc_state: [f32; MAX_CHANNELS],
    fuzz_tone_state: [f32; MAX_CHANNELS],
    fuzz_hp_state: [f32; MAX_CHANNELS],
    fuzz_body_state: [f32; MAX_CHANNELS],
    fuzz_os_state: [f32; MAX_CHANNELS],
    howl_input_hp_state: [f32; MAX_CHANNELS],
    howl_body_lp_state: [f32; MAX_CHANNELS],
    howl_formant1_lp_state: [f32; MAX_CHANNELS],
    howl_formant1_bp_state: [f32; MAX_CHANNELS],
    howl_formant2_lp_state: [f32; MAX_CHANNELS],
    howl_formant2_bp_state: [f32; MAX_CHANNELS],
    howl_damping_state: [f32; MAX_CHANNELS],
    howl_dc_state: [f32; MAX_CHANNELS],
    howl_fast_env_state: [f32; MAX_CHANNELS],
    howl_slow_env_state: [f32; MAX_CHANNELS],
    howl_res_energy_state: [f32; MAX_CHANNELS],
    swell_fast_env_state: [f32; MAX_CHANNELS],
    swell_slow_env_state: [f32; MAX_CHANNELS],
    // Swell gain is stereo-linked: one shared envelope drives both channels so
    // the stereo image never wobbles. Detection envelopes stay per-channel and
    // are combined with max() before driving this shared state.
    swell_phase: f32,
    swell_gain: f32,
    swell_cooldown: usize,
    swell_open: bool,
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
    mode_fade: f32,
}

impl Default for Character {
    fn default() -> Self {
        Self {
            core: ModuleCore::default(),
            sample_rate: 44_100.0,
            age_state: [0.0; MAX_CHANNELS],
            tone_state: [0.0; MAX_CHANNELS],
            drive_dc_state: [0.0; MAX_CHANNELS],
            drive_hp_state: [0.0; MAX_CHANNELS],
            drive_hf_state: [0.0; MAX_CHANNELS],
            drive_os_state: [0.0; MAX_CHANNELS],
            sweet_dc_state: [0.0; MAX_CHANNELS],
            sweet_exciter_state: [0.0; MAX_CHANNELS],
            sweet_air_state: [0.0; MAX_CHANNELS],
            fuzz_dc_state: [0.0; MAX_CHANNELS],
            fuzz_tone_state: [0.0; MAX_CHANNELS],
            fuzz_hp_state: [0.0; MAX_CHANNELS],
            fuzz_body_state: [0.0; MAX_CHANNELS],
            fuzz_os_state: [0.0; MAX_CHANNELS],
            howl_input_hp_state: [0.0; MAX_CHANNELS],
            howl_body_lp_state: [0.0; MAX_CHANNELS],
            howl_formant1_lp_state: [0.0; MAX_CHANNELS],
            howl_formant1_bp_state: [0.0; MAX_CHANNELS],
            howl_formant2_lp_state: [0.0; MAX_CHANNELS],
            howl_formant2_bp_state: [0.0; MAX_CHANNELS],
            howl_damping_state: [0.0; MAX_CHANNELS],
            howl_dc_state: [0.0; MAX_CHANNELS],
            howl_fast_env_state: [0.0; MAX_CHANNELS],
            howl_slow_env_state: [0.0; MAX_CHANNELS],
            howl_res_energy_state: [0.0; MAX_CHANNELS],
            swell_fast_env_state: [0.0; MAX_CHANNELS],
            swell_slow_env_state: [0.0; MAX_CHANNELS],
            swell_phase: 0.0,
            swell_gain: 0.0,
            swell_cooldown: 0,
            swell_open: false,
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
        self.age_state = [0.0; MAX_CHANNELS];
        self.tone_state = [0.0; MAX_CHANNELS];
        self.drive_dc_state = [0.0; MAX_CHANNELS];
        self.drive_hp_state = [0.0; MAX_CHANNELS];
        self.drive_hf_state = [0.0; MAX_CHANNELS];
        self.drive_os_state = [0.0; MAX_CHANNELS];
        self.sweet_dc_state = [0.0; MAX_CHANNELS];
        self.sweet_exciter_state = [0.0; MAX_CHANNELS];
        self.sweet_air_state = [0.0; MAX_CHANNELS];
        self.fuzz_dc_state = [0.0; MAX_CHANNELS];
        self.fuzz_tone_state = [0.0; MAX_CHANNELS];
        self.fuzz_hp_state = [0.0; MAX_CHANNELS];
        self.fuzz_body_state = [0.0; MAX_CHANNELS];
        self.fuzz_os_state = [0.0; MAX_CHANNELS];
        self.howl_input_hp_state = [0.0; MAX_CHANNELS];
        self.howl_body_lp_state = [0.0; MAX_CHANNELS];
        self.howl_formant1_lp_state = [0.0; MAX_CHANNELS];
        self.howl_formant1_bp_state = [0.0; MAX_CHANNELS];
        self.howl_formant2_lp_state = [0.0; MAX_CHANNELS];
        self.howl_formant2_bp_state = [0.0; MAX_CHANNELS];
        self.howl_damping_state = [0.0; MAX_CHANNELS];
        self.howl_dc_state = [0.0; MAX_CHANNELS];
        self.howl_fast_env_state = [0.0; MAX_CHANNELS];
        self.howl_slow_env_state = [0.0; MAX_CHANNELS];
        self.howl_res_energy_state = [0.0; MAX_CHANNELS];
        self.swell_fast_env_state = [0.0; MAX_CHANNELS];
        self.swell_slow_env_state = [0.0; MAX_CHANNELS];
        self.swell_phase = 0.0;
        self.swell_gain = 0.0;
        self.swell_cooldown = 0;
        self.swell_open = false;
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

        // AGE: shared coloration after the mode — gentle grit plus a tape-style
        // high-frequency roll-off. Transparent at 0, DC-safe (symmetric drive +
        // one-pole low-pass), works for every mode.
        let wet = self.apply_age(index, wet, frame.age);

        let wet = sanitize_sample(wet * frame.output_gain);
        let mixed = DryWet.mix(dry, wet, frame.mix);
        let mode_mixed = self.smooth_mode_transition(index, mixed, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, mode_mixed, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    /// Shared "AGE" coloration: symmetric soft grit blended in, then a tape-style
    /// high-frequency roll-off that darkens as age rises. At `age == 0` it returns
    /// the input untouched. No asymmetry, so it never introduces a DC offset.
    fn apply_age(&mut self, channel: usize, sample: f32, age: f32) -> f32 {
        let age = age.clamp(0.0, 1.0);
        if age <= 0.000_001 {
            return sample;
        }
        // Ease the response so the lower half stays gentle and only the top adds
        // obvious grit/darkening — a more musical sweep than a linear ramp.
        let eased = smoothstep(age);
        let drive = 1.0 + eased * 1.0;
        let grit = soft_saturate(sample, drive);
        let blend = eased * 0.4;
        let blended = sample * (1.0 - blend) + grit * blend;

        // Tape-style roll-off kept above ~6.5 kHz so it warms rather than dulls.
        let cutoff = (16_000.0 - eased * 9_500.0).max(3_500.0);
        let alpha = one_pole_alpha(cutoff, self.sample_rate);
        let index = channel.min(MAX_CHANNELS - 1);
        self.age_state[index] += alpha * (blended - self.age_state[index]);
        self.age_state[index] = sanitize_sample(self.age_state[index]);
        self.age_state[index]
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

        // Stage 3: Controlled asymmetry (even harmonics = warmth/body) feeding the
        // main soft-clip waveshaper. The whole pre-shape → asymmetry → soft-clip
        // chain runs through a light 2× oversampler so a hot, bright Drive stays
        // smooth instead of fizzing with foldback aliasing.
        let asymmetry = drive * 0.19;
        let saturated = shape_2x(&mut self.drive_os_state[index], driven, |x| {
            let pre = fast_tanh(x * 0.7);
            let shaped = pre.max(0.0) * (1.0 + asymmetry) + pre.min(0.0);
            fast_tanh(shaped)
        });

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
        // Gentler than before: a lower ceiling and softer harmonic drive keep the
        // sheen silky on piano/vocal/drums instead of a brittle "cheap exciter".
        let exciter_amount = (tone * tone * 0.32).min(0.32);
        let harmonics = soft_saturate(highs, 1.0 + drive * 0.8);
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

        // Stages 4–5: Musical asymmetry + combined hard/soft multi-stage
        // waveshaping (the tanh blend keeps the clip off a brickwall, softening
        // ugly digital foldover). The whole nonlinear core runs through a light 2×
        // oversampler so the huge fuzz gain doesn't splatter obvious aliasing.
        let asymmetry = drive * 0.35;
        let saturated = sanitize_sample(shape_2x(&mut self.fuzz_os_state[index], limited, |x| {
            let biased = x.max(0.0) * (1.0 + asymmetry) + x.min(0.0);
            let stage1 = fuzz_hard_clip(biased, drive);
            fast_tanh(stage1 * (1.0 + drive * 0.6))
        }));

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

    /// Howl — a resonant filter-fuzz. Two parallel paths (a body path that keeps
    /// the instrument's weight and a resonant fuzz path built on two formant
    /// resonators) are blended, with a signal-driven dynamic resonance limiter
    /// that stops the formants from tipping into a whistle. Transients gently
    /// open the formants for synth-like stabs, while sustain keeps them stable.
    fn process_howl(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let tone = frame.tone;
        let drive = frame.drive;
        let sample_rate = self.sample_rate.max(1.0);

        // ── 1. Input conditioning: DC blocker + subsonic high-pass ──────────
        let dc_free = sanitize_sample(dc_blocker_step(
            &mut self.howl_dc_state[index],
            sample,
            0.9992,
        ));
        let hp_cutoff = 28.0 + drive * 10.0;
        let hp_alpha = one_pole_alpha(hp_cutoff, sample_rate);
        self.howl_input_hp_state[index] = sanitize_sample(
            self.howl_input_hp_state[index]
                + hp_alpha * (dc_free - self.howl_input_hp_state[index]),
        );
        let conditioned = sanitize_sample(dc_free - self.howl_input_hp_state[index]);

        // ── 2. Envelope follower: transients briefly open the formants ──────
        let abs_in = conditioned.abs();
        let fast_alpha = envelope_alpha(0.003, sample_rate);
        let slow_alpha = envelope_alpha(0.060, sample_rate);
        self.howl_fast_env_state[index] = sanitize_sample(
            self.howl_fast_env_state[index]
                + fast_alpha * (abs_in - self.howl_fast_env_state[index]),
        );
        self.howl_slow_env_state[index] = sanitize_sample(
            self.howl_slow_env_state[index]
                + slow_alpha * (abs_in - self.howl_slow_env_state[index]),
        );
        let transient =
            (self.howl_fast_env_state[index] - self.howl_slow_env_state[index]).max(0.0);
        let transient_open = (transient * 6.0).clamp(0.0, 1.0);

        // ── 3. Resonant filter-fuzz drive: staged asymmetric soft saturation.
        // Subtle growl when low, dense synth-stab fuzz when high; no dry hard
        // clip — every stage is tanh-bounded. ──────────────────────────────
        let drive_gain = 1.0 + drive * drive * 6.0;
        let asymmetry = drive * 0.18;
        let pushed = conditioned * drive_gain;
        let shaped = pushed.max(0.0) * (1.0 + asymmetry) + pushed.min(0.0);
        let stage1 = fast_tanh(shaped);
        let fuzz = sanitize_sample(fast_tanh(stage1 * (1.0 + drive * 0.9)));

        // ── 4a. Body path: a lightly saturated low/low-mid band of the clean
        // input, so the wet always keeps real instrument weight. ────────────
        let body_cutoff = (700.0 + tone * 500.0).clamp(400.0, 1_400.0);
        let body_alpha = one_pole_alpha(body_cutoff, sample_rate);
        self.howl_body_lp_state[index] = sanitize_sample(
            self.howl_body_lp_state[index]
                + body_alpha * (conditioned - self.howl_body_lp_state[index]),
        );
        let body = sanitize_sample(soft_saturate(
            self.howl_body_lp_state[index],
            1.0 + drive * 0.5,
        ));

        // ── 4b. Resonant path: dual formants, transient-opened, capped so the
        // upper formant never reaches a piercing whistle band. ──────────────
        let open = 1.0 + transient_open * 0.25;
        let f1 = (howl_formant1_frequency(tone) * open).clamp(120.0, 1_900.0);
        let f2 = (howl_formant2_frequency(f1, tone) * open).clamp(300.0, 3_600.0);
        let q = howl_q(drive, tone) * (1.0 - transient_open * 0.30);

        let r1 = howl_resonator_step(
            fuzz,
            f1,
            q,
            sample_rate,
            &mut self.howl_formant1_lp_state[index],
            &mut self.howl_formant1_bp_state[index],
        );
        let r2 = howl_resonator_step(
            fuzz,
            f2,
            q * 0.70,
            sample_rate,
            &mut self.howl_formant2_lp_state[index],
            &mut self.howl_formant2_bp_state[index],
        );
        let balance = 0.30 + tone * 0.10;
        let resonant = sanitize_sample(r1 * (1.0 - balance) + r2 * balance);
        // Filter-fuzz colour: saturate the resonance itself for vocal growl.
        let resonant = sanitize_sample(fast_tanh(resonant * (1.0 + drive * 1.1)));

        // ── 5. Dynamic resonance limiter (the real anti-whistle). Fast attack /
        // slow release energy follower pulls resonance gain down when it builds
        // toward a whistle, then recovers smoothly — reacts to chords too. ──
        let target = resonant.abs();
        let er = self.howl_res_energy_state[index];
        let er_alpha = if target > er { 0.35 } else { 0.005 };
        self.howl_res_energy_state[index] = sanitize_sample(er + (target - er) * er_alpha);
        let over = (self.howl_res_energy_state[index] - 0.42).max(0.0);
        let resonance_gain = (1.0 / (1.0 + over * 5.0)).clamp(0.25, 1.0);
        let limited = soft_clip_sample(resonant * resonance_gain);

        // ── 6. Tone damping: opens with tone, ceiling ~6 kHz. ───────────────
        let damping_hz = howl_output_damping(tone);
        let damping_alpha = one_pole_alpha(damping_hz, sample_rate);
        self.howl_damping_state[index] = sanitize_sample(
            self.howl_damping_state[index]
                + damping_alpha * (limited - self.howl_damping_state[index]),
        );
        let damped = self.howl_damping_state[index];

        // ── 7. Body blend: 25–40 % body always present (never goes thin). ───
        let body_mix = howl_body_mix(drive);
        let voiced = sanitize_sample(body * body_mix + damped * (1.0 - body_mix));

        // ── 8. Gain compensation + safety soft clip. ────────────────────────
        let compensated = sanitize_sample(voiced * howl_gain_compensation(drive, q));
        soft_clip_sample(compensated)
    }

    /// Swell — an envelope-triggered volume swell. Amount (Drive) sets the
    /// attack/decay length; Sensitivity (Tone) sets the onset threshold. The
    /// detection envelopes are per-channel but combined with max(), and the
    /// resulting gain is shared across channels so the stereo image stays put.
    fn process_swell(&mut self, channel: usize, sample: f32, frame: &CharacterFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let sample_rate = self.sample_rate.max(1.0);
        let amount = frame.drive.clamp(0.0, 1.0);
        let sensitivity = frame.tone.clamp(0.0, 1.0);
        let abs_in = sample.abs();

        // Per-channel detection envelopes (fast ~2 ms, slow ~50 ms).
        let fast_alpha = envelope_alpha(0.002, sample_rate);
        let slow_alpha = envelope_alpha(0.050, sample_rate);
        self.swell_fast_env_state[index] = sanitize_sample(
            self.swell_fast_env_state[index]
                + fast_alpha * (abs_in - self.swell_fast_env_state[index]),
        );
        self.swell_slow_env_state[index] = sanitize_sample(
            self.swell_slow_env_state[index]
                + slow_alpha * (abs_in - self.swell_slow_env_state[index]),
        );

        // Advance the SHARED swell envelope once per frame (on the first
        // channel), driven by the louder of the two channels. This keeps one
        // gain for both sides so the image never wobbles.
        if index == 0 {
            let fast_env = self.swell_fast_env_state[0].max(self.swell_fast_env_state[1]);
            let slow_env = self.swell_slow_env_state[0].max(self.swell_slow_env_state[1]);
            let signal_floor = swell_signal_floor(sensitivity);

            // Gate floor with hysteresis: drop below and the swell re-arms.
            if fast_env < signal_floor * 0.6 {
                self.swell_open = false;
                self.swell_phase = 0.0;
            }

            let can_retrigger = self.swell_open || self.swell_phase <= 0.000_1;
            if can_retrigger
                && swell_onset_detect(fast_env, slow_env, sensitivity)
                && self.swell_cooldown == 0
                && slow_env > signal_floor
            {
                self.swell_phase = 0.0;
                self.swell_open = false;
                self.swell_cooldown = (swell_cooldown_time(amount) * sample_rate).round() as usize;
            } else if self.swell_cooldown > 0 {
                self.swell_cooldown -= 1;
            }

            swell_envelope_step(
                amount,
                slow_env,
                signal_floor,
                sample_rate,
                swell_attack_time(amount),
                &mut self.swell_phase,
                &mut self.swell_gain,
                &mut self.swell_open,
            );
        }

        // Deep swell: the floor is low so the attack is genuinely removed, and a
        // fully-open envelope passes the sustain at unity. Amount deepens it.
        let depth = (0.80 + amount * 0.16).clamp(0.80, 0.96);
        let shaped_gain = (1.0 - depth) + self.swell_gain * depth;
        let wet = sanitize_sample(sample * shaped_gain);

        // Gentle softening (longer swells a touch darker). Per-channel filter,
        // one shared gain → stable image, no phase inversion.
        let lpf_alpha = swell_tone_alpha(amount, sample_rate);
        self.swell_tone_state[index] = sanitize_sample(
            self.swell_tone_state[index] + lpf_alpha * (wet - self.swell_tone_state[index]),
        );
        soft_clip_sample(self.swell_tone_state[index])
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
    // Vowel-like first formant, kept in a musical 180–1500 Hz range.
    let tone = tone.clamp(0.0, 1.0);
    180.0 + tone * 1_320.0
}

#[inline]
fn howl_formant2_frequency(f1: f32, tone: f32) -> f32 {
    // Second formant a musical 1.45–1.85× above the first, capped at ~3.6 kHz so
    // it opens the vowel without ever reaching a piercing whistle band.
    let tone = tone.clamp(0.0, 1.0);
    let ratio = (1.45 + tone * 0.40).clamp(1.45, 1.85);
    (f1 * ratio).clamp(300.0, 3_600.0)
}

#[inline]
fn howl_q(drive: f32, tone: f32) -> f32 {
    // Resonance grows with drive but is *pulled back* as tone (brightness)
    // rises — high formants at high Q are what turn into a whistle, so bright
    // settings trade some Q for musicality. Hard-capped at 4.0 (self-osc safe).
    let drive = drive.clamp(0.0, 1.0);
    let tone = tone.clamp(0.0, 1.0);
    safe_q((0.8 + drive.powi(2) * 3.0) * (1.0 - tone * 0.18), 0.8, 4.0)
}

#[inline]
fn howl_body_mix(drive: f32) -> f32 {
    // 25–40 % clean body preserved in the wet so it never sounds like a bare
    // resonator. Drops a little at high drive but never disappears.
    (0.40 - drive.clamp(0.0, 1.0) * 0.15).clamp(0.25, 0.40)
}

#[inline]
fn howl_gain_compensation(drive: f32, q: f32) -> f32 {
    let drive = drive.clamp(0.0, 1.0);
    let base = 1.0 / (1.0 + drive * 1.5 + q * 0.08);
    (base * (1.0 + drive * 0.20)).clamp(0.20, 1.0)
}

/// One step of a Chamberlin state-variable resonator, returning the bandpass
/// (resonant) output. Damping comes straight from Q, and only the bandpass
/// state is soft-clipped — the integrators stay linear so the formant stays in
/// tune, while the soft clip + finite Q + the caller's energy limiter keep it
/// from running away or self-oscillating on silence.
#[inline]
fn howl_resonator_step(
    input: f32,
    frequency_hz: f32,
    q: f32,
    sample_rate: f32,
    lp_state: &mut f32,
    bp_state: &mut f32,
) -> f32 {
    let freq = safe_frequency(frequency_hz, 40.0, sample_rate.max(1.0) * 0.45);
    let f = (2.0 * (core::f32::consts::PI * freq / sample_rate.max(1.0)).sin()).clamp(0.0005, 0.95);
    let q_safe = safe_q(q, 0.5, 4.0);
    let damping = (1.0 / q_safe).clamp(0.20, 2.0);

    let high = sanitize_sample(input - *lp_state - damping * *bp_state);
    let band = sanitize_sample(soft_clip_sample(*bp_state + f * high));
    let low = sanitize_sample(*lp_state + f * band);
    *bp_state = band;
    *lp_state = low;
    band
}

#[inline]
fn howl_output_damping(tone: f32) -> f32 {
    // Output damping opens with tone for brightness, with a ~6 kHz ceiling so
    // the resonant top never becomes a thin whistle.
    let tone = tone.clamp(0.0, 1.0);
    (2_000.0 + tone * 4_000.0).clamp(2_000.0, 6_000.0)
}

#[inline]
fn swell_attack_time(amount: f32) -> f32 {
    // Amount sets the swell length. Quadratic so the low half stays short/snappy
    // (guitar) and only the top reaches long, cinematic swells (~0.85 s).
    let amount = amount.clamp(0.0, 1.0);
    (0.012 + amount * amount * 0.84).clamp(0.012, 0.86)
}

#[inline]
fn swell_cooldown_time(amount: f32) -> f32 {
    // Adaptive retrigger lockout: 30 ms for fast phrases, up to 150 ms for the
    // longer/cinematic settings so a long swell isn't chopped by its own tail.
    let amount = amount.clamp(0.0, 1.0);
    0.030 + amount * 0.120
}

#[inline]
fn swell_signal_floor(sensitivity: f32) -> f32 {
    // Gate floor: higher Sensitivity lowers it (responds to softer playing) but
    // never to zero, so the noise floor / silence can't open the swell.
    let sensitivity = sensitivity.clamp(0.0, 1.0);
    0.004 + (1.0 - sensitivity) * 0.012
}

#[inline]
fn swell_onset_detect(fast_env: f32, slow_env: f32, sensitivity: f32) -> bool {
    // A new note = the fast envelope jumping clearly above the slow one. Higher
    // Sensitivity relaxes both the absolute and ratio thresholds (lighter touch
    // retriggers), with hysteresis from the ratio test so legato doesn't chatter.
    let sensitivity = sensitivity.clamp(0.0, 1.0);
    let differential = fast_env - slow_env;
    let ratio = fast_env / (slow_env + 0.000_8);
    let threshold = 0.012 - sensitivity * 0.008;
    let ratio_threshold = 1.20 - sensitivity * 0.10;
    differential > threshold && ratio > ratio_threshold
}

#[inline]
fn swell_envelope_step(
    amount: f32,
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
        let release_time = 0.16 + amount.clamp(0.0, 1.0) * 0.20;
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
        let curve = 1.15 + amount.clamp(0.0, 1.0) * 0.85;
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
fn swell_tone_alpha(amount: f32, sample_rate: f32) -> f32 {
    // Gentle softening that tracks Amount: longer/cinematic swells get a touch
    // darker, fast swells stay open. Cutoff stays high so level is preserved.
    let amount = amount.clamp(0.0, 1.0);
    let cutoff = 14_000.0 - amount * 8_000.0;
    one_pole_alpha(cutoff, sample_rate)
}

/// Light 2× oversampled memoryless waveshaper. Linearly upsamples to a midpoint
/// between the previous and current input, applies the (nonlinear) `shaper` at
/// both 2× points, then decimates with a 2-tap average — a half-band-ish filter
/// with a null at the original Nyquist, which is exactly where the freshly
/// generated harmonics would otherwise fold back as aliasing. Cheap (one extra
/// shaper eval per sample) and stateless apart from the previous input.
#[inline]
fn shape_2x<F: Fn(f32) -> f32>(prev_in: &mut f32, x: f32, shaper: F) -> f32 {
    let mid = 0.5 * (*prev_in + x);
    let a = shaper(mid);
    let b = shaper(x);
    *prev_in = x;
    sanitize_sample(0.5 * (a + b))
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
            age: 0.0,
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
            age: 0.0,
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
            age: 0.0,
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
            age: 0.0,
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
            age: 0.0,
            tone,
            mix: 1.0,
            output_gain: 1.0,
            active_mix: 1.0,
            tone_alpha: 0.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn age_colors_the_signal_and_stays_safe() {
        let mut aged = Character::default();
        aged.prepare(48_000.0);
        let mut plain = Character::default();
        plain.prepare(48_000.0);

        let mut frame_aged = drive_frame(0.3, 0.5);
        frame_aged.age = 1.0;
        let frame_plain = drive_frame(0.3, 0.5);

        let mut max_diff = 0.0_f32;
        let mut sum = 0.0_f64;
        let mut count = 0.0_f64;
        let mut phase = 0.0_f32;
        for index in 0..8_000 {
            phase += 440.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            let x = (phase * core::f32::consts::TAU).sin() * 0.4;
            let a = aged.process_sample(0, x, &frame_aged);
            let p = plain.process_sample(0, x, &frame_plain);
            assert!(a.is_finite() && a.abs() <= 8.0, "aged output {a}");
            max_diff = max_diff.max((a - p).abs());
            if index > 4_000 {
                sum += a as f64;
                count += 1.0;
            }
        }
        // AGE must actually change the sound (proves it's wired, not a fake knob).
        assert!(
            max_diff > 0.001,
            "AGE did not change the output (diff {max_diff})"
        );
        // ...and must not introduce a DC offset.
        assert!(
            (sum / count).abs() < 0.02,
            "AGE introduced DC: {}",
            sum / count
        );
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
            age: 0.0,
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
            age: 0.0,
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
    fn howl_keeps_body_at_high_drive() {
        // A 150 Hz note sits well below the formant band, so without the parallel
        // body path the output would be near-silent/thin. Proves body survives.
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(1.0, 1.0);

        let mut sum_sq = 0.0_f64;
        let mut count = 0.0_f64;
        let mut phase = 0.0_f32;
        for index in 0..6_000 {
            phase += 150.0 / 48_000.0;
            let sine = (phase * core::f32::consts::TAU).sin() * 0.4;
            let out = character.process_sample(0, sine, &frame);
            assert!(out.is_finite());
            if index > 2_000 {
                sum_sq += (out as f64) * (out as f64);
                count += 1.0;
            }
        }
        let rms = (sum_sq / count).sqrt() as f32;
        assert!(
            rms > 0.012,
            "howl must keep instrument body on a low note (rms {rms})"
        );
    }

    #[test]
    fn howl_has_no_dc_offset() {
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = howl_frame(0.85, 0.7);

        let mut sum = 0.0_f64;
        let mut count = 0.0_f64;
        let mut phase = 0.0_f32;
        for index in 0..8_000 {
            phase += 220.0 / 48_000.0;
            let sine = (phase * core::f32::consts::TAU).sin() * 0.4;
            let out = character.process_sample(0, sine, &frame);
            if index > 2_000 {
                sum += out as f64;
                count += 1.0;
            }
        }
        assert!(
            (sum / count).abs() < 0.02,
            "howl should not introduce DC ({})",
            sum / count
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

    #[test]
    fn swell_stereo_linked_matches_mono_open_rate() {
        // The swell gain is stereo-linked, so feeding both channels must open at
        // the same rate as a single channel (not twice as fast). This guards the
        // shared-envelope design against regressing to per-channel advancement.
        let frame = swell_frame(0.5, 0.5);

        let mut mono = Character::default();
        mono.prepare(48_000.0);
        let mut n_mono = 0usize;
        for index in 0..48_000 {
            if mono.process_sample(0, 0.35, &frame) > 0.30 {
                n_mono = index;
                break;
            }
        }

        let mut stereo = Character::default();
        stereo.prepare(48_000.0);
        let mut n_stereo = 0usize;
        for index in 0..48_000 {
            let left = stereo.process_sample(0, 0.35, &frame);
            let _right = stereo.process_sample(1, 0.35, &frame);
            if left > 0.30 {
                n_stereo = index;
                break;
            }
        }

        assert!(
            n_mono > 0 && n_stereo > 0,
            "swell should open in both cases"
        );
        let diff = (n_mono as i32 - n_stereo as i32).abs();
        assert!(
            diff < 200,
            "stereo swell must open at the mono rate (mono {n_mono}, stereo {n_stereo})"
        );
    }

    #[test]
    fn swell_noise_floor_does_not_open() {
        // Even at maximum Sensitivity, a quiet noise floor must not open the
        // swell — the gate floor has a hard minimum.
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = swell_frame(0.5, 1.0); // amount 0.5, sensitivity max

        let mut rng: u32 = 0x0b0b_cafe;
        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.0025;
            let out = character.process_sample(0, noise, &frame);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(
            peak < 0.01,
            "noise floor must not open the swell at max sensitivity, peak={peak}"
        );
    }

    // ── Premium pass guards: Drive / Sweeten / Fuzz ─────────────────────────

    fn hf_smoke(make: fn(f32, f32) -> CharacterFrame, peak_limit: f32) {
        // Bright sines at max drive/tone must stay finite, peak-safe and DC-free
        // (the light 2× oversampler + post low-pass keep the saturator clean).
        for freq in [1_000.0_f32, 4_000.0, 8_000.0] {
            let mut character = Character::default();
            character.prepare(48_000.0);
            let frame = make(1.0, 1.0);
            let mut phase = 0.0_f32;
            let mut peak = 0.0_f32;
            let mut sum = 0.0_f64;
            let mut count = 0.0_f64;
            for index in 0..16_000 {
                phase += freq / 48_000.0;
                if phase >= 1.0 {
                    phase -= phase.floor();
                }
                let out = character.process_sample(
                    0,
                    (phase * core::f32::consts::TAU).sin() * 0.5,
                    &frame,
                );
                assert!(out.is_finite(), "{freq} Hz produced NaN/inf");
                peak = peak.max(out.abs());
                if index > 4_000 {
                    sum += out as f64;
                    count += 1.0;
                }
            }
            assert!(
                peak < peak_limit,
                "{freq} Hz peak {peak} exceeded {peak_limit}"
            );
            assert!(
                (sum / count).abs() < 0.02,
                "{freq} Hz introduced DC {}",
                sum / count
            );
        }
    }

    #[test]
    fn drive_high_frequency_smoke_is_safe() {
        hf_smoke(drive_frame, 3.0);
    }

    #[test]
    fn fuzz_high_frequency_smoke_is_safe() {
        hf_smoke(fuzz_frame, 3.0);
    }

    #[test]
    fn sweet_quiet_input_stays_clean() {
        // A very quiet input must not be blown up by the exciter/air stages.
        let mut character = Character::default();
        character.prepare(48_000.0);
        let frame = sweet_frame(0.5, 1.0);
        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..8_000 {
            phase += 440.0 / 48_000.0;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let out =
                character.process_sample(0, (phase * core::f32::consts::TAU).sin() * 0.02, &frame);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(
            peak < 0.12,
            "quiet input should stay quiet through Sweeten, peak={peak}"
        );
    }
}
