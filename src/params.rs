use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;

use crate::dsp::{
    chain::{validate_chain_order, ChainModule},
    character::CharacterMode,
    diffusion::DiffusionMode,
    eq::{EqBandType, EqMode},
    movement::LfoShape,
    movement::MovementMode,
    texture::TextureMode,
};
use crate::ui::{BASE_HEIGHT, BASE_WIDTH};

#[derive(Params)]
pub struct Cc22Params {
    pub editor_state: Arc<EguiState>,

    #[id = "input_gain"]
    pub input_gain: FloatParam,

    #[nested(group = "Global Pre EQ")]
    pub global_pre_eq: GlobalPreEqParams,

    #[nested(group = "Character")]
    pub character: CharacterParams,

    #[nested(group = "Character Pre EQ")]
    pub character_pre_eq: CharacterPreEq,

    #[nested(group = "Character Post EQ")]
    pub character_post_eq: CharacterPostEq,

    #[nested(group = "Movement")]
    pub movement: MovementParams,

    #[nested(group = "Movement Pre EQ")]
    pub movement_pre_eq: MovementPreEq,

    #[nested(group = "Movement Post EQ")]
    pub movement_post_eq: MovementPostEq,

    #[nested(group = "Diffusion")]
    pub diffusion: DiffusionParams,

    #[nested(group = "Diffusion Pre EQ")]
    pub diffusion_pre_eq: DiffusionPreEq,

    #[nested(group = "Diffusion Post EQ")]
    pub diffusion_post_eq: DiffusionPostEq,

    #[nested(group = "Texture")]
    pub texture: TextureParams,

    #[nested(group = "Texture Pre EQ")]
    pub texture_pre_eq: TexturePreEq,

    #[nested(group = "Texture Post EQ")]
    pub texture_post_eq: TexturePostEq,

    #[nested(group = "Global Post EQ")]
    pub global_post_eq: GlobalPostEqParams,

    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,

    #[id = "global_bypass"]
    pub global_bypass: BoolParam,

    #[id = "chain_s0"]
    pub chain_slot_0: IntParam,

    #[id = "chain_s1"]
    pub chain_slot_1: IntParam,

    #[id = "chain_s2"]
    pub chain_slot_2: IntParam,

    #[id = "chain_s3"]
    pub chain_slot_3: IntParam,
}

impl Default for Cc22Params {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(BASE_WIDTH as u32, BASE_HEIGHT as u32),
            input_gain: gain_param("Input Gain"),
            global_pre_eq: GlobalPreEqParams::default(),
            character: CharacterParams::default(),
            character_pre_eq: CharacterPreEq::default(),
            character_post_eq: CharacterPostEq::default(),
            movement: MovementParams::default(),
            movement_pre_eq: MovementPreEq::default(),
            movement_post_eq: MovementPostEq::default(),
            diffusion: DiffusionParams::default(),
            diffusion_pre_eq: DiffusionPreEq::default(),
            diffusion_post_eq: DiffusionPostEq::default(),
            texture: TextureParams::default(),
            texture_pre_eq: TexturePreEq::default(),
            texture_post_eq: TexturePostEq::default(),
            global_post_eq: GlobalPostEqParams::default(),
            output_gain: gain_param("Output Gain"),
            dry_wet: FloatParam::new("Dry/Wet", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(1))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            global_bypass: BoolParam::new("Global Bypass", false)
                .with_value_to_string(formatters::v2s_bool_bypass())
                .with_string_to_value(formatters::s2v_bool_bypass()),
            chain_slot_0: chain_slot_param(0),
            chain_slot_1: chain_slot_param(1),
            chain_slot_2: chain_slot_param(2),
            chain_slot_3: chain_slot_param(3),
        }
    }
}

impl Cc22Params {
    pub fn chain_order(&self) -> [ChainModule; 4] {
        let slots = [
            self.chain_slot_0.value() as usize,
            self.chain_slot_1.value() as usize,
            self.chain_slot_2.value() as usize,
            self.chain_slot_3.value() as usize,
        ];
        validate_chain_order(&slots)
    }

    pub fn reset_smoothers(&self) {
        self.input_gain.smoothed.reset(self.input_gain.value());
        self.global_pre_eq.reset_smoothers();
        self.character.reset_smoothers();
        self.character_pre_eq.reset_smoothers();
        self.character_post_eq.reset_smoothers();
        self.movement.reset_smoothers();
        self.movement_pre_eq.reset_smoothers();
        self.movement_post_eq.reset_smoothers();
        self.diffusion.reset_smoothers();
        self.diffusion_pre_eq.reset_smoothers();
        self.diffusion_post_eq.reset_smoothers();
        self.texture.reset_smoothers();
        self.texture_pre_eq.reset_smoothers();
        self.texture_post_eq.reset_smoothers();
        self.global_post_eq.reset_smoothers();
        self.output_gain.smoothed.reset(self.output_gain.value());
        self.dry_wet.smoothed.reset(self.dry_wet.value());
    }
}

#[derive(Params)]
pub struct CharacterParams {
    #[id = "character_mode"]
    pub mode: EnumParam<CharacterMode>,

    #[id = "character_bypass"]
    pub bypass: BoolParam,

    #[id = "character_drive"]
    pub drive: FloatParam,

    #[id = "character_age"]
    pub age: FloatParam,

    #[id = "character_tone"]
    pub tone: FloatParam,

    #[id = "character_noise"]
    pub noise: FloatParam,

    #[id = "character_mix"]
    pub mix: FloatParam,

    #[id = "character_output_trim"]
    pub output_trim: FloatParam,
}

impl Default for CharacterParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Character Mode", CharacterMode::Clean),
            bypass: module_bypass_param("Character Bypass"),
            drive: percent_param("Drive", 0.0, 20.0),
            age: percent_param("Age", 0.0, 40.0),
            tone: percent_param("Tone", 0.5, 20.0),
            noise: percent_param("Noise", 0.0, 40.0),
            mix: percent_param("Mix", 1.0, 30.0),
            output_trim: trim_param("Output Trim"),
        }
    }
}

