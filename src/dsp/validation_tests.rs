use crate::{
    dsp::{
        bypass::BypassCrossfade,
        character::Character,
        diffusion::Diffusion,
        dry_wet::DryWet,
        eq::Eq,
        movement::Movement,
        test_utils::{
            assert_audio_sane, assert_finite_buffer, assert_peak_below, make_sine,
            make_stereo_different, make_stereo_impulse, make_white_noise, max_abs_difference, peak,
            process_test_signal, render_mode_for_seconds, with_stereo_buffer, TestSignal,
            MAX_EXPECTED_ABS_SAMPLE, TEN_SECONDS_SAMPLES, TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE,
        },
        texture::Texture,
        Processor,
    },
    meters::Meters,
    params::{
        Cc22Params, CharacterParams, DiffusionParams, EqParamRefs, GlobalPostEqParams,
        GlobalPreEqParams, MovementParams, TextureParams,
    },
};
use nih_plug::prelude::{BoolParam, EnumParam, FloatParam, FloatRange};

use super::{
    character::CharacterMode,
    diffusion::DiffusionMode,
    eq::{EqBandType, EqMode},
    movement::MovementMode,
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

// Verifies the final processor output is explicitly safety-limited after global dry/wet and bypass summing.
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

#[test]
fn global_bypass_preserves_input_signal() {
    let mut params = Cc22Params::default();
    params.global_bypass = BoolParam::new("Global Bypass", true);
    params.input_gain = float_param("Input Gain", 12.0);
    params.output_gain = float_param("Output Gain", -12.0);
    params.character.mode = EnumParam::new("Character Mode", CharacterMode::Saturation);
    params.character.drive = float_param("Drive", 1.0);
    params.character.mix = float_param("Mix", 1.0);
    params.reset_smoothers();

    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(true);

    let original = TestSignal::WhiteNoise.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
    let mut audio = original.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        processor.process_block(buffer, &params, &meters);
    });

    assert_audio_sane("processor/global-bypass", &audio);
    assert!(
        max_abs_difference(&audio, &original) < 0.000_001,
        "global bypass should preserve input"
    );
}

// Verifies all Character modes against sine, impulse, noise, and silence.
#[test]
fn character_modes_handle_standard_test_signals() {
    for (mode_id, mode) in [
        ("clean", CharacterMode::Clean),
        ("saturation", CharacterMode::Saturation),
        ("cassette", CharacterMode::Cassette),
        ("drive", CharacterMode::Drive),
        ("sweet", CharacterMode::Sweet),
        ("fuzz", CharacterMode::Fuzz),
        ("howl", CharacterMode::Howl),
        ("swell", CharacterMode::Swell),
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
        ("doubler", MovementMode::Doubler),
        ("phaser", MovementMode::Phaser),
        ("pitch", MovementMode::Pitch),
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
        ("cascade", DiffusionMode::Cascade),
        ("reels", DiffusionMode::Reels),
        ("space", DiffusionMode::Space),
        ("collage", DiffusionMode::Collage),
        ("reverse", DiffusionMode::Reverse),
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
        ("tape", TextureMode::Tape),
        ("filter", TextureMode::Filter),
        ("squash", TextureMode::Squash),
        ("cassette", TextureMode::Cassette),
        ("broken", TextureMode::Broken),
        ("interference", TextureMode::Interference),
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
            let mut params = GlobalPostEqParams::default();
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

#[test]
fn default_post_eq_does_not_cut_sub_100hz() {
    let params = GlobalPostEqParams::default();
    params.reset_smoothers();

    for band in 0..5 {
        assert!(
            params.band_enabled(band).value(),
            "default POST EQ band {} should be enabled",
            band + 1
        );
        assert_eq!(
            params.band_type(band).value(),
            EqBandType::Bell,
            "default POST EQ band {} should be Bell",
            band + 1
        );
        assert!(
            params.band_gain(band).value().abs() < 0.000_001,
            "default POST EQ band {} should be flat",
            band + 1
        );
    }

    let mut eq = Eq::default();
    eq.prepare(TEST_SAMPLE_RATE);
    eq.reset();

    let original = low_frequency_sine(40.0, TEST_BLOCK_SAMPLES * 2, TEST_SAMPLE_RATE);
    let mut audio = original.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        eq.process_block(buffer, &params);
    });

    assert_audio_sane("eq/default/40hz", &audio);
    assert!(
        max_abs_difference(&audio, &original) < 0.000_001,
        "default POST EQ should preserve 40 Hz"
    );
}

#[test]
fn module_bypass_preserves_signal_for_each_module() {
    let signal = TestSignal::Sine.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);

    let mut character_params = CharacterParams::default();
    character_params.mode = EnumParam::new("Character Mode", CharacterMode::Saturation);
    character_params.bypass = BoolParam::new("Character Bypass", true);
    character_params.drive = float_param("Drive", 1.0);
    character_params.mix = float_param("Mix", 1.0);
    character_params.reset_smoothers();
    let mut character = Character::default();
    character.prepare(TEST_SAMPLE_RATE);
    character.reset();
    let mut warmup = signal.clone();
    with_stereo_buffer(&mut warmup, |buffer| {
        character.process_block(buffer, &character_params);
    });
    let mut audio = signal.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        character.process_block(buffer, &character_params);
    });
    assert_audio_sane("character/module-bypass", &audio);
    assert!(max_abs_difference(&audio, &signal) < 0.000_001);

    let mut movement_params = MovementParams::default();
    movement_params.mode = EnumParam::new("Movement Mode", MovementMode::Chorus);
    movement_params.bypass = BoolParam::new("Movement Bypass", true);
    movement_params.depth = float_param("Depth", 1.0);
    movement_params.mix = float_param("Mix", 1.0);
    movement_params.reset_smoothers();
    let mut movement = Movement::default();
    movement.prepare(TEST_SAMPLE_RATE);
    movement.reset();
    let mut warmup = signal.clone();
    with_stereo_buffer(&mut warmup, |buffer| {
        movement.process_block(buffer, &movement_params);
    });
    let mut audio = signal.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        movement.process_block(buffer, &movement_params);
    });
    assert_audio_sane("movement/module-bypass", &audio);
    assert!(max_abs_difference(&audio, &signal) < 0.000_001);

    let mut diffusion_params = DiffusionParams::default();
    diffusion_params.mode = EnumParam::new("Diffusion Mode", DiffusionMode::Delay);
    diffusion_params.bypass = BoolParam::new("Diffusion Bypass", true);
    diffusion_params.feedback = float_param("Feedback", 0.9);
    diffusion_params.mix = float_param("Mix", 1.0);
    diffusion_params.reset_smoothers();
    let mut diffusion = Diffusion::default();
    diffusion.prepare(TEST_SAMPLE_RATE);
    diffusion.reset();
    let mut warmup = signal.clone();
    with_stereo_buffer(&mut warmup, |buffer| {
        diffusion.process_block(buffer, &diffusion_params);
    });
    let mut audio = signal.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        diffusion.process_block(buffer, &diffusion_params);
    });
    assert_audio_sane("diffusion/module-bypass", &audio);
    assert!(max_abs_difference(&audio, &signal) < 0.000_001);

    let mut texture_params = TextureParams::default();
    texture_params.mode = EnumParam::new("Texture Mode", TextureMode::Tape);
    texture_params.bypass = BoolParam::new("Texture Bypass", true);
    texture_params.wow_depth = float_param("Wow Depth", 1.0);
    texture_params.noise_amount = float_param("Noise Amount", 1.0);
    texture_params.mix = float_param("Mix", 1.0);
    texture_params.reset_smoothers();
    let mut texture = Texture::default();
    texture.prepare(TEST_SAMPLE_RATE);
    texture.reset();
    let mut warmup = signal.clone();
    with_stereo_buffer(&mut warmup, |buffer| {
        texture.process_block(buffer, &texture_params);
    });
    let mut audio = signal.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        texture.process_block(buffer, &texture_params);
    });
    assert_audio_sane("texture/module-bypass", &audio);
    assert!(max_abs_difference(&audio, &signal) < 0.000_001);

    let mut eq_params = GlobalPostEqParams::default();
    eq_params.bypass = BoolParam::new("EQ Bypass", true);
    eq_params.band1_gain = float_param("Band 1 Gain", 24.0);
    eq_params.band3_gain = float_param("Band 3 Gain", 24.0);
    eq_params.band5_gain = float_param("Band 5 Gain", 24.0);
    eq_params.reset_smoothers();
    let mut eq = Eq::default();
    eq.prepare(TEST_SAMPLE_RATE);
    eq.reset();
    let mut warmup = signal.clone();
    with_stereo_buffer(&mut warmup, |buffer| {
        eq.process_block(buffer, &eq_params);
    });
    let mut audio = signal.clone();
    with_stereo_buffer(&mut audio, |buffer| {
        eq.process_block(buffer, &eq_params);
    });
    assert_audio_sane("eq/module-bypass", &audio);
    assert!(max_abs_difference(&audio, &signal) < 0.000_001);
}

