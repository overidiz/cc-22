use nih_plug::prelude::Buffer;

use crate::params::Cc22Params;

use super::{
    bypass::BypassCrossfade,
    character::{Character, CharacterFrame},
    diffusion::{Diffusion, DiffusionFrame},
    dry_wet::DryWet,
    eq::{Eq, EqFrame},
    gain::db_to_gain,
    movement::{Movement, MovementFrame},
    texture::{Texture, TextureFrame},
};

pub const SAFETY_LIMIT_CEILING: f32 = 8.0;
const SOFT_CLIP_START: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainModule {
    Character = 0,
    Movement = 1,
    Diffusion = 2,
    Texture = 3,
}

pub fn default_chain_order() -> [ChainModule; 4] {
    [
        ChainModule::Character,
        ChainModule::Movement,
        ChainModule::Diffusion,
        ChainModule::Texture,
    ]
}

pub fn module_from_slot(value: usize) -> ChainModule {
    match value {
        0 => ChainModule::Character,
        1 => ChainModule::Movement,
        2 => ChainModule::Diffusion,
        3 => ChainModule::Texture,
        _ => ChainModule::Character,
    }
}

pub fn slot_from_module(module: ChainModule) -> usize {
    module as usize
}

pub fn reorder_module(
    order: [ChainModule; 4],
    from_index: usize,
    to_index: usize,
) -> [ChainModule; 4] {
    if from_index == to_index || from_index >= 4 || to_index >= 4 {
        return order;
    }
    let mut result = order;
    let moved = result[from_index];
    if from_index < to_index {
        for i in from_index..to_index {
            result[i] = result[i + 1];
        }
    } else {
        for i in (to_index + 1..=from_index).rev() {
            result[i] = result[i - 1];
        }
    }
    result[to_index] = moved;
    result
}

pub fn validate_chain_order(slots: &[usize; 4]) -> [ChainModule; 4] {
    let mut seen = [false; 4];
    for &s in slots {
        if s >= 4 {
            return default_chain_order();
        }
        if seen[s] {
            return default_chain_order();
        }
        seen[s] = true;
    }
    slots.map(module_from_slot)
}

#[derive(Default)]
pub struct EffectChain {
    pre_eq: Eq,
    character: Character,
    movement: Movement,
    diffusion: Diffusion,
    texture: Texture,
    post_eq: Eq,
}

pub struct ChainFrame {
    pre_eq: EqFrame,
    character: CharacterFrame,
    movement: MovementFrame,
    diffusion: DiffusionFrame,
    texture: TextureFrame,
    post_eq: EqFrame,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleCore {
    bypass: BypassCrossfade,
}

#[derive(Debug, Clone, Copy)]
pub struct ModuleFrame {
    pub mix: f32,
    pub output_gain: f32,
    pub active_mix: f32,
}

impl ModuleCore {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.bypass.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.bypass.reset(false);
    }

    pub fn next_frame(&mut self, bypassed: bool, mix: f32, output_trim_db: f32) -> ModuleFrame {
        self.bypass.set_bypassed(bypassed);

        ModuleFrame {
            mix: mix.clamp(0.0, 1.0),
            output_gain: db_to_gain(output_trim_db.clamp(-12.0, 12.0)),
            active_mix: self.bypass.next_active_mix(),
        }
    }

    pub fn apply_frame(&self, dry: f32, wet: f32, frame: &ModuleFrame) -> f32 {
        let staged = sanitize_sample(wet * frame.output_gain);
        let mixed = DryWet.mix(dry, staged, frame.mix);
        sanitize_sample(self.bypass.mix(dry, mixed, frame.active_mix))
    }

    #[inline]
    pub fn bypass_mix(&self, dry: f32, processed: f32, active_mix: f32) -> f32 {
        self.apply_frame(
            dry,
            processed,
            &ModuleFrame {
                mix: 1.0,
                output_gain: 1.0,
                active_mix,
            },
        )
    }
}

