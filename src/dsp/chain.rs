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

/// The plugin's only two EQs: one before the module chain, one after. There are
/// no per-module EQs — both run on every sample; the UI just chooses which to edit.
#[derive(Default)]
pub struct EqSection {
    pub pre: Eq,
    pub post: Eq,
}

pub struct EqSectionFrame {
    pub pre: EqFrame,
    pub post: EqFrame,
}

impl EqSection {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.pre.prepare(sample_rate);
        self.post.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.pre.reset();
        self.post.reset();
    }

    pub fn next_frame(&mut self, params: &Cc22Params) -> EqSectionFrame {
        EqSectionFrame {
            pre: self.pre.next_frame(&params.pre_eq),
            post: self.post.next_frame(&params.post_eq),
        }
    }
}

pub struct EffectChain {
    eq_section: EqSection,
    character: Character,
    movement: Movement,
    diffusion: Diffusion,
    texture: Texture,
    transport: TransportFrame,
}

impl Default for EffectChain {
    fn default() -> Self {
        Self {
            eq_section: EqSection::default(),
            character: Character::default(),
            movement: Movement::default(),
            diffusion: Diffusion::default(),
            texture: Texture::default(),
            transport: TransportFrame::default(),
        }
    }
}

pub struct ChainFrame {
    eq_section: EqSectionFrame,
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
        self.eq_section.prepare(sample_rate);
        self.character.prepare(sample_rate);
        self.movement.prepare(sample_rate);
        self.diffusion.prepare(sample_rate);
        self.texture.prepare(sample_rate);
    }

    pub fn reset(&mut self) {
        self.eq_section.reset();
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
            eq_section: self.eq_section.next_frame(params),
            character: self.character.next_frame(&params.character),
            movement: self.movement.next_frame(&params.movement, &self.transport),
            diffusion: self
                .diffusion
                .next_frame(&params.diffusion, &self.transport),
            texture: self.texture.next_frame(&params.texture, &self.transport),
        }
    }

    pub fn process_sample(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &ChainFrame,
        order: &[ChainModule; 4],
    ) -> f32 {
        // Pre EQ → modules in chain order → Post EQ.
        let mut sample =
            self.eq_section
                .pre
                .process_sample_for_channel(channel, sample, &frame.eq_section.pre);

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

        self.eq_section
            .post
            .process_sample_for_channel(channel, sample, &frame.eq_section.post)
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
    use crate::params::Cc22Params;

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

    // ── Two-EQ system (Pre / Post) ──────────────────────────────────────────

    fn gain_range() -> FloatRange {
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        }
    }

    fn render_rms(params: &Cc22Params, order: &[ChainModule; 4], freq: f32, amp: f32) -> f32 {
        let mut chain = EffectChain::default();
        chain.prepare(48_000.0);
        chain.reset();
        let mut phase = 0.0_f32;
        let mut sq = 0.0_f64;
        let mut n = 0.0_f64;
        for i in 0..8_000 {
            phase += freq / 48_000.0;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            let input = (phase * core::f32::consts::TAU).sin() * amp;
            let frame = chain.next_frame(params);
            let out = chain.process_sample(0, input, &frame, order);
            assert!(out.is_finite());
            if i > 2_000 {
                sq += (out as f64).powi(2);
                n += 1.0;
            }
        }
        (sq / n).sqrt() as f32
    }

    #[test]
    fn pre_and_post_eq_default_are_transparent_in_series() {
        // Default = both EQs flat (Bell @ 0 dB) and every module bypassed.
        let params = Cc22Params::default();
        let amp = 0.3_f32;
        let out_rms = render_rms(&params, &default_chain_order(), 440.0, amp);
        let in_rms = amp / 2.0_f32.sqrt();
        assert!(
            (out_rms - in_rms).abs() < 0.01,
            "default Pre+Post EQ should be transparent (in {in_rms}, out {out_rms})"
        );
    }

    #[test]
    fn pre_eq_and_post_eq_params_are_independent() {
        let mut params = Cc22Params::default();
        let post_before = params.post_eq.band3_gain.value();
        params.pre_eq.band3_gain = FloatParam::new("g", 9.0, gain_range());
        assert_eq!(
            params.post_eq.band3_gain.value(),
            post_before,
            "editing Pre EQ must not touch Post EQ"
        );

        let pre_now = params.pre_eq.band3_gain.value();
        params.post_eq.band3_gain = FloatParam::new("g", -6.0, gain_range());
        assert_eq!(
            params.pre_eq.band3_gain.value(),
            pre_now,
            "editing Post EQ must not touch Pre EQ"
        );
    }

    #[test]
    fn pre_eq_processes_before_modules_and_post_after() {
        // Filtering is *not* commutative across a nonlinear module: HP→Drive→LP
        // differs from LP→Drive→HP. If Pre/Post were applied in the wrong slot (or
        // the same slot), swapping them around the module would be a no-op.
        // Broadband noise makes the energy difference robust against level makeup.
        let render_noise = |params: &Cc22Params| -> f32 {
            params.reset_smoothers();
            let order = default_chain_order();
            let mut chain = EffectChain::default();
            chain.prepare(48_000.0);
            chain.reset();
            let mut rng = 0x1234_5678_u32;
            let mut sq = 0.0_f64;
            let mut n = 0.0_f64;
            for i in 0..16_000 {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let input = ((rng as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.3;
                let frame = chain.next_frame(params);
                let out = chain.process_sample(0, input, &frame, &order);
                assert!(out.is_finite());
                if i > 4_000 {
                    sq += (out as f64).powi(2);
                    n += 1.0;
                }
            }
            (sq / n).sqrt() as f32
        };
        let freq = |f: f32| {
            FloatParam::new(
                "f",
                f,
                FloatRange::Linear {
                    min: 20.0,
                    max: 20_000.0,
                },
            )
        };

        let mut a = Cc22Params::default();
        a.character.bypass = BoolParam::new("b", false);
        a.character.drive = FloatParam::new("d", 0.9, FloatRange::Linear { min: 0.0, max: 1.0 });
        let mut b = Cc22Params::default();
        b.character.bypass = BoolParam::new("b", false);
        b.character.drive = FloatParam::new("d", 0.9, FloatRange::Linear { min: 0.0, max: 1.0 });

        // A: boost lows *before* the saturator (the boost gets saturated/tamed).
        a.pre_eq.band2_type = EnumParam::new("t", EqBandType::LowShelf);
        a.pre_eq.band2_frequency = freq(250.0);
        a.pre_eq.band2_gain = FloatParam::new("g", 18.0, gain_range());
        // B: boost the same lows *after* the saturator (boost lands on the output).
        b.post_eq.band2_type = EnumParam::new("t", EqBandType::LowShelf);
        b.post_eq.band2_frequency = freq(250.0);
        b.post_eq.band2_gain = FloatParam::new("g", 18.0, gain_range());

        let a_rms = render_noise(&a);
        let b_rms = render_noise(&b);
        assert!(a_rms.is_finite() && b_rms.is_finite());
        assert!(
            (a_rms - b_rms).abs() > 0.01,
            "Pre/Post must be non-commutative around a nonlinear module (a {a_rms}, b {b_rms})"
        );
    }

    #[test]
    fn each_eq_only_changes_the_signal_when_engaged() {
        let order = default_chain_order();
        let flat = Cc22Params::default();
        let flat_rms = render_rms(&flat, &order, 120.0, 0.3);

        let mut pre = Cc22Params::default();
        pre.pre_eq.band1_type = EnumParam::new("t", EqBandType::HighPass);
        pre.pre_eq.band1_frequency = FloatParam::new(
            "f",
            400.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        let pre_rms = render_rms(&pre, &order, 120.0, 0.3);
        assert!(pre_rms < flat_rms * 0.9, "Pre high-pass should cut 120 Hz");

        let mut post = Cc22Params::default();
        post.post_eq.band1_type = EnumParam::new("t", EqBandType::HighPass);
        post.post_eq.band1_frequency = FloatParam::new(
            "f",
            400.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20_000.0,
            },
        );
        let post_rms = render_rms(&post, &order, 120.0, 0.3);
        assert!(
            post_rms < flat_rms * 0.9,
            "Post high-pass should cut 120 Hz"
        );
    }
}