#[test]
fn processor_dry_wet_endpoints_are_stable() {
    let mut wet_params = Cc22Params::default();
    wet_params.character.mode = EnumParam::new("Character Mode", CharacterMode::Saturation);
    wet_params.character.drive = float_param("Drive", 0.7);
    wet_params.character.mix = float_param("Mix", 1.0);
    wet_params.dry_wet = float_param("Dry/Wet", 1.0);
    wet_params.reset_smoothers();

    let mut dry_params = Cc22Params::default();
    dry_params.character.mode = EnumParam::new("Character Mode", CharacterMode::Saturation);
    dry_params.character.drive = float_param("Drive", 0.7);
    dry_params.character.mix = float_param("Mix", 1.0);
    dry_params.dry_wet = float_param("Dry/Wet", 0.0);
    dry_params.reset_smoothers();

    let original = TestSignal::Sine.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
    let dry_output = process_with_params(original.clone(), &dry_params);
    let wet_output = process_with_params(original.clone(), &wet_params);

    assert_audio_sane("processor/dry-wet/dry", &dry_output);
    assert_audio_sane("processor/dry-wet/wet", &wet_output);
    assert!(
        max_abs_difference(&dry_output, &original) < 0.000_001,
        "dry/wet at 0% should preserve dry signal"
    );
    assert!(
        max_abs_difference(&wet_output, &original) > 0.001,
        "dry/wet at 100% should expose processed signal"
    );
}

#[test]
fn eq_extreme_settings_stay_finite_and_gain_safe() {
    let mut params = GlobalPostEqParams::default();
    params.mode = EnumParam::new("EQ Mode", EqMode::On);
    params.band1_enabled = BoolParam::new("Band 1 Enabled", true);
    params.band1_type = EnumParam::new("Band 1 Type", EqBandType::HighPass);
    params.band1_frequency = float_param("Band 1 Frequency", 500.0);
    params.band1_q = float_param("Band 1 Q", 12.0);
    params.band2_enabled = BoolParam::new("Band 2 Enabled", true);
    params.band2_type = EnumParam::new("Band 2 Type", EqBandType::LowShelf);
    params.band2_frequency = float_param("Band 2 Frequency", 500.0);
    params.band2_gain = float_param("Band 2 Gain", 24.0);
    params.band3_enabled = BoolParam::new("Band 3 Enabled", true);
    params.band3_type = EnumParam::new("Band 3 Type", EqBandType::Bell);
    params.band3_frequency = float_param("Band 3 Frequency", 8_000.0);
    params.band3_gain = float_param("Band 3 Gain", -24.0);
    params.band3_q = float_param("Band 3 Q", 12.0);
    params.band4_enabled = BoolParam::new("Band 4 Enabled", true);
    params.band4_type = EnumParam::new("Band 4 Type", EqBandType::HighShelf);
    params.band4_frequency = float_param("Band 4 Frequency", 16_000.0);
    params.band4_gain = float_param("Band 4 Gain", 24.0);
    params.band5_enabled = BoolParam::new("Band 5 Enabled", true);
    params.band5_type = EnumParam::new("Band 5 Type", EqBandType::LowPass);
    params.band5_frequency = float_param("Band 5 Frequency", 2_000.0);
    params.band5_q = float_param("Band 5 Q", 12.0);
    params.reset_smoothers();

    let mut eq = Eq::default();
    eq.prepare(TEST_SAMPLE_RATE);

    for signal in TestSignal::ALL {
        eq.reset();
        let mut audio = signal.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
        with_stereo_buffer(&mut audio, |buffer| {
            eq.process_block(buffer, &params);
        });
        assert_audio_sane(&format!("eq/extreme/{}", signal.name()), &audio);
    }
}

#[test]
fn diffusion_high_feedback_integration_stays_finite() {
    let mut params = DiffusionParams::default();
    params.mode = EnumParam::new("Diffusion Mode", DiffusionMode::Delay);
    params.time = float_param("Time", 120.0);
    params.feedback = float_param("Feedback", 0.949);
    params.mix = float_param("Mix", 1.0);
    params.tone = float_param("Tone", 0.35);
    params.stereo_offset = float_param("Stereo Offset", 0.35);
    params.width = float_param("Width", 1.0);
    params.reset_smoothers();

    let mut diffusion = Diffusion::default();
    diffusion.prepare(TEST_SAMPLE_RATE);
    diffusion.reset();

    let mut audio = TestSignal::Impulse.render(TEST_BLOCK_SAMPLES * 8, TEST_SAMPLE_RATE);
    with_stereo_buffer(&mut audio, |buffer| {
        diffusion.process_block(buffer, &params);
    });

    assert_audio_sane("diffusion/high-feedback", &audio);
}

#[test]
fn texture_wow_flutter_integration_moves_audio_without_instability() {
    let mut params = TextureParams::default();
    params.mode = EnumParam::new("Texture Mode", TextureMode::Tape);
    params.wow_depth = float_param("Wow Depth", 1.0);
    params.wow_rate = float_param("Wow Rate", 1.7);
    params.flutter_depth = float_param("Flutter Depth", 0.8);
    params.flutter_rate = float_param("Flutter Rate", 17.0);
    params.random_drift = float_param("Random Drift", 0.6);
    params.noise_amount = float_param("Noise Amount", 0.25);
    params.noise_color = float_param("Noise Color", 0.7);
    params.degrade = float_param("Degrade", 0.35);
    params.mix = float_param("Mix", 1.0);
    params.reset_smoothers();

    let original = TestSignal::Sine.render(TEST_BLOCK_SAMPLES * 4, TEST_SAMPLE_RATE);
    let mut audio = original.clone();
    let mut texture = Texture::default();
    texture.prepare(TEST_SAMPLE_RATE);
    texture.reset();
    with_stereo_buffer(&mut audio, |buffer| {
        texture.process_block(buffer, &params);
    });

    assert_audio_sane("texture/wow-flutter", &audio);
    assert!(
        max_abs_difference(&audio, &original) > 0.001,
        "texture wow/flutter should audibly alter the signal"
    );
}