impl CharacterParams {
    pub fn reset_smoothers(&self) {
        self.drive.smoothed.reset(self.drive.value());
        self.age.smoothed.reset(self.age.value());
        self.tone.smoothed.reset(self.tone.value());
        self.noise.smoothed.reset(self.noise.value());
        self.mix.smoothed.reset(self.mix.value());
        self.output_trim.smoothed.reset(self.output_trim.value());
    }
}

#[derive(Params)]
pub struct MovementParams {
    #[id = "movement_mode"]
    pub mode: EnumParam<MovementMode>,

    #[id = "movement_bypass"]
    pub bypass: BoolParam,

    #[id = "movement_rate"]
    pub rate: FloatParam,

    #[id = "movement_depth"]
    pub depth: FloatParam,

    #[id = "movement_shape"]
    pub shape: EnumParam<LfoShape>,

    #[id = "movement_delay"]
    pub delay: FloatParam,

    #[id = "movement_feedback"]
    pub feedback: FloatParam,

    #[id = "movement_width"]
    pub width: FloatParam,

    #[id = "movement_phase"]
    pub phase: FloatParam,

    #[id = "movement_tone"]
    pub tone: FloatParam,

    #[id = "movement_mix"]
    pub mix: FloatParam,
}

impl Default for MovementParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Movement Mode", MovementMode::Off),
            bypass: module_bypass_param("Movement Bypass"),
            rate: FloatParam::new(
                "Rate",
                0.45,
                FloatRange::Skewed {
                    min: 0.05,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(80.0))
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            depth: percent_param("Depth", 0.35, 60.0),
            shape: EnumParam::new("Shape", LfoShape::Sine),
            delay: FloatParam::new(
                "Delay",
                16.0,
                FloatRange::Linear {
                    min: 5.0,
                    max: 30.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(60.0))
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            feedback: FloatParam::new("Feedback", 0.12, FloatRange::Linear { min: 0.0, max: 0.6 })
                .with_smoother(SmoothingStyle::Linear(60.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(1))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            width: percent_param("Width", 0.85, 60.0),
            phase: FloatParam::new(
                "Phase",
                180.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 180.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(60.0))
            .with_unit(" deg")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            tone: percent_param("Tone", 0.55, 50.0),
            mix: percent_param("Mix", 0.45, 50.0),
        }
    }
}

impl MovementParams {
    pub fn reset_smoothers(&self) {
        self.rate.smoothed.reset(self.rate.value());
        self.depth.smoothed.reset(self.depth.value());
        self.delay.smoothed.reset(self.delay.value());
        self.feedback.smoothed.reset(self.feedback.value());
        self.width.smoothed.reset(self.width.value());
        self.phase.smoothed.reset(self.phase.value());
        self.tone.smoothed.reset(self.tone.value());
        self.mix.smoothed.reset(self.mix.value());
    }
}

#[derive(Params)]
pub struct DiffusionParams {
    #[id = "diffusion_mode"]
    pub mode: EnumParam<DiffusionMode>,

    #[id = "diffusion_bypass"]
    pub bypass: BoolParam,

    #[id = "diffusion_time"]
    pub time: FloatParam,

    #[id = "diffusion_feedback"]
    pub feedback: FloatParam,

    #[id = "diffusion_size"]
    pub size: FloatParam,

    #[id = "diffusion_decay"]
    pub decay: FloatParam,

    #[id = "diffusion_pre_delay"]
    pub pre_delay: FloatParam,

    #[id = "diffusion_damping"]
    pub damping: FloatParam,

    #[id = "diffusion_mix"]
    pub mix: FloatParam,

    #[id = "diffusion_tone"]
    pub tone: FloatParam,

    #[id = "diffusion_stereo_offset"]
    pub stereo_offset: FloatParam,

    #[id = "diffusion_width"]
    pub width: FloatParam,
}

impl Default for DiffusionParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Diffusion Mode", DiffusionMode::Off),
            bypass: module_bypass_param("Diffusion Bypass"),
            time: FloatParam::new(
                "Time",
                350.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 2_000.0,
                    factor: FloatRange::skew_factor(-3.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(80.0))
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            feedback: FloatParam::new(
                "Feedback",
                0.25,
                FloatRange::Linear {
                    min: 0.0,
                    max: 0.95,
                },
            )
            .with_smoother(SmoothingStyle::Linear(60.0))
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(1))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            size: percent_param("Size", 0.45, 80.0),
            decay: percent_param("Decay", 0.45, 100.0),
            pre_delay: FloatParam::new(
                "Pre-delay",
                18.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 120.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(60.0))
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            damping: percent_param("Damping", 0.45, 80.0),
            mix: percent_param("Mix", 0.3, 50.0),
            tone: percent_param("Tone", 0.55, 50.0),
            stereo_offset: FloatParam::new(
                "Stereo Offset",
                0.0,
                FloatRange::Linear {
                    min: -0.5,
                    max: 0.5,
                },
            )
            .with_smoother(SmoothingStyle::Linear(80.0))
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(1))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            width: percent_param("Width", 1.0, 50.0),
        }
    }
}

impl DiffusionParams {
    pub fn reset_smoothers(&self) {
        self.time.smoothed.reset(self.time.value());
        self.feedback.smoothed.reset(self.feedback.value());
        self.size.smoothed.reset(self.size.value());
        self.decay.smoothed.reset(self.decay.value());
        self.pre_delay.smoothed.reset(self.pre_delay.value());
        self.damping.smoothed.reset(self.damping.value());
        self.mix.smoothed.reset(self.mix.value());
        self.tone.smoothed.reset(self.tone.value());
        self.stereo_offset
            .smoothed
            .reset(self.stereo_offset.value());
        self.width.smoothed.reset(self.width.value());
    }
}

#[derive(Params)]
pub struct TextureParams {
    #[id = "texture_mode"]
    pub mode: EnumParam<TextureMode>,

    #[id = "texture_bypass"]
    pub bypass: BoolParam,

    #[id = "texture_wow_depth"]
    pub wow_depth: FloatParam,

    #[id = "texture_wow_rate"]
    pub wow_rate: FloatParam,

    #[id = "texture_flutter_depth"]
    pub flutter_depth: FloatParam,

    #[id = "texture_flutter_rate"]
    pub flutter_rate: FloatParam,

