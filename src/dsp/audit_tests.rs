//! DSP safety/quality audit. These tests exercise every official mode across
//! silence, tones, impulses, noise, stereo-different input, parameter extremes,
//! mix endpoints, bypass, mode switching, preset combinations, feedback stress,
//! and the full chain — asserting the hard invariants (finite, bounded peak,
//! no runaway feedback, no relevant DC offset) and gathering measurements.
//!
//! This module only adds tests/helpers; it does not change any DSP.

use nih_plug::prelude::{BoolParam, EnumParam, FloatParam, FloatRange};

use crate::dsp::character::{Character, CharacterMode};
use crate::dsp::diffusion::{Diffusion, DiffusionMode};
use crate::dsp::movement::{Movement, MovementMode};
use crate::dsp::test_utils::{
    assert_finite_buffer, assert_no_dc_offset, assert_peak_below, dc_offset_range, make_sine,
    make_stereo_different, make_stereo_impulse, make_white_noise, max_abs_difference, peak,
    process_test_signal, rms_range, with_stereo_buffer, MAX_EXPECTED_ABS_SAMPLE, TEST_SAMPLE_RATE,
};
use crate::dsp::texture::{Texture, TextureMode};
use crate::dsp::Processor;
use crate::meters::Meters;
use crate::params::{Cc22Params, CharacterParams, DiffusionParams, MovementParams, TextureParams};

// ─── Audit signals (checklist items 1–6) ────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum AuditSignal {
    Silence,
    Sine110,
    Sine440,
    Impulse,
    WhiteNoise,
    StereoDifferent,
}

impl AuditSignal {
    const ALL: [Self; 6] = [
        Self::Silence,
        Self::Sine110,
        Self::Sine440,
        Self::Impulse,
        Self::WhiteNoise,
        Self::StereoDifferent,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Silence => "silence",
            Self::Sine110 => "sine-110",
            Self::Sine440 => "sine-440",
            Self::Impulse => "impulse",
            Self::WhiteNoise => "white-noise",
            Self::StereoDifferent => "stereo-diff",
        }
    }

    fn render(self, samples: usize, sample_rate: f32) -> [Vec<f32>; 2] {
        match self {
            Self::Silence => [vec![0.0; samples], vec![0.0; samples]],
            Self::Sine110 => make_sine(samples, sample_rate, 110.0, 0.35),
            Self::Sine440 => make_sine(samples, sample_rate, 440.0, 0.35),
            Self::Impulse => make_stereo_impulse(samples),
            Self::WhiteNoise => make_white_noise(samples, 0.18),
            Self::StereoDifferent => make_stereo_different(samples, sample_rate),
        }
    }
}

// ─── Mode identification ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum ModuleMode {
    Character(CharacterMode),
    Movement(MovementMode),
    Diffusion(DiffusionMode),
    Texture(TextureMode),
}

fn all_modes() -> Vec<ModuleMode> {
    let mut modes = Vec::new();
    modes.extend(CharacterMode::PRODUCT_MODES.map(ModuleMode::Character));
    modes.extend(MovementMode::PRODUCT_MODES.map(ModuleMode::Movement));
    modes.extend(DiffusionMode::PRODUCT_MODES.map(ModuleMode::Diffusion));
    modes.extend(TextureMode::PRODUCT_MODES.map(ModuleMode::Texture));
    modes
}

fn mode_label(mode: ModuleMode) -> String {
    match mode {
        ModuleMode::Character(m) => format!("character/{m:?}"),
        ModuleMode::Movement(m) => format!("movement/{m:?}"),
        ModuleMode::Diffusion(m) => format!("diffusion/{m:?}"),
        ModuleMode::Texture(m) => format!("texture/{m:?}"),
    }
}

/// Intensity levels (checklist item 7): minimum, medium, maximum.
const INTENSITIES: [(&str, f32); 3] = [("min", 0.0), ("mid", 0.5), ("max", 1.0)];

