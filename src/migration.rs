//! Backwards-compatibility migration for projects saved by older CC-22 builds.
//!
//! Earlier versions exposed extra effect modes that have since been removed in
//! favour of the 20 official modes. nih-plug serializes enum parameters by their
//! stable string `#[id]`, so when an old project that stored a now-removed mode
//! id is loaded, `set_from_id` silently fails and the parameter falls back to its
//! default. That avoids a panic, but it discards the user's intent and — for the
//! old dedicated "Off" mode — could make a module suddenly apply an effect.
//!
//! This module is wired into [`Plugin::filter_state`], which runs just before a
//! saved state is applied, and rewrites the parameter map so every removed mode
//! maps onto its closest surviving equivalent.
//!
//! No legacy enum variants are kept: the migration happens purely at the state
//! level, so the runtime enums only ever contain the 20 official modes.

use nih_plug::prelude::PluginState;
use nih_plug::wrapper::state::ParamValue;

/// What a removed mode migrates to: the surviving mode `#[id]`, plus an optional
/// module-bypass override. The bypass override reproduces the old dedicated
/// "Off" mode by engaging the module bypass, so the migrated module stays
/// transparent instead of suddenly applying an effect (and never opens silent).
struct LegacyTarget {
    mode_id: &'static str,
    bypass: Option<bool>,
}

impl LegacyTarget {
    /// Map an old mode onto a surviving mode, leaving the saved bypass untouched.
    const fn mode(mode_id: &'static str) -> Self {
        Self {
            mode_id,
            bypass: None,
        }
    }

    /// Map an old "Off" pseudo-mode onto the module's first product mode and
    /// engage the module bypass so the result stays transparent.
    const fn bypassed(mode_id: &'static str) -> Self {
        Self {
            mode_id,
            bypass: Some(true),
        }
    }
}

fn character_target(old_id: &str) -> Option<LegacyTarget> {
    Some(match old_id {
        // "Clean" had no coloration; Sweeten is the gentlest surviving character.
        "clean" => LegacyTarget::mode("sweet"),
        "saturation" => LegacyTarget::mode("drive"),
        "cassette" => LegacyTarget::mode("sweet"),
        _ => return None,
    })
}

fn movement_target(old_id: &str) -> Option<LegacyTarget> {
    Some(match old_id {
        "off" => LegacyTarget::bypassed("doubler"),
        "chorus" => LegacyTarget::mode("doubler"),
        _ => return None,
    })
}

fn diffusion_target(old_id: &str) -> Option<LegacyTarget> {
    Some(match old_id {
        "off" => LegacyTarget::bypassed("cascade"),
        "delay" => LegacyTarget::mode("reels"),
        "slap" => LegacyTarget::mode("reels"),
        "reverb" => LegacyTarget::mode("space"),
        _ => return None,
    })
}

fn texture_target(old_id: &str) -> Option<LegacyTarget> {
    Some(match old_id {
        "off" => LegacyTarget::bypassed("filter"),
        "wow-flutter" => LegacyTarget::mode("cassette"),
        "noise" => LegacyTarget::mode("broken"),
        "tape" => LegacyTarget::mode("cassette"),
        _ => return None,
    })
}

/// Rewrites removed mode ids in a saved [`PluginState`] to the 20 official modes.
///
/// Safe to call on any state: modern/unknown ids and non-enum values are left
/// untouched, and it never panics on missing or unexpected entries.
pub fn migrate_legacy_state(state: &mut PluginState) {
    migrate_module(
        state,
        "character_mode",
        "character_bypass",
        character_target,
    );
    migrate_module(state, "movement_mode", "movement_bypass", movement_target);
    migrate_module(
        state,
        "diffusion_mode",
        "diffusion_bypass",
        diffusion_target,
    );
    migrate_module(state, "texture_mode", "texture_bypass", texture_target);
}