    #[id = "texture_random_drift"]
    pub random_drift: FloatParam,

    #[id = "texture_noise_amount"]
    pub noise_amount: FloatParam,

    #[id = "texture_noise_color"]
    pub noise_color: FloatParam,

    #[id = "texture_degrade"]
    pub degrade: FloatParam,

    #[id = "texture_stereo_spread"]
    pub stereo_spread: FloatParam,

    #[id = "texture_mix"]
    pub mix: FloatParam,
}

impl Default for TextureParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Texture Mode", TextureMode::Off),
            bypass: module_bypass_param("Texture Bypass"),
            wow_depth: percent_param("Wow Depth", 0.18, 80.0),
            wow_rate: FloatParam::new(
                "Wow Rate",
                0.45,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(120.0))
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            flutter_depth: percent_param("Flutter Depth", 0.08, 60.0),
            flutter_rate: FloatParam::new(
                "Flutter Rate",
                7.0,
                FloatRange::Skewed {
                    min: 3.0,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-0.75),
                },
            )
            .with_smoother(SmoothingStyle::Linear(80.0))
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            random_drift: percent_param("Random Drift", 0.08, 120.0),
            noise_amount: percent_param("Noise Amount", 0.0, 80.0),
            noise_color: percent_param("Noise Color", 0.45, 80.0),
            degrade: percent_param("Degrade", 0.0, 80.0),
            stereo_spread: percent_param("Stereo Spread", 0.75, 80.0),
            mix: percent_param("Mix", 0.35, 60.0),
        }
    }
}

impl TextureParams {
    pub fn reset_smoothers(&self) {
        self.wow_depth.smoothed.reset(self.wow_depth.value());
        self.wow_rate.smoothed.reset(self.wow_rate.value());
        self.flutter_depth
            .smoothed
            .reset(self.flutter_depth.value());
        self.flutter_rate.smoothed.reset(self.flutter_rate.value());
        self.random_drift.smoothed.reset(self.random_drift.value());
        self.noise_amount.smoothed.reset(self.noise_amount.value());
        self.noise_color.smoothed.reset(self.noise_color.value());
        self.degrade.smoothed.reset(self.degrade.value());
        self.stereo_spread
            .smoothed
            .reset(self.stereo_spread.value());
        self.mix.smoothed.reset(self.mix.value());
    }
}

// ═══════════════════════════════════════════════════════════════════
// Global Pre EQ — keeps legacy pre_eq_* IDs for backward compat
// ═══════════════════════════════════════════════════════════════════

#[derive(Params)]
pub struct GlobalPreEqParams {
    #[id = "global_pre_eq_mode"]
    pub mode: EnumParam<EqMode>,
    #[id = "global_pre_eq_bypass"]
    pub bypass: BoolParam,
    #[id = "global_pre_eq_b1_enabled"]
    pub band1_enabled: BoolParam,
    #[id = "global_pre_eq_b1_type"]
    pub band1_type: EnumParam<EqBandType>,
    #[id = "global_pre_eq_b1_freq"]
    pub band1_frequency: FloatParam,
    #[id = "global_pre_eq_b1_gain"]
    pub band1_gain: FloatParam,
    #[id = "global_pre_eq_b1_q"]
    pub band1_q: FloatParam,
    #[id = "global_pre_eq_b2_enabled"]
    pub band2_enabled: BoolParam,
    #[id = "global_pre_eq_b2_type"]
    pub band2_type: EnumParam<EqBandType>,
    #[id = "global_pre_eq_b2_freq"]
    pub band2_frequency: FloatParam,
    #[id = "global_pre_eq_b2_gain"]
    pub band2_gain: FloatParam,
    #[id = "global_pre_eq_b2_q"]
    pub band2_q: FloatParam,
    #[id = "global_pre_eq_b3_enabled"]
    pub band3_enabled: BoolParam,
    #[id = "global_pre_eq_b3_type"]
    pub band3_type: EnumParam<EqBandType>,
    #[id = "global_pre_eq_b3_freq"]
    pub band3_frequency: FloatParam,
    #[id = "global_pre_eq_b3_gain"]
    pub band3_gain: FloatParam,
    #[id = "global_pre_eq_b3_q"]
    pub band3_q: FloatParam,
    #[id = "global_pre_eq_b4_enabled"]
    pub band4_enabled: BoolParam,
    #[id = "global_pre_eq_b4_type"]
    pub band4_type: EnumParam<EqBandType>,
    #[id = "global_pre_eq_b4_freq"]
    pub band4_frequency: FloatParam,
    #[id = "global_pre_eq_b4_gain"]
    pub band4_gain: FloatParam,
    #[id = "global_pre_eq_b4_q"]
    pub band4_q: FloatParam,
    #[id = "global_pre_eq_b5_enabled"]
    pub band5_enabled: BoolParam,
    #[id = "global_pre_eq_b5_type"]
    pub band5_type: EnumParam<EqBandType>,
    #[id = "global_pre_eq_b5_freq"]
    pub band5_frequency: FloatParam,
    #[id = "global_pre_eq_b5_gain"]
    pub band5_gain: FloatParam,
    #[id = "global_pre_eq_b5_q"]
    pub band5_q: FloatParam,
}

