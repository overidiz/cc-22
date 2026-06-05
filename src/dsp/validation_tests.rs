use crate::{
    dsp::{
        bypass::BypassCrossfade,
        character::Character,
        diffusion::Diffusion,
        dry_wet::DryWet,
        eq::Eq,
        movement::Movement,
        test_utils::{
            assert_audio_sane, max_abs_difference, with_stereo_buffer, TestSignal,
            TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE,
        },
        texture::Texture,
        Processor,
    },
    meters::Meters,
    params::{
        Cc22Params, CharacterParams, DiffusionParams, EqParams, MovementParams, TextureParams,
    },
};
use nih_plug::prelude::EnumParam;

use super::{
    character::CharacterMode, diffusion::DiffusionMode, eq::EqMode, movement::MovementMode,
    texture::TextureMode,
};

// Verifies that the full processor accepts standard test signals without NaN, infinity, or runaway gain.
#[test]
fn processor_handles_standard_test_signals() {
    for signal in TestSignal::ALL {
        let params = Cc22Params::default();
        params.reset_smoothers();
        let meters = Meters::default();
        let mut processor = Processor::default();
        processor.prepare(TEST_SAMPLE_RATE);
        processor.reset(params.global_bypass.value());

        let mut audio = signal.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
        with_stereo_buffer(&mut audio, |buffer| {
            processor.process_block(buffer, &params, &meters);
        });

        assert_audio_sane(signal.name(), &audio);
    }
}

// Verifies the final processor output is still clamped after global dry/wet and bypass summing.
#[test]
fn processor_final_output_is_sanitized_for_extreme_input() {
    let params = Cc22Params::default();
    params.reset_smoothers();
    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(params.global_bypass.value());

    let mut audio = [
        vec![100.0; TEST_BLOCK_SAMPLES],
        vec![-100.0; TEST_BLOCK_SAMPLES],
    ];
    with_stereo_buffer(&mut audio, |buffer| {
        processor.process_block(buffer, &params, &meters);
    });

    assert_audio_sane("processor/extreme-input", &audio);
}

// Verifies all Character modes against sine, impulse, noise, and silence.
#[test]
fn character_modes_handle_standard_test_signals() {
    for (mode_id, mode) in [
        ("clean", CharacterMode::Clean),
        ("saturation", CharacterMode::Saturation),
        ("cassette", CharacterMode::Cassette),
    ] {
        for signal in TestSignal::ALL {
            let mut params = CharacterParams::default();
            params.mode = EnumParam::new("Character Mode", mode);
            params.reset_smoothers();

            let mut character = Character::default();
            character.prepare(TEST_SAMPLE_RATE);
            character.reset();

            let mut audio = signal.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
            with_stereo_buffer(&mut audio, |buffer| {
                character.process_block(buffer, &params);
            });

            assert_audio_sane(&format!("character/{mode_id}/{}", signal.name()), &audio);
        }
    }
}

// Verifies all Movement modes against sine, impulse, noise, and silence.
#[test]
fn movement_modes_handle_standard_test_signals() {
    for (mode_id, mode) in [
        ("off", MovementMode::Off),
        ("chorus", MovementMode::Chorus),
        ("vibrato", MovementMode::Vibrato),
        ("tremolo", MovementMode::Tremolo),
    ] {
        for signal in TestSignal::ALL {
            let mut params = MovementParams::default();
            params.mode = EnumParam::new("Movement Mode", mode);
            params.reset_smoothers();

            let mut movement = Movement::default();
            movement.prepare(TEST_SAMPLE_RATE);
            movement.reset();

            let mut audio = signal.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
            with_stereo_buffer(&mut audio, |buffer| {
                movement.process_block(buffer, &params);
            });

            assert_audio_sane(&format!("movement/{mode_id}/{}", signal.name()), &audio);
        }
    }
}

// Verifies all Diffusion modes against sine, impulse, noise, and silence.
#[test]
fn diffusion_modes_handle_standard_test_signals() {
    for (mode_id, mode) in [
        ("off", DiffusionMode::Off),
        ("delay", DiffusionMode::Delay),
        ("slap", DiffusionMode::Slap),
        ("reverb", DiffusionMode::Reverb),
    ] {
        for signal in TestSignal::ALL {
            let mut params = DiffusionParams::default();
            params.mode = EnumParam::new("Diffusion Mode", mode);
            params.reset_smoothers();

            let mut diffusion = Diffusion::default();
            diffusion.prepare(TEST_SAMPLE_RATE);
            diffusion.reset();

            let mut audio = signal.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
            with_stereo_buffer(&mut audio, |buffer| {
                diffusion.process_block(buffer, &params);
            });

            assert_audio_sane(&format!("diffusion/{mode_id}/{}", signal.name()), &audio);
        }
    }
}