impl EffectChain {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.pre_eq.prepare(sample_rate);
        self.character.prepare(sample_rate);
        self.movement.prepare(sample_rate);
        self.diffusion.prepare(sample_rate);
        self.texture.prepare(sample_rate);
        self.post_eq.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.pre_eq.reset();
        self.character.reset();
        self.movement.reset();
        self.diffusion.reset();
        self.texture.reset();
        self.post_eq.reset();
    }

    pub fn next_frame(&mut self, params: &Cc22Params) -> ChainFrame {
        ChainFrame {
            pre_eq: self.pre_eq.next_frame(&params.pre_eq),
            character: self.character.next_frame(&params.character),
            movement: self.movement.next_frame(&params.movement),
            diffusion: self.diffusion.next_frame(&params.diffusion),
            texture: self.texture.next_frame(&params.texture),
            post_eq: self.post_eq.next_frame(&params.post_eq),
        }
    }

    pub fn process_sample(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &ChainFrame,
        order: &[ChainModule; 4],
    ) -> f32 {
        let mut sample = self
            .pre_eq
            .process_sample_for_channel(channel, sample, &frame.pre_eq);
        for &module in order.iter() {
            sample = match module {
                ChainModule::Character => {
                    self.character
                        .process_sample(channel, sample, &frame.character)
                }
                ChainModule::Movement => {
                    self.movement
                        .process_sample_for_channel(channel, sample, &frame.movement)
                }
                ChainModule::Diffusion => {
                    self.diffusion
                        .process_sample_for_channel(channel, sample, &frame.diffusion)
                }
                ChainModule::Texture => {
                    self.texture
                        .process_sample_for_channel(channel, sample, &frame.texture)
                }
            };
        }
        self.post_eq
            .process_sample_for_channel(channel, sample, &frame.post_eq)
    }

    pub fn process_block(
        &mut self,
        buffer: &mut Buffer,
        params: &Cc22Params,
        order: &[ChainModule; 4],
    ) {
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params);

            for (channel_index, sample) in channel_samples.into_iter().enumerate() {
                *sample = self.process_sample(channel_index, *sample, &frame, order);
            }
        }
    }
}

#[inline]
pub fn sanitize_sample(sample: f32) -> f32 {
    if sample.is_finite() {
        sample
    } else {
        0.0
    }
}

#[inline]
pub fn soft_clip_sample(sample: f32) -> f32 {
    let sample = sanitize_sample(sample);
    let magnitude = sample.abs();

    if magnitude <= SOFT_CLIP_START {
        sample
    } else {
        let headroom = SAFETY_LIMIT_CEILING - SOFT_CLIP_START;
        let limited =
            SOFT_CLIP_START + (headroom * ((magnitude - SOFT_CLIP_START) / headroom).tanh());
        sample.signum() * limited
    }
}

#[inline]
pub fn safety_limit_sample(sample: f32) -> f32 {
    soft_clip_sample(sample).clamp(-SAFETY_LIMIT_CEILING, SAFETY_LIMIT_CEILING)
}

#[cfg(test)]
mod tests {
    use nih_plug::prelude::{BoolParam, EnumParam, FloatParam, FloatRange};

    use crate::dsp::eq::EqBandType;
    use crate::params::{Cc22Params, EqParams, PreEqParams};

    use super::{
        default_chain_order, reorder_module, safety_limit_sample, sanitize_sample,
        soft_clip_sample, validate_chain_order, ChainModule, EffectChain, ModuleCore,
    };

    #[test]
    fn sanitizes_invalid_samples_without_limiting_finite_audio() {
        assert_eq!(sanitize_sample(f32::NAN), 0.0);
        assert_eq!(sanitize_sample(f32::INFINITY), 0.0);
        assert_eq!(sanitize_sample(-f32::INFINITY), 0.0);
        assert_eq!(sanitize_sample(32.0), 32.0);
    }

    #[test]
    fn safety_limiter_is_explicit_and_bounded() {
        assert_eq!(soft_clip_sample(1.0), 1.0);
        assert!(soft_clip_sample(16.0) < 8.0);
        assert!(safety_limit_sample(1_000.0) <= 8.0);
        assert!(safety_limit_sample(-1_000.0) >= -8.0);
    }

    #[test]
    fn module_core_applies_trim_mix_and_bypass() {
        let mut core = ModuleCore::default();
        core.prepare(1_000.0);
        let frame = core.next_frame(false, 1.0, 0.0);

        assert!((core.apply_frame(0.25, 0.5, &frame) - 0.5).abs() < 0.000_001);
    }

    #[test]
    fn default_order_is_character_movement_diffusion_texture() {
        let order = default_chain_order();
        assert_eq!(order[0], ChainModule::Character);
        assert_eq!(order[1], ChainModule::Movement);
        assert_eq!(order[2], ChainModule::Diffusion);
        assert_eq!(order[3], ChainModule::Texture);
    }