impl Default for GlobalPreEqParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("Pre EQ Mode", EqMode::On),
            bypass: BoolParam::new("Pre EQ Bypass", false)
                .with_value_to_string(formatters::v2s_bool_bypass())
                .with_string_to_value(formatters::s2v_bool_bypass()),
            band1_enabled: eq_band_enabled_param("Pre Band 1 Enabled", true),
            band1_type: EnumParam::new("Pre Band 1 Type", EqBandType::Bell),
            band1_frequency: frequency_param("Pre Band 1 Frequency", 80.0, 20.0, 20_000.0, 80.0),
            band1_gain: eq_gain_param("Pre Band 1 Gain"),
            band1_q: eq_q_param("Pre Band 1 Q"),
            band2_enabled: eq_band_enabled_param("Pre Band 2 Enabled", true),
            band2_type: EnumParam::new("Pre Band 2 Type", EqBandType::Bell),
            band2_frequency: frequency_param("Pre Band 2 Frequency", 250.0, 20.0, 20_000.0, 80.0),
            band2_gain: eq_gain_param("Pre Band 2 Gain"),
            band2_q: eq_q_param("Pre Band 2 Q"),
            band3_enabled: eq_band_enabled_param("Pre Band 3 Enabled", true),
            band3_type: EnumParam::new("Pre Band 3 Type", EqBandType::Bell),
            band3_frequency: frequency_param("Pre Band 3 Frequency", 1_000.0, 20.0, 20_000.0, 80.0),
            band3_gain: eq_gain_param("Pre Band 3 Gain"),
            band3_q: eq_q_param("Pre Band 3 Q"),
            band4_enabled: eq_band_enabled_param("Pre Band 4 Enabled", true),
            band4_type: EnumParam::new("Pre Band 4 Type", EqBandType::Bell),
            band4_frequency: frequency_param("Pre Band 4 Frequency", 4_000.0, 20.0, 20_000.0, 80.0),
            band4_gain: eq_gain_param("Pre Band 4 Gain"),
            band4_q: eq_q_param("Pre Band 4 Q"),
            band5_enabled: eq_band_enabled_param("Pre Band 5 Enabled", true),
            band5_type: EnumParam::new("Pre Band 5 Type", EqBandType::Bell),
            band5_frequency: frequency_param(
                "Pre Band 5 Frequency",
                12_000.0,
                20.0,
                20_000.0,
                80.0,
            ),
            band5_gain: eq_gain_param("Pre Band 5 Gain"),
            band5_q: eq_q_param("Pre Band 5 Q"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Global Post EQ — keeps legacy eq_* IDs for backward compat
// ═══════════════════════════════════════════════════════════════════

/// Keeps the legacy `eq_*` IDs for backward compatibility. This bank is the
/// migration destination for EQ state saved before independent PRE/POST and
/// per-module EQ banks were introduced.
#[derive(Params)]
pub struct GlobalPostEqParams {
    #[id = "eq_mode"]
    pub mode: EnumParam<EqMode>,
    #[id = "eq_bypass"]
    pub bypass: BoolParam,
    #[id = "eq_band1_enabled"]
    pub band1_enabled: BoolParam,
    #[id = "eq_band1_type"]
    pub band1_type: EnumParam<EqBandType>,
    #[id = "eq_low_cut_frequency"]
    pub band1_frequency: FloatParam,
    #[id = "eq_band1_gain"]
    pub band1_gain: FloatParam,
    #[id = "eq_band1_q"]
    pub band1_q: FloatParam,
    #[id = "eq_band2_enabled"]
    pub band2_enabled: BoolParam,
    #[id = "eq_band2_type"]
    pub band2_type: EnumParam<EqBandType>,
    #[id = "eq_low_shelf_frequency"]
    pub band2_frequency: FloatParam,
    #[id = "eq_low_shelf_gain"]
    pub band2_gain: FloatParam,
    #[id = "eq_band2_q"]
    pub band2_q: FloatParam,
    #[id = "eq_band3_enabled"]
    pub band3_enabled: BoolParam,
    #[id = "eq_band3_type"]
    pub band3_type: EnumParam<EqBandType>,
    #[id = "eq_mid_frequency"]
    pub band3_frequency: FloatParam,
    #[id = "eq_mid_gain"]
    pub band3_gain: FloatParam,
    #[id = "eq_mid_q"]
    pub band3_q: FloatParam,
    #[id = "eq_band4_enabled"]
    pub band4_enabled: BoolParam,
    #[id = "eq_band4_type"]
    pub band4_type: EnumParam<EqBandType>,
    #[id = "eq_high_shelf_frequency"]
    pub band4_frequency: FloatParam,
    #[id = "eq_high_shelf_gain"]
    pub band4_gain: FloatParam,
    #[id = "eq_band4_q"]
    pub band4_q: FloatParam,
    #[id = "eq_band5_enabled"]
    pub band5_enabled: BoolParam,
    #[id = "eq_band5_type"]
    pub band5_type: EnumParam<EqBandType>,
    #[id = "eq_high_cut_frequency"]
    pub band5_frequency: FloatParam,
    #[id = "eq_band5_gain"]
    pub band5_gain: FloatParam,
    #[id = "eq_band5_q"]
    pub band5_q: FloatParam,
}

impl Default for GlobalPostEqParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("EQ Mode", EqMode::On),
            bypass: module_bypass_param("EQ Bypass"),
            band1_enabled: eq_band_enabled_param("Band 1 Enabled", true),
            band1_type: EnumParam::new("Band 1 Type", EqBandType::Bell),
            band1_frequency: frequency_param("Band 1 Frequency", 80.0, 20.0, 20_000.0, 80.0),
            band1_gain: eq_gain_param("Band 1 Gain"),
            band1_q: eq_q_param("Band 1 Q"),
            band2_enabled: eq_band_enabled_param("Band 2 Enabled", true),
            band2_type: EnumParam::new("Band 2 Type", EqBandType::Bell),
            band2_frequency: frequency_param("Band 2 Frequency", 250.0, 20.0, 20_000.0, 80.0),
            band2_gain: eq_gain_param("Band 2 Gain"),
            band2_q: eq_q_param("Band 2 Q"),
            band3_enabled: eq_band_enabled_param("Band 3 Enabled", true),
            band3_type: EnumParam::new("Band 3 Type", EqBandType::Bell),
            band3_frequency: frequency_param("Band 3 Frequency", 1_000.0, 20.0, 20_000.0, 80.0),
            band3_gain: eq_gain_param("Band 3 Gain"),
            band3_q: eq_q_param("Band 3 Q"),
            band4_enabled: eq_band_enabled_param("Band 4 Enabled", true),
            band4_type: EnumParam::new("Band 4 Type", EqBandType::Bell),
            band4_frequency: frequency_param("Band 4 Frequency", 4_000.0, 20.0, 20_000.0, 80.0),
            band4_gain: eq_gain_param("Band 4 Gain"),
            band4_q: eq_q_param("Band 4 Q"),
            band5_enabled: eq_band_enabled_param("Band 5 Enabled", true),
            band5_type: EnumParam::new("Band 5 Type", EqBandType::Bell),
            band5_frequency: frequency_param("Band 5 Frequency", 12_000.0, 20.0, 20_000.0, 80.0),
            band5_gain: eq_gain_param("Band 5 Gain"),
            band5_q: eq_q_param("Band 5 Q"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Module EQ macro — full explicit ID convention
// ═══════════════════════════════════════════════════════════════════

macro_rules! define_module_eq {
    ($name:ident,
     $mode_id:literal, $bypass_id:literal,
     $b1_en:literal, $b1_ty:literal, $b1_freq:literal, $b1_gain:literal, $b1_q:literal,
     $b2_en:literal, $b2_ty:literal, $b2_freq:literal, $b2_gain:literal, $b2_q:literal,
     $b3_en:literal, $b3_ty:literal, $b3_freq:literal, $b3_gain:literal, $b3_q:literal,
     $b4_en:literal, $b4_ty:literal, $b4_freq:literal, $b4_gain:literal, $b4_q:literal,
     $b5_en:literal, $b5_ty:literal, $b5_freq:literal, $b5_gain:literal, $b5_q:literal $(,)?) => {
        #[derive(Params)]
        pub struct $name {
            #[id = $mode_id]
            pub mode: EnumParam<EqMode>,
            #[id = $bypass_id]
            pub bypass: BoolParam,
            #[id = $b1_en]
            pub band1_enabled: BoolParam,
            #[id = $b1_ty]
            pub band1_type: EnumParam<EqBandType>,
            #[id = $b1_freq]
            pub band1_frequency: FloatParam,
            #[id = $b1_gain]
            pub band1_gain: FloatParam,
            #[id = $b1_q]
            pub band1_q: FloatParam,
            #[id = $b2_en]
            pub band2_enabled: BoolParam,
            #[id = $b2_ty]
            pub band2_type: EnumParam<EqBandType>,
            #[id = $b2_freq]
            pub band2_frequency: FloatParam,
            #[id = $b2_gain]
            pub band2_gain: FloatParam,
            #[id = $b2_q]
            pub band2_q: FloatParam,
            #[id = $b3_en]
            pub band3_enabled: BoolParam,
            #[id = $b3_ty]
            pub band3_type: EnumParam<EqBandType>,
            #[id = $b3_freq]
            pub band3_frequency: FloatParam,
            #[id = $b3_gain]
            pub band3_gain: FloatParam,
            #[id = $b3_q]
            pub band3_q: FloatParam,
            #[id = $b4_en]
            pub band4_enabled: BoolParam,
            #[id = $b4_ty]
            pub band4_type: EnumParam<EqBandType>,
            #[id = $b4_freq]
            pub band4_frequency: FloatParam,
            #[id = $b4_gain]
            pub band4_gain: FloatParam,
            #[id = $b4_q]
            pub band4_q: FloatParam,
            #[id = $b5_en]
            pub band5_enabled: BoolParam,
            #[id = $b5_ty]
            pub band5_type: EnumParam<EqBandType>,
            #[id = $b5_freq]
            pub band5_frequency: FloatParam,
            #[id = $b5_gain]
            pub band5_gain: FloatParam,
            #[id = $b5_q]
            pub band5_q: FloatParam,
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    mode: EnumParam::new("EQ Mode", EqMode::On),
                    bypass: module_bypass_param("EQ Bypass"),
                    band1_enabled: eq_band_enabled_param("Band 1 Enabled", true),
                    band1_type: EnumParam::new("Band 1 Type", EqBandType::Bell),
                    band1_frequency: frequency_param(
                        "Band 1 Frequency",
                        80.0,
                        20.0,
                        20_000.0,
                        80.0,
                    ),
                    band1_gain: eq_gain_param("Band 1 Gain"),
                    band1_q: eq_q_param("Band 1 Q"),
                    band2_enabled: eq_band_enabled_param("Band 2 Enabled", true),
                    band2_type: EnumParam::new("Band 2 Type", EqBandType::Bell),
                    band2_frequency: frequency_param(
                        "Band 2 Frequency",
                        250.0,
                        20.0,
                        20_000.0,
                        80.0,
                    ),
                    band2_gain: eq_gain_param("Band 2 Gain"),
                    band2_q: eq_q_param("Band 2 Q"),
                    band3_enabled: eq_band_enabled_param("Band 3 Enabled", true),
                    band3_type: EnumParam::new("Band 3 Type", EqBandType::Bell),
                    band3_frequency: frequency_param(
                        "Band 3 Frequency",
                        1_000.0,
                        20.0,
                        20_000.0,
                        80.0,
                    ),
                    band3_gain: eq_gain_param("Band 3 Gain"),
                    band3_q: eq_q_param("Band 3 Q"),
                    band4_enabled: eq_band_enabled_param("Band 4 Enabled", true),
                    band4_type: EnumParam::new("Band 4 Type", EqBandType::Bell),
                    band4_frequency: frequency_param(
                        "Band 4 Frequency",
                        4_000.0,
                        20.0,
                        20_000.0,
                        80.0,
                    ),
                    band4_gain: eq_gain_param("Band 4 Gain"),
                    band4_q: eq_q_param("Band 4 Q"),
                    band5_enabled: eq_band_enabled_param("Band 5 Enabled", true),
                    band5_type: EnumParam::new("Band 5 Type", EqBandType::Bell),
                    band5_frequency: frequency_param(
                        "Band 5 Frequency",
                        12_000.0,
                        20.0,
                        20_000.0,
                        80.0,
                    ),
                    band5_gain: eq_gain_param("Band 5 Gain"),
                    band5_q: eq_q_param("Band 5 Q"),
                }
            }
        }

        impl EqParamRefs for $name {
            fn mode(&self) -> &EnumParam<EqMode> {
                &self.mode
            }
            fn bypass(&self) -> &BoolParam {
                &self.bypass
            }
            fn band_enabled(&self, band: usize) -> &BoolParam {
                match band {
                    0 => &self.band1_enabled,
                    1 => &self.band2_enabled,
                    2 => &self.band3_enabled,
                    3 => &self.band4_enabled,
                    _ => &self.band5_enabled,
                }
            }
            fn band_type(&self, band: usize) -> &EnumParam<EqBandType> {
                match band {
                    0 => &self.band1_type,
                    1 => &self.band2_type,
                    2 => &self.band3_type,
                    3 => &self.band4_type,
                    _ => &self.band5_type,
                }
            }
            fn band_frequency(&self, band: usize) -> &FloatParam {
                match band {
                    0 => &self.band1_frequency,
                    1 => &self.band2_frequency,
                    2 => &self.band3_frequency,
                    3 => &self.band4_frequency,
                    _ => &self.band5_frequency,
                }
            }
            fn band_gain(&self, band: usize) -> &FloatParam {
                match band {
                    0 => &self.band1_gain,
                    1 => &self.band2_gain,
                    2 => &self.band3_gain,
                    3 => &self.band4_gain,
                    _ => &self.band5_gain,
                }
            }
            fn band_q(&self, band: usize) -> &FloatParam {
                match band {
                    0 => &self.band1_q,
                    1 => &self.band2_q,
                    2 => &self.band3_q,
                    3 => &self.band4_q,
                    _ => &self.band5_q,
                }
            }
        }

        impl $name {
            pub fn reset_smoothers(&self) {
                reset_eq_smoothers(self);
            }
        }
    };
}

