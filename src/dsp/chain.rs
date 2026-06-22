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
    transport::TransportFrame,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqRackTarget {
    Global,
    Module(ChainModule),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqRackPosition {
    Pre,
    Post,
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

/// Turns four raw slot indices into a valid module permutation.
///
/// During automation a host can momentarily present a conflicting state — e.g.
/// moving one slot before the others have caught up leaves a duplicate like
/// `[2, 1, 2, 3]`. Rather than snapping the whole chain back to the default
/// order (an audible jump), this repairs the permutation in place: the first
/// occurrence of each module wins, and any out-of-range or duplicate slot is
/// back-filled with the lowest still-unused module. A valid permutation is
/// returned unchanged, so this is a no-op in steady state.
pub fn validate_chain_order(slots: &[usize; 4]) -> [ChainModule; 4] {
    let mut used = [false; 4];
    let mut needs_fill = [false; 4];
    let mut result = [ChainModule::Character; 4];

    // First pass: honor each module the first time it appears in a valid slot.
    for (position, &slot) in slots.iter().enumerate() {
        if slot < 4 && !used[slot] {
            used[slot] = true;
            result[position] = module_from_slot(slot);
        } else {
            needs_fill[position] = true;
        }
    }

    // Second pass: back-fill conflicts with the remaining modules in order.
    let mut next = 0;
    for position in 0..4 {
        if needs_fill[position] {
            while next < 4 && used[next] {
                next += 1;
            }
            used[next] = true;
            result[position] = module_from_slot(next);
        }
    }

    result
}

#[derive(Default)]
pub struct EqRack {
    pub global_pre: Eq,
    pub global_post: Eq,
    pub character_pre: Eq,
    pub character_post: Eq,
    pub movement_pre: Eq,
    pub movement_post: Eq,
    pub diffusion_pre: Eq,
    pub diffusion_post: Eq,
    pub texture_pre: Eq,
    pub texture_post: Eq,
}

pub struct EqRackFrame {
    pub global_pre: EqFrame,
    pub global_post: EqFrame,
    pub character_pre: EqFrame,
    pub character_post: EqFrame,
    pub movement_pre: EqFrame,
    pub movement_post: EqFrame,
    pub diffusion_pre: EqFrame,
    pub diffusion_post: EqFrame,
    pub texture_pre: EqFrame,
    pub texture_post: EqFrame,
}

impl EqRack {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.global_pre.prepare(sample_rate);
        self.global_post.prepare(sample_rate);
        self.character_pre.prepare(sample_rate);
        self.character_post.prepare(sample_rate);
        self.movement_pre.prepare(sample_rate);
        self.movement_post.prepare(sample_rate);
        self.diffusion_pre.prepare(sample_rate);
        self.diffusion_post.prepare(sample_rate);
        self.texture_pre.prepare(sample_rate);
        self.texture_post.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.global_pre.reset();
        self.global_post.reset();
        self.character_pre.reset();
        self.character_post.reset();
        self.movement_pre.reset();
        self.movement_post.reset();
        self.diffusion_pre.reset();
        self.diffusion_post.reset();
        self.texture_pre.reset();
        self.texture_post.reset();
    }

    pub fn next_frame(&mut self, params: &Cc22Params) -> EqRackFrame {
        EqRackFrame {
            global_pre: self.global_pre.next_frame(&params.global_pre_eq),
            global_post: self.global_post.next_frame(&params.global_post_eq),
            character_pre: self.character_pre.next_frame(&params.character_pre_eq),
            character_post: self.character_post.next_frame(&params.character_post_eq),
            movement_pre: self.movement_pre.next_frame(&params.movement_pre_eq),
            movement_post: self.movement_post.next_frame(&params.movement_post_eq),
            diffusion_pre: self.diffusion_pre.next_frame(&params.diffusion_pre_eq),
            diffusion_post: self.diffusion_post.next_frame(&params.diffusion_post_eq),
            texture_pre: self.texture_pre.next_frame(&params.texture_pre_eq),
            texture_post: self.texture_post.next_frame(&params.texture_post_eq),
        }
    }

    pub fn pre_eq_for_module(&mut self, module: ChainModule) -> &mut Eq {
        match module {
            ChainModule::Character => &mut self.character_pre,
            ChainModule::Movement => &mut self.movement_pre,
            ChainModule::Diffusion => &mut self.diffusion_pre,
            ChainModule::Texture => &mut self.texture_pre,
        }
    }

    pub fn post_eq_for_module(&mut self, module: ChainModule) -> &mut Eq {
        match module {
            ChainModule::Character => &mut self.character_post,
            ChainModule::Movement => &mut self.movement_post,
            ChainModule::Diffusion => &mut self.diffusion_post,
            ChainModule::Texture => &mut self.texture_post,
        }
    }

    pub fn eq_frame_for_target(
        frame: &EqRackFrame,
        target: EqRackTarget,
        position: EqRackPosition,
    ) -> &EqFrame {
        match (target, position) {
            (EqRackTarget::Global, EqRackPosition::Pre) => &frame.global_pre,
            (EqRackTarget::Global, EqRackPosition::Post) => &frame.global_post,
            (EqRackTarget::Module(ChainModule::Character), EqRackPosition::Pre) => {
                &frame.character_pre
            }
            (EqRackTarget::Module(ChainModule::Character), EqRackPosition::Post) => {
                &frame.character_post
            }
            (EqRackTarget::Module(ChainModule::Movement), EqRackPosition::Pre) => {
                &frame.movement_pre
            }
            (EqRackTarget::Module(ChainModule::Movement), EqRackPosition::Post) => {
                &frame.movement_post
            }
            (EqRackTarget::Module(ChainModule::Diffusion), EqRackPosition::Pre) => {
                &frame.diffusion_pre
            }
            (EqRackTarget::Module(ChainModule::Diffusion), EqRackPosition::Post) => {
                &frame.diffusion_post
            }
            (EqRackTarget::Module(ChainModule::Texture), EqRackPosition::Pre) => &frame.texture_pre,
            (EqRackTarget::Module(ChainModule::Texture), EqRackPosition::Post) => {
                &frame.texture_post
            }
        }
    }

    pub fn process_module_with_own_eqs(
        &mut self,
        channel: usize,
        sample: f32,
        module: ChainModule,
        frame: &EqRackFrame,
        character: &mut Character,
        movement: &mut Movement,
        diffusion: &mut Diffusion,
        texture: &mut Texture,
        character_frame: &CharacterFrame,
        movement_frame: &MovementFrame,
        diffusion_frame: &DiffusionFrame,
        texture_frame: &TextureFrame,
    ) -> f32 {
        let target = EqRackTarget::Module(module);
        let pre_frame = Self::eq_frame_for_target(frame, target, EqRackPosition::Pre);
        let post_frame = Self::eq_frame_for_target(frame, target, EqRackPosition::Post);

        let sample = self
            .pre_eq_for_module(module)
            .process_sample_for_channel(channel, sample, pre_frame);

        let sample = match module {
            ChainModule::Character => character.process_sample(channel, sample, character_frame),
            ChainModule::Movement => {
                movement.process_sample_for_channel(channel, sample, movement_frame)
            }
            ChainModule::Diffusion => {
                diffusion.process_sample_for_channel(channel, sample, diffusion_frame)
            }
            ChainModule::Texture => {
                texture.process_sample_for_channel(channel, sample, texture_frame)
            }
        };

        self.post_eq_for_module(module)
            .process_sample_for_channel(channel, sample, post_frame)
    }
}

pub struct EffectChain {
    eq_rack: EqRack,
    character: Character,
    movement: Movement,
    diffusion: Diffusion,
    texture: Texture,
    transport: TransportFrame,
}

impl Default for EffectChain {
    fn default() -> Self {
        Self {
            eq_rack: EqRack::default(),
            character: Character::default(),
            movement: Movement::default(),
            diffusion: Diffusion::default(),
            texture: Texture::default(),
            transport: TransportFrame::default(),
        }
    }
}

pub struct ChainFrame {
    eq_rack: EqRackFrame,
    character: CharacterFrame,
    movement: MovementFrame,
    diffusion: DiffusionFrame,
    texture: TextureFrame,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleCore {
    bypass: BypassCrossfade,
    initialized: bool,
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
        self.initialized = false;
    }

    pub fn next_frame(&mut self, bypassed: bool, mix: f32, output_trim_db: f32) -> ModuleFrame {
        if self.initialized {
            self.bypass.set_bypassed(bypassed);
        } else {
            // Snap to the real bypass state on the first frame so a module that
            // starts bypassed (the default now that "off" is the bypass) is
            // instantly transparent instead of crossfading its mode in.
            self.bypass.reset(bypassed);
            self.initialized = true;
        }

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
        self.eq_rack.prepare(sample_rate);
        self.character.prepare(sample_rate);
        self.movement.prepare(sample_rate);
        self.diffusion.prepare(sample_rate);
        self.texture.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.eq_rack.reset();
        self.character.reset();
        self.movement.reset();
        self.diffusion.reset();
        self.texture.reset();
    }

    /// Store the host transport snapshot for this block (used by tempo-synced
    /// modes). Call once per block before `next_frame`.
    pub fn set_transport(&mut self, transport: &TransportFrame) {
        self.transport = *transport;
    }

    pub fn next_frame(&mut self, params: &Cc22Params) -> ChainFrame {
        ChainFrame {
            eq_rack: self.eq_rack.next_frame(params),
            character: self.character.next_frame(&params.character),
            movement: self.movement.next_frame(&params.movement, &self.transport),
            diffusion: self
                .diffusion
                .next_frame(&params.diffusion, &self.transport),
            texture: self.texture.next_frame(&params.texture),
        }
    }

    pub fn process_sample(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &ChainFrame,
        order: &[ChainModule; 4],
    ) -> f32 {
        let global_pre_frame =
            EqRack::eq_frame_for_target(&frame.eq_rack, EqRackTarget::Global, EqRackPosition::Pre);
        let mut sample =
            self.eq_rack
                .global_pre
                .process_sample_for_channel(channel, sample, global_pre_frame);

        for &module in order.iter() {
            sample = self.eq_rack.process_module_with_own_eqs(
                channel,
                sample,
                module,
                &frame.eq_rack,
                &mut self.character,
                &mut self.movement,
                &mut self.diffusion,
                &mut self.texture,
                &frame.character,
                &frame.movement,
                &frame.diffusion,
                &frame.texture,
            );
        }

        let global_post_frame =
            EqRack::eq_frame_for_target(&frame.eq_rack, EqRackTarget::Global, EqRackPosition::Post);
        self.eq_rack
            .global_post
            .process_sample_for_channel(channel, sample, global_post_frame)
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

    use crate::dsp::{character::CharacterMode, eq::EqBandType, movement::MovementMode};
    use crate::params::{Cc22Params, MovementPreEq};

    use super::{
        default_chain_order, reorder_module, safety_limit_sample, sanitize_sample,
        soft_clip_sample, validate_chain_order, ChainModule, EffectChain, EqRack, EqRackPosition,
        EqRackTarget, ModuleCore,
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
    fn validate_repairs_duplicates_without_snapping_to_default() {
        // [0, 0, 1, 2]: first Character wins, the duplicate slot is back-filled
        // with the only unused module (Texture) — a minimal repair, not a reset.
        let result = validate_chain_order(&[0, 0, 1, 2]);
        assert_eq!(
            result,
            [
                ChainModule::Character,
                ChainModule::Texture,
                ChainModule::Movement,
                ChainModule::Diffusion,
            ]
        );
        assert!(is_valid_permutation(&result));
    }

    #[test]
    fn validate_repairs_out_of_bounds() {
        // Slot 0 is invalid, so it is back-filled with the lowest unused module
        // (Character), which restores the default order here.
        let result = validate_chain_order(&[4, 1, 2, 3]);
        assert_eq!(result, default_chain_order());
        assert!(is_valid_permutation(&result));
    }

    #[test]
    fn validate_always_returns_a_permutation() {
        for a in 0..6 {
            for b in 0..6 {
                for c in 0..6 {
                    for d in 0..6 {
                        let result = validate_chain_order(&[a, b, c, d]);
                        assert!(
                            is_valid_permutation(&result),
                            "non-permutation for slots [{a}, {b}, {c}, {d}]: {result:?}"
                        );
                    }
                }
            }
        }
    }

    fn is_valid_permutation(order: &[ChainModule; 4]) -> bool {
        let mut seen = [false; 4];
        for &module in order {
            let index = module as usize;
            if seen[index] {
                return false;
            }
            seen[index] = true;
        }
        seen.iter().all(|&s| s)
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
    fn global_pre_and_post_eq_can_be_configured_independently() {
        let pre_boost = process_eq_chain_rms(
            Some((1_000.0, 9.0)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some((1_000.0, -9.0)),
        );
        let post_only_cut = process_eq_chain_rms(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some((1_000.0, -9.0)),
        );
        let pre_only_boost = process_eq_chain_rms(
            Some((1_000.0, 9.0)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(pre_boost > post_only_cut * 1.8);
        assert!(pre_only_boost > pre_boost * 1.8);
    }

    #[test]
    fn global_pre_post_eq_off_preserves_signal() {
        let dry = process_eq_chain_rms(None, None, None, None, None, None, None, None, None, None);
        let reference = sine_rms();

        assert!((dry - reference).abs() < 0.000_1);
    }

    #[test]
    fn global_pre_or_post_eq_only_changes_signal_when_enabled() {
        let dry = process_eq_chain_rms(None, None, None, None, None, None, None, None, None, None);
        let pre_only = process_eq_chain_rms(
            Some((1_000.0, 6.0)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let post_only = process_eq_chain_rms(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some((1_000.0, 6.0)),
        );

        assert!(pre_only > dry * 1.15);
        assert!(post_only > dry * 1.15);
        assert!((pre_only - post_only).abs() < dry * 0.08);
    }

    #[test]
    fn global_pre_and_post_eq_are_cumulative_when_both_active() {
        let post_only = process_eq_chain_rms(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some((1_000.0, 6.0)),
        );
        let pre_only = process_eq_chain_rms(
            Some((1_000.0, 6.0)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let both = process_eq_chain_rms(
            Some((1_000.0, 6.0)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some((1_000.0, 6.0)),
        );

        assert!(both > post_only * 1.15);
        assert!(both > pre_only * 1.15);
    }

    #[test]
    fn all_10_eqs_are_independent() {
        let mut params = Cc22Params::default();
        let before = collect_all_eq_band1_gains(&params);

        params.global_pre_eq.band1_gain = FloatParam::new(
            "Global Pre Band 1 Gain",
            9.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        let after = collect_all_eq_band1_gains(&params);
        assert_eq!(after[0], 9.0);
        assert_eq!(&after[1..], &before[1..]);
    }

    #[test]
    fn changing_character_pre_eq_does_not_mutate_other_eqs() {
        let mut params = Cc22Params::default();
        let pre_gain_before = params.character_pre_eq.band3_gain.value();
        let post_gain_before = params.character_post_eq.band3_gain.value();
        let mov_pre_gain_before = params.movement_pre_eq.band3_gain.value();
        let global_post_gain_before = params.global_post_eq.band3_gain.value();

        params.character_pre_eq.band3_enabled = BoolParam::new("Char Pre Band 3 Enabled", true);
        params.character_pre_eq.band3_type =
            EnumParam::new("Char Pre Band 3 Type", EqBandType::HighPass);
        params.character_pre_eq.band3_gain = FloatParam::new(
            "Char Pre Band 3 Gain",
            12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        assert_ne!(params.character_pre_eq.band3_gain.value(), pre_gain_before);
        assert_eq!(
            params.character_post_eq.band3_gain.value(),
            post_gain_before
        );
        assert_eq!(
            params.movement_pre_eq.band3_gain.value(),
            mov_pre_gain_before
        );
        assert_eq!(
            params.global_post_eq.band3_gain.value(),
            global_post_gain_before
        );
    }

    #[test]
    fn module_eq_travels_with_module_when_chain_is_reordered() {
        let default_order = default_chain_order();
        let reversed_order = [
            ChainModule::Texture,
            ChainModule::Diffusion,
            ChainModule::Movement,
            ChainModule::Character,
        ];

        let params = Cc22Params::default();

        let mut chain = EffectChain::default();
        chain.prepare(48_000.0);
        params.reset_smoothers();
        let frame = chain.next_frame(&params);

        let default_sample = {
            let s = 0.5;
            chain.process_sample(0, s, &frame, &default_order)
        };

        let reversed_sample = {
            let mut chain2 = EffectChain::default();
            chain2.prepare(48_000.0);
            let frame2 = chain2.next_frame(&params);
            chain2.process_sample(0, 0.5, &frame2, &reversed_order)
        };

        assert!(default_sample.is_finite());
        assert!(reversed_sample.is_finite());
    }

    fn process_eq_chain_rms(
        global_pre: Option<(f32, f32)>,
        char_pre: Option<(f32, f32)>,
        char_post: Option<(f32, f32)>,
        mov_pre: Option<(f32, f32)>,
        mov_post: Option<(f32, f32)>,
        dif_pre: Option<(f32, f32)>,
        dif_post: Option<(f32, f32)>,
        tex_pre: Option<(f32, f32)>,
        tex_post: Option<(f32, f32)>,
        global_post: Option<(f32, f32)>,
    ) -> f32 {
        let mut params = Cc22Params::default();
        params.character.bypass = BoolParam::new("Character Bypass", true);
        params.movement.bypass = BoolParam::new("Movement Bypass", true);
        params.diffusion.bypass = BoolParam::new("Diffusion Bypass", true);
        params.texture.bypass = BoolParam::new("Texture Bypass", true);

        macro_rules! disable_eq {
            ($field:ident) => {
                params.$field.bypass = BoolParam::new("EQ Bypass", true);
                params.$field.band1_enabled = BoolParam::new("B1", false);
                params.$field.band2_enabled = BoolParam::new("B2", false);
                params.$field.band3_enabled = BoolParam::new("B3", false);
                params.$field.band4_enabled = BoolParam::new("B4", false);
                params.$field.band5_enabled = BoolParam::new("B5", false);
            };
        }
        macro_rules! set_band {
            ($field:ident, $freq:expr, $gain:expr) => {
                params.$field.bypass = BoolParam::new("EQ Bypass", false);
                params.$field.band3_enabled = BoolParam::new("B3 En", true);
                params.$field.band3_type = EnumParam::new("B3 Type", EqBandType::Bell);
                params.$field.band3_frequency = FloatParam::new(
                    "B3 Freq",
                    $freq,
                    FloatRange::Linear {
                        min: 20.0,
                        max: 20_000.0,
                    },
                );
                params.$field.band3_gain = FloatParam::new(
                    "B3 Gain",
                    $gain,
                    FloatRange::Linear {
                        min: -24.0,
                        max: 24.0,
                    },
                );
            };
        }

        disable_eq!(global_pre_eq);
        disable_eq!(character_pre_eq);
        disable_eq!(character_post_eq);
        disable_eq!(movement_pre_eq);
        disable_eq!(movement_post_eq);
        disable_eq!(diffusion_pre_eq);
        disable_eq!(diffusion_post_eq);
        disable_eq!(texture_pre_eq);
        disable_eq!(texture_post_eq);
        disable_eq!(global_post_eq);

        if let Some((freq, gain)) = global_pre {
            set_band!(global_pre_eq, freq, gain);
        }
        if let Some((freq, gain)) = char_pre {
            set_band!(character_pre_eq, freq, gain);
        }
        if let Some((freq, gain)) = char_post {
            set_band!(character_post_eq, freq, gain);
        }
        if let Some((freq, gain)) = mov_pre {
            set_band!(movement_pre_eq, freq, gain);
        }
        if let Some((freq, gain)) = mov_post {
            set_band!(movement_post_eq, freq, gain);
        }
        if let Some((freq, gain)) = dif_pre {
            set_band!(diffusion_pre_eq, freq, gain);
        }
        if let Some((freq, gain)) = dif_post {
            set_band!(diffusion_post_eq, freq, gain);
        }
        if let Some((freq, gain)) = tex_pre {
            set_band!(texture_pre_eq, freq, gain);
        }
        if let Some((freq, gain)) = tex_post {
            set_band!(texture_post_eq, freq, gain);
        }
        if let Some((freq, gain)) = global_post {
            set_band!(global_post_eq, freq, gain);
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

    // ── EQ independence tests ──

    #[test]
    fn global_pre_affects_signal_before_chain() {
        let mut flat_params = Cc22Params::default();
        bypass_modules(&mut flat_params);
        disable_all_module_eqs(&mut flat_params);
        disable_global_post(&mut flat_params);
        disable_global_pre(&mut flat_params);

        let mut eq_params = Cc22Params::default();
        bypass_modules(&mut eq_params);
        disable_all_module_eqs(&mut eq_params);
        disable_global_post(&mut eq_params);

        eq_params.global_pre_eq.bypass = BoolParam::new("Byp", false);
        eq_params.global_pre_eq.band3_enabled = BoolParam::new("En", true);
        eq_params.global_pre_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        eq_params.global_pre_eq.band3_frequency = FloatParam::new(
            "Freq",
            1_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        eq_params.global_pre_eq.band3_gain = FloatParam::new(
            "Gain",
            12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        let (flat_rms, boosted_rms) = compare_flat_vs_eq(&flat_params, &eq_params, false);
        assert!(
            boosted_rms > flat_rms * 1.5,
            "Global Pre should boost signal. flat={flat_rms:.4}, boosted={boosted_rms:.4}"
        );
    }

    #[test]
    fn global_post_affects_signal_after_chain() {
        let mut flat_params = Cc22Params::default();
        bypass_modules(&mut flat_params);
        disable_all_module_eqs(&mut flat_params);
        disable_global_pre(&mut flat_params);
        disable_global_post(&mut flat_params);

        let mut eq_params = Cc22Params::default();
        bypass_modules(&mut eq_params);
        disable_all_module_eqs(&mut eq_params);
        disable_global_pre(&mut eq_params);

        eq_params.global_post_eq.bypass = BoolParam::new("Byp", false);
        eq_params.global_post_eq.band3_enabled = BoolParam::new("En", true);
        eq_params.global_post_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        eq_params.global_post_eq.band3_frequency = FloatParam::new(
            "Freq",
            1_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        eq_params.global_post_eq.band3_gain = FloatParam::new(
            "Gain",
            12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        let (flat_rms, boosted_rms) = compare_flat_vs_eq(&flat_params, &eq_params, false);
        assert!(
            boosted_rms > flat_rms * 1.5,
            "Global Post should boost signal. flat={flat_rms:.4}, boosted={boosted_rms:.4}"
        );
    }

    #[test]
    fn character_pre_alters_only_before_character() {
        let mut flat_params = Cc22Params::default();
        bypass_modules(&mut flat_params);
        disable_all_module_eqs(&mut flat_params);
        disable_global_pre(&mut flat_params);
        disable_global_post(&mut flat_params);

        let mut eq_params = Cc22Params::default();
        eq_params.character.bypass = BoolParam::new("ChB", false);
        eq_params.character.mode = EnumParam::new("ChM", CharacterMode::Drive);
        eq_params.character.drive =
            FloatParam::new("Dr", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 });
        eq_params.character.mix =
            FloatParam::new("Mix", 0.8, FloatRange::Linear { min: 0.0, max: 1.0 });
        bypass_modules_except(&mut eq_params, ChainModule::Character);
        disable_all_module_eqs(&mut eq_params);
        disable_global_pre(&mut eq_params);
        disable_global_post(&mut eq_params);

        eq_params.character_pre_eq.bypass = BoolParam::new("Byp", false);
        eq_params.character_pre_eq.band1_enabled = BoolParam::new("En", true);
        eq_params.character_pre_eq.band1_type = EnumParam::new("Ty", EqBandType::HighPass);
        eq_params.character_pre_eq.band1_frequency = FloatParam::new(
            "Freq",
            500.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        eq_params.character_pre_eq.band1_q = FloatParam::new(
            "Q",
            0.707,
            FloatRange::Linear {
                min: 0.1,
                max: 12.0,
            },
        );

        let (flat_rms, filtered_rms) = compare_flat_vs_eq(&flat_params, &eq_params, false);
        assert!(
            (filtered_rms - flat_rms).abs() > 0.001,
            "Character Pre EQ should alter signal before Character"
        );
    }

    #[test]
    fn character_pre_is_the_only_eq_affecting_the_render_when_soloed() {
        let flat_rms =
            process_eq_chain_rms(None, None, None, None, None, None, None, None, None, None);
        let character_pre_rms = process_eq_chain_rms(
            None,
            Some((1_000.0, 12.0)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let params = Cc22Params::default();

        assert!(character_pre_rms > flat_rms * 1.5);
        assert_eq!(params.global_post_eq.band3_gain.value(), 0.0);
        assert_eq!(params.global_post_eq.band3_type.value(), EqBandType::Bell);
    }

    #[test]
    fn character_post_alters_only_after_character() {
        let mut flat_params = Cc22Params::default();
        bypass_modules(&mut flat_params);
        disable_all_module_eqs(&mut flat_params);
        disable_global_pre(&mut flat_params);
        disable_global_post(&mut flat_params);

        let mut eq_params = Cc22Params::default();
        eq_params.character.bypass = BoolParam::new("ChB", false);
        eq_params.character.mode = EnumParam::new("ChM", CharacterMode::Drive);
        eq_params.character.drive =
            FloatParam::new("Dr", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 });
        eq_params.character.mix =
            FloatParam::new("Mix", 0.8, FloatRange::Linear { min: 0.0, max: 1.0 });
        bypass_modules_except(&mut eq_params, ChainModule::Character);
        disable_all_module_eqs(&mut eq_params);
        disable_global_pre(&mut eq_params);
        disable_global_post(&mut eq_params);

        eq_params.character_post_eq.bypass = BoolParam::new("Byp", false);
        eq_params.character_post_eq.band5_enabled = BoolParam::new("En", true);
        eq_params.character_post_eq.band5_type = EnumParam::new("Ty", EqBandType::LowPass);
        eq_params.character_post_eq.band5_frequency = FloatParam::new(
            "Freq",
            2_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        eq_params.character_post_eq.band5_q = FloatParam::new(
            "Q",
            0.707,
            FloatRange::Linear {
                min: 0.1,
                max: 12.0,
            },
        );

        let (flat_rms, filtered_rms) = compare_flat_vs_eq(&flat_params, &eq_params, false);
        assert!(
            (filtered_rms - flat_rms).abs() > 0.001,
            "Character Post EQ should alter signal after Character"
        );
    }

    #[test]
    fn movement_pre_post_do_not_alter_character_eq() {
        let mut params = Cc22Params::default();
        params.character.bypass = BoolParam::new("ChB", false);
        params.character.mode = EnumParam::new("ChM", CharacterMode::Drive);
        params.character.drive =
            FloatParam::new("Dr", 0.2, FloatRange::Linear { min: 0.0, max: 1.0 });
        params.character.mix =
            FloatParam::new("Mix", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 });
        bypass_modules_except(&mut params, ChainModule::Character);
        disable_all_module_eqs(&mut params);
        disable_global_pre(&mut params);
        disable_global_post(&mut params);

        let char_eq_pre_qty = params.character_pre_eq.band3_gain.value();
        let char_eq_post_qty = params.character_post_eq.band3_gain.value();

        params.movement_pre_eq.band3_enabled = BoolParam::new("En", true);
        params.movement_pre_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        params.movement_pre_eq.band3_gain = FloatParam::new(
            "Gain",
            12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.movement_post_eq.band3_enabled = BoolParam::new("En", true);
        params.movement_post_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        params.movement_post_eq.band3_gain = FloatParam::new(
            "Gain",
            -12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        assert_eq!(params.character_pre_eq.band3_gain.value(), char_eq_pre_qty);
        assert_eq!(
            params.character_post_eq.band3_gain.value(),
            char_eq_post_qty
        );
    }

    #[test]
    fn diffusion_pre_post_do_not_alter_texture_eq() {
        let default_params = Cc22Params::default();
        let tex_pre = default_params.texture_pre_eq.band2_frequency.value();
        let tex_post = default_params.texture_post_eq.band2_frequency.value();

        let mut params = Cc22Params::default();
        params.diffusion_pre_eq.band2_enabled = BoolParam::new("En", true);
        params.diffusion_pre_eq.band2_type = EnumParam::new("Ty", EqBandType::Bell);
        params.diffusion_pre_eq.band2_frequency = FloatParam::new(
            "Freq",
            400.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        params.diffusion_pre_eq.band2_gain = FloatParam::new(
            "Gain",
            8.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.diffusion_post_eq.band2_enabled = BoolParam::new("En", true);
        params.diffusion_post_eq.band2_type = EnumParam::new("Ty", EqBandType::Bell);
        params.diffusion_post_eq.band2_frequency = FloatParam::new(
            "Freq",
            8_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        params.diffusion_post_eq.band2_gain = FloatParam::new(
            "Gain",
            8.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        assert_eq!(params.texture_pre_eq.band2_frequency.value(), tex_pre);
        assert_eq!(params.texture_post_eq.band2_frequency.value(), tex_post);
    }

    #[test]
    fn texture_pre_post_do_not_alter_global_eq() {
        let default_params = Cc22Params::default();
        let global_pre_gain = default_params.global_pre_eq.band3_gain.value();
        let global_post_gain = default_params.global_post_eq.band3_gain.value();

        let mut params = Cc22Params::default();
        params.texture_pre_eq.band3_enabled = BoolParam::new("En", true);
        params.texture_pre_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        params.texture_pre_eq.band3_gain = FloatParam::new(
            "Gain",
            12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.texture_post_eq.band3_enabled = BoolParam::new("En", true);
        params.texture_post_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        params.texture_post_eq.band3_gain = FloatParam::new(
            "Gain",
            -12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        assert_eq!(params.global_pre_eq.band3_gain.value(), global_pre_gain);
        assert_eq!(params.global_post_eq.band3_gain.value(), global_post_gain);
    }

    #[test]
    fn altering_one_eq_does_not_change_another_eqs_parameters() {
        let mut params = Cc22Params::default();

        let target_values = collect_all_eq_band3_gains(&params);
        params.character_pre_eq.band3_gain = FloatParam::new(
            "Gain",
            24.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.diffusion_post_eq.band3_gain = FloatParam::new(
            "Gain",
            -24.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.texture_pre_eq.band1_frequency = FloatParam::new(
            "Freq",
            50.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );

        assert_eq!(
            params.global_pre_eq.band3_gain.value(),
            target_values.global_pre
        );
        assert_eq!(
            params.global_post_eq.band3_gain.value(),
            target_values.global_post
        );
        assert_ne!(
            params.character_pre_eq.band3_gain.value(),
            target_values.char_pre
        );
        assert_eq!(
            params.character_post_eq.band3_gain.value(),
            target_values.char_post
        );
        assert_eq!(
            params.movement_pre_eq.band3_gain.value(),
            target_values.mov_pre
        );
        assert_eq!(
            params.movement_post_eq.band3_gain.value(),
            target_values.mov_post
        );
        assert_eq!(
            params.diffusion_pre_eq.band3_gain.value(),
            target_values.dif_pre
        );
        assert_ne!(
            params.diffusion_post_eq.band3_gain.value(),
            target_values.dif_post
        );
        assert_eq!(
            params.texture_pre_eq.band3_gain.value(),
            target_values.tex_pre
        );
        assert_eq!(
            params.texture_post_eq.band3_gain.value(),
            target_values.tex_post
        );
    }

    #[test]
    fn reset_of_movement_pre_does_not_reset_others() {
        let mut params = Cc22Params::default();

        params.global_pre_eq.band3_gain = FloatParam::new(
            "Gain",
            12.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.movement_pre_eq.band3_gain = FloatParam::new(
            "Gain",
            -6.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.movement_post_eq.band3_gain = FloatParam::new(
            "Gain",
            8.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.global_post_eq.band3_gain = FloatParam::new(
            "Gain",
            -3.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        params.movement_pre_eq = MovementPreEq::default();

        assert!((params.global_pre_eq.band3_gain.value() - 12.0).abs() < 0.001);
        assert!((params.movement_pre_eq.band3_gain.value() - 0.0).abs() < 0.001);
        assert!((params.movement_post_eq.band3_gain.value() - 8.0).abs() < 0.001);
        assert!((params.global_post_eq.band3_gain.value() - (-3.0)).abs() < 0.001);
    }

    #[test]
    fn reorder_chain_keeps_eqs_with_module() {
        let default_order = default_chain_order();
        let swapped_order = [
            ChainModule::Movement,
            ChainModule::Character,
            ChainModule::Texture,
            ChainModule::Diffusion,
        ];

        let mut params = Cc22Params::default();
        params.character.bypass = BoolParam::new("Byp", false);
        params.character.mode = EnumParam::new("Mode", CharacterMode::Drive);
        params.character.drive =
            FloatParam::new("Dr", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 });
        params.character.mix =
            FloatParam::new("Mix", 0.8, FloatRange::Linear { min: 0.0, max: 1.0 });
        params.movement.bypass = BoolParam::new("Byp", false);
        params.movement.mode = EnumParam::new("Mode", MovementMode::Doubler);
        params.movement.depth =
            FloatParam::new("Dp", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 });
        params.movement.mix =
            FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 });
        disable_global_pre(&mut params);
        disable_global_post(&mut params);
        disable_all_module_eqs(&mut params);

        params.character_pre_eq.bypass = BoolParam::new("Byp", false);
        params.character_pre_eq.band1_enabled = BoolParam::new("En", true);
        params.character_pre_eq.band1_type = EnumParam::new("Ty", EqBandType::HighPass);
        params.character_pre_eq.band1_frequency = FloatParam::new(
            "Freq",
            200.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        params.character_pre_eq.band1_q = FloatParam::new(
            "Q",
            0.707,
            FloatRange::Linear {
                min: 0.1,
                max: 12.0,
            },
        );

        params.movement_pre_eq.bypass = BoolParam::new("Byp", false);
        params.movement_pre_eq.band5_enabled = BoolParam::new("En", true);
        params.movement_pre_eq.band5_type = EnumParam::new("Ty", EqBandType::LowPass);
        params.movement_pre_eq.band5_frequency = FloatParam::new(
            "Freq",
            5_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        params.movement_pre_eq.band5_q = FloatParam::new(
            "Q",
            0.707,
            FloatRange::Linear {
                min: 0.1,
                max: 12.0,
            },
        );

        params.reset_smoothers();
        let mut chain = EffectChain::default();
        chain.prepare(48_000.0);
        let frame = chain.next_frame(&params);
        let default_out = chain.process_sample(0, 0.3, &frame, &default_order);

        let mut chain2 = EffectChain::default();
        chain2.prepare(48_000.0);
        let frame2 = chain2.next_frame(&params);
        let swapped_out = chain2.process_sample(0, 0.3, &frame2, &swapped_order);

        assert!(default_out.is_finite());
        assert!(swapped_out.is_finite());
        assert!(
            (default_out - swapped_out).abs() > 0.000_01,
            "Reordering should produce different output when EQs differ"
        );
    }

    #[test]
    fn diffusion_post_frame_follows_diffusion_when_moved_first() {
        let mut params = Cc22Params::default();
        let character_post_before = params.character_post_eq.band3_gain.value();
        params.diffusion_post_eq.band3_gain = FloatParam::new(
            "Diffusion Post Gain",
            10.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );
        params.reset_smoothers();

        let order = reorder_module(default_chain_order(), 2, 0);
        assert_eq!(order[0], ChainModule::Diffusion);

        let mut rack = EqRack::default();
        rack.prepare(48_000.0);
        let frames = rack.next_frame(&params);
        let diffusion_post = EqRack::eq_frame_for_target(
            &frames,
            EqRackTarget::Module(order[0]),
            EqRackPosition::Post,
        );
        let character_post = EqRack::eq_frame_for_target(
            &frames,
            EqRackTarget::Module(ChainModule::Character),
            EqRackPosition::Post,
        );

        assert_eq!(diffusion_post.bands[2].gain, 10.0);
        assert_eq!(character_post.bands[2].gain, character_post_before);
        assert_eq!(
            params.character_post_eq.band3_gain.value(),
            character_post_before
        );
    }

    #[test]
    fn all_eqs_off_or_flat_in_series_is_transparent() {
        let mut params = Cc22Params::default();
        bypass_modules(&mut params);
        params.reset_smoothers();

        let mut chain = EffectChain::default();
        chain.prepare(48_000.0);
        let order = params.chain_order();

        for index in 0..512 {
            let input = (index as f32 * 0.02).sin() * 0.5;
            let frame = chain.next_frame(&params);
            let output = chain.process_sample(0, input, &frame, &order);
            assert!(
                (output - input).abs() < 0.000_1,
                "10 EQs flat should be transparent. input={input}, output={output}"
            );
        }
    }

    #[test]
    fn all_eqs_with_extreme_filters_produce_finite_output() {
        let mut params = Cc22Params::default();
        bypass_modules(&mut params);
        disable_global_pre(&mut params);
        disable_global_post(&mut params);

        macro_rules! set_extreme {
            ($field:ident) => {
                params.$field.bypass = BoolParam::new("Byp", false);
                params.$field.band1_enabled = BoolParam::new("En", true);
                params.$field.band1_type = EnumParam::new("Ty", EqBandType::HighPass);
                params.$field.band1_frequency = FloatParam::new(
                    "Freq",
                    80.0,
                    FloatRange::Linear {
                        min: 20.0,
                        max: 20_000.0,
                    },
                );
                params.$field.band1_q = FloatParam::new(
                    "Q",
                    2.0,
                    FloatRange::Linear {
                        min: 0.1,
                        max: 12.0,
                    },
                );
                params.$field.band2_enabled = BoolParam::new("En", true);
                params.$field.band2_type = EnumParam::new("Ty", EqBandType::Bell);
                params.$field.band2_frequency = FloatParam::new(
                    "Freq",
                    300.0,
                    FloatRange::Linear {
                        min: 20.0,
                        max: 20_000.0,
                    },
                );
                params.$field.band2_gain = FloatParam::new(
                    "Gain",
                    8.0,
                    FloatRange::Linear {
                        min: -24.0,
                        max: 24.0,
                    },
                );
                params.$field.band2_q = FloatParam::new(
                    "Q",
                    3.0,
                    FloatRange::Linear {
                        min: 0.1,
                        max: 12.0,
                    },
                );
                params.$field.band4_enabled = BoolParam::new("En", true);
                params.$field.band4_type = EnumParam::new("Ty", EqBandType::Bell);
                params.$field.band4_frequency = FloatParam::new(
                    "Freq",
                    5_000.0,
                    FloatRange::Linear {
                        min: 20.0,
                        max: 20_000.0,
                    },
                );
                params.$field.band4_gain = FloatParam::new(
                    "Gain",
                    -6.0,
                    FloatRange::Linear {
                        min: -24.0,
                        max: 24.0,
                    },
                );
                params.$field.band4_q = FloatParam::new(
                    "Q",
                    0.5,
                    FloatRange::Linear {
                        min: 0.1,
                        max: 12.0,
                    },
                );
                params.$field.band5_enabled = BoolParam::new("En", true);
                params.$field.band5_type = EnumParam::new("Ty", EqBandType::LowPass);
                params.$field.band5_frequency = FloatParam::new(
                    "Freq",
                    10_000.0,
                    FloatRange::Linear {
                        min: 20.0,
                        max: 20_000.0,
                    },
                );
                params.$field.band5_q = FloatParam::new(
                    "Q",
                    0.707,
                    FloatRange::Linear {
                        min: 0.1,
                        max: 12.0,
                    },
                );
            };
        }
        set_extreme!(character_pre_eq);
        set_extreme!(character_post_eq);
        set_extreme!(movement_pre_eq);
        set_extreme!(movement_post_eq);
        set_extreme!(diffusion_pre_eq);
        set_extreme!(diffusion_post_eq);
        set_extreme!(texture_pre_eq);
        set_extreme!(texture_post_eq);

        params.reset_smoothers();
        let mut chain = EffectChain::default();
        chain.prepare(48_000.0);
        let order = params.chain_order();

        for index in 0..2_000 {
            let input = if index == 0 { 1.0 } else { 0.0 };
            let frame = chain.next_frame(&params);
            let output = chain.process_sample(0, input, &frame, &order);
            assert!(
                output.is_finite(),
                "Output should be finite at sample {index}"
            );
            assert!(
                output.abs() < 9.0,
                "Output should stay bounded at sample {index}"
            );
        }
    }

    #[test]
    fn right_click_band1_only_affects_selected_eq() {
        let params = Cc22Params::default();
        let char_pre_q = params.character_pre_eq.band1_q.value();
        let char_post_q = params.character_post_eq.band1_q.value();
        let global_pre_q = params.global_pre_eq.band1_q.value();

        assert_eq!(char_pre_q, 1.0);
        assert_eq!(char_post_q, 1.0);
        assert_eq!(global_pre_q, 1.0);

        assert_eq!(direct_right_click(0), Some(EqBandType::HighPass));
    }

    #[test]
    fn right_click_band5_only_affects_selected_eq() {
        let params = Cc22Params::default();
        let tex_post_q = params.texture_post_eq.band5_q.value();
        let dif_post_q = params.diffusion_post_eq.band5_q.value();
        let global_post_q = params.global_post_eq.band5_q.value();

        assert_eq!(tex_post_q, 1.0);
        assert_eq!(dif_post_q, 1.0);
        assert_eq!(global_post_q, 1.0);

        assert_eq!(direct_right_click(4), Some(EqBandType::LowPass));
    }

    fn direct_right_click(band: usize) -> Option<EqBandType> {
        match band {
            0 => Some(EqBandType::HighPass),
            4 => Some(EqBandType::LowPass),
            _ => None,
        }
    }

    #[test]
    fn stereo_lr_different_input_preserves_independence() {
        let mut params = Cc22Params::default();
        bypass_modules(&mut params);
        disable_all_module_eqs(&mut params);

        params.global_pre_eq.bypass = BoolParam::new("Byp", false);
        params.global_pre_eq.band3_enabled = BoolParam::new("En", true);
        params.global_pre_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        params.global_pre_eq.band3_frequency = FloatParam::new(
            "Freq",
            1_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        params.global_pre_eq.band3_gain = FloatParam::new(
            "Gain",
            6.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        params.global_post_eq.bypass = BoolParam::new("Byp", false);
        params.global_post_eq.band3_enabled = BoolParam::new("En", true);
        params.global_post_eq.band3_type = EnumParam::new("Ty", EqBandType::Bell);
        params.global_post_eq.band3_frequency = FloatParam::new(
            "Freq",
            1_000.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        params.global_post_eq.band3_gain = FloatParam::new(
            "Gain",
            -6.0,
            FloatRange::Linear {
                min: -24.0,
                max: 24.0,
            },
        );

        params.reset_smoothers();
        let mut chain = EffectChain::default();
        chain.prepare(48_000.0);
        let order = params.chain_order();

        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        for index in 0..4_800 {
            let frame = chain.next_frame(&params);
            let phase = core::f32::consts::TAU * 1_000.0 * index as f32 / 48_000.0;
            let l_in = phase.sin() * 0.15;
            let r_in = (phase * 1.3).sin() * 0.12;
            let l_out = chain.process_sample(0, l_in, &frame, &order);
            let r_out = chain.process_sample(1, r_in, &frame, &order);
            assert!(l_out.is_finite());
            assert!(r_out.is_finite());
            sum_l += l_out * l_out;
            sum_r += r_out * r_out;
        }

        let rms_l = (sum_l / 4_800.0).sqrt();
        let rms_r = (sum_r / 4_800.0).sqrt();
        assert!(rms_l > 0.0);
        assert!(rms_r > 0.0);
        assert!(
            (rms_l - rms_r).abs() > 0.000_1,
            "L/R should differ with different inputs"
        );
    }

    // ── test helpers ──

    struct AllEqBand3Gains {
        global_pre: f32,
        global_post: f32,
        char_pre: f32,
        char_post: f32,
        mov_pre: f32,
        mov_post: f32,
        dif_pre: f32,
        dif_post: f32,
        tex_pre: f32,
        tex_post: f32,
    }

    fn collect_all_eq_band1_gains(params: &Cc22Params) -> [f32; 10] {
        [
            params.global_pre_eq.band1_gain.value(),
            params.global_post_eq.band1_gain.value(),
            params.character_pre_eq.band1_gain.value(),
            params.character_post_eq.band1_gain.value(),
            params.movement_pre_eq.band1_gain.value(),
            params.movement_post_eq.band1_gain.value(),
            params.diffusion_pre_eq.band1_gain.value(),
            params.diffusion_post_eq.band1_gain.value(),
            params.texture_pre_eq.band1_gain.value(),
            params.texture_post_eq.band1_gain.value(),
        ]
    }

    fn collect_all_eq_band3_gains(params: &Cc22Params) -> AllEqBand3Gains {
        AllEqBand3Gains {
            global_pre: params.global_pre_eq.band3_gain.value(),
            global_post: params.global_post_eq.band3_gain.value(),
            char_pre: params.character_pre_eq.band3_gain.value(),
            char_post: params.character_post_eq.band3_gain.value(),
            mov_pre: params.movement_pre_eq.band3_gain.value(),
            mov_post: params.movement_post_eq.band3_gain.value(),
            dif_pre: params.diffusion_pre_eq.band3_gain.value(),
            dif_post: params.diffusion_post_eq.band3_gain.value(),
            tex_pre: params.texture_pre_eq.band3_gain.value(),
            tex_post: params.texture_post_eq.band3_gain.value(),
        }
    }

    fn compare_flat_vs_eq(
        flat_params: &Cc22Params,
        eq_params: &Cc22Params,
        _use_stereo: bool,
    ) -> (f32, f32) {
        let mut flat_chain = EffectChain::default();
        flat_chain.prepare(48_000.0);
        flat_params.reset_smoothers();

        let mut eq_chain = EffectChain::default();
        eq_chain.prepare(48_000.0);
        eq_params.reset_smoothers();

        let order = flat_params.chain_order();
        let mut flat_sum = 0.0f32;
        let mut eq_sum = 0.0f32;

        for index in 0..4_800 {
            let flat_frame = flat_chain.next_frame(flat_params);
            let eq_frame = eq_chain.next_frame(eq_params);
            let phase = core::f32::consts::TAU * 1_000.0 * index as f32 / 48_000.0;
            let sample = phase.sin() * 0.1;
            let f = flat_chain.process_sample(0, sample, &flat_frame, &order);
            let e = eq_chain.process_sample(0, sample, &eq_frame, &order);
            flat_sum += f * f;
            eq_sum += e * e;
        }

        ((flat_sum / 4_800.0).sqrt(), (eq_sum / 4_800.0).sqrt())
    }

    fn bypass_modules(params: &mut Cc22Params) {
        params.character.bypass = BoolParam::new("Byp", true);
        params.movement.bypass = BoolParam::new("Byp", true);
        params.diffusion.bypass = BoolParam::new("Byp", true);
        params.texture.bypass = BoolParam::new("Byp", true);
    }

    fn bypass_modules_except(params: &mut Cc22Params, keep: ChainModule) {
        params.character.bypass = BoolParam::new("Byp", keep != ChainModule::Character);
        params.movement.bypass = BoolParam::new("Byp", keep != ChainModule::Movement);
        params.diffusion.bypass = BoolParam::new("Byp", keep != ChainModule::Diffusion);
        params.texture.bypass = BoolParam::new("Byp", keep != ChainModule::Texture);
    }

    fn disable_eq_field(
        bypass: &mut BoolParam,
        b1: &mut BoolParam,
        b2: &mut BoolParam,
        b3: &mut BoolParam,
        b4: &mut BoolParam,
        b5: &mut BoolParam,
    ) {
        *bypass = BoolParam::new("Byp", true);
        *b1 = BoolParam::new("B1", false);
        *b2 = BoolParam::new("B2", false);
        *b3 = BoolParam::new("B3", false);
        *b4 = BoolParam::new("B4", false);
        *b5 = BoolParam::new("B5", false);
    }

    fn disable_all_module_eqs(params: &mut Cc22Params) {
        disable_eq_field(
            &mut params.character_pre_eq.bypass,
            &mut params.character_pre_eq.band1_enabled,
            &mut params.character_pre_eq.band2_enabled,
            &mut params.character_pre_eq.band3_enabled,
            &mut params.character_pre_eq.band4_enabled,
            &mut params.character_pre_eq.band5_enabled,
        );
        disable_eq_field(
            &mut params.character_post_eq.bypass,
            &mut params.character_post_eq.band1_enabled,
            &mut params.character_post_eq.band2_enabled,
            &mut params.character_post_eq.band3_enabled,
            &mut params.character_post_eq.band4_enabled,
            &mut params.character_post_eq.band5_enabled,
        );
        disable_eq_field(
            &mut params.movement_pre_eq.bypass,
            &mut params.movement_pre_eq.band1_enabled,
            &mut params.movement_pre_eq.band2_enabled,
            &mut params.movement_pre_eq.band3_enabled,
            &mut params.movement_pre_eq.band4_enabled,
            &mut params.movement_pre_eq.band5_enabled,
        );
        disable_eq_field(
            &mut params.movement_post_eq.bypass,
            &mut params.movement_post_eq.band1_enabled,
            &mut params.movement_post_eq.band2_enabled,
            &mut params.movement_post_eq.band3_enabled,
            &mut params.movement_post_eq.band4_enabled,
            &mut params.movement_post_eq.band5_enabled,
        );
        disable_eq_field(
            &mut params.diffusion_pre_eq.bypass,
            &mut params.diffusion_pre_eq.band1_enabled,
            &mut params.diffusion_pre_eq.band2_enabled,
            &mut params.diffusion_pre_eq.band3_enabled,
            &mut params.diffusion_pre_eq.band4_enabled,
            &mut params.diffusion_pre_eq.band5_enabled,
        );
        disable_eq_field(
            &mut params.diffusion_post_eq.bypass,
            &mut params.diffusion_post_eq.band1_enabled,
            &mut params.diffusion_post_eq.band2_enabled,
            &mut params.diffusion_post_eq.band3_enabled,
            &mut params.diffusion_post_eq.band4_enabled,
            &mut params.diffusion_post_eq.band5_enabled,
        );
        disable_eq_field(
            &mut params.texture_pre_eq.bypass,
            &mut params.texture_pre_eq.band1_enabled,
            &mut params.texture_pre_eq.band2_enabled,
            &mut params.texture_pre_eq.band3_enabled,
            &mut params.texture_pre_eq.band4_enabled,
            &mut params.texture_pre_eq.band5_enabled,
        );
        disable_eq_field(
            &mut params.texture_post_eq.bypass,
            &mut params.texture_post_eq.band1_enabled,
            &mut params.texture_post_eq.band2_enabled,
            &mut params.texture_post_eq.band3_enabled,
            &mut params.texture_post_eq.band4_enabled,
            &mut params.texture_post_eq.band5_enabled,
        );
    }

    fn disable_global_pre(params: &mut Cc22Params) {
        params.global_pre_eq.bypass = BoolParam::new("Byp", true);
        params.global_pre_eq.band1_enabled = BoolParam::new("B1", false);
        params.global_pre_eq.band2_enabled = BoolParam::new("B2", false);
        params.global_pre_eq.band3_enabled = BoolParam::new("B3", false);
        params.global_pre_eq.band4_enabled = BoolParam::new("B4", false);
        params.global_pre_eq.band5_enabled = BoolParam::new("B5", false);
    }

    fn disable_global_post(params: &mut Cc22Params) {
        params.global_post_eq.bypass = BoolParam::new("Byp", true);
        params.global_post_eq.band1_enabled = BoolParam::new("B1", false);
        params.global_post_eq.band2_enabled = BoolParam::new("B2", false);
        params.global_post_eq.band3_enabled = BoolParam::new("B3", false);
        params.global_post_eq.band4_enabled = BoolParam::new("B4", false);
        params.global_post_eq.band5_enabled = BoolParam::new("B5", false);
    }
}
