use nih_plug::prelude::{Param, ParamSetter};

use crate::{
    dsp::{
        character::CharacterMode,
        diffusion::DiffusionMode,
        eq::{EqBandType, EqMode},
        movement::LfoShape,
        movement::MovementMode,
        texture::TextureMode,
    },
    params::Cc22Params,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetId {
    WarmTapeChorus,
    DreamyDiffusion,
    PsychedelicMotion,
    LoFiRoom,
    CleanWiden,
    SweetConsole,
    FuzzCollage,
    ReverseDream,
    TapeReels,
    InterferenceSwell,
    HowlingTapeLead,
    SwellReverseBloom,
    ReelsDubEcho,
    ReversePsychedelic,
    SoftSwellSpace,
    PreEqSculpted,
}

#[derive(Debug, Clone, Copy)]
pub struct InternalPreset {
    pub id: PresetId,
    pub name: &'static str,
    pub values: PresetValues,
}

#[derive(Debug, Clone, Copy)]
pub struct PresetValues {
    pub character: CharacterPreset,
    pub movement: MovementPreset,
    pub diffusion: DiffusionPreset,
    pub texture: TexturePreset,
    pub eq: EqPreset,
    pub pre_eq: Option<EqPreset>,
    pub input_gain_db: f32,
    pub output_gain_db: f32,
    pub dry_wet: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct CharacterPreset {
    pub mode: CharacterMode,
    pub drive: f32,
    pub age: f32,
    pub tone: f32,
    pub noise: f32,
    pub mix: f32,
    pub output_trim_db: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct MovementPreset {
    pub mode: MovementMode,
    pub rate_hz: f32,
    pub depth: f32,
    pub shape: LfoShape,
    pub delay_ms: f32,
    pub feedback: f32,
    pub width: f32,
    pub phase_degrees: f32,
    pub tone: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DiffusionPreset {
    pub mode: DiffusionMode,
    pub time_ms: f32,
    pub feedback: f32,
    pub size: f32,
    pub decay: f32,
    pub pre_delay_ms: f32,
    pub damping: f32,
    pub mix: f32,
    pub tone: f32,
    pub stereo_offset: f32,
    pub width: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct TexturePreset {
    pub mode: TextureMode,
    pub wow_depth: f32,
    pub wow_rate_hz: f32,
    pub flutter_depth: f32,
    pub flutter_rate_hz: f32,
    pub random_drift: f32,
    pub noise_amount: f32,
    pub noise_color: f32,
    pub degrade: f32,
    pub stereo_spread: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct EqPreset {
    pub mode: EqMode,
    pub low_cut_hz: f32,
    pub low_shelf_gain_db: f32,
    pub low_shelf_hz: f32,
    pub mid_gain_db: f32,
    pub mid_hz: f32,
    pub mid_q: f32,
    pub high_shelf_gain_db: f32,
    pub high_shelf_hz: f32,
    pub high_cut_hz: f32,
}

pub const INTERNAL_PRESETS: [InternalPreset; 16] = [
    InternalPreset {
        id: PresetId::WarmTapeChorus,
        name: "Warm Tape Chorus",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Saturation,
                drive: 0.18,
                age: 0.0,
                tone: 0.42,
                noise: 0.0,
                mix: 0.72,
                output_trim_db: -1.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Chorus,
                rate_hz: 0.38,
                depth: 0.28,
                shape: LfoShape::Sine,
                delay_ms: 17.0,
                feedback: 0.08,
                width: 0.78,
                phase_degrees: 180.0,
                tone: 0.45,
                mix: 0.36,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Slap,
                time_ms: 95.0,
                feedback: 0.18,
                size: 0.25,
                decay: 0.25,
                pre_delay_ms: 10.0,
                damping: 0.45,
                mix: 0.18,
                tone: 0.40,
                stereo_offset: 0.0,
                width: 0.65,
            },
            texture: TexturePreset {
                mode: TextureMode::WowFlutter,
                wow_depth: 0.10,
                wow_rate_hz: 0.32,
                flutter_depth: 0.04,
                flutter_rate_hz: 6.5,
                random_drift: 0.07,
                noise_amount: 0.0,
                noise_color: 0.4,
                degrade: 0.0,
                stereo_spread: 0.7,
                mix: 0.22,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 35.0,
                low_shelf_gain_db: -0.8,
                low_shelf_hz: 120.0,
                mid_gain_db: -0.8,
                mid_hz: 2_000.0,
                mid_q: 0.8,
                high_shelf_gain_db: -1.8,
                high_shelf_hz: 7_500.0,
                high_cut_hz: 16_000.0,
            },
            pre_eq: None,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::DreamyDiffusion,
        name: "Dreamy Diffusion",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Cassette,
                drive: 0.16,
                age: 0.28,
                tone: 0.48,
                noise: 0.08,
                mix: 0.62,
                output_trim_db: -1.5,
            },
            movement: MovementPreset {
                mode: MovementMode::Chorus,
                rate_hz: 0.22,
                depth: 0.42,
                shape: LfoShape::Sine,
                delay_ms: 21.0,
                feedback: 0.10,
                width: 0.95,
                phase_degrees: 180.0,
                tone: 0.52,
                mix: 0.45,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reverb,
                time_ms: 420.0,
                feedback: 0.28,
                size: 0.55,
                decay: 0.58,
                pre_delay_ms: 28.0,
                damping: 0.55,
                mix: 0.38,
                tone: 0.48,
                stereo_offset: 0.0,
                width: 0.92,
            },
            texture: TexturePreset {
                mode: TextureMode::Noise,
                wow_depth: 0.0,
                wow_rate_hz: 0.35,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.0,
                noise_amount: 0.12,
                noise_color: 0.35,
                degrade: 0.08,
                stereo_spread: 0.85,
                mix: 0.18,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 45.0,
                low_shelf_gain_db: -0.5,
                low_shelf_hz: 150.0,
                mid_gain_db: -1.0,
                mid_hz: 2_800.0,
                mid_q: 0.7,
                high_shelf_gain_db: -3.0,
                high_shelf_hz: 6_500.0,
                high_cut_hz: 15_000.0,
            },
            pre_eq: None,
            input_gain_db: 0.0,
            output_gain_db: -0.5,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::PsychedelicMotion,
        name: "Psychedelic Motion",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Saturation,
                drive: 0.42,
                age: 0.0,
                tone: 0.55,
                noise: 0.0,
                mix: 0.80,
                output_trim_db: -2.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Vibrato,
                rate_hz: 3.2,
                depth: 0.52,
                shape: LfoShape::Triangle,
                delay_ms: 14.0,
                feedback: 0.0,
                width: 0.9,
                phase_degrees: 180.0,
                tone: 0.60,
                mix: 0.55,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Delay,
                time_ms: 430.0,
                feedback: 0.42,
                size: 0.3,
                decay: 0.3,
                pre_delay_ms: 20.0,
                damping: 0.45,
                mix: 0.30,
                tone: 0.55,
                stereo_offset: 0.18,
                width: 0.9,
            },
            texture: TexturePreset {
                mode: TextureMode::WowFlutter,
                wow_depth: 0.32,
                wow_rate_hz: 0.55,
                flutter_depth: 0.12,
                flutter_rate_hz: 9.0,
                random_drift: 0.18,
                noise_amount: 0.0,
                noise_color: 0.5,
                degrade: 0.0,
                stereo_spread: 0.9,
                mix: 0.42,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 50.0,
                low_shelf_gain_db: -0.5,
                low_shelf_hz: 120.0,
                mid_gain_db: 1.5,
                mid_hz: 1_100.0,
                mid_q: 0.9,
                high_shelf_gain_db: -0.5,
                high_shelf_hz: 9_000.0,
                high_cut_hz: 17_000.0,
            },
            pre_eq: None,
            input_gain_db: -1.0,
            output_gain_db: -1.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::LoFiRoom,
        name: "Lo-Fi Room",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Cassette,
                drive: 0.46,
                age: 0.62,
                tone: 0.34,
                noise: 0.18,
                mix: 0.82,
                output_trim_db: -2.5,
            },
            movement: MovementPreset {
                mode: MovementMode::Off,
                rate_hz: 0.45,
                depth: 0.0,
                shape: LfoShape::Sine,
                delay_ms: 16.0,
                feedback: 0.0,
                width: 0.0,
                phase_degrees: 0.0,
                tone: 0.5,
                mix: 0.0,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Slap,
                time_ms: 130.0,
                feedback: 0.22,
                size: 0.25,
                decay: 0.25,
                pre_delay_ms: 8.0,
                damping: 0.65,
                mix: 0.26,
                tone: 0.34,
                stereo_offset: 0.0,
                width: 0.45,
            },
            texture: TexturePreset {
                mode: TextureMode::Noise,
                wow_depth: 0.0,
                wow_rate_hz: 0.4,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.0,
                noise_amount: 0.28,
                noise_color: 0.22,
                degrade: 0.26,
                stereo_spread: 0.65,
                mix: 0.32,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 70.0,
                low_shelf_gain_db: -1.5,
                low_shelf_hz: 150.0,
                mid_gain_db: -1.2,
                mid_hz: 3_200.0,
                mid_q: 0.9,
                high_shelf_gain_db: -4.5,
                high_shelf_hz: 5_500.0,
                high_cut_hz: 9_500.0,
            },
            pre_eq: None,
            input_gain_db: -1.0,
            output_gain_db: -1.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::CleanWiden,
        name: "Clean Widen",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Clean,
                drive: 0.0,
                age: 0.0,
                tone: 0.5,
                noise: 0.0,
                mix: 0.0,
                output_trim_db: 0.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Chorus,
                rate_hz: 0.28,
                depth: 0.22,
                shape: LfoShape::Sine,
                delay_ms: 18.0,
                feedback: 0.05,
                width: 0.85,
                phase_degrees: 180.0,
                tone: 0.60,
                mix: 0.28,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reverb,
                time_ms: 280.0,
                feedback: 0.18,
                size: 0.32,
                decay: 0.34,
                pre_delay_ms: 18.0,
                damping: 0.42,
                mix: 0.18,
                tone: 0.58,
                stereo_offset: 0.0,
                width: 0.75,
            },
            texture: TexturePreset {
                mode: TextureMode::Off,
                wow_depth: 0.0,
                wow_rate_hz: 0.4,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.0,
                noise_amount: 0.0,
                noise_color: 0.5,
                degrade: 0.0,
                stereo_spread: 0.0,
                mix: 0.0,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 20.0,
                low_shelf_gain_db: 0.0,
                low_shelf_hz: 120.0,
                mid_gain_db: 0.0,
                mid_hz: 1_000.0,
                mid_q: 1.0,
                high_shelf_gain_db: 0.0,
                high_shelf_hz: 8_000.0,
                high_cut_hz: 20_000.0,
            },
            pre_eq: None,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::SweetConsole,
        name: "Sweet Console",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Sweet,
                drive: 0.34,
                age: 0.0,
                tone: 0.58,
                noise: 0.0,
                mix: 0.70,
                output_trim_db: -1.2,
            },
            movement: MovementPreset {
                mode: MovementMode::Doubler,
                rate_hz: 0.35,
                depth: 0.18,
                shape: LfoShape::Sine,
                delay_ms: 20.0,
                feedback: 0.06,
                width: 0.72,
                phase_degrees: 120.0,
                tone: 0.58,
                mix: 0.32,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Space,
                time_ms: 360.0,
                feedback: 0.24,
                size: 0.52,
                decay: 0.48,
                pre_delay_ms: 18.0,
                damping: 0.50,
                mix: 0.26,
                tone: 0.56,
                stereo_offset: 0.08,
                width: 0.82,
            },
            texture: TexturePreset {
                mode: TextureMode::Cassette,
                wow_depth: 0.18,
                wow_rate_hz: 0.38,
                flutter_depth: 0.07,
                flutter_rate_hz: 7.2,
                random_drift: 0.10,
                noise_amount: 0.16,
                noise_color: 0.42,
                degrade: 0.22,
                stereo_spread: 0.72,
                mix: 0.24,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 35.0,
                low_shelf_gain_db: -0.4,
                low_shelf_hz: 140.0,
                mid_gain_db: 0.6,
                mid_hz: 1_500.0,
                mid_q: 0.8,
                high_shelf_gain_db: -0.8,
                high_shelf_hz: 8_500.0,
                high_cut_hz: 17_000.0,
            },
            pre_eq: None,
            input_gain_db: -0.5,
            output_gain_db: -1.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::FuzzCollage,
        name: "Fuzz Collage",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Fuzz,
                drive: 0.44,
                age: 0.0,
                tone: 0.46,
                noise: 0.0,
                mix: 0.64,
                output_trim_db: -3.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Phaser,
                rate_hz: 0.72,
                depth: 0.42,
                shape: LfoShape::Sine,
                delay_ms: 14.0,
                feedback: 0.24,
                width: 0.70,
                phase_degrees: 140.0,
                tone: 0.48,
                mix: 0.38,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Collage,
                time_ms: 520.0,
                feedback: 0.34,
                size: 0.46,
                decay: 0.36,
                pre_delay_ms: 12.0,
                damping: 0.58,
                mix: 0.30,
                tone: 0.50,
                stereo_offset: 0.18,
                width: 0.78,
            },
            texture: TexturePreset {
                mode: TextureMode::Broken,
                wow_depth: 0.0,
                wow_rate_hz: 0.4,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.32,
                noise_amount: 0.20,
                noise_color: 0.62,
                degrade: 0.38,
                stereo_spread: 0.76,
                mix: 0.28,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 70.0,
                low_shelf_gain_db: -1.2,
                low_shelf_hz: 160.0,
                mid_gain_db: 0.8,
                mid_hz: 1_200.0,
                mid_q: 0.95,
                high_shelf_gain_db: -2.0,
                high_shelf_hz: 7_000.0,
                high_cut_hz: 14_000.0,
            },
            pre_eq: None,
            input_gain_db: -2.0,
            output_gain_db: -2.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::ReverseDream,
        name: "Reverse Dream",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Drive,
                drive: 0.26,
                age: 0.0,
                tone: 0.60,
                noise: 0.0,
                mix: 0.58,
                output_trim_db: -1.4,
            },
            movement: MovementPreset {
                mode: MovementMode::Vibrato,
                rate_hz: 2.1,
                depth: 0.24,
                shape: LfoShape::Triangle,
                delay_ms: 13.0,
                feedback: 0.0,
                width: 0.84,
                phase_degrees: 160.0,
                tone: 0.62,
                mix: 0.34,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reverse,
                time_ms: 620.0,
                feedback: 0.28,
                size: 0.42,
                decay: 0.36,
                pre_delay_ms: 16.0,
                damping: 0.52,
                mix: 0.32,
                tone: 0.55,
                stereo_offset: 0.22,
                width: 0.86,
            },
            texture: TexturePreset {
                mode: TextureMode::Filter,
                wow_depth: 0.0,
                wow_rate_hz: 0.4,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.0,
                noise_amount: 0.0,
                noise_color: 0.36,
                degrade: 0.24,
                stereo_spread: 0.58,
                mix: 0.24,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 45.0,
                low_shelf_gain_db: -0.6,
                low_shelf_hz: 140.0,
                mid_gain_db: -0.4,
                mid_hz: 2_500.0,
                mid_q: 0.75,
                high_shelf_gain_db: -1.0,
                high_shelf_hz: 9_000.0,
                high_cut_hz: 16_000.0,
            },
            pre_eq: None,
            input_gain_db: -0.5,
            output_gain_db: -1.2,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::TapeReels,
        name: "Tape Reels",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Sweet,
                drive: 0.30,
                age: 0.0,
                tone: 0.52,
                noise: 0.0,
                mix: 0.66,
                output_trim_db: -1.5,
            },
            movement: MovementPreset {
                mode: MovementMode::Doubler,
                rate_hz: 0.30,
                depth: 0.14,
                shape: LfoShape::Sine,
                delay_ms: 24.0,
                feedback: 0.08,
                width: 0.82,
                phase_degrees: 150.0,
                tone: 0.50,
                mix: 0.30,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reels,
                time_ms: 460.0,
                feedback: 0.36,
                size: 0.38,
                decay: 0.34,
                pre_delay_ms: 14.0,
                damping: 0.54,
                mix: 0.28,
                tone: 0.44,
                stereo_offset: 0.16,
                width: 0.74,
            },
            texture: TexturePreset {
                mode: TextureMode::Cassette,
                wow_depth: 0.25,
                wow_rate_hz: 0.42,
                flutter_depth: 0.10,
                flutter_rate_hz: 8.4,
                random_drift: 0.18,
                noise_amount: 0.20,
                noise_color: 0.35,
                degrade: 0.30,
                stereo_spread: 0.78,
                mix: 0.30,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 50.0,
                low_shelf_gain_db: -0.8,
                low_shelf_hz: 130.0,
                mid_gain_db: -0.8,
                mid_hz: 2_200.0,
                mid_q: 0.8,
                high_shelf_gain_db: -2.6,
                high_shelf_hz: 6_800.0,
                high_cut_hz: 13_500.0,
            },
            pre_eq: None,
            input_gain_db: -0.5,
            output_gain_db: -1.5,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::InterferenceSwell,
        name: "Interference Swell",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Swell,
                drive: 0.48,
                age: 0.0,
                tone: 0.54,
                noise: 0.0,
                mix: 0.72,
                output_trim_db: -2.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Pitch,
                rate_hz: 0.80,
                depth: 0.28,
                shape: LfoShape::Sine,
                delay_ms: 15.0,
                feedback: 0.0,
                width: 0.76,
                phase_degrees: 180.0,
                tone: 0.55,
                mix: 0.30,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Space,
                time_ms: 500.0,
                feedback: 0.22,
                size: 0.58,
                decay: 0.52,
                pre_delay_ms: 24.0,
                damping: 0.56,
                mix: 0.30,
                tone: 0.50,
                stereo_offset: 0.12,
                width: 0.88,
            },
            texture: TexturePreset {
                mode: TextureMode::Interference,
                wow_depth: 0.0,
                wow_rate_hz: 0.4,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.36,
                noise_amount: 0.26,
                noise_color: 0.68,
                degrade: 0.28,
                stereo_spread: 0.84,
                mix: 0.24,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 60.0,
                low_shelf_gain_db: -1.0,
                low_shelf_hz: 150.0,
                mid_gain_db: -0.6,
                mid_hz: 1_800.0,
                mid_q: 0.85,
                high_shelf_gain_db: -1.5,
                high_shelf_hz: 8_000.0,
                high_cut_hz: 15_500.0,
            },
            pre_eq: None,
            input_gain_db: -1.0,
            output_gain_db: -2.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::HowlingTapeLead,
        name: "Howling Tape Lead",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Howl,
                drive: 0.46,
                age: 0.0,
                tone: 0.58,
                noise: 0.0,
                mix: 0.74,
                output_trim_db: -2.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Phaser,
                rate_hz: 0.42,
                depth: 0.24,
                shape: LfoShape::Sine,
                delay_ms: 14.0,
                feedback: 0.12,
                width: 0.62,
                phase_degrees: 140.0,
                tone: 0.52,
                mix: 0.24,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reels,
                time_ms: 330.0,
                feedback: 0.32,
                size: 0.34,
                decay: 0.30,
                pre_delay_ms: 10.0,
                damping: 0.50,
                mix: 0.24,
                tone: 0.50,
                stereo_offset: 0.12,
                width: 0.70,
            },
            texture: TexturePreset {
                mode: TextureMode::Cassette,
                wow_depth: 0.16,
                wow_rate_hz: 0.36,
                flutter_depth: 0.06,
                flutter_rate_hz: 7.8,
                random_drift: 0.12,
                noise_amount: 0.10,
                noise_color: 0.40,
                degrade: 0.18,
                stereo_spread: 0.62,
                mix: 0.20,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 75.0,
                low_shelf_gain_db: -1.4,
                low_shelf_hz: 150.0,
                mid_gain_db: 1.2,
                mid_hz: 1_650.0,
                mid_q: 0.85,
                high_shelf_gain_db: -0.8,
                high_shelf_hz: 8_200.0,
                high_cut_hz: 16_000.0,
            },
            pre_eq: None,
            input_gain_db: -1.0,
            output_gain_db: -2.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::SwellReverseBloom,
        name: "Swell Reverse Bloom",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Swell,
                drive: 0.56,
                age: 0.0,
                tone: 0.48,
                noise: 0.0,
                mix: 0.78,
                output_trim_db: -2.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Vibrato,
                rate_hz: 1.15,
                depth: 0.16,
                shape: LfoShape::Sine,
                delay_ms: 13.0,
                feedback: 0.0,
                width: 0.72,
                phase_degrees: 160.0,
                tone: 0.50,
                mix: 0.22,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reverse,
                time_ms: 720.0,
                feedback: 0.26,
                size: 0.50,
                decay: 0.48,
                pre_delay_ms: 20.0,
                damping: 0.58,
                mix: 0.34,
                tone: 0.48,
                stereo_offset: 0.18,
                width: 0.82,
            },
            texture: TexturePreset {
                mode: TextureMode::Cassette,
                wow_depth: 0.12,
                wow_rate_hz: 0.34,
                flutter_depth: 0.04,
                flutter_rate_hz: 7.0,
                random_drift: 0.08,
                noise_amount: 0.08,
                noise_color: 0.38,
                degrade: 0.14,
                stereo_spread: 0.66,
                mix: 0.18,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 50.0,
                low_shelf_gain_db: -0.6,
                low_shelf_hz: 140.0,
                mid_gain_db: -0.4,
                mid_hz: 2_200.0,
                mid_q: 0.75,
                high_shelf_gain_db: -1.2,
                high_shelf_hz: 7_800.0,
                high_cut_hz: 16_500.0,
            },
            pre_eq: None,
            input_gain_db: -1.0,
            output_gain_db: -2.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::ReelsDubEcho,
        name: "Reels Dub Echo",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Sweet,
                drive: 0.24,
                age: 0.0,
                tone: 0.46,
                noise: 0.0,
                mix: 0.60,
                output_trim_db: -1.4,
            },
            movement: MovementPreset {
                mode: MovementMode::Doubler,
                rate_hz: 0.24,
                depth: 0.10,
                shape: LfoShape::Sine,
                delay_ms: 22.0,
                feedback: 0.04,
                width: 0.66,
                phase_degrees: 120.0,
                tone: 0.46,
                mix: 0.18,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reels,
                time_ms: 520.0,
                feedback: 0.48,
                size: 0.44,
                decay: 0.46,
                pre_delay_ms: 12.0,
                damping: 0.64,
                mix: 0.36,
                tone: 0.40,
                stereo_offset: 0.20,
                width: 0.78,
            },
            texture: TexturePreset {
                mode: TextureMode::Broken,
                wow_depth: 0.0,
                wow_rate_hz: 0.40,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.20,
                noise_amount: 0.12,
                noise_color: 0.42,
                degrade: 0.22,
                stereo_spread: 0.70,
                mix: 0.18,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 60.0,
                low_shelf_gain_db: -1.0,
                low_shelf_hz: 130.0,
                mid_gain_db: -0.8,
                mid_hz: 2_000.0,
                mid_q: 0.8,
                high_shelf_gain_db: -2.2,
                high_shelf_hz: 6_500.0,
                high_cut_hz: 13_000.0,
            },
            pre_eq: None,
            input_gain_db: -1.0,
            output_gain_db: -2.5,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::ReversePsychedelic,
        name: "Reverse Psychedelic",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Drive,
                drive: 0.34,
                age: 0.0,
                tone: 0.56,
                noise: 0.0,
                mix: 0.66,
                output_trim_db: -1.8,
            },
            movement: MovementPreset {
                mode: MovementMode::Phaser,
                rate_hz: 0.58,
                depth: 0.38,
                shape: LfoShape::Sine,
                delay_ms: 14.0,
                feedback: 0.18,
                width: 0.76,
                phase_degrees: 170.0,
                tone: 0.54,
                mix: 0.34,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Reverse,
                time_ms: 560.0,
                feedback: 0.34,
                size: 0.36,
                decay: 0.42,
                pre_delay_ms: 18.0,
                damping: 0.50,
                mix: 0.32,
                tone: 0.54,
                stereo_offset: 0.26,
                width: 0.86,
            },
            texture: TexturePreset {
                mode: TextureMode::Interference,
                wow_depth: 0.0,
                wow_rate_hz: 0.40,
                flutter_depth: 0.0,
                flutter_rate_hz: 7.0,
                random_drift: 0.22,
                noise_amount: 0.14,
                noise_color: 0.60,
                degrade: 0.18,
                stereo_spread: 0.74,
                mix: 0.16,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 55.0,
                low_shelf_gain_db: -0.8,
                low_shelf_hz: 145.0,
                mid_gain_db: 0.8,
                mid_hz: 1_250.0,
                mid_q: 0.9,
                high_shelf_gain_db: -1.0,
                high_shelf_hz: 8_000.0,
                high_cut_hz: 15_500.0,
            },
            pre_eq: None,
            input_gain_db: -1.0,
            output_gain_db: -2.0,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::SoftSwellSpace,
        name: "Soft Swell Space",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Swell,
                drive: 0.36,
                age: 0.0,
                tone: 0.42,
                noise: 0.0,
                mix: 0.66,
                output_trim_db: -1.5,
            },
            movement: MovementPreset {
                mode: MovementMode::Chorus,
                rate_hz: 0.24,
                depth: 0.22,
                shape: LfoShape::Sine,
                delay_ms: 20.0,
                feedback: 0.06,
                width: 0.80,
                phase_degrees: 180.0,
                tone: 0.48,
                mix: 0.28,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Space,
                time_ms: 440.0,
                feedback: 0.24,
                size: 0.62,
                decay: 0.56,
                pre_delay_ms: 26.0,
                damping: 0.60,
                mix: 0.30,
                tone: 0.46,
                stereo_offset: 0.10,
                width: 0.90,
            },
            texture: TexturePreset {
                mode: TextureMode::Tape,
                wow_depth: 0.14,
                wow_rate_hz: 0.30,
                flutter_depth: 0.05,
                flutter_rate_hz: 6.8,
                random_drift: 0.10,
                noise_amount: 0.06,
                noise_color: 0.36,
                degrade: 0.12,
                stereo_spread: 0.70,
                mix: 0.16,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 45.0,
                low_shelf_gain_db: -0.4,
                low_shelf_hz: 135.0,
                mid_gain_db: -0.6,
                mid_hz: 2_400.0,
                mid_q: 0.75,
                high_shelf_gain_db: -1.4,
                high_shelf_hz: 7_200.0,
                high_cut_hz: 16_000.0,
            },
            pre_eq: None,
            input_gain_db: -0.5,
            output_gain_db: -1.8,
            dry_wet: 1.0,
        },
    },
    InternalPreset {
        id: PresetId::PreEqSculpted,
        name: "Pre EQ Sculpted",
        values: PresetValues {
            character: CharacterPreset {
                mode: CharacterMode::Saturation,
                drive: 0.14,
                age: 0.0,
                tone: 0.50,
                noise: 0.0,
                mix: 0.80,
                output_trim_db: -2.0,
            },
            movement: MovementPreset {
                mode: MovementMode::Chorus,
                rate_hz: 0.45,
                depth: 0.20,
                shape: LfoShape::Sine,
                delay_ms: 14.0,
                feedback: 0.0,
                width: 0.70,
                phase_degrees: 180.0,
                tone: 0.46,
                mix: 0.18,
            },
            diffusion: DiffusionPreset {
                mode: DiffusionMode::Space,
                time_ms: 380.0,
                feedback: 0.18,
                size: 0.55,
                decay: 0.48,
                pre_delay_ms: 22.0,
                damping: 0.50,
                mix: 0.28,
                tone: 0.45,
                stereo_offset: 0.10,
                width: 0.80,
            },
            texture: TexturePreset {
                mode: TextureMode::Filter,
                wow_depth: 0.0,
                wow_rate_hz: 0.5,
                flutter_depth: 0.0,
                flutter_rate_hz: 5.0,
                random_drift: 0.0,
                noise_amount: 0.0,
                noise_color: 0.40,
                degrade: 0.0,
                stereo_spread: 0.0,
                mix: 0.10,
            },
            eq: EqPreset {
                mode: EqMode::On,
                low_cut_hz: 45.0,
                low_shelf_gain_db: 0.6,
                low_shelf_hz: 130.0,
                mid_gain_db: -1.5,
                mid_hz: 2_800.0,
                mid_q: 0.80,
                high_shelf_gain_db: -0.8,
                high_shelf_hz: 7_800.0,
                high_cut_hz: 17_000.0,
            },
            pre_eq: Some(EqPreset {
                mode: EqMode::On,
                low_cut_hz: 55.0,
                low_shelf_gain_db: 2.0,
                low_shelf_hz: 180.0,
                mid_gain_db: -2.5,
                mid_hz: 420.0,
                mid_q: 1.2,
                high_shelf_gain_db: 0.0,
                high_shelf_hz: 5_500.0,
                high_cut_hz: 15_000.0,
            }),
            input_gain_db: -1.0,
            output_gain_db: -1.5,
            dry_wet: 1.0,
        },
    },
];