define_module_eq!(
    CharacterPreEq,
    "character_pre_eq_mode",
    "character_pre_eq_bypass",
    "character_pre_eq_b1_enabled",
    "character_pre_eq_b1_type",
    "character_pre_eq_b1_freq",
    "character_pre_eq_b1_gain",
    "character_pre_eq_b1_q",
    "character_pre_eq_b2_enabled",
    "character_pre_eq_b2_type",
    "character_pre_eq_b2_freq",
    "character_pre_eq_b2_gain",
    "character_pre_eq_b2_q",
    "character_pre_eq_b3_enabled",
    "character_pre_eq_b3_type",
    "character_pre_eq_b3_freq",
    "character_pre_eq_b3_gain",
    "character_pre_eq_b3_q",
    "character_pre_eq_b4_enabled",
    "character_pre_eq_b4_type",
    "character_pre_eq_b4_freq",
    "character_pre_eq_b4_gain",
    "character_pre_eq_b4_q",
    "character_pre_eq_b5_enabled",
    "character_pre_eq_b5_type",
    "character_pre_eq_b5_freq",
    "character_pre_eq_b5_gain",
    "character_pre_eq_b5_q",
);

define_module_eq!(
    CharacterPostEq,
    "character_post_eq_mode",
    "character_post_eq_bypass",
    "character_post_eq_b1_enabled",
    "character_post_eq_b1_type",
    "character_post_eq_b1_freq",
    "character_post_eq_b1_gain",
    "character_post_eq_b1_q",
    "character_post_eq_b2_enabled",
    "character_post_eq_b2_type",
    "character_post_eq_b2_freq",
    "character_post_eq_b2_gain",
    "character_post_eq_b2_q",
    "character_post_eq_b3_enabled",
    "character_post_eq_b3_type",
    "character_post_eq_b3_freq",
    "character_post_eq_b3_gain",
    "character_post_eq_b3_q",
    "character_post_eq_b4_enabled",
    "character_post_eq_b4_type",
    "character_post_eq_b4_freq",
    "character_post_eq_b4_gain",
    "character_post_eq_b4_q",
    "character_post_eq_b5_enabled",
    "character_post_eq_b5_type",
    "character_post_eq_b5_freq",
    "character_post_eq_b5_gain",
    "character_post_eq_b5_q",
);