#[test]
fn new_character_modes_validate_active_settings_mix_and_bypass() {
    for (mode_id, mode) in [
        ("drive", CharacterMode::Drive),
        ("sweet", CharacterMode::Sweet),
        ("fuzz", CharacterMode::Fuzz),
        ("howl", CharacterMode::Howl),
        ("swell", CharacterMode::Swell),
    ] {
        for signal in TestSignal::ALL {
            let input = signal.render(TEST_BLOCK_SAMPLES * 2, TEST_SAMPLE_RATE);

            let mut active_params = active_character_params(mode, 1.0, false);
            let mut character = Character::default();
            character.prepare(TEST_SAMPLE_RATE);
            character.reset();
            let mut active = input.clone();
            with_stereo_buffer(&mut active, |buffer| {
                character.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("character/{mode_id}/active/{}", signal.name()),
                &active,
            );

            let dry_params = active_character_params(mode, 0.0, false);
            let mut character = Character::default();
            character.prepare(TEST_SAMPLE_RATE);
            character.reset();
            let mut dry = input.clone();
            with_stereo_buffer(&mut dry, |buffer| {
                character.process_block(buffer, &dry_params);
            });
            assert_audio_sane(&format!("character/{mode_id}/mix0/{}", signal.name()), &dry);
            assert!(
                max_abs_difference(&dry, &input) < 0.000_001,
                "character/{mode_id} mix 0 should preserve dry {}",
                signal.name()
            );

            active_params.bypass = BoolParam::new("Character Bypass", true);
            active_params.reset_smoothers();
            let mut character = Character::default();
            character.prepare(TEST_SAMPLE_RATE);
            character.reset();
            let mut warmup = input.clone();
            with_stereo_buffer(&mut warmup, |buffer| {
                character.process_block(buffer, &active_params);
            });
            let mut bypassed = input.clone();
            with_stereo_buffer(&mut bypassed, |buffer| {
                character.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("character/{mode_id}/bypass/{}", signal.name()),
                &bypassed,
            );
            assert!(
                max_abs_difference(&bypassed, &input) < 0.000_001,
                "character/{mode_id} bypass should preserve dry {}",
                signal.name()
            );
        }
    }
}

#[test]
fn new_movement_modes_validate_active_settings_mix_and_bypass() {
    for (mode_id, mode) in [
        ("doubler", MovementMode::Doubler),
        ("vibrato", MovementMode::Vibrato),
        ("phaser", MovementMode::Phaser),
        ("tremolo", MovementMode::Tremolo),
        ("pitch", MovementMode::Pitch),
    ] {
        for signal in TestSignal::ALL {
            let input = signal.render(TEST_BLOCK_SAMPLES * 2, TEST_SAMPLE_RATE);

            let mut active_params = active_movement_params(mode, 1.0, false);
            let mut movement = Movement::default();
            movement.prepare(TEST_SAMPLE_RATE);
            movement.reset();
            let mut active = input.clone();
            with_stereo_buffer(&mut active, |buffer| {
                movement.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("movement/{mode_id}/active/{}", signal.name()),
                &active,
            );

            let dry_params = active_movement_params(mode, 0.0, false);
            let mut movement = Movement::default();
            movement.prepare(TEST_SAMPLE_RATE);
            movement.reset();
            let mut dry = input.clone();
            with_stereo_buffer(&mut dry, |buffer| {
                movement.process_block(buffer, &dry_params);
            });
            assert_audio_sane(&format!("movement/{mode_id}/mix0/{}", signal.name()), &dry);
            assert!(
                max_abs_difference(&dry, &input) < 0.000_001,
                "movement/{mode_id} mix 0 should preserve dry {}",
                signal.name()
            );

            active_params.bypass = BoolParam::new("Movement Bypass", true);
            active_params.reset_smoothers();
            let mut movement = Movement::default();
            movement.prepare(TEST_SAMPLE_RATE);
            movement.reset();
            let mut warmup = input.clone();
            with_stereo_buffer(&mut warmup, |buffer| {
                movement.process_block(buffer, &active_params);
            });
            let mut bypassed = input.clone();
            with_stereo_buffer(&mut bypassed, |buffer| {
                movement.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("movement/{mode_id}/bypass/{}", signal.name()),
                &bypassed,
            );
            assert!(
                max_abs_difference(&bypassed, &input) < 0.000_001,
                "movement/{mode_id} bypass should preserve dry {}",
                signal.name()
            );
        }
    }
}

#[test]
fn new_diffusion_modes_validate_active_settings_mix_and_bypass() {
    for (mode_id, mode) in [
        ("cascade", DiffusionMode::Cascade),
        ("reels", DiffusionMode::Reels),
        ("space", DiffusionMode::Space),
        ("collage", DiffusionMode::Collage),
        ("reverse", DiffusionMode::Reverse),
    ] {
        for signal in TestSignal::ALL {
            let input = signal.render(TEST_BLOCK_SAMPLES * 4, TEST_SAMPLE_RATE);

            let mut active_params = active_diffusion_params(mode, 1.0, false);
            let mut diffusion = Diffusion::default();
            diffusion.prepare(TEST_SAMPLE_RATE);
            diffusion.reset();
            let mut active = input.clone();
            with_stereo_buffer(&mut active, |buffer| {
                diffusion.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("diffusion/{mode_id}/active/{}", signal.name()),
                &active,
            );

            let dry_params = active_diffusion_params(mode, 0.0, false);
            let mut diffusion = Diffusion::default();
            diffusion.prepare(TEST_SAMPLE_RATE);
            diffusion.reset();
            let mut dry = input.clone();
            with_stereo_buffer(&mut dry, |buffer| {
                diffusion.process_block(buffer, &dry_params);
            });
            assert_audio_sane(&format!("diffusion/{mode_id}/mix0/{}", signal.name()), &dry);
            assert!(
                max_abs_difference(&dry, &input) < 0.000_001,
                "diffusion/{mode_id} mix 0 should preserve dry {}",
                signal.name()
            );

            active_params.bypass = BoolParam::new("Diffusion Bypass", true);
            active_params.reset_smoothers();
            let mut diffusion = Diffusion::default();
            diffusion.prepare(TEST_SAMPLE_RATE);
            diffusion.reset();
            let mut warmup = input.clone();
            with_stereo_buffer(&mut warmup, |buffer| {
                diffusion.process_block(buffer, &active_params);
            });
            let mut bypassed = input.clone();
            with_stereo_buffer(&mut bypassed, |buffer| {
                diffusion.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("diffusion/{mode_id}/bypass/{}", signal.name()),
                &bypassed,
            );
            assert!(
                max_abs_difference(&bypassed, &input) < 0.000_001,
                "diffusion/{mode_id} bypass should preserve dry {}",
                signal.name()
            );
        }
    }
}

#[test]
fn target_character_modes_cover_long_stability_signals_and_parameter_ranges() {
    for (mode_id, mode) in [
        ("howl", CharacterMode::Howl),
        ("swell", CharacterMode::Swell),
    ] {
        for (signal_id, input) in target_test_signals(TEN_SECONDS_SAMPLES) {
            let params = target_character_params(mode, 1.0, false, ParamLevel::Mid);
            let mut character = Character::default();
            character.prepare(TEST_SAMPLE_RATE);
            character.reset();

            let output = process_test_signal(input, |buffer| {
                character.process_block(buffer, &params);
            });

            assert_finite_buffer(&format!("character/{mode_id}/{signal_id}/finite"), &output);
            assert_peak_below(
                &format!("character/{mode_id}/{signal_id}/peak"),
                &output,
                MAX_EXPECTED_ABS_SAMPLE + 0.000_1,
            );
        }

        for level in [ParamLevel::Min, ParamLevel::Mid, ParamLevel::Max] {
            let params = target_character_params(mode, 1.0, false, level);
            let mut character = Character::default();
            character.prepare(TEST_SAMPLE_RATE);
            character.reset();
            let output = render_mode_for_seconds(
                1.0,
                TEST_SAMPLE_RATE,
                |samples, sample_rate| make_sine(samples, sample_rate, 440.0, 0.28),
                |buffer| character.process_block(buffer, &params),
            );
            assert_audio_sane(&format!("character/{mode_id}/params/{level:?}"), &output);
        }
    }
}

#[test]
fn target_diffusion_modes_cover_long_stability_signals_and_parameter_ranges() {
    for (mode_id, mode) in [
        ("reels", DiffusionMode::Reels),
        ("reverse", DiffusionMode::Reverse),
    ] {
        for (signal_id, input) in target_test_signals(TEN_SECONDS_SAMPLES) {
            let params = target_diffusion_params(mode, 1.0, false, ParamLevel::Mid);
            let mut diffusion = Diffusion::default();
            diffusion.prepare(TEST_SAMPLE_RATE);
            diffusion.reset();

            let output = process_test_signal(input, |buffer| {
                diffusion.process_block(buffer, &params);
            });

            assert_finite_buffer(&format!("diffusion/{mode_id}/{signal_id}/finite"), &output);
            assert_peak_below(
                &format!("diffusion/{mode_id}/{signal_id}/peak"),
                &output,
                MAX_EXPECTED_ABS_SAMPLE + 0.000_1,
            );
        }

        for level in [ParamLevel::Min, ParamLevel::Mid, ParamLevel::Max] {
            let params = target_diffusion_params(mode, 1.0, false, level);
            let mut diffusion = Diffusion::default();
            diffusion.prepare(TEST_SAMPLE_RATE);
            diffusion.reset();
            let output = render_mode_for_seconds(
                10.0,
                TEST_SAMPLE_RATE,
                |samples, sample_rate| make_sine(samples, sample_rate, 110.0, 0.24),
                |buffer| diffusion.process_block(buffer, &params),
            );
            assert_audio_sane(&format!("diffusion/{mode_id}/params/{level:?}"), &output);
        }
    }
}

#[test]
fn target_modes_validate_mix_endpoints_bypass_and_audible_wet_output() {
    let dry = make_sine(TEN_SECONDS_SAMPLES, TEST_SAMPLE_RATE, 440.0, 0.28);

    for (mode_id, mode) in [
        ("howl", CharacterMode::Howl),
        ("swell", CharacterMode::Swell),
    ] {
        let mix0 = render_character_mode(mode, 0.0, false, dry.clone());
        let mix50 = render_character_mode(mode, 0.5, false, dry.clone());
        let mix100 = render_character_mode(mode, 1.0, false, dry.clone());
        let bypass = render_character_mode(mode, 1.0, true, dry.clone());

        assert_audio_sane(&format!("character/{mode_id}/mix50"), &mix50);
        assert!(max_abs_difference(&mix0, &dry) < 0.000_001);
        assert!(max_abs_difference(&bypass, &dry) < 0.000_001);
        assert!(
            max_abs_difference(&mix100, &dry) > 0.001 || peak(&mix100) > peak(&dry) * 1.02,
            "character/{mode_id} mix 100 should be audibly different from dry"
        );
    }

    for (mode_id, mode) in [
        ("reels", DiffusionMode::Reels),
        ("reverse", DiffusionMode::Reverse),
    ] {
        let mix0 = render_diffusion_mode(mode, 0.0, false, dry.clone());
        let mix50 = render_diffusion_mode(mode, 0.5, false, dry.clone());
        let mix100 = render_diffusion_mode(mode, 1.0, false, dry.clone());
        let bypass = render_diffusion_mode(mode, 1.0, true, dry.clone());

        assert_audio_sane(&format!("diffusion/{mode_id}/mix50"), &mix50);
        assert!(max_abs_difference(&mix0, &dry) < 0.000_001);
        assert!(max_abs_difference(&bypass, &dry) < 0.000_001);
        assert!(
            max_abs_difference(&mix100, &dry) > 0.001 || peak(&mix100) > peak(&dry) * 1.02,
            "diffusion/{mode_id} mix 100 should be audibly different from dry"
        );
    }
}

#[test]
fn target_modes_survive_continuous_audio_mode_switching() {
    let block_samples = TEST_BLOCK_SAMPLES;
    let blocks = TEN_SECONDS_SAMPLES / block_samples;

    let mut character = Character::default();
    character.prepare(TEST_SAMPLE_RATE);
    character.reset();
    for block in 0..blocks {
        let mode = if block % 2 == 0 {
            CharacterMode::Howl
        } else {
            CharacterMode::Swell
        };
        let params = target_character_params(mode, 1.0, false, ParamLevel::Mid);
        let mut audio = make_white_noise(block_samples, 0.12);
        with_stereo_buffer(&mut audio, |buffer| {
            character.process_block(buffer, &params);
        });
        assert_audio_sane("character/howl-swell/switch", &audio);
    }

    let mut diffusion = Diffusion::default();
    diffusion.prepare(TEST_SAMPLE_RATE);
    diffusion.reset();
    for block in 0..blocks {
        let mode = if block % 2 == 0 {
            DiffusionMode::Reels
        } else {
            DiffusionMode::Reverse
        };
        let params = target_diffusion_params(mode, 1.0, false, ParamLevel::Mid);
        let mut audio = make_white_noise(block_samples, 0.12);
        with_stereo_buffer(&mut audio, |buffer| {
            diffusion.process_block(buffer, &params);
        });
        assert_audio_sane("diffusion/reels-reverse/switch", &audio);
    }
}

#[test]
fn reels_reverse_switching_avoids_large_clicks() {
    let mut diffusion = Diffusion::default();
    diffusion.prepare(TEST_SAMPLE_RATE);
    diffusion.reset();

    let mut previous = 0.0_f32;
    let mut max_step = 0.0_f32;
    for block in 0..24 {
        let mode = if block % 2 == 0 {
            DiffusionMode::Reels
        } else {
            DiffusionMode::Reverse
        };
        let params = target_diffusion_params(mode, 1.0, false, ParamLevel::Mid);
        let mut audio = make_sine(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE, 220.0, 0.24);
        with_stereo_buffer(&mut audio, |buffer| {
            diffusion.process_block(buffer, &params);
        });
        assert_audio_sane(&format!("diffusion/reels-reverse-click/{mode:?}"), &audio);

        for sample in audio[0].iter().copied().skip(64) {
            max_step = max_step.max((sample - previous).abs());
            previous = sample;
        }
    }

    assert!(
        max_step < 1.0,
        "Reels <-> Reverse switching produced a large sample step: {max_step}"
    );
}

#[test]
fn howl_survives_fuzz_howl_sweet_switch_without_large_click() {
    let mut character = Character::default();
    character.prepare(TEST_SAMPLE_RATE);
    character.reset();

    let sequence = [
        CharacterMode::Fuzz,
        CharacterMode::Howl,
        CharacterMode::Sweet,
        CharacterMode::Howl,
    ];
    let mut previous = 0.0_f32;
    let mut max_step = 0.0_f32;

    for mode in sequence {
        let params = target_character_params(mode, 1.0, false, ParamLevel::Max);
        let mut audio = make_sine(TEST_BLOCK_SAMPLES * 2, TEST_SAMPLE_RATE, 220.0, 0.24);
        with_stereo_buffer(&mut audio, |buffer| {
            character.process_block(buffer, &params);
        });
        assert_audio_sane(&format!("character/fuzz-howl-sweet/{mode:?}"), &audio);

        for sample in audio[0].iter().copied().skip(256) {
            max_step = max_step.max((sample - previous).abs());
            previous = sample;
        }
    }

    assert!(
        max_step < 1.25,
        "Fuzz -> Howl -> Sweet switching produced a large sample step: {max_step}"
    );
}

#[test]
fn new_texture_modes_validate_active_settings_mix_and_bypass() {
    for (mode_id, mode) in [
        ("filter", TextureMode::Filter),
        ("squash", TextureMode::Squash),
        ("cassette", TextureMode::Cassette),
        ("broken", TextureMode::Broken),
        ("interference", TextureMode::Interference),
    ] {
        for signal in TestSignal::ALL {
            let input = signal.render(TEST_BLOCK_SAMPLES * 4, TEST_SAMPLE_RATE);

            let mut active_params = active_texture_params(mode, 1.0, false);
            let mut texture = Texture::default();
            texture.prepare(TEST_SAMPLE_RATE);
            texture.reset();
            let mut active = input.clone();
            with_stereo_buffer(&mut active, |buffer| {
                texture.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("texture/{mode_id}/active/{}", signal.name()),
                &active,
            );

            let dry_params = active_texture_params(mode, 0.0, false);
            let mut texture = Texture::default();
            texture.prepare(TEST_SAMPLE_RATE);
            texture.reset();
            let mut dry = input.clone();
            with_stereo_buffer(&mut dry, |buffer| {
                texture.process_block(buffer, &dry_params);
            });
            assert_audio_sane(&format!("texture/{mode_id}/mix0/{}", signal.name()), &dry);
            assert!(
                max_abs_difference(&dry, &input) < 0.000_001,
                "texture/{mode_id} mix 0 should preserve dry {}",
                signal.name()
            );

            active_params.bypass = BoolParam::new("Texture Bypass", true);
            active_params.reset_smoothers();
            let mut texture = Texture::default();
            texture.prepare(TEST_SAMPLE_RATE);
            texture.reset();
            let mut warmup = input.clone();
            with_stereo_buffer(&mut warmup, |buffer| {
                texture.process_block(buffer, &active_params);
            });
            let mut bypassed = input.clone();
            with_stereo_buffer(&mut bypassed, |buffer| {
                texture.process_block(buffer, &active_params);
            });
            assert_audio_sane(
                &format!("texture/{mode_id}/bypass/{}", signal.name()),
                &bypassed,
            );
            assert!(
                max_abs_difference(&bypassed, &input) < 0.000_001,
                "texture/{mode_id} bypass should preserve dry {}",
                signal.name()
            );
        }
    }
}

#[test]
fn new_modes_survive_mode_switch_sequences() {
    let mut character = Character::default();
    character.prepare(TEST_SAMPLE_RATE);
    character.reset();
    for mode in [
        CharacterMode::Drive,
        CharacterMode::Sweet,
        CharacterMode::Fuzz,
        CharacterMode::Howl,
        CharacterMode::Swell,
        CharacterMode::Drive,
    ] {
        let params = active_character_params(mode, 1.0, false);
        let mut audio = TestSignal::WhiteNoise.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
        with_stereo_buffer(&mut audio, |buffer| {
            character.process_block(buffer, &params);
        });
        assert_audio_sane(&format!("character/switch/{mode:?}"), &audio);
    }

    let mut movement = Movement::default();
    movement.prepare(TEST_SAMPLE_RATE);
    movement.reset();
    for mode in [
        MovementMode::Doubler,
        MovementMode::Vibrato,
        MovementMode::Phaser,
        MovementMode::Tremolo,
        MovementMode::Pitch,
        MovementMode::Doubler,
    ] {
        let params = active_movement_params(mode, 1.0, false);
        let mut audio = TestSignal::WhiteNoise.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
        with_stereo_buffer(&mut audio, |buffer| {
            movement.process_block(buffer, &params);
        });
        assert_audio_sane(&format!("movement/switch/{mode:?}"), &audio);
    }

    let mut diffusion = Diffusion::default();
    diffusion.prepare(TEST_SAMPLE_RATE);
    diffusion.reset();
    for mode in [
        DiffusionMode::Cascade,
        DiffusionMode::Reels,
        DiffusionMode::Space,
        DiffusionMode::Collage,
        DiffusionMode::Reverse,
        DiffusionMode::Cascade,
    ] {
        let params = active_diffusion_params(mode, 1.0, false);
        let mut audio = TestSignal::WhiteNoise.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
        with_stereo_buffer(&mut audio, |buffer| {
            diffusion.process_block(buffer, &params);
        });
        assert_audio_sane(&format!("diffusion/switch/{mode:?}"), &audio);
    }

    let mut texture = Texture::default();
    texture.prepare(TEST_SAMPLE_RATE);
    texture.reset();
    for mode in [
        TextureMode::Filter,
        TextureMode::Squash,
        TextureMode::Cassette,
        TextureMode::Broken,
        TextureMode::Interference,
        TextureMode::Filter,
    ] {
        let params = active_texture_params(mode, 1.0, false);
        let mut audio = TestSignal::WhiteNoise.render(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE);
        with_stereo_buffer(&mut audio, |buffer| {
            texture.process_block(buffer, &params);
        });
        assert_audio_sane(&format!("texture/switch/{mode:?}"), &audio);
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

#[derive(Debug, Clone, Copy)]
enum ParamLevel {
    Min,
    Mid,
    Max,
}

fn target_test_signals(samples: usize) -> Vec<(&'static str, [Vec<f32>; 2])> {
    vec![
        (
            "silence",
            TestSignal::Silence.render(samples, TEST_SAMPLE_RATE),
        ),
        (
            "sine-440",
            make_sine(samples, TEST_SAMPLE_RATE, 440.0, 0.28),
        ),
        (
            "sine-110",
            make_sine(samples, TEST_SAMPLE_RATE, 110.0, 0.28),
        ),
        ("impulse", make_stereo_impulse(samples)),
        ("white-noise", make_white_noise(samples, 0.18)),
        (
            "stereo-different-lr",
            make_stereo_different(samples, TEST_SAMPLE_RATE),
        ),
    ]
}

fn render_character_mode(
    mode: CharacterMode,
    mix: f32,
    bypass: bool,
    input: [Vec<f32>; 2],
) -> [Vec<f32>; 2] {
    let params = target_character_params(mode, mix, bypass, ParamLevel::Mid);
    let mut character = Character::default();
    character.prepare(TEST_SAMPLE_RATE);
    character.reset();

    let mut warmup = input.clone();
    with_stereo_buffer(&mut warmup, |buffer| {
        character.process_block(buffer, &params);
    });

    process_test_signal(input, |buffer| {
        character.process_block(buffer, &params);
    })
}

fn render_diffusion_mode(
    mode: DiffusionMode,
    mix: f32,
    bypass: bool,
    input: [Vec<f32>; 2],
) -> [Vec<f32>; 2] {
    let params = target_diffusion_params(mode, mix, bypass, ParamLevel::Mid);
    let mut diffusion = Diffusion::default();
    diffusion.prepare(TEST_SAMPLE_RATE);
    diffusion.reset();

    let mut warmup = input.clone();
    with_stereo_buffer(&mut warmup, |buffer| {
        diffusion.process_block(buffer, &params);
    });

    process_test_signal(input, |buffer| {
        diffusion.process_block(buffer, &params);
    })
}

fn target_character_params(
    mode: CharacterMode,
    mix: f32,
    bypass: bool,
    level: ParamLevel,
) -> CharacterParams {
    let mut params = CharacterParams::default();
    params.mode = EnumParam::new("Character Mode", mode);
    params.bypass = BoolParam::new("Character Bypass", bypass);
    let (drive, age, tone, noise, trim) = match level {
        ParamLevel::Min => (0.0, 0.0, 0.0, 0.0, -12.0),
        ParamLevel::Mid => (0.55, 0.35, 0.55, 0.04, -3.0),
        ParamLevel::Max => (1.0, 1.0, 1.0, 0.25, 0.0),
    };
    params.drive = float_param("Drive", drive);
    params.age = float_param("Age", age);
    params.tone = float_param("Tone", tone);
    params.noise = float_param("Noise", noise);
    params.mix = float_param("Mix", mix);
    params.output_trim = float_param("Output Trim", trim);
    params.reset_smoothers();
    params
}

fn target_diffusion_params(
    mode: DiffusionMode,
    mix: f32,
    bypass: bool,
    level: ParamLevel,
) -> DiffusionParams {
    let mut params = DiffusionParams::default();
    params.mode = EnumParam::new("Diffusion Mode", mode);
    params.bypass = BoolParam::new("Diffusion Bypass", bypass);
    let (time, feedback, size, decay, pre_delay, damping, tone, stereo_offset, width) = match level
    {
        ParamLevel::Min => (1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0),
        ParamLevel::Mid => (320.0, 0.42, 0.58, 0.56, 18.0, 0.45, 0.52, 0.18, 0.85),
        ParamLevel::Max => (2_000.0, 0.95, 1.0, 1.0, 120.0, 1.0, 1.0, 0.5, 1.0),
    };
    params.time = float_param("Time", time);
    params.feedback = float_param("Feedback", feedback);
    params.size = float_param("Size", size);
    params.decay = float_param("Decay", decay);
    params.pre_delay = float_param("Pre-delay", pre_delay);
    params.damping = float_param("Damping", damping);
    params.mix = float_param("Mix", mix);
    params.tone = float_param("Tone", tone);
    params.stereo_offset = float_param("Stereo Offset", stereo_offset);
    params.width = float_param("Width", width);
    params.reset_smoothers();
    params
}

fn process_with_params(mut audio: [Vec<f32>; 2], params: &Cc22Params) -> [Vec<f32>; 2] {
    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(params.global_bypass.value());
    with_stereo_buffer(&mut audio, |buffer| {
        processor.process_block(buffer, params, &meters);
    });
    audio
}

fn active_character_params(mode: CharacterMode, mix: f32, bypass: bool) -> CharacterParams {
    let mut params = CharacterParams::default();
    params.mode = EnumParam::new("Character Mode", mode);
    params.bypass = BoolParam::new("Character Bypass", bypass);
    params.drive = float_param("Drive", 0.46);
    params.age = float_param("Age", 0.35);
    params.tone = float_param("Tone", 0.56);
    params.noise = float_param("Noise", 0.08);
    params.mix = float_param("Mix", mix);
    params.output_trim = float_param("Output Trim", -3.0);
    params.reset_smoothers();
    params
}

fn active_movement_params(mode: MovementMode, mix: f32, bypass: bool) -> MovementParams {
    let mut params = MovementParams::default();
    params.mode = EnumParam::new("Movement Mode", mode);
    params.bypass = BoolParam::new("Movement Bypass", bypass);
    params.rate = float_param("Rate", 1.4);
    params.depth = float_param("Depth", 0.48);
    params.delay = float_param("Delay", 18.0);
    params.feedback = float_param("Feedback", 0.24);
    params.width = float_param("Width", 0.78);
    params.phase = float_param("Phase", 150.0);
    params.tone = float_param("Tone", 0.52);
    params.mix = float_param("Mix", mix);
    params.reset_smoothers();
    params
}

fn active_diffusion_params(mode: DiffusionMode, mix: f32, bypass: bool) -> DiffusionParams {
    let mut params = DiffusionParams::default();
    params.mode = EnumParam::new("Diffusion Mode", mode);
    params.bypass = BoolParam::new("Diffusion Bypass", bypass);
    params.time = float_param("Time", 460.0);
    params.feedback = float_param("Feedback", 0.34);
    params.size = float_param("Size", 0.52);
    params.decay = float_param("Decay", 0.48);
    params.pre_delay = float_param("Pre-delay", 18.0);
    params.damping = float_param("Damping", 0.55);
    params.mix = float_param("Mix", mix);
    params.tone = float_param("Tone", 0.52);
    params.stereo_offset = float_param("Stereo Offset", 0.18);
    params.width = float_param("Width", 0.82);
    params.reset_smoothers();
    params
}

fn active_texture_params(mode: TextureMode, mix: f32, bypass: bool) -> TextureParams {
    let mut params = TextureParams::default();
    params.mode = EnumParam::new("Texture Mode", mode);
    params.bypass = BoolParam::new("Texture Bypass", bypass);
    params.wow_depth = float_param("Wow Depth", 0.24);
    params.wow_rate = float_param("Wow Rate", 0.45);
    params.flutter_depth = float_param("Flutter Depth", 0.10);
    params.flutter_rate = float_param("Flutter Rate", 8.0);
    params.random_drift = float_param("Random Drift", 0.34);
    params.noise_amount = float_param("Noise Amount", 0.28);
    params.noise_color = float_param("Noise Color", 0.58);
    params.degrade = float_param("Degrade", 0.36);
    params.stereo_spread = float_param("Stereo Spread", 0.82);
    params.mix = float_param("Mix", mix);
    params.reset_smoothers();
    params
}

fn float_param(name: &'static str, value: f32) -> FloatParam {
    FloatParam::new(
        name,
        value,
        nih_plug::prelude::FloatRange::Linear {
            min: -100.0,
            max: 100.0,
        },
    )
}

fn low_frequency_sine(frequency: f32, samples: usize, sample_rate: f32) -> [Vec<f32>; 2] {
    let channel = (0..samples)
        .map(|index| {
            let phase = core::f32::consts::TAU * frequency * index as f32 / sample_rate;
            phase.sin() * 0.1
        })
        .collect::<Vec<_>>();
    [channel.clone(), channel]
}

#[test]
fn pre_and_post_eq_are_independent_in_processor() {
    use crate::dsp::eq::EqBandType;

    let pre_gain = 6.0;
    let post_gain = -6.0;
    let frequency = 1_000.0;

    let mut params = Cc22Params::default();
    params.character.bypass = BoolParam::new("Character Bypass", true);
    params.movement.bypass = BoolParam::new("Movement Bypass", true);
    params.diffusion.bypass = BoolParam::new("Diffusion Bypass", true);
    params.texture.bypass = BoolParam::new("Texture Bypass", true);

    disable_all_pre_eq_bands(&mut params.global_pre_eq);
    disable_all_eq_bands(&mut params.global_post_eq);

    params.global_pre_eq.band3_enabled = BoolParam::new("Pre Band 3 Enabled", true);
    params.global_pre_eq.band3_type = EnumParam::new("Pre Band 3 Type", EqBandType::Bell);
    params.global_pre_eq.band3_frequency = FloatParam::new(
        "Pre Band 3 Freq",
        frequency,
        FloatRange::Linear {
            min: 20.0,
            max: 20_000.0,
        },
    );
    params.global_pre_eq.band3_gain = FloatParam::new(
        "Pre Band 3 Gain",
        pre_gain,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    );
    params.global_pre_eq.bypass = BoolParam::new("Pre EQ Bypass", false);

    params.global_post_eq.band3_enabled = BoolParam::new("Post Band 3 Enabled", true);
    params.global_post_eq.band3_type = EnumParam::new("Post Band 3 Type", EqBandType::Bell);
    params.global_post_eq.band3_frequency = FloatParam::new(
        "Post Band 3 Freq",
        frequency,
        FloatRange::Linear {
            min: 20.0,
            max: 20_000.0,
        },
    );
    params.global_post_eq.band3_gain = FloatParam::new(
        "Post Band 3 Gain",
        post_gain,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    );
    params.global_post_eq.bypass = BoolParam::new("Post EQ Bypass", false);

    params.reset_smoothers();
    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(params.global_bypass.value());

    let mut audio = make_sine(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE, frequency, 0.1);
    with_stereo_buffer(&mut audio, |buffer| {
        processor.process_block(buffer, &params, &meters);
    });
    assert_audio_sane("pre-post-eq-independent", &audio);
    assert!(peak(&audio) < 2.0);
}

#[test]
fn every_band_type_produces_valid_processor_output() {
    for band_type in [
        EqBandType::Off,
        EqBandType::Bell,
        EqBandType::LowShelf,
        EqBandType::HighShelf,
        EqBandType::HighPass,
        EqBandType::LowPass,
    ] {
        let mut params = Cc22Params::default();
        params.character.bypass = BoolParam::new("Character Bypass", true);
        params.movement.bypass = BoolParam::new("Movement Bypass", true);
        params.diffusion.bypass = BoolParam::new("Diffusion Bypass", true);
        params.texture.bypass = BoolParam::new("Texture Bypass", true);
        disable_all_pre_eq_bands(&mut params.global_pre_eq);
        disable_all_eq_bands(&mut params.global_post_eq);

        params.global_post_eq.band1_enabled = BoolParam::new("Post Band 1 Enabled", true);
        params.global_post_eq.band1_type = EnumParam::new("Post Band 1 Type", band_type);
        params.global_post_eq.band1_frequency = FloatParam::new(
            "Post Band 1 Freq",
            1_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        params.global_post_eq.band1_gain = FloatParam::new(
            "Post Band 1 Gain",
            6.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.global_post_eq.band1_q = FloatParam::new(
            "Post Band 1 Q",
            1.0,
            FloatRange::Linear {
                min: 0.1,
                max: 12.0,
            },
        );
        params.global_post_eq.bypass = BoolParam::new("Post EQ Bypass", false);
        params.global_post_eq.mode = EnumParam::new("Post EQ Mode", EqMode::On);

        params.reset_smoothers();
        let meters = Meters::default();
        let mut processor = Processor::default();
        processor.prepare(TEST_SAMPLE_RATE);
        processor.reset(params.global_bypass.value());

        let mut audio = make_sine(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE, 440.0, 0.08);
        with_stereo_buffer(&mut audio, |buffer| {
            processor.process_block(buffer, &params, &meters);
        });
        assert_audio_sane(&format!("processor/band-type/{:?}", band_type), &audio);
    }
}

#[test]
fn pre_eq_off_band_does_not_process_audio() {
    let mut params = Cc22Params::default();
    disable_all_modules(&mut params);
    disable_all_eq_bands(&mut params.global_post_eq);

    params.global_pre_eq.band2_enabled = BoolParam::new("Pre Band 2 Enabled", true);
    params.global_pre_eq.band2_type = EnumParam::new("Pre Band 2 Type", EqBandType::Off);
    params.global_pre_eq.band2_frequency = FloatParam::new(
        "Pre Band 2 Freq",
        400.0,
        FloatRange::Linear {
            min: 20.0,
            max: 20_000.0,
        },
    );
    params.global_pre_eq.band2_gain = FloatParam::new(
        "Pre Band 2 Gain",
        0.0,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    );
    params.global_pre_eq.band2_q = FloatParam::new(
        "Pre Band 2 Q",
        1.0,
        FloatRange::Linear {
            min: 0.1,
            max: 12.0,
        },
    );
    params.global_pre_eq.bypass = BoolParam::new("Pre EQ Bypass", false);
    params.global_pre_eq.mode = EnumParam::new("Pre EQ Mode", EqMode::On);

    params.reset_smoothers();
    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(params.global_bypass.value());

    let audio = make_sine(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE, 440.0, 0.1);
    let mut processed = audio.clone();
    with_stereo_buffer(&mut processed, |buffer| {
        processor.process_block(buffer, &params, &meters);
    });
    assert_audio_sane("pre-eq-off-band", &processed);
    assert!(
        max_abs_difference(&processed, &audio) < 0.002,
        "Off band should not alter audio"
    );
}

#[test]
fn post_eq_off_band_does_not_process_audio() {
    let mut params = Cc22Params::default();
    disable_all_modules(&mut params);
    disable_all_pre_eq_bands(&mut params.global_pre_eq);

    params.global_post_eq.band5_enabled = BoolParam::new("Post Band 5 Enabled", true);
    params.global_post_eq.band5_type = EnumParam::new("Post Band 5 Type", EqBandType::Off);
    params.global_post_eq.band5_frequency = FloatParam::new(
        "Post Band 5 Freq",
        10_000.0,
        FloatRange::Linear {
            min: 20.0,
            max: 20_000.0,
        },
    );
    params.global_post_eq.band5_gain = FloatParam::new(
        "Post Band 5 Gain",
        0.0,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    );
    params.global_post_eq.band5_q = FloatParam::new(
        "Post Band 5 Q",
        1.0,
        FloatRange::Linear {
            min: 0.1,
            max: 12.0,
        },
    );
    params.global_post_eq.bypass = BoolParam::new("Post EQ Bypass", false);
    params.global_post_eq.mode = EnumParam::new("Post EQ Mode", EqMode::On);

    params.reset_smoothers();
    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(params.global_bypass.value());

    let audio = make_sine(TEST_BLOCK_SAMPLES, TEST_SAMPLE_RATE, 440.0, 0.1);
    let mut processed = audio.clone();
    with_stereo_buffer(&mut processed, |buffer| {
        processor.process_block(buffer, &params, &meters);
    });
    assert_audio_sane("post-eq-off-band", &processed);
    assert!(
        max_abs_difference(&processed, &audio) < 0.002,
        "Off band should not alter audio"
    );
}

#[test]
fn pre_eq_high_pass_band_attenuates_low_frequencies() {
    let frequency = 1_000.0;
    let low_freq = 50.0;

    let rms_boosted = compute_pre_eq_rms(frequency, EqBandType::Bell, 6.0);
    let rms_hp = compute_pre_eq_rms(frequency, EqBandType::HighPass, 0.0);
    let rms_hp_low = compute_pre_eq_rms(low_freq, EqBandType::HighPass, 0.0);

    assert!(
        rms_hp_low < rms_hp * 0.7,
        "HighPass should attenuate low frequencies: low_rms={rms_hp_low}, mid_rms={rms_hp}"
    );
    assert!(
        rms_hp < rms_boosted * 0.9,
        "HighPass mid RMS {rms_hp} should be lower than Bell boost RMS {rms_boosted}"
    );
}

#[test]
fn post_eq_low_pass_band_attenuates_high_frequencies() {
    let mid_freq = 500.0;
    let high_freq = 8_000.0;

    let rms_mid = compute_post_eq_rms(mid_freq, EqBandType::LowPass, 0.0);
    let rms_high = compute_post_eq_rms(high_freq, EqBandType::LowPass, 0.0);

    assert!(
        rms_high < rms_mid * 0.7,
        "LowPass should attenuate high frequencies: mid_rms={rms_mid}, high_rms={rms_high}"
    );
}

#[test]
fn pre_eq_disabled_band_passes_signal_unaffected() {
    let freq = 1_000.0;

    let rms_disabled = compute_pre_eq_rms_disabled(freq, EqBandType::Bell, 12.0);
    let rms_reference = compute_pre_eq_rms_flat(freq);

    assert!(
        (rms_disabled - rms_reference).abs() < 0.002,
        "Disabled band should not alter signal: disabled={rms_disabled}, ref={rms_reference}"
    );
}

#[test]
fn post_eq_disabled_band_passes_signal_unaffected() {
    let freq = 1_000.0;

    let rms_disabled = compute_post_eq_rms_disabled(freq, EqBandType::Bell, 12.0);
    let rms_reference = compute_post_eq_rms_flat(freq);

    assert!(
        (rms_disabled - rms_reference).abs() < 0.002,
        "Disabled POST band should not alter signal: disabled={rms_disabled}, ref={rms_reference}"
    );
}

#[test]
fn both_eqs_active_apply_cumulative_effect() {
    let freq = 1_000.0;

    let rms_both = compute_both_eq_rms(freq, 3.0, 3.0);
    let rms_pre_only = compute_pre_eq_rms(freq, EqBandType::Bell, 3.0);
    let rms_post_only = compute_post_eq_rms(freq, EqBandType::Bell, 3.0);

    assert!(
        rms_both > rms_pre_only * 1.1,
        "Both EQs should boost more than PRE alone"
    );
    assert!(
        rms_both > rms_post_only * 1.1,
        "Both EQs should boost more than POST alone"
    );
}

fn disable_all_eq_bands(eq: &mut GlobalPostEqParams) {
    eq.band1_enabled = BoolParam::new("EB1E", false);
    eq.band2_enabled = BoolParam::new("EB2E", false);
    eq.band3_enabled = BoolParam::new("EB3E", false);
    eq.band4_enabled = BoolParam::new("EB4E", false);
    eq.band5_enabled = BoolParam::new("EB5E", false);
    eq.bypass = BoolParam::new("EB", true);
}

fn disable_all_pre_eq_bands(eq: &mut GlobalPreEqParams) {
    eq.band1_enabled = BoolParam::new("PE1E", false);
    eq.band2_enabled = BoolParam::new("PE2E", false);
    eq.band3_enabled = BoolParam::new("PE3E", false);
    eq.band4_enabled = BoolParam::new("PE4E", false);
    eq.band5_enabled = BoolParam::new("PE5E", false);
    eq.bypass = BoolParam::new("PEB", true);
}

fn disable_all_modules(params: &mut Cc22Params) {
    params.character.bypass = BoolParam::new("Character Bypass", true);
    params.movement.bypass = BoolParam::new("Movement Bypass", true);
    params.diffusion.bypass = BoolParam::new("Diffusion Bypass", true);
    params.texture.bypass = BoolParam::new("Texture Bypass", true);
}

fn compute_pre_eq_rms(sine_freq: f32, band_type: EqBandType, gain_db: f32) -> f32 {
    compute_eq_rms_with_config(
        |params| {
            disable_all_pre_eq_bands(&mut params.global_pre_eq);
            params.global_pre_eq.band2_enabled = BoolParam::new("PB2E", true);
            params.global_pre_eq.band2_type = EnumParam::new("PB2T", band_type);
            params.global_pre_eq.band2_frequency = FloatParam::new(
                "PB2F",
                1_000.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 20_000.0,
                },
            );
            params.global_pre_eq.band2_gain = FloatParam::new(
                "PB2G",
                gain_db,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            );
            params.global_pre_eq.band2_q = FloatParam::new(
                "PB2Q",
                1.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 12.0,
                },
            );
            params.global_pre_eq.bypass = BoolParam::new("PB", false);
            params.global_pre_eq.mode = EnumParam::new("PM", EqMode::On);
        },
        sine_freq,
    )
}

fn compute_post_eq_rms(sine_freq: f32, band_type: EqBandType, gain_db: f32) -> f32 {
    compute_eq_rms_with_config(
        |params| {
            disable_all_eq_bands(&mut params.global_post_eq);
            params.global_post_eq.band2_enabled = BoolParam::new("PB2E", true);
            params.global_post_eq.band2_type = EnumParam::new("PB2T", band_type);
            params.global_post_eq.band2_frequency = FloatParam::new(
                "PB2F",
                1_000.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 20_000.0,
                },
            );
            params.global_post_eq.band2_gain = FloatParam::new(
                "PB2G",
                gain_db,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            );
            params.global_post_eq.band2_q = FloatParam::new(
                "PB2Q",
                1.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 12.0,
                },
            );
            params.global_post_eq.bypass = BoolParam::new("PB", false);
            params.global_post_eq.mode = EnumParam::new("PM", EqMode::On);
        },
        sine_freq,
    )
}

fn compute_pre_eq_rms_disabled(sine_freq: f32, band_type: EqBandType, gain_db: f32) -> f32 {
    compute_eq_rms_with_config(
        |params| {
            disable_all_pre_eq_bands(&mut params.global_pre_eq);
            params.global_pre_eq.band2_enabled = BoolParam::new("PB2E", false);
            params.global_pre_eq.band2_type = EnumParam::new("PB2T", band_type);
            params.global_pre_eq.band2_frequency = FloatParam::new(
                "PB2F",
                1_000.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 20_000.0,
                },
            );
            params.global_pre_eq.band2_gain = FloatParam::new(
                "PB2G",
                gain_db,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            );
            params.global_pre_eq.band2_q = FloatParam::new(
                "PB2Q",
                1.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 12.0,
                },
            );
            params.global_pre_eq.bypass = BoolParam::new("PB", false);
            params.global_pre_eq.mode = EnumParam::new("PM", EqMode::On);
        },
        sine_freq,
    )
}

fn compute_post_eq_rms_disabled(sine_freq: f32, band_type: EqBandType, gain_db: f32) -> f32 {
    compute_eq_rms_with_config(
        |params| {
            disable_all_eq_bands(&mut params.global_post_eq);
            params.global_post_eq.band2_enabled = BoolParam::new("PB2E", false);
            params.global_post_eq.band2_type = EnumParam::new("PB2T", band_type);
            params.global_post_eq.band2_frequency = FloatParam::new(
                "PB2F",
                1_000.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 20_000.0,
                },
            );
            params.global_post_eq.band2_gain = FloatParam::new(
                "PB2G",
                gain_db,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            );
            params.global_post_eq.band2_q = FloatParam::new(
                "PB2Q",
                1.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 12.0,
                },
            );
            params.global_post_eq.bypass = BoolParam::new("PB", false);
            params.global_post_eq.mode = EnumParam::new("PM", EqMode::On);
        },
        sine_freq,
    )
}

fn compute_pre_eq_rms_flat(sine_freq: f32) -> f32 {
    compute_eq_rms_with_config(
        |params| {
            disable_all_pre_eq_bands(&mut params.global_pre_eq);
            params.global_pre_eq.bypass = BoolParam::new("PB", false);
            params.global_pre_eq.mode = EnumParam::new("PM", EqMode::On);
        },
        sine_freq,
    )
}

fn compute_post_eq_rms_flat(sine_freq: f32) -> f32 {
    compute_eq_rms_with_config(
        |params| {
            disable_all_eq_bands(&mut params.global_post_eq);
            params.global_post_eq.bypass = BoolParam::new("PB", false);
            params.global_post_eq.mode = EnumParam::new("PM", EqMode::On);
        },
        sine_freq,
    )
}

fn compute_both_eq_rms(sine_freq: f32, pre_gain: f32, post_gain: f32) -> f32 {
    compute_eq_rms_with_config(
        |params| {
            disable_all_pre_eq_bands(&mut params.global_pre_eq);
            disable_all_eq_bands(&mut params.global_post_eq);

            params.global_pre_eq.band2_enabled = BoolParam::new("PreE", true);
            params.global_pre_eq.band2_type = EnumParam::new("PreT", EqBandType::Bell);
            params.global_pre_eq.band2_frequency = FloatParam::new(
                "PreF",
                1_000.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 20_000.0,
                },
            );
            params.global_pre_eq.band2_gain = FloatParam::new(
                "PreG",
                pre_gain,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            );
            params.global_pre_eq.band2_q = FloatParam::new(
                "PreQ",
                1.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 12.0,
                },
            );
            params.global_pre_eq.bypass = BoolParam::new("PreB", false);
            params.global_pre_eq.mode = EnumParam::new("PreM", EqMode::On);

            params.global_post_eq.band2_enabled = BoolParam::new("PostE", true);
            params.global_post_eq.band2_type = EnumParam::new("PostT", EqBandType::Bell);
            params.global_post_eq.band2_frequency = FloatParam::new(
                "PostF",
                1_000.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 20_000.0,
                },
            );
            params.global_post_eq.band2_gain = FloatParam::new(
                "PostG",
                post_gain,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            );
            params.global_post_eq.band2_q = FloatParam::new(
                "PostQ",
                1.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 12.0,
                },
            );
            params.global_post_eq.bypass = BoolParam::new("PostB", false);
            params.global_post_eq.mode = EnumParam::new("PostM", EqMode::On);
        },
        sine_freq,
    )
}

fn compute_eq_rms_with_config(configure: impl Fn(&mut Cc22Params), sine_freq: f32) -> f32 {
    let mut params = Cc22Params::default();
    disable_all_modules(&mut params);
    disable_all_pre_eq_bands(&mut params.global_pre_eq);
    disable_all_eq_bands(&mut params.global_post_eq);
    configure(&mut params);
    params.reset_smoothers();

    let meters = Meters::default();
    let mut processor = Processor::default();
    processor.prepare(TEST_SAMPLE_RATE);
    processor.reset(params.global_bypass.value());

    let samples = 4_800;
    let mut sum = 0.0_f32;
    for index in 0..samples {
        let phase = core::f32::consts::TAU * sine_freq * index as f32 / TEST_SAMPLE_RATE;
        let sample = phase.sin() * 0.1;
        let mut audio = [vec![sample; 1], vec![sample; 1]];
        with_stereo_buffer(&mut audio, |buffer| {
            processor.process_block(buffer, &params, &meters);
        });
        sum += audio[0][0] * audio[0][0];
    }

    (sum / samples as f32).sqrt()
}