// ─── Parameter builders (intensity 0..1 lerps every knob) ───────────────────

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Wide-range, unsmoothed parameter holding a fixed test value. The DSP clamps
/// to its own valid range, and unsmoothed params are the harshest input for the
/// module's internal smoothing.
fn fp(value: f32) -> FloatParam {
    FloatParam::new(
        "p",
        value,
        FloatRange::Linear {
            min: -10_000.0,
            max: 10_000.0,
        },
    )
}

fn character_params(mode: CharacterMode, t: f32, mix: f32, bypass: bool) -> CharacterParams {
    let mut params = CharacterParams::default();
    params.mode = EnumParam::new("mode", mode);
    params.bypass = BoolParam::new("bypass", bypass);
    params.drive = fp(lerp(0.0, 1.0, t));
    params.age = fp(lerp(0.0, 1.0, t));
    params.tone = fp(lerp(0.0, 1.0, t));
    params.noise = fp(lerp(0.0, 0.25, t));
    params.mix = fp(mix);
    params.output_trim = fp(0.0);
    params.reset_smoothers();
    params
}

fn movement_params(mode: MovementMode, t: f32, mix: f32, bypass: bool) -> MovementParams {
    let mut params = MovementParams::default();
    params.mode = EnumParam::new("mode", mode);
    params.bypass = BoolParam::new("bypass", bypass);
    params.rate = fp(lerp(0.05, 12.0, t));
    params.depth = fp(lerp(0.0, 1.0, t));
    params.delay = fp(lerp(5.0, 30.0, t));
    params.feedback = fp(lerp(0.0, 0.6, t));
    params.width = fp(lerp(0.0, 1.0, t));
    params.phase = fp(lerp(0.0, 180.0, t));
    params.tone = fp(lerp(0.0, 1.0, t));
    params.mix = fp(mix);
    params.reset_smoothers();
    params
}

fn diffusion_params(mode: DiffusionMode, t: f32, mix: f32, bypass: bool) -> DiffusionParams {
    let mut params = DiffusionParams::default();
    params.mode = EnumParam::new("mode", mode);
    params.bypass = BoolParam::new("bypass", bypass);
    params.time = fp(lerp(1.0, 2_000.0, t));
    params.feedback = fp(lerp(0.0, 0.95, t));
    params.size = fp(lerp(0.0, 1.0, t));
    params.decay = fp(lerp(0.0, 1.0, t));
    params.pre_delay = fp(lerp(0.0, 120.0, t));
    params.damping = fp(lerp(0.0, 1.0, t));
    params.tone = fp(lerp(0.0, 1.0, t));
    params.stereo_offset = fp(lerp(0.0, 0.5, t));
    params.width = fp(lerp(0.0, 1.0, t));
    params.mix = fp(mix);
    params.reset_smoothers();
    params
}

fn texture_params(mode: TextureMode, t: f32, mix: f32, bypass: bool) -> TextureParams {
    let mut params = TextureParams::default();
    params.mode = EnumParam::new("mode", mode);
    params.bypass = BoolParam::new("bypass", bypass);
    params.wow_depth = fp(lerp(0.0, 1.0, t));
    params.wow_rate = fp(lerp(0.1, 2.0, t));
    params.flutter_depth = fp(lerp(0.0, 1.0, t));
    params.flutter_rate = fp(lerp(3.0, 20.0, t));
    params.random_drift = fp(lerp(0.0, 1.0, t));
    params.noise_amount = fp(lerp(0.0, 1.0, t));
    params.noise_color = fp(lerp(0.0, 1.0, t));
    params.degrade = fp(lerp(0.0, 1.0, t));
    params.stereo_spread = fp(lerp(0.0, 1.0, t));
    params.mix = fp(mix);
    params.reset_smoothers();
    params
}

// ─── Core runner + required render helpers ──────────────────────────────────