define_module_eq!(
    MovementPreEq,
    "movement_pre_eq_mode",
    "movement_pre_eq_bypass",
    "movement_pre_eq_b1_enabled",
    "movement_pre_eq_b1_type",
    "movement_pre_eq_b1_freq",
    "movement_pre_eq_b1_gain",
    "movement_pre_eq_b1_q",
    "movement_pre_eq_b2_enabled",
    "movement_pre_eq_b2_type",
    "movement_pre_eq_b2_freq",
    "movement_pre_eq_b2_gain",
    "movement_pre_eq_b2_q",
    "movement_pre_eq_b3_enabled",
    "movement_pre_eq_b3_type",
    "movement_pre_eq_b3_freq",
    "movement_pre_eq_b3_gain",
    "movement_pre_eq_b3_q",
    "movement_pre_eq_b4_enabled",
    "movement_pre_eq_b4_type",
    "movement_pre_eq_b4_freq",
    "movement_pre_eq_b4_gain",
    "movement_pre_eq_b4_q",
    "movement_pre_eq_b5_enabled",
    "movement_pre_eq_b5_type",
    "movement_pre_eq_b5_freq",
    "movement_pre_eq_b5_gain",
    "movement_pre_eq_b5_q",
);

define_module_eq!(
    MovementPostEq,
    "movement_post_eq_mode",
    "movement_post_eq_bypass",
    "movement_post_eq_b1_enabled",
    "movement_post_eq_b1_type",
    "movement_post_eq_b1_freq",
    "movement_post_eq_b1_gain",
    "movement_post_eq_b1_q",
    "movement_post_eq_b2_enabled",
    "movement_post_eq_b2_type",
    "movement_post_eq_b2_freq",
    "movement_post_eq_b2_gain",
    "movement_post_eq_b2_q",
    "movement_post_eq_b3_enabled",
    "movement_post_eq_b3_type",
    "movement_post_eq_b3_freq",
    "movement_post_eq_b3_gain",
    "movement_post_eq_b3_q",
    "movement_post_eq_b4_enabled",
    "movement_post_eq_b4_type",
    "movement_post_eq_b4_freq",
    "movement_post_eq_b4_gain",
    "movement_post_eq_b4_q",
    "movement_post_eq_b5_enabled",
    "movement_post_eq_b5_type",
    "movement_post_eq_b5_freq",
    "movement_post_eq_b5_gain",
    "movement_post_eq_b5_q",
);