    #[test]
    fn reorder_moves_module_to_new_position() {
        let order = default_chain_order();
        let result = reorder_module(order, 0, 2);
        assert_eq!(
            result,
            [
                ChainModule::Movement,
                ChainModule::Diffusion,
                ChainModule::Character,
                ChainModule::Texture,
            ]
        );
    }

    #[test]
    fn reorder_noop_when_indices_equal() {
        let order = default_chain_order();
        let result = reorder_module(order, 1, 1);
        assert_eq!(result, order);
    }

    #[test]
    fn reorder_handles_forward_and_backward() {
        let order = default_chain_order();
        let fwd = reorder_module(order, 0, 3);
        assert_eq!(
            fwd,
            [
                ChainModule::Movement,
                ChainModule::Diffusion,
                ChainModule::Texture,
                ChainModule::Character,
            ]
        );

        let back = reorder_module(fwd, 3, 0);
        assert_eq!(back, default_chain_order());
    }

    #[test]
    fn validate_rejects_duplicates() {
        let result = validate_chain_order(&[0, 0, 1, 2]);
        assert_eq!(result, default_chain_order());
    }

    #[test]
    fn validate_rejects_out_of_bounds() {
        let result = validate_chain_order(&[4, 1, 2, 3]);
        assert_eq!(result, default_chain_order());
    }

    #[test]
    fn validate_accepts_valid_permutation() {
        let result = validate_chain_order(&[3, 2, 1, 0]);
        assert_eq!(
            result,
            [
                ChainModule::Texture,
                ChainModule::Diffusion,
                ChainModule::Movement,
                ChainModule::Character,
            ]
        );
    }

    #[test]
    fn pre_and_post_eq_can_be_configured_independently() {
        let pre_boost = process_eq_chain_rms(Some((1_000.0, 9.0)), Some((1_000.0, -9.0)));
        let post_only_cut = process_eq_chain_rms(None, Some((1_000.0, -9.0)));
        let pre_only_boost = process_eq_chain_rms(Some((1_000.0, 9.0)), None);

        assert!(pre_boost > post_only_cut * 1.8);
        assert!(pre_only_boost > pre_boost * 1.8);
    }

    #[test]
    fn pre_post_eq_off_preserves_signal() {
        let dry = process_eq_chain_rms(None, None);
        let reference = sine_rms();

        assert!((dry - reference).abs() < 0.000_1);
    }

    #[test]
    fn pre_or_post_eq_only_changes_signal_when_enabled() {
        let dry = process_eq_chain_rms(None, None);
        let pre_only = process_eq_chain_rms(Some((1_000.0, 6.0)), None);
        let post_only = process_eq_chain_rms(None, Some((1_000.0, 6.0)));

        assert!(pre_only > dry * 1.15);
        assert!(post_only > dry * 1.15);
        assert!((pre_only - post_only).abs() < dry * 0.08);
    }

    #[test]
    fn pre_and_post_eq_are_cumulative_when_both_active() {
        let post_only = process_eq_chain_rms(None, Some((1_000.0, 6.0)));
        let pre_only = process_eq_chain_rms(Some((1_000.0, 6.0)), None);
        let both = process_eq_chain_rms(Some((1_000.0, 6.0)), Some((1_000.0, 6.0)));

        assert!(both > post_only * 1.15);
        assert!(both > pre_only * 1.15);
    }