fn run_module(
    mode: ModuleMode,
    t: f32,
    mix: f32,
    bypass: bool,
    input: [Vec<f32>; 2],
) -> [Vec<f32>; 2] {
    match mode {
        ModuleMode::Character(m) => {
            let params = character_params(m, t, mix, bypass);
            let mut module = Character::default();
            module.prepare(TEST_SAMPLE_RATE);
            module.reset();
            process_test_signal(input, |buffer| module.process_block(buffer, &params))
        }
        ModuleMode::Movement(m) => {
            let params = movement_params(m, t, mix, bypass);
            let mut module = Movement::default();
            module.prepare(TEST_SAMPLE_RATE);
            module.reset();
            process_test_signal(input, |buffer| module.process_block(buffer, &params))
        }
        ModuleMode::Diffusion(m) => {
            let params = diffusion_params(m, t, mix, bypass);
            let mut module = Diffusion::default();
            module.prepare(TEST_SAMPLE_RATE);
            module.reset();
            process_test_signal(input, |buffer| module.process_block(buffer, &params))
        }
        ModuleMode::Texture(m) => {
            let params = texture_params(m, t, mix, bypass);
            let mut module = Texture::default();
            module.prepare(TEST_SAMPLE_RATE);
            module.reset();
            process_test_signal(input, |buffer| module.process_block(buffer, &params))
        }
    }
}

fn render_mode_for_seconds(
    mode: ModuleMode,
    t: f32,
    mix: f32,
    signal: AuditSignal,
    seconds: f32,
) -> [Vec<f32>; 2] {
    let samples = (seconds * TEST_SAMPLE_RATE).round() as usize;
    let input = signal.render(samples, TEST_SAMPLE_RATE);
    run_module(mode, t, mix, false, input)
}

/// Renders a signal while ramping every parameter from minimum to maximum across
/// the buffer (in steps, on a single persistent processor) to surface zipper
/// noise, clicks, or instability during automation.
fn render_parameter_sweep(mode: ModuleMode, signal: AuditSignal, seconds: f32) -> [Vec<f32>; 2] {
    let samples = (seconds * TEST_SAMPLE_RATE).round() as usize;
    let input = signal.render(samples, TEST_SAMPLE_RATE);
    let steps = 24usize;

    macro_rules! sweep {
        ($module:expr, $make:expr) => {{
            let mut module = $module;
            module.prepare(TEST_SAMPLE_RATE);
            module.reset();
            let total = input[0].len();
            let mut out = [Vec::with_capacity(total), Vec::with_capacity(total)];
            for step in 0..steps {
                let start = total * step / steps;
                let end = total * (step + 1) / steps;
                if end <= start {
                    continue;
                }
                let t = if steps > 1 {
                    step as f32 / (steps - 1) as f32
                } else {
                    0.0
                };
                let params = $make(t);
                let mut chunk = [input[0][start..end].to_vec(), input[1][start..end].to_vec()];
                with_stereo_buffer(&mut chunk, |buffer| module.process_block(buffer, &params));
                out[0].extend_from_slice(&chunk[0]);
                out[1].extend_from_slice(&chunk[1]);
            }
            out
        }};
    }

    match mode {
        ModuleMode::Character(m) => {
            sweep!(Character::default(), |t| character_params(m, t, 1.0, false))
        }
        ModuleMode::Movement(m) => {
            sweep!(Movement::default(), |t| movement_params(m, t, 1.0, false))
        }
        ModuleMode::Diffusion(m) => {
            sweep!(Diffusion::default(), |t| diffusion_params(m, t, 1.0, false))
        }
        ModuleMode::Texture(m) => sweep!(Texture::default(), |t| texture_params(m, t, 1.0, false)),
    }
}