pub fn internal_presets() -> &'static [InternalPreset] {
    &INTERNAL_PRESETS
}

pub fn find_preset(id: PresetId) -> Option<&'static InternalPreset> {
    INTERNAL_PRESETS.iter().find(|preset| preset.id == id)
}

impl InternalPreset {
    pub fn apply_with_setter(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        self.values.apply_with_setter(setter, params);
    }
}

impl PresetValues {
    pub fn apply_with_setter(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        set_param(setter, &params.input_gain, self.input_gain_db);
        set_param(setter, &params.output_gain, self.output_gain_db);
        set_param(setter, &params.dry_wet, self.dry_wet);

        self.character.apply_with_setter(setter, params);
        self.movement.apply_with_setter(setter, params);
        self.diffusion.apply_with_setter(setter, params);
        self.texture.apply_with_setter(setter, params);
        self.eq.apply_with_setter(setter, params);
        if let Some(pre_eq) = &self.pre_eq {
            pre_eq.apply_to_pre_eq(setter, params);
        }
    }
}

impl CharacterPreset {
    fn apply_with_setter(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        let character = &params.character;
        set_param(setter, &character.mode, self.mode);
        set_param(setter, &character.bypass, false);
        set_param(setter, &character.drive, self.drive);
        set_param(setter, &character.age, self.age);
        set_param(setter, &character.tone, self.tone);
        set_param(setter, &character.noise, self.noise);
        set_param(setter, &character.mix, self.mix);
        set_param(setter, &character.output_trim, self.output_trim_db);
    }
}