define_module_eq!(
    DiffusionPreEq,
    "diffusion_pre_eq_mode",
    "diffusion_pre_eq_bypass",
    "diffusion_pre_eq_b1_enabled",
    "diffusion_pre_eq_b1_type",
    "diffusion_pre_eq_b1_freq",
    "diffusion_pre_eq_b1_gain",
    "diffusion_pre_eq_b1_q",
    "diffusion_pre_eq_b2_enabled",
    "diffusion_pre_eq_b2_type",
    "diffusion_pre_eq_b2_freq",
    "diffusion_pre_eq_b2_gain",
    "diffusion_pre_eq_b2_q",
    "diffusion_pre_eq_b3_enabled",
    "diffusion_pre_eq_b3_type",
    "diffusion_pre_eq_b3_freq",
    "diffusion_pre_eq_b3_gain",
    "diffusion_pre_eq_b3_q",
    "diffusion_pre_eq_b4_enabled",
    "diffusion_pre_eq_b4_type",
    "diffusion_pre_eq_b4_freq",
    "diffusion_pre_eq_b4_gain",
    "diffusion_pre_eq_b4_q",
    "diffusion_pre_eq_b5_enabled",
    "diffusion_pre_eq_b5_type",
    "diffusion_pre_eq_b5_freq",
    "diffusion_pre_eq_b5_gain",
    "diffusion_pre_eq_b5_q",
);

define_module_eq!(
    DiffusionPostEq,
    "diffusion_post_eq_mode",
    "diffusion_post_eq_bypass",
    "diffusion_post_eq_b1_enabled",
    "diffusion_post_eq_b1_type",
    "diffusion_post_eq_b1_freq",
    "diffusion_post_eq_b1_gain",
    "diffusion_post_eq_b1_q",
    "diffusion_post_eq_b2_enabled",
    "diffusion_post_eq_b2_type",
    "diffusion_post_eq_b2_freq",
    "diffusion_post_eq_b2_gain",
    "diffusion_post_eq_b2_q",
    "diffusion_post_eq_b3_enabled",
    "diffusion_post_eq_b3_type",
    "diffusion_post_eq_b3_freq",
    "diffusion_post_eq_b3_gain",
    "diffusion_post_eq_b3_q",
    "diffusion_post_eq_b4_enabled",
    "diffusion_post_eq_b4_type",
    "diffusion_post_eq_b4_freq",
    "diffusion_post_eq_b4_gain",
    "diffusion_post_eq_b4_q",
    "diffusion_post_eq_b5_enabled",
    "diffusion_post_eq_b5_type",
    "diffusion_post_eq_b5_freq",
    "diffusion_post_eq_b5_gain",
    "diffusion_post_eq_b5_q",
);

define_module_eq!(
    TexturePreEq,
    "texture_pre_eq_mode",
    "texture_pre_eq_bypass",
    "texture_pre_eq_b1_enabled",
    "texture_pre_eq_b1_type",
    "texture_pre_eq_b1_freq",
    "texture_pre_eq_b1_gain",
    "texture_pre_eq_b1_q",
    "texture_pre_eq_b2_enabled",
    "texture_pre_eq_b2_type",
    "texture_pre_eq_b2_freq",
    "texture_pre_eq_b2_gain",
    "texture_pre_eq_b2_q",
    "texture_pre_eq_b3_enabled",
    "texture_pre_eq_b3_type",
    "texture_pre_eq_b3_freq",
    "texture_pre_eq_b3_gain",
    "texture_pre_eq_b3_q",
    "texture_pre_eq_b4_enabled",
    "texture_pre_eq_b4_type",
    "texture_pre_eq_b4_freq",
    "texture_pre_eq_b4_gain",
    "texture_pre_eq_b4_q",
    "texture_pre_eq_b5_enabled",
    "texture_pre_eq_b5_type",
    "texture_pre_eq_b5_freq",
    "texture_pre_eq_b5_gain",
    "texture_pre_eq_b5_q",
);

define_module_eq!(
    TexturePostEq,
    "texture_post_eq_mode",
    "texture_post_eq_bypass",
    "texture_post_eq_b1_enabled",
    "texture_post_eq_b1_type",
    "texture_post_eq_b1_freq",
    "texture_post_eq_b1_gain",
    "texture_post_eq_b1_q",
    "texture_post_eq_b2_enabled",
    "texture_post_eq_b2_type",
    "texture_post_eq_b2_freq",
    "texture_post_eq_b2_gain",
    "texture_post_eq_b2_q",
    "texture_post_eq_b3_enabled",
    "texture_post_eq_b3_type",
    "texture_post_eq_b3_freq",
    "texture_post_eq_b3_gain",
    "texture_post_eq_b3_q",
    "texture_post_eq_b4_enabled",
    "texture_post_eq_b4_type",
    "texture_post_eq_b4_freq",
    "texture_post_eq_b4_gain",
    "texture_post_eq_b4_q",
    "texture_post_eq_b5_enabled",
    "texture_post_eq_b5_type",
    "texture_post_eq_b5_freq",
    "texture_post_eq_b5_gain",
    "texture_post_eq_b5_q",
);

pub trait EqParamRefs {
    fn mode(&self) -> &EnumParam<EqMode>;
    fn bypass(&self) -> &BoolParam;
    fn band_enabled(&self, band: usize) -> &BoolParam;
    fn band_type(&self, band: usize) -> &EnumParam<EqBandType>;
    fn band_frequency(&self, band: usize) -> &FloatParam;
    fn band_gain(&self, band: usize) -> &FloatParam;
    fn band_q(&self, band: usize) -> &FloatParam;

    fn band(&self, band: usize) -> EqBandParams<'_> {
        EqBandParams {
            enabled: self.band_enabled(band),
            band_type: self.band_type(band),
            frequency: self.band_frequency(band),
            gain: self.band_gain(band),
            q: self.band_q(band),
        }
    }
}

pub struct EqBandParams<'a> {
    pub enabled: &'a BoolParam,
    pub band_type: &'a EnumParam<EqBandType>,
    pub frequency: &'a FloatParam,
    pub gain: &'a FloatParam,
    pub q: &'a FloatParam,
}