/// Renders a signal through one processor while switching from mode `a` to mode
/// `b` halfway, to surface clicks/NaN on the mode-change crossfade. Both modes
/// must belong to the same module.
fn render_mode_switch(
    a: ModuleMode,
    b: ModuleMode,
    signal: AuditSignal,
    seconds: f32,
) -> [Vec<f32>; 2] {
    let samples = (seconds * TEST_SAMPLE_RATE).round() as usize;
    let input = signal.render(samples, TEST_SAMPLE_RATE);
    let half = samples / 2;

    macro_rules! switch {
        ($module:expr, $pa:expr, $pb:expr) => {{
            let mut module = $module;
            module.prepare(TEST_SAMPLE_RATE);
            module.reset();
            let mut out = [Vec::with_capacity(samples), Vec::with_capacity(samples)];
            for (range, params) in [(0..half, $pa), (half..samples, $pb)] {
                if range.is_empty() {
                    continue;
                }
                let mut chunk = [input[0][range.clone()].to_vec(), input[1][range].to_vec()];
                with_stereo_buffer(&mut chunk, |buffer| module.process_block(buffer, &params));
                out[0].extend_from_slice(&chunk[0]);
                out[1].extend_from_slice(&chunk[1]);
            }
            out
        }};
    }

    match (a, b) {
        (ModuleMode::Character(ma), ModuleMode::Character(mb)) => switch!(
            Character::default(),
            character_params(ma, 0.5, 1.0, false),
            character_params(mb, 0.5, 1.0, false)
        ),
        (ModuleMode::Movement(ma), ModuleMode::Movement(mb)) => switch!(
            Movement::default(),
            movement_params(ma, 0.5, 1.0, false),
            movement_params(mb, 0.5, 1.0, false)
        ),
        (ModuleMode::Diffusion(ma), ModuleMode::Diffusion(mb)) => switch!(
            Diffusion::default(),
            diffusion_params(ma, 0.5, 1.0, false),
            diffusion_params(mb, 0.5, 1.0, false)
        ),
        (ModuleMode::Texture(ma), ModuleMode::Texture(mb)) => switch!(
            Texture::default(),
            texture_params(ma, 0.5, 1.0, false),
            texture_params(mb, 0.5, 1.0, false)
        ),
        _ => panic!("render_mode_switch requires two modes from the same module"),
    }
}

/// Hits a Diffusion mode with a single impulse at maximum feedback for several
/// seconds so a runaway feedback loop would show up as growth or overflow.
fn render_feedback_stress(mode: DiffusionMode, seconds: f32) -> [Vec<f32>; 2] {
    let samples = (seconds * TEST_SAMPLE_RATE).round() as usize;
    let input = make_stereo_impulse(samples);
    let params = diffusion_params(mode, 1.0, 1.0, false);
    let mut module = Diffusion::default();
    module.prepare(TEST_SAMPLE_RATE);
    module.reset();
    process_test_signal(input, |buffer| module.process_block(buffer, &params))
}

// ─── Checklist tests ────────────────────────────────────────────────────────

// Items 1–7: every mode on every signal at min/mid/max parameters stays finite
// and below the safe peak ceiling.
#[test]
fn every_mode_stays_finite_and_bounded_across_signals_and_levels() {
    for mode in all_modes() {
        for signal in AuditSignal::ALL {
            for (level, t) in INTENSITIES {
                let out = render_mode_for_seconds(mode, t, 1.0, signal, 0.25);
                let label = format!("{}/{}/{}", mode_label(mode), signal.name(), level);
                assert_finite_buffer(&label, &out);
                assert_peak_below(&label, &out, MAX_EXPECTED_ABS_SAMPLE);
            }
        }
    }
}