impl MovementPreset {
    fn apply_with_setter(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        let movement = &params.movement;
        set_param(setter, &movement.mode, self.mode);
        set_param(setter, &movement.bypass, false);
        set_param(setter, &movement.rate, self.rate_hz);
        set_param(setter, &movement.depth, self.depth);
        set_param(setter, &movement.shape, self.shape);
        set_param(setter, &movement.delay, self.delay_ms);
        set_param(setter, &movement.feedback, self.feedback);
        set_param(setter, &movement.width, self.width);
        set_param(setter, &movement.phase, self.phase_degrees);
        set_param(setter, &movement.tone, self.tone);
        set_param(setter, &movement.mix, self.mix);
    }
}

impl DiffusionPreset {
    fn apply_with_setter(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        let diffusion = &params.diffusion;
        set_param(setter, &diffusion.mode, self.mode);
        set_param(setter, &diffusion.bypass, false);
        set_param(setter, &diffusion.time, self.time_ms);
        set_param(setter, &diffusion.feedback, self.feedback);
        set_param(setter, &diffusion.size, self.size);
        set_param(setter, &diffusion.decay, self.decay);
        set_param(setter, &diffusion.pre_delay, self.pre_delay_ms);
        set_param(setter, &diffusion.damping, self.damping);
        set_param(setter, &diffusion.mix, self.mix);
        set_param(setter, &diffusion.tone, self.tone);
        set_param(setter, &diffusion.stereo_offset, self.stereo_offset);
        set_param(setter, &diffusion.width, self.width);
    }
}