macro_rules! impl_eq_param_refs {
    ($ty:ty) => {
        impl EqParamRefs for $ty {
            fn mode(&self) -> &EnumParam<EqMode> {
                &self.mode
            }
            fn bypass(&self) -> &BoolParam {
                &self.bypass
            }
            fn band_enabled(&self, band: usize) -> &BoolParam {
                match band {
                    0 => &self.band1_enabled,
                    1 => &self.band2_enabled,
                    2 => &self.band3_enabled,
                    3 => &self.band4_enabled,
                    _ => &self.band5_enabled,
                }
            }
            fn band_type(&self, band: usize) -> &EnumParam<EqBandType> {
                match band {
                    0 => &self.band1_type,
                    1 => &self.band2_type,
                    2 => &self.band3_type,
                    3 => &self.band4_type,
                    _ => &self.band5_type,
                }
            }
            fn band_frequency(&self, band: usize) -> &FloatParam {
                match band {
                    0 => &self.band1_frequency,
                    1 => &self.band2_frequency,
                    2 => &self.band3_frequency,
                    3 => &self.band4_frequency,
                    _ => &self.band5_frequency,
                }
            }
            fn band_gain(&self, band: usize) -> &FloatParam {
                match band {
                    0 => &self.band1_gain,
                    1 => &self.band2_gain,
                    2 => &self.band3_gain,
                    3 => &self.band4_gain,
                    _ => &self.band5_gain,
                }
            }
            fn band_q(&self, band: usize) -> &FloatParam {
                match band {
                    0 => &self.band1_q,
                    1 => &self.band2_q,
                    2 => &self.band3_q,
                    3 => &self.band4_q,
                    _ => &self.band5_q,
                }
            }
        }
    };
}

impl_eq_param_refs!(GlobalPreEqParams);
impl_eq_param_refs!(GlobalPostEqParams);

impl GlobalPreEqParams {
    pub fn reset_smoothers(&self) {
        reset_eq_smoothers(self);
    }
}

impl GlobalPostEqParams {
    pub fn reset_smoothers(&self) {
        reset_eq_smoothers(self);
    }
}

fn reset_eq_smoothers(params: &impl EqParamRefs) {
    for band in 0..5 {
        params
            .band_frequency(band)
            .smoothed
            .reset(params.band_frequency(band).value());
        params
            .band_gain(band)
            .smoothed
            .reset(params.band_gain(band).value());
        params
            .band_q(band)
            .smoothed
            .reset(params.band_q(band).value());
    }
}

fn gain_param(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    )
    .with_smoother(SmoothingStyle::Linear(20.0))
    .with_unit(" dB")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn eq_gain_param(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    )
    .with_smoother(SmoothingStyle::Linear(80.0))
    .with_unit(" dB")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn eq_q_param(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        1.0,
        FloatRange::Skewed {
            min: 0.1,
            max: 12.0,
            factor: FloatRange::skew_factor(-1.0),
        },
    )
    .with_smoother(SmoothingStyle::Linear(80.0))
    .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn eq_band_enabled_param(name: &'static str, default: bool) -> BoolParam {
    BoolParam::new(name, default)
}

fn frequency_param(
    name: &'static str,
    default: f32,
    min: f32,
    max: f32,
    smoothing_ms: f32,
) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_smoother(SmoothingStyle::Linear(smoothing_ms))
    .with_unit(" Hz")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn trim_param(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        0.0,
        FloatRange::Linear {
            min: -12.0,
            max: 12.0,
        },
    )
    .with_smoother(SmoothingStyle::Linear(20.0))
    .with_unit(" dB")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn percent_param(name: &'static str, default: f32, smoothing_ms: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(smoothing_ms))
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(1))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn module_bypass_param(name: &'static str) -> BoolParam {
    BoolParam::new(name, false)
        .with_value_to_string(formatters::v2s_bool_bypass())
        .with_string_to_value(formatters::s2v_bool_bypass())
}

fn chain_slot_param(default: i32) -> IntParam {
    IntParam::new("Chain Slot", default, IntRange::Linear { min: 0, max: 3 })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nih_plug::prelude::Params;

    use super::Cc22Params;

    #[test]
    fn parameter_ids_are_unique() {
        let params = Cc22Params::default();
        let mut ids = BTreeSet::new();

        for (id, _, _) in params.param_map() {
            assert!(ids.insert(id.clone()), "duplicate parameter ID: {id}");
        }
    }

    #[test]
    fn eq_parameter_ids_match_the_independent_bank_schema() {
        let params = Cc22Params::default();
        let all_ids = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect::<BTreeSet<_>>();

        for id in expected_eq_parameter_ids() {
            assert!(all_ids.contains(&id), "missing EQ parameter ID: {id}");
        }
    }

    fn expected_eq_parameter_ids() -> BTreeSet<String> {
        let mut ids = BTreeSet::new();

        for bank in [
            "global_pre_eq",
            "character_pre_eq",
            "character_post_eq",
            "movement_pre_eq",
            "movement_post_eq",
            "diffusion_pre_eq",
            "diffusion_post_eq",
            "texture_pre_eq",
            "texture_post_eq",
        ] {
            ids.insert(format!("{bank}_mode"));
            ids.insert(format!("{bank}_bypass"));
            for band in 1..=5 {
                for field in ["enabled", "type", "freq", "gain", "q"] {
                    ids.insert(format!("{bank}_b{band}_{field}"));
                }
            }
        }

        for id in [
            "eq_mode",
            "eq_bypass",
            "eq_band1_enabled",
            "eq_band1_type",
            "eq_low_cut_frequency",
            "eq_band1_gain",
            "eq_band1_q",
            "eq_band2_enabled",
            "eq_band2_type",
            "eq_low_shelf_frequency",
            "eq_low_shelf_gain",
            "eq_band2_q",
            "eq_band3_enabled",
            "eq_band3_type",
            "eq_mid_frequency",
            "eq_mid_gain",
            "eq_mid_q",
            "eq_band4_enabled",
            "eq_band4_type",
            "eq_high_shelf_frequency",
            "eq_high_shelf_gain",
            "eq_band4_q",
            "eq_band5_enabled",
            "eq_band5_type",
            "eq_high_cut_frequency",
            "eq_band5_gain",
            "eq_band5_q",
        ] {
            ids.insert(id.to_string());
        }

        ids
    }
}