// Item 8 + 17: mix 0% is a clean dry passthrough; mix 100% (engaged) is audible.
#[test]
fn mix_endpoints_are_dry_passthrough_and_audible_wet() {
    let samples = (0.4 * TEST_SAMPLE_RATE) as usize;
    for mode in all_modes() {
        let dry = AuditSignal::Sine440.render(samples, TEST_SAMPLE_RATE);

        // 0% wet must reproduce the dry signal.
        let mix0 = run_module(mode, 0.5, 0.0, false, dry.clone());
        assert_finite_buffer(&format!("{}/mix0", mode_label(mode)), &mix0);
        assert!(
            max_abs_difference(&mix0, &dry) < 1e-4,
            "{} at mix 0% should be a dry passthrough",
            mode_label(mode)
        );

        // 50% and 100% wet must stay sane.
        for mix in [0.5, 1.0] {
            let out = run_module(mode, 0.5, mix, false, dry.clone());
            assert_finite_buffer(&format!("{}/mix{mix}", mode_label(mode)), &out);
            assert_peak_below(
                &format!("{}/mix{mix}", mode_label(mode)),
                &out,
                MAX_EXPECTED_ABS_SAMPLE,
            );
        }

        // 100% wet, engaged, must not be silent (mode is doing something).
        // Use a longer window so delay/reverb-based Diffusion modes (whose taps
        // can sit ~800 ms out) have time to emerge before we measure energy.
        let long_samples = (1.5 * TEST_SAMPLE_RATE) as usize;
        let long_sine = AuditSignal::Sine440.render(long_samples, TEST_SAMPLE_RATE);
        let wet = run_module(mode, 0.5, 1.0, false, long_sine);
        let rms = rms_range(&wet, 0, long_samples);
        assert!(
            rms[0].max(rms[1]) > 0.003,
            "{} is inaudible at 100% wet (rms {:?})",
            mode_label(mode),
            rms
        );
    }
}

// Item 9: module bypass is a clean passthrough.
#[test]
fn module_bypass_is_clean_passthrough() {
    let samples = (0.3 * TEST_SAMPLE_RATE) as usize;
    for mode in all_modes() {
        let dry = AuditSignal::WhiteNoise.render(samples, TEST_SAMPLE_RATE);
        let out = run_module(mode, 0.7, 1.0, true, dry.clone());
        assert_finite_buffer(&format!("{}/bypass", mode_label(mode)), &out);
        assert!(
            max_abs_difference(&out, &dry) < 1e-4,
            "{} module bypass should preserve the input",
            mode_label(mode)
        );
    }
}

// Item 10: global bypass is a clean passthrough through the whole processor.
#[test]
fn global_bypass_is_clean_passthrough() {
    let mut params = Cc22Params::default();
    params.global_bypass = BoolParam::new("global", true);
    params.reset_smoothers();

    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(true);

    let samples = (0.3 * TEST_SAMPLE_RATE) as usize;
    let dry = AuditSignal::WhiteNoise.render(samples, TEST_SAMPLE_RATE);
    let mut audio = dry.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        processor.process_block(buffer, &params, &meters);
    });

    assert_finite_buffer("global-bypass", &audio);
    assert!(
        max_abs_difference(&audio, &dry) < 1e-4,
        "global bypass should preserve the input"
    );
}

// Item 11: switching modes during audio stays finite and bounded (no click blowups).
#[test]
fn mode_switching_during_audio_stays_clean() {
    let pairs = [
        (
            ModuleMode::Character(CharacterMode::Drive),
            ModuleMode::Character(CharacterMode::Fuzz),
        ),
        (
            ModuleMode::Character(CharacterMode::Howl),
            ModuleMode::Character(CharacterMode::Swell),
        ),
        (
            ModuleMode::Movement(MovementMode::Doubler),
            ModuleMode::Movement(MovementMode::Tremolo),
        ),
        (
            ModuleMode::Movement(MovementMode::Phaser),
            ModuleMode::Movement(MovementMode::Pitch),
        ),
        (
            ModuleMode::Diffusion(DiffusionMode::Cascade),
            ModuleMode::Diffusion(DiffusionMode::Reverse),
        ),
        (
            ModuleMode::Diffusion(DiffusionMode::Space),
            ModuleMode::Diffusion(DiffusionMode::Reels),
        ),
        (
            ModuleMode::Diffusion(DiffusionMode::Reels),
            ModuleMode::Diffusion(DiffusionMode::Reverse),
        ),
        (
            ModuleMode::Texture(TextureMode::Filter),
            ModuleMode::Texture(TextureMode::Broken),
        ),
        (
            ModuleMode::Texture(TextureMode::Cassette),
            ModuleMode::Texture(TextureMode::Interference),
        ),
    ];

    for (a, b) in pairs {
        for signal in [AuditSignal::Sine440, AuditSignal::WhiteNoise] {
            let out = render_mode_switch(a, b, signal, 0.5);
            let label = format!(
                "switch/{}->{}/{}",
                mode_label(a),
                mode_label(b),
                signal.name()
            );
            assert_finite_buffer(&label, &out);
            assert_peak_below(&label, &out, MAX_EXPECTED_ABS_SAMPLE);
        }
    }
}