impl TexturePreset {
    fn apply_with_setter(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        let texture = &params.texture;
        set_param(setter, &texture.mode, self.mode);
        set_param(setter, &texture.bypass, false);
        set_param(setter, &texture.wow_depth, self.wow_depth);
        set_param(setter, &texture.wow_rate, self.wow_rate_hz);
        set_param(setter, &texture.flutter_depth, self.flutter_depth);
        set_param(setter, &texture.flutter_rate, self.flutter_rate_hz);
        set_param(setter, &texture.random_drift, self.random_drift);
        set_param(setter, &texture.noise_amount, self.noise_amount);
        set_param(setter, &texture.noise_color, self.noise_color);
        set_param(setter, &texture.degrade, self.degrade);
        set_param(setter, &texture.stereo_spread, self.stereo_spread);
        set_param(setter, &texture.mix, self.mix);
    }
}

impl EqPreset {
    fn apply_with_setter(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        let eq = &params.post_eq;
        set_param(setter, &eq.mode, self.mode);
        set_param(setter, &eq.bypass, false);
        set_param(setter, &eq.band1_enabled, true);
        set_param(setter, &eq.band1_type, EqBandType::LowShelf);
        set_param(setter, &eq.band1_frequency, self.low_shelf_hz);
        set_param(setter, &eq.band1_gain, self.low_shelf_gain_db);
        set_param(setter, &eq.band1_q, 1.0);
        set_param(setter, &eq.band2_enabled, true);
        set_param(setter, &eq.band2_type, EqBandType::Bell);
        set_param(setter, &eq.band2_frequency, self.mid_hz);
        set_param(setter, &eq.band2_gain, self.mid_gain_db);
        set_param(setter, &eq.band2_q, self.mid_q);
        set_param(setter, &eq.band3_enabled, true);
        set_param(setter, &eq.band3_type, EqBandType::HighShelf);
        set_param(setter, &eq.band3_frequency, self.high_shelf_hz);
        set_param(setter, &eq.band3_gain, self.high_shelf_gain_db);
        set_param(setter, &eq.band3_q, 1.0);
        set_param(setter, &eq.band4_enabled, false);
        set_param(setter, &eq.band4_type, EqBandType::Off);
        set_param(setter, &eq.band4_frequency, self.low_cut_hz);
        set_param(setter, &eq.band4_gain, 0.0);
        set_param(setter, &eq.band4_q, 0.707);
        set_param(setter, &eq.band5_enabled, false);
        set_param(setter, &eq.band5_type, EqBandType::Off);
        set_param(setter, &eq.band5_frequency, self.high_cut_hz);
        set_param(setter, &eq.band5_gain, 0.0);
        set_param(setter, &eq.band5_q, 0.707);
    }

