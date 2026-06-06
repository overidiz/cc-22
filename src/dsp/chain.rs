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
    character: Character,
    movement: Movement,
    diffusion: Diffusion,
    texture: Texture,
    eq: Eq,
}

pub struct ChainFrame {
    character: CharacterFrame,
    movement: MovementFrame,
    diffusion: DiffusionFrame,
    texture: TextureFrame,
    eq: EqFrame,
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
        self.character.prepare(sample_rate);
        self.movement.prepare(sample_rate);
        self.diffusion.prepare(sample_rate);
        self.texture.prepare(sample_rate);
        self.eq.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.character.reset();
        self.movement.reset();
        self.diffusion.reset();
        self.texture.reset();
        self.eq.reset();
    }

    pub fn next_frame(&mut self, params: &Cc22Params) -> ChainFrame {
        ChainFrame {
            character: self.character.next_frame(&params.character),
            movement: self.movement.next_frame(&params.movement),
            diffusion: self.diffusion.next_frame(&params.diffusion),
            texture: self.texture.next_frame(&params.texture),
            eq: self.eq.next_frame(&params.eq),
        }
    }

    pub fn process_sample(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &ChainFrame,
        order: &[ChainModule; 4],
    ) -> f32 {
        let mut sample = sample;
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
        self.eq
            .process_sample_for_channel(channel, sample, &frame.eq)
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
    use super::{
        default_chain_order, reorder_module, safety_limit_sample, sanitize_sample,
        soft_clip_sample, validate_chain_order, ChainModule, ModuleCore,
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
}