// Item 12: every internal preset's mode combination renders sane through the chain.
#[test]
fn internal_preset_mode_combinations_render_sane() {
    let samples = (0.3 * TEST_SAMPLE_RATE) as usize;
    for preset in crate::presets::internal_presets() {
        let values = preset.values;
        let mut params = Cc22Params::default();
        params.character.mode = EnumParam::new("c", values.character.mode);
        params.movement.mode = EnumParam::new("m", values.movement.mode);
        params.diffusion.mode = EnumParam::new("d", values.diffusion.mode);
        params.texture.mode = EnumParam::new("t", values.texture.mode);
        params.character.bypass = BoolParam::new("cb", false);
        params.movement.bypass = BoolParam::new("mb", false);
        params.diffusion.bypass = BoolParam::new("db", false);
        params.texture.bypass = BoolParam::new("tb", false);
        params.reset_smoothers();

        let meters = Meters::default();
        let mut processor = Processor::default();
        processor.prepare(TEST_SAMPLE_RATE);
        processor.reset(false);

        let mut audio = AuditSignal::WhiteNoise.render(samples, TEST_SAMPLE_RATE);
        with_stereo_buffer(&mut audio, |buffer| {
            processor.process_block(buffer, &params, &meters);
        });

        let label = format!("preset/{}", preset.name);
        assert_finite_buffer(&label, &audio);
        assert_peak_below(&label, &audio, MAX_EXPECTED_ABS_SAMPLE);
    }
}

// Item 13: maximum feedback in Diffusion stays finite, bounded, and decays.
#[test]
fn diffusion_max_feedback_decays_without_runaway() {
    let seconds = 6.0;
    let sr = TEST_SAMPLE_RATE as usize;
    for mode in DiffusionMode::PRODUCT_MODES {
        let out = render_feedback_stress(mode, seconds);
        let label = format!("feedback-stress/{mode:?}");
        assert_finite_buffer(&label, &out);
        assert_peak_below(&label, &out, MAX_EXPECTED_ABS_SAMPLE);

        let early = rms_range(&out, sr, sr + sr / 2); // 1.0–1.5 s
        let late = rms_range(&out, 5 * sr, 5 * sr + sr / 2); // 5.0–5.5 s
        let early_peak = early[0].max(early[1]);
        let late_peak = late[0].max(late[1]);
        assert!(
            late_peak <= early_peak.max(1e-6) * 1.2,
            "{mode:?} feedback is growing (early rms {early_peak}, late rms {late_peak})"
        );
    }
}

// Item 14: maximum drive in Character stays bounded even with a hot input.
#[test]
fn character_max_drive_stays_bounded_with_hot_input() {
    let samples = (0.3 * TEST_SAMPLE_RATE) as usize;
    let hot = make_sine(samples, TEST_SAMPLE_RATE, 220.0, 1.0);
    for mode in CharacterMode::PRODUCT_MODES {
        let out = run_module(ModuleMode::Character(mode), 1.0, 1.0, false, hot.clone());
        let label = format!("max-drive/{mode:?}");
        assert_finite_buffer(&label, &out);
        assert_peak_below(&label, &out, MAX_EXPECTED_ABS_SAMPLE);
    }
}

// Item 15: no relevant DC offset on a steady tone (after transients settle).
#[test]
fn modes_do_not_introduce_relevant_dc_offset() {
    for mode in all_modes() {
        let out = render_mode_for_seconds(mode, 0.5, 1.0, AuditSignal::Sine440, 0.6);
        // Generous limit: this is a guard rail, not a precision check.
        assert_no_dc_offset(&mode_label(mode), &out, 0.10);
    }
}