    fn apply_to_pre_eq(&self, setter: &ParamSetter<'_>, params: &Cc22Params) {
        let eq = &params.pre_eq;
        set_param(setter, &eq.mode, self.mode);
        set_param(setter, &eq.bypass, false);
        set_param(setter, &eq.band1_enabled, true);
        set_param(setter, &eq.band1_type, EqBandType::LowShelf);
        set_param(setter, &eq.band1_frequency, self.low_shelf_hz);
        set_param(setter, &eq.band1_gain, self.low_shelf_gain_db);
        set_param(setter, &eq.band1_q, 1.0);
        set_param(setter, &eq.band2_enabled, true);
        set_param(setter, &eq.band2_type, EqBandType::Bell);
        set_param(setter, &eq.band2_frequency, self.mid_hz);
        set_param(setter, &eq.band2_gain, self.mid_gain_db);
        set_param(setter, &eq.band2_q, self.mid_q);
        set_param(setter, &eq.band3_enabled, true);
        set_param(setter, &eq.band3_type, EqBandType::HighShelf);
        set_param(setter, &eq.band3_frequency, self.high_shelf_hz);
        set_param(setter, &eq.band3_gain, self.high_shelf_gain_db);
        set_param(setter, &eq.band3_q, 1.0);
        set_param(setter, &eq.band4_enabled, false);
        set_param(setter, &eq.band4_type, EqBandType::Off);
        set_param(setter, &eq.band4_frequency, self.low_cut_hz);
        set_param(setter, &eq.band4_gain, 0.0);
        set_param(setter, &eq.band4_q, 0.707);
        set_param(setter, &eq.band5_enabled, false);
        set_param(setter, &eq.band5_type, EqBandType::Off);
        set_param(setter, &eq.band5_frequency, self.high_cut_hz);
        set_param(setter, &eq.band5_gain, 0.0);
        set_param(setter, &eq.band5_q, 0.707);
    }
}