// Verifies all Texture modes against sine, impulse, noise, and silence.
#[test]
fn texture_modes_handle_standard_test_signals() {
    for (mode_id, mode) in [
        ("off", TextureMode::Off),
        ("wow-flutter", TextureMode::WowFlutter),
        ("noise", TextureMode::Noise),
    ] {
        for signal in TestSignal::ALL {
            let mut params = TextureParams::default();
            params.mode = EnumParam::new("Texture Mode", mode);
            params.reset_smoothers();

            let mut texture = Texture::default();
            texture.prepare(TEST_SAMPLE_RATE);
            texture.reset();

            let mut audio = signal.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
            with_stereo_buffer(&mut audio, |buffer| {
                texture.process_block(buffer, &params);
            });

            assert_audio_sane(&format!("texture/{mode_id}/{}", signal.name()), &audio);
        }
    }
}

// Verifies EQ on/off modes against sine, impulse, noise, and silence.
#[test]
fn eq_modes_handle_standard_test_signals() {
    for (mode_id, mode) in [("off", EqMode::Off), ("on", EqMode::On)] {
        for signal in TestSignal::ALL {
            let mut params = EqParams::default();
            params.mode = EnumParam::new("EQ Mode", mode);
            params.reset_smoothers();

            let mut eq = Eq::default();
            eq.prepare(TEST_SAMPLE_RATE);
            eq.reset();

            let mut audio = signal.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
            with_stereo_buffer(&mut audio, |buffer| {
                eq.process_block(buffer, &params);
            });

            assert_audio_sane(&format!("eq/{mode_id}/{}", signal.name()), &audio);
        }
    }
}

// Verifies bypass crossfade endpoints and reset behavior without depending on a DAW bypass lane.
#[test]
fn bypass_crossfade_reaches_expected_dry_and_processed_states() {
    let mut bypass = BypassCrossfade::default();
    bypass.prepare(TEST_SAMPLE_RATE);

    bypass.reset(false);
    assert!((bypass.next_active_mix() - 1.0).abs() < 0.000_001);

    bypass.reset(true);
    assert!(bypass.next_active_mix().abs() < 0.000_001);
    assert!((bypass.mix(0.25, 0.75, 0.0) - 0.25).abs() < 0.000_001);
    assert!((bypass.mix(0.25, 0.75, 1.0) - 0.75).abs() < 0.000_001);
}

// Verifies dry/wet equal-power endpoints and that the midpoint stays finite and gain-safe.
#[test]
fn dry_wet_crossfade_is_finite_and_gain_safe() {
    let dry_wet = DryWet;
    let dry = 0.4;
    let wet = -0.3;

    assert!((dry_wet.mix(dry, wet, 0.0) - dry).abs() < 0.000_001);
    assert!((dry_wet.mix(dry, wet, 1.0) - wet).abs() < 0.000_001);

    let middle = dry_wet.mix(dry, wet, 0.5);
    assert!(middle.is_finite());
    assert!(middle.abs() <= 1.0);
}

// Verifies prepare/reset after a sample-rate change keeps the processor stable.
#[test]
fn processor_handles_sample_rate_change_and_reset() {
    let params = Cc22Params::default();
    params.reset_smoothers();
    let meters = Meters::default();
    let mut processor = Processor::default();

    for sample_rate in [44_100.0, 96_000.0] {
        processor.prepare(sample_rate);
        processor.reset(params.global_bypass.value());

        let mut audio = TestSignal::WhiteNoise.render(TEST_BLOCK_SAMPLES, sample_rate);
        with_stereo_buffer(&mut audio, |buffer| {
            processor.process_block(buffer, &params, &meters);
        });

        assert_audio_sane(&format!("processor/sample-rate/{sample_rate}"), &audio);
    }
}

// Verifies reset clears stateful delay/reverb memory enough for repeatable output from the same input.
#[test]
fn diffusion_reset_makes_reverb_repeatable() {
    let mut params = DiffusionParams::default();
    params.mode = EnumParam::new("Diffusion Mode", DiffusionMode::Reverb);
    params.reset_smoothers();

    let mut diffusion = Diffusion::default();
    diffusion.prepare(TEST_SAMPLE_RATE);

    // Establish the selected mode before comparing reset output, so this test validates state
    // clearing instead of the intentional first-time mode crossfade.
    let mut warmup = TestSignal::Silence.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
    with_stereo_buffer(&mut warmup, |buffer| {
        diffusion.process_block(buffer, &params);
    });

    let mut first = TestSignal::Impulse.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
    diffusion.reset();
    with_stereo_buffer(&mut first, |buffer| {
        diffusion.process_block(buffer, &params);
    });

    let mut second = TestSignal::Impulse.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
    diffusion.reset();
    params.reset_smoothers();
    with_stereo_buffer(&mut second, |buffer| {
        diffusion.process_block(buffer, &params);
    });

    assert_audio_sane("diffusion/reset/first", &first);
    assert_audio_sane("diffusion/reset/second", &second);
    assert!(
        max_abs_difference(&first, &second) < 0.000_001,
        "reverb output changed after reset for identical input"
    );
}