// Parameter automation sweeps stay finite and bounded (zipper/instability guard).
#[test]
fn parameter_sweeps_stay_finite_and_bounded() {
    for mode in all_modes() {
        for signal in [AuditSignal::Sine440, AuditSignal::WhiteNoise] {
            let out = render_parameter_sweep(mode, signal, 0.5);
            let label = format!("sweep/{}/{}", mode_label(mode), signal.name());
            assert_finite_buffer(&label, &out);
            assert_peak_below(&label, &out, MAX_EXPECTED_ABS_SAMPLE);
        }
    }
}

// Item 18: full chain with every module engaged on a hot signal stays sane.
#[test]
fn full_chain_with_all_modules_engaged_stays_sane() {
    let dangerous = [
        (
            CharacterMode::Fuzz,
            MovementMode::Phaser,
            DiffusionMode::Reels,
            TextureMode::Broken,
        ),
        (
            CharacterMode::Drive,
            MovementMode::Pitch,
            DiffusionMode::Space,
            TextureMode::Interference,
        ),
        (
            CharacterMode::Howl,
            MovementMode::Tremolo,
            DiffusionMode::Collage,
            TextureMode::Squash,
        ),
    ];

    for (character, movement, diffusion, texture) in dangerous {
        let mut params = Cc22Params::default();
        params.character.mode = EnumParam::new("c", character);
        params.character.bypass = BoolParam::new("cb", false);
        params.character.drive = fp(0.85);
        params.movement.mode = EnumParam::new("m", movement);
        params.movement.bypass = BoolParam::new("mb", false);
        params.movement.feedback = fp(0.5);
        params.diffusion.mode = EnumParam::new("d", diffusion);
        params.diffusion.bypass = BoolParam::new("db", false);
        params.diffusion.feedback = fp(0.9);
        params.texture.mode = EnumParam::new("t", texture);
        params.texture.bypass = BoolParam::new("tb", false);
        params.texture.degrade = fp(0.8);
        params.input_gain = fp(6.0);
        params.reset_smoothers();

        let meters = Meters::default();
        let mut processor = Processor::default();
        processor.prepare(TEST_SAMPLE_RATE);
        processor.reset(false);

        for signal in [
            AuditSignal::WhiteNoise,
            AuditSignal::Impulse,
            AuditSignal::Sine110,
        ] {
            let samples = (0.5 * TEST_SAMPLE_RATE) as usize;
            let mut audio = signal.render(samples, TEST_SAMPLE_RATE);
            with_stereo_buffer(&mut audio, |buffer| {
                processor.process_block(buffer, &params, &meters);
            });
            let label = format!("full-chain/{character:?}/{signal:?}");
            assert_finite_buffer(&label, &audio);
            assert_peak_below(&label, &audio, MAX_EXPECTED_ABS_SAMPLE);
        }
    }
}

// Measurement harness: prints peak / RMS / DC per mode for the written audit
// report. Run with `cargo test audit_measurements -- --nocapture`.
#[test]
fn audit_measurements_report() {
    eprintln!("mode,signal,peak,rms,dc");
    let samples = (0.5 * TEST_SAMPLE_RATE) as usize;
    let half = samples / 2;
    for mode in all_modes() {
        for signal in AuditSignal::ALL {
            let out = render_mode_for_seconds(mode, 0.5, 1.0, signal, 0.5);
            assert_finite_buffer(&mode_label(mode), &out);
            let pk = peak(&out);
            let rms = rms_range(&out, half, samples);
            let dc = dc_offset_range(&out, half, samples);
            eprintln!(
                "{},{},{:.4},{:.4},{:.5}",
                mode_label(mode),
                signal.name(),
                pk,
                rms[0].max(rms[1]),
                dc[0].abs().max(dc[1].abs()),
            );
        }
    }
}