fn migrate_module(
    state: &mut PluginState,
    mode_id: &str,
    bypass_id: &str,
    map: fn(&str) -> Option<LegacyTarget>,
) {
    let old_id = match state.params.get(mode_id) {
        Some(ParamValue::String(value)) => value.clone(),
        _ => return,
    };
    let Some(target) = map(&old_id) else {
        return;
    };

    state.params.insert(
        mode_id.to_string(),
        ParamValue::String(target.mode_id.to_string()),
    );
    if let Some(bypass) = target.bypass {
        state
            .params
            .insert(bypass_id.to_string(), ParamValue::Bool(bypass));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn state_with(mode_id: &str, value: &str) -> PluginState {
        let mut params = BTreeMap::new();
        params.insert(mode_id.to_string(), ParamValue::String(value.to_string()));
        PluginState {
            version: "old".to_string(),
            params,
            fields: BTreeMap::new(),
        }
    }

    fn mode_of(state: &PluginState, id: &str) -> Option<String> {
        match state.params.get(id) {
            Some(ParamValue::String(s)) => Some(s.clone()),
            _ => None,
        }
    }

    fn bypass_of(state: &PluginState, id: &str) -> Option<bool> {
        match state.params.get(id) {
            Some(ParamValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    #[test]
    fn maps_every_removed_character_mode() {
        for (old, new) in [
            ("clean", "sweet"),
            ("saturation", "drive"),
            ("cassette", "sweet"),
        ] {
            let mut state = state_with("character_mode", old);
            migrate_legacy_state(&mut state);
            assert_eq!(mode_of(&state, "character_mode").as_deref(), Some(new));
        }
    }

    #[test]
    fn maps_every_removed_movement_mode() {
        let mut chorus = state_with("movement_mode", "chorus");
        migrate_legacy_state(&mut chorus);
        assert_eq!(
            mode_of(&chorus, "movement_mode").as_deref(),
            Some("doubler")
        );

        let mut off = state_with("movement_mode", "off");
        migrate_legacy_state(&mut off);
        assert_eq!(mode_of(&off, "movement_mode").as_deref(), Some("doubler"));
        assert_eq!(bypass_of(&off, "movement_bypass"), Some(true));
    }

    #[test]
    fn maps_every_removed_diffusion_mode() {
        for (old, new) in [("delay", "reels"), ("slap", "reels"), ("reverb", "space")] {
            let mut state = state_with("diffusion_mode", old);
            migrate_legacy_state(&mut state);
            assert_eq!(mode_of(&state, "diffusion_mode").as_deref(), Some(new));
            assert_eq!(bypass_of(&state, "diffusion_bypass"), None);
        }

        let mut off = state_with("diffusion_mode", "off");
        migrate_legacy_state(&mut off);
        assert_eq!(mode_of(&off, "diffusion_mode").as_deref(), Some("cascade"));
        assert_eq!(bypass_of(&off, "diffusion_bypass"), Some(true));
    }

    #[test]
    fn maps_every_removed_texture_mode() {
        for (old, new) in [
            ("wow-flutter", "cassette"),
            ("noise", "broken"),
            ("tape", "cassette"),
        ] {
            let mut state = state_with("texture_mode", old);
            migrate_legacy_state(&mut state);
            assert_eq!(mode_of(&state, "texture_mode").as_deref(), Some(new));
        }

        let mut off = state_with("texture_mode", "off");
        migrate_legacy_state(&mut off);
        assert_eq!(mode_of(&off, "texture_mode").as_deref(), Some("filter"));
        assert_eq!(bypass_of(&off, "texture_bypass"), Some(true));
    }

    #[test]
    fn leaves_surviving_modes_untouched() {
        for id in ["drive", "sweet", "fuzz", "howl", "swell"] {
            let mut state = state_with("character_mode", id);
            migrate_legacy_state(&mut state);
            assert_eq!(mode_of(&state, "character_mode").as_deref(), Some(id));
        }
        // "cassette" survives for Texture even though it was removed from Character.
        let mut tex = state_with("texture_mode", "cassette");
        migrate_legacy_state(&mut tex);
        assert_eq!(mode_of(&tex, "texture_mode").as_deref(), Some("cassette"));
    }

    #[test]
    fn does_not_panic_on_empty_or_unexpected_state() {
        let mut empty = PluginState {
            version: String::new(),
            params: BTreeMap::new(),
            fields: BTreeMap::new(),
        };
        migrate_legacy_state(&mut empty);
        assert!(empty.params.is_empty());

        // Wrong value type for a mode key must be ignored, not rewritten/panicked.
        let mut wrong = PluginState {
            version: String::new(),
            params: BTreeMap::from([("character_mode".to_string(), ParamValue::I32(3))]),
            fields: BTreeMap::new(),
        };
        migrate_legacy_state(&mut wrong);
        assert!(matches!(
            wrong.params.get("character_mode"),
            Some(ParamValue::I32(3))
        ));
    }

    #[test]
    fn migration_preserves_chain_order_and_eq_params() {
        // A realistic saved state: a removed mode that must migrate, alongside
        // chain-order slots and EQ parameters that must survive untouched.
        let mut params = BTreeMap::new();
        params.insert(
            "character_mode".to_string(),
            ParamValue::String("clean".to_string()),
        );
        params.insert(
            "chain_s0".to_string(),
            ParamValue::String("texture".to_string()),
        );
        params.insert(
            "chain_s1".to_string(),
            ParamValue::String("character".to_string()),
        );
        params.insert("global_pre_eq_b1_gain".to_string(), ParamValue::F32(-4.5));
        params.insert(
            "texture_post_eq_b3_freq".to_string(),
            ParamValue::F32(2_200.0),
        );
        let mut state = PluginState {
            version: "old".to_string(),
            params,
            fields: BTreeMap::new(),
        };

        migrate_legacy_state(&mut state);

        // Removed mode is migrated...
        assert_eq!(mode_of(&state, "character_mode").as_deref(), Some("sweet"));
        // ...while chain order and EQ params pass through byte-for-byte.
        assert_eq!(mode_of(&state, "chain_s0").as_deref(), Some("texture"));
        assert_eq!(mode_of(&state, "chain_s1").as_deref(), Some("character"));
        assert!(matches!(
            state.params.get("global_pre_eq_b1_gain"),
            Some(ParamValue::F32(g)) if (*g - (-4.5)).abs() < 1e-6
        ));
        assert!(matches!(
            state.params.get("texture_post_eq_b3_freq"),
            Some(ParamValue::F32(f)) if (*f - 2_200.0).abs() < 1e-3
        ));
    }
}