fn set_param<P: Param>(setter: &ParamSetter<'_>, param: &P, value: P::Plain) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

#[cfg(test)]
mod tests {
    use super::{find_preset, internal_presets, PresetId};
    use crate::dsp::{
        character::CharacterMode, diffusion::DiffusionMode, movement::MovementMode,
        texture::TextureMode,
    };

    #[test]
    fn exposes_named_presets_without_reordering_existing_ones() {
        let presets = internal_presets();

        assert_eq!(presets.len(), 16);
        assert_eq!(presets[0].name, "Warm Tape Chorus");
        assert_eq!(presets[4].name, "Clean Widen");
        assert_eq!(presets[5].name, "Sweet Console");
        assert_eq!(presets[9].name, "Interference Swell");
        assert_eq!(presets[10].name, "Howling Tape Lead");
        assert_eq!(presets[14].name, "Soft Swell Space");
    }

    #[test]
    fn preset_lookup_uses_stable_ids() {
        let preset = find_preset(PresetId::LoFiRoom).expect("preset exists");

        assert_eq!(preset.name, "Lo-Fi Room");
        assert_eq!(preset.values.character.mode, CharacterMode::Cassette);
        assert_eq!(preset.values.diffusion.mode, DiffusionMode::Slap);
        assert_eq!(preset.values.texture.mode, TextureMode::Noise);
    }

    #[test]
    fn presets_avoid_extreme_mix_and_feedback_values() {
        for preset in internal_presets() {
            let values = preset.values;
            assert!((0.0..=1.0).contains(&values.dry_wet));
            assert!(values.diffusion.feedback <= 0.60);
            assert!(values.diffusion.mix <= 0.40);
            assert!(values.texture.mix <= 0.45);
            assert!(values.movement.mix <= 0.60);
            assert!(values.character.mix <= 0.85);
            assert!(values.input_gain_db <= 0.0);
            assert!(values.output_gain_db <= 0.0);
        }
    }

    #[test]
    fn legacy_presets_load_with_migratable_eq_values() {
        for preset in internal_presets() {
            let eq = preset.values.eq;

            assert!(eq.low_cut_hz.is_finite());
            assert!(eq.low_shelf_hz.is_finite());
            assert!(eq.low_shelf_gain_db.is_finite());
            assert!(eq.mid_hz.is_finite());
            assert!(eq.mid_gain_db.is_finite());
            assert!(eq.mid_q.is_finite());
            assert!(eq.high_shelf_hz.is_finite());
            assert!(eq.high_shelf_gain_db.is_finite());
            assert!(eq.high_cut_hz.is_finite());
            assert!((20.0..=20_000.0).contains(&eq.low_cut_hz));
            assert!((20.0..=20_000.0).contains(&eq.low_shelf_hz));
            assert!((20.0..=20_000.0).contains(&eq.mid_hz));
            assert!((20.0..=20_000.0).contains(&eq.high_shelf_hz));
            assert!((20.0..=20_000.0).contains(&eq.high_cut_hz));
            assert!((-24.0..=24.0).contains(&eq.low_shelf_gain_db));
            assert!((-24.0..=24.0).contains(&eq.mid_gain_db));
            assert!((-24.0..=24.0).contains(&eq.high_shelf_gain_db));
            assert!((0.1..=12.0).contains(&eq.mid_q));
        }
    }

    #[test]
    fn clean_widen_keeps_character_and_texture_neutral() {
        let preset = find_preset(PresetId::CleanWiden).expect("preset exists");

        assert_eq!(preset.values.character.mode, CharacterMode::Clean);
        assert_eq!(preset.values.texture.mode, TextureMode::Off);
        assert_eq!(preset.values.movement.mode, MovementMode::Chorus);
    }

    #[test]
    fn new_presets_cover_new_mode_combinations() {
        let sweet_console = find_preset(PresetId::SweetConsole).expect("preset exists");
        assert_eq!(sweet_console.values.character.mode, CharacterMode::Sweet);
        assert_eq!(sweet_console.values.movement.mode, MovementMode::Doubler);
        assert_eq!(sweet_console.values.diffusion.mode, DiffusionMode::Space);
        assert_eq!(sweet_console.values.texture.mode, TextureMode::Cassette);

        let fuzz_collage = find_preset(PresetId::FuzzCollage).expect("preset exists");
        assert_eq!(fuzz_collage.values.character.mode, CharacterMode::Fuzz);
        assert_eq!(fuzz_collage.values.movement.mode, MovementMode::Phaser);
        assert_eq!(fuzz_collage.values.diffusion.mode, DiffusionMode::Collage);
        assert_eq!(fuzz_collage.values.texture.mode, TextureMode::Broken);

        let reverse_dream = find_preset(PresetId::ReverseDream).expect("preset exists");
        assert_eq!(reverse_dream.values.character.mode, CharacterMode::Drive);
        assert_eq!(reverse_dream.values.movement.mode, MovementMode::Vibrato);
        assert_eq!(reverse_dream.values.diffusion.mode, DiffusionMode::Reverse);
        assert_eq!(reverse_dream.values.texture.mode, TextureMode::Filter);

        let tape_reels = find_preset(PresetId::TapeReels).expect("preset exists");
        assert_eq!(tape_reels.values.character.mode, CharacterMode::Sweet);
        assert_eq!(tape_reels.values.movement.mode, MovementMode::Doubler);
        assert_eq!(tape_reels.values.diffusion.mode, DiffusionMode::Reels);
        assert_eq!(tape_reels.values.texture.mode, TextureMode::Cassette);

        let interference_swell = find_preset(PresetId::InterferenceSwell).expect("preset exists");
        assert_eq!(
            interference_swell.values.character.mode,
            CharacterMode::Swell
        );
        assert_eq!(interference_swell.values.movement.mode, MovementMode::Pitch);
        assert_eq!(
            interference_swell.values.diffusion.mode,
            DiffusionMode::Space
        );
        assert_eq!(
            interference_swell.values.texture.mode,
            TextureMode::Interference
        );

        let howling_tape_lead = find_preset(PresetId::HowlingTapeLead).expect("preset exists");
        assert_eq!(howling_tape_lead.values.character.mode, CharacterMode::Howl);
        assert_eq!(howling_tape_lead.values.movement.mode, MovementMode::Phaser);
        assert_eq!(
            howling_tape_lead.values.diffusion.mode,
            DiffusionMode::Reels
        );
        assert_eq!(howling_tape_lead.values.texture.mode, TextureMode::Cassette);

        let swell_reverse_bloom = find_preset(PresetId::SwellReverseBloom).expect("preset exists");
        assert_eq!(
            swell_reverse_bloom.values.character.mode,
            CharacterMode::Swell
        );
        assert_eq!(
            swell_reverse_bloom.values.movement.mode,
            MovementMode::Vibrato
        );
        assert_eq!(
            swell_reverse_bloom.values.diffusion.mode,
            DiffusionMode::Reverse
        );
        assert_eq!(
            swell_reverse_bloom.values.texture.mode,
            TextureMode::Cassette
        );

        let reels_dub_echo = find_preset(PresetId::ReelsDubEcho).expect("preset exists");
        assert_eq!(reels_dub_echo.values.character.mode, CharacterMode::Sweet);
        assert_eq!(reels_dub_echo.values.movement.mode, MovementMode::Doubler);
        assert_eq!(reels_dub_echo.values.diffusion.mode, DiffusionMode::Reels);
        assert_eq!(reels_dub_echo.values.texture.mode, TextureMode::Broken);

        let reverse_psychedelic = find_preset(PresetId::ReversePsychedelic).expect("preset exists");
        assert_eq!(
            reverse_psychedelic.values.character.mode,
            CharacterMode::Drive
        );
        assert_eq!(
            reverse_psychedelic.values.movement.mode,
            MovementMode::Phaser
        );
        assert_eq!(
            reverse_psychedelic.values.diffusion.mode,
            DiffusionMode::Reverse
        );
        assert_eq!(
            reverse_psychedelic.values.texture.mode,
            TextureMode::Interference
        );

        let soft_swell_space = find_preset(PresetId::SoftSwellSpace).expect("preset exists");
        assert_eq!(soft_swell_space.values.character.mode, CharacterMode::Swell);
        assert_eq!(soft_swell_space.values.movement.mode, MovementMode::Chorus);
        assert_eq!(soft_swell_space.values.diffusion.mode, DiffusionMode::Space);
        assert_eq!(soft_swell_space.values.texture.mode, TextureMode::Tape);
    }

    #[test]
    fn pre_eq_sculpted_preset_sets_pre_and_post_eq() {
        let preset = find_preset(PresetId::PreEqSculpted).expect("preset exists");
        assert!(preset.values.pre_eq.is_some(), "should have pre_eq");
        assert_eq!(preset.values.character.mode, CharacterMode::Saturation);
        assert_eq!(preset.values.movement.mode, MovementMode::Chorus);
        assert_eq!(preset.values.diffusion.mode, DiffusionMode::Space);
        assert_eq!(preset.values.texture.mode, TextureMode::Filter);
    }
}
