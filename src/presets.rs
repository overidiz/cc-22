use nih_plug::prelude::{Param, ParamSetter};

use crate::{
    dsp::{
        character::CharacterMode, diffusion::DiffusionMode, eq::EqMode, movement::LfoShape,
        movement::MovementMode, texture::TextureMode,
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

pub const INTERNAL_PRESETS: [InternalPreset; 5] = [
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
            input_gain_db: 0.0,
            output_gain_db: 0.0,
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
        let eq = &params.eq;
        set_param(setter, &eq.mode, self.mode);
        set_param(setter, &eq.bypass, false);
        set_param(setter, &eq.low_cut_frequency, self.low_cut_hz);
        set_param(setter, &eq.low_shelf_gain, self.low_shelf_gain_db);
        set_param(setter, &eq.low_shelf_frequency, self.low_shelf_hz);
        set_param(setter, &eq.mid_gain, self.mid_gain_db);
        set_param(setter, &eq.mid_frequency, self.mid_hz);
        set_param(setter, &eq.mid_q, self.mid_q);
        set_param(setter, &eq.high_shelf_gain, self.high_shelf_gain_db);
        set_param(setter, &eq.high_shelf_frequency, self.high_shelf_hz);
        set_param(setter, &eq.high_cut_frequency, self.high_cut_hz);
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
    fn exposes_five_named_presets() {
        let presets = internal_presets();

        assert_eq!(presets.len(), 5);
        assert_eq!(presets[0].name, "Warm Tape Chorus");
        assert_eq!(presets[4].name, "Clean Widen");
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
        }
    }

    #[test]
    fn clean_widen_keeps_character_and_texture_neutral() {
        let preset = find_preset(PresetId::CleanWiden).expect("preset exists");

        assert_eq!(preset.values.character.mode, CharacterMode::Clean);
        assert_eq!(preset.values.texture.mode, TextureMode::Off);
        assert_eq!(preset.values.movement.mode, MovementMode::Chorus);
    }
}
