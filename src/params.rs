use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;

use crate::dsp::{
    character::CharacterMode, diffusion::DiffusionMode, eq::EqMode, movement::LfoShape,
    movement::MovementMode, texture::TextureMode,
};

#[derive(Params)]
pub struct Cc22Params {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[id = "input_gain"]
    pub input_gain: FloatParam,

    #[nested(group = "Character")]
    pub character: CharacterParams,

    #[nested(group = "Movement")]
    pub movement: MovementParams,

    #[nested(group = "Diffusion")]
    pub diffusion: DiffusionParams,

    #[nested(group = "Texture")]
    pub texture: TextureParams,

    #[nested(group = "EQ")]
    pub eq: EqParams,

    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,

    #[id = "global_bypass"]
    pub global_bypass: BoolParam,
}

impl Default for Cc22Params {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(980, 720),
            input_gain: gain_param("Input Gain"),
            character: CharacterParams::default(),
            movement: MovementParams::default(),
            diffusion: DiffusionParams::default(),
            texture: TextureParams::default(),
            eq: EqParams::default(),
            output_gain: gain_param("Output Gain"),
            dry_wet: FloatParam::new("Dry/Wet", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(1))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            global_bypass: BoolParam::new("Global Bypass", false)
                .with_value_to_string(formatters::v2s_bool_bypass())
                .with_string_to_value(formatters::s2v_bool_bypass()),
        }
    }
}

impl Cc22Params {
    pub fn reset_smoothers(&self) {
        self.input_gain.smoothed.reset(self.input_gain.value());
        self.character.reset_smoothers();
        self.movement.reset_smoothers();
        self.diffusion.reset_smoothers();
        self.texture.reset_smoothers();
        self.eq.reset_smoothers();
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

#[derive(Params)]
pub struct EqParams {
    #[id = "eq_mode"]
    pub mode: EnumParam<EqMode>,

    #[id = "eq_bypass"]
    pub bypass: BoolParam,

    #[id = "eq_low_cut_frequency"]
    pub low_cut_frequency: FloatParam,

    #[id = "eq_low_shelf_gain"]
    pub low_shelf_gain: FloatParam,

    #[id = "eq_low_shelf_frequency"]
    pub low_shelf_frequency: FloatParam,

    #[id = "eq_mid_gain"]
    pub mid_gain: FloatParam,

    #[id = "eq_mid_frequency"]
    pub mid_frequency: FloatParam,

    #[id = "eq_mid_q"]
    pub mid_q: FloatParam,

    #[id = "eq_high_shelf_gain"]
    pub high_shelf_gain: FloatParam,

    #[id = "eq_high_shelf_frequency"]
    pub high_shelf_frequency: FloatParam,

    #[id = "eq_high_cut_frequency"]
    pub high_cut_frequency: FloatParam,
}

impl Default for EqParams {
    fn default() -> Self {
        Self {
            mode: EnumParam::new("EQ Mode", EqMode::On),
            bypass: module_bypass_param("EQ Bypass"),
            low_cut_frequency: frequency_param("Low Cut Frequency", 20.0, 20.0, 500.0, 80.0),
            low_shelf_gain: eq_gain_param("Low Shelf Gain"),
            low_shelf_frequency: frequency_param("Low Shelf Frequency", 120.0, 40.0, 500.0, 80.0),
            mid_gain: eq_gain_param("Mid Gain"),
            mid_frequency: frequency_param("Mid Frequency", 1_000.0, 100.0, 8_000.0, 80.0),
            mid_q: FloatParam::new(
                "Mid Q",
                1.0,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(80.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            high_shelf_gain: eq_gain_param("High Shelf Gain"),
            high_shelf_frequency: frequency_param(
                "High Shelf Frequency",
                8_000.0,
                1_000.0,
                16_000.0,
                80.0,
            ),
            high_cut_frequency: frequency_param(
                "High Cut Frequency",
                20_000.0,
                2_000.0,
                20_000.0,
                80.0,
            ),
        }
    }
}

impl EqParams {
    pub fn reset_smoothers(&self) {
        self.low_cut_frequency
            .smoothed
            .reset(self.low_cut_frequency.value());
        self.low_shelf_gain
            .smoothed
            .reset(self.low_shelf_gain.value());
        self.low_shelf_frequency
            .smoothed
            .reset(self.low_shelf_frequency.value());
        self.mid_gain.smoothed.reset(self.mid_gain.value());
        self.mid_frequency
            .smoothed
            .reset(self.mid_frequency.value());
        self.mid_q.smoothed.reset(self.mid_q.value());
        self.high_shelf_gain
            .smoothed
            .reset(self.high_shelf_gain.value());
        self.high_shelf_frequency
            .smoothed
            .reset(self.high_shelf_frequency.value());
        self.high_cut_frequency
            .smoothed
            .reset(self.high_cut_frequency.value());
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
            min: -18.0,
            max: 18.0,
        },
    )
    .with_smoother(SmoothingStyle::Linear(80.0))
    .with_unit(" dB")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
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