    #[test]
    fn changing_pre_eq_does_not_mutate_post_eq_params() {
        let mut params = Cc22Params::default();
        let post_gain = params.post_eq.band3_gain.value();
        let post_type = params.post_eq.band3_type.value();

        params.pre_eq.band3_enabled = BoolParam::new("Pre Band 3 Enabled", true);
        params.pre_eq.band3_type = EnumParam::new("Pre Band 3 Type", EqBandType::HighPass);
        params.pre_eq.band3_gain = FloatParam::new(
            "Pre Band 3 Gain",
            12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        assert_eq!(params.post_eq.band3_gain.value(), post_gain);
        assert_eq!(params.post_eq.band3_type.value(), post_type);
    }

    #[test]
    fn changing_post_eq_does_not_mutate_pre_eq_params() {
        let mut params = Cc22Params::default();
        let pre_gain = params.pre_eq.band3_gain.value();
        let pre_type = params.pre_eq.band3_type.value();

        params.post_eq.band3_enabled = BoolParam::new("Post Band 3 Enabled", true);
        params.post_eq.band3_type = EnumParam::new("Post Band 3 Type", EqBandType::LowPass);
        params.post_eq.band3_gain = FloatParam::new(
            "Post Band 3 Gain",
            -12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        assert_eq!(params.pre_eq.band3_gain.value(), pre_gain);
        assert_eq!(params.pre_eq.band3_type.value(), pre_type);
    }

    fn process_eq_chain_rms(pre_band: Option<(f32, f32)>, post_band: Option<(f32, f32)>) -> f32 {
        fn eq_gain_param(value: f32) -> FloatParam {
            FloatParam::new(
                "EQ Gain",
                value,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
        }

        fn eq_frequency_param(value: f32) -> FloatParam {
            FloatParam::new(
                "EQ Frequency",
                value,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
        }

        let mut params = Cc22Params::default();
        params.character.bypass = BoolParam::new("Character Bypass", true);
        params.movement.bypass = BoolParam::new("Movement Bypass", true);
        params.diffusion.bypass = BoolParam::new("Diffusion Bypass", true);
        params.texture.bypass = BoolParam::new("Texture Bypass", true);
        disable_pre_eq_bands(&mut params.pre_eq);
        disable_post_eq_bands(&mut params.post_eq);

        if let Some((frequency, gain)) = pre_band {
            params.pre_eq.bypass = BoolParam::new("Pre EQ Bypass", false);
            params.pre_eq.band3_enabled = BoolParam::new("Pre Band 3 Enabled", true);
            params.pre_eq.band3_type = EnumParam::new("Pre Band 3 Type", EqBandType::Bell);
            params.pre_eq.band3_frequency = eq_frequency_param(frequency);
            params.pre_eq.band3_gain = eq_gain_param(gain);
        }

        if let Some((frequency, gain)) = post_band {
            params.post_eq.bypass = BoolParam::new("Post EQ Bypass", false);
            params.post_eq.band3_enabled = BoolParam::new("Post Band 3 Enabled", true);
            params.post_eq.band3_type = EnumParam::new("Post Band 3 Type", EqBandType::Bell);
            params.post_eq.band3_frequency = eq_frequency_param(frequency);
            params.post_eq.band3_gain = eq_gain_param(gain);
        }

        params.reset_smoothers();

        let mut chain = EffectChain::default();
        chain.prepare(48_000.0);
        let order = params.chain_order();
        let mut sum = 0.0;

        for index in 0..4_800 {
            let frame = chain.next_frame(&params);
            let phase = core::f32::consts::TAU * 1_000.0 * index as f32 / 48_000.0;
            let sample = phase.sin() * 0.1;
            let out = chain.process_sample(0, sample, &frame, &order);
            sum += out * out;
        }

        (sum / 4_800.0).sqrt()
    }

    fn sine_rms() -> f32 {
        let mut sum = 0.0;
        for index in 0..4_800 {
            let phase = core::f32::consts::TAU * 1_000.0 * index as f32 / 48_000.0;
            let sample = phase.sin() * 0.1;
            sum += sample * sample;
        }
        (sum / 4_800.0).sqrt()
    }

    fn disable_pre_eq_bands(eq: &mut PreEqParams) {
        eq.bypass = BoolParam::new("Pre EQ Bypass", true);
        eq.band1_enabled = BoolParam::new("Pre Band 1 Enabled", false);
        eq.band2_enabled = BoolParam::new("Pre Band 2 Enabled", false);
        eq.band3_enabled = BoolParam::new("Pre Band 3 Enabled", false);
        eq.band4_enabled = BoolParam::new("Pre Band 4 Enabled", false);
        eq.band5_enabled = BoolParam::new("Pre Band 5 Enabled", false);
    }

    fn disable_post_eq_bands(eq: &mut EqParams) {
        eq.bypass = BoolParam::new("Post EQ Bypass", true);
        eq.band1_enabled = BoolParam::new("Post Band 1 Enabled", false);
        eq.band2_enabled = BoolParam::new("Post Band 2 Enabled", false);
        eq.band3_enabled = BoolParam::new("Post Band 3 Enabled", false);
        eq.band4_enabled = BoolParam::new("Post Band 4 Enabled", false);
        eq.band5_enabled = BoolParam::new("Post Band 5 Enabled", false);
    }
}
