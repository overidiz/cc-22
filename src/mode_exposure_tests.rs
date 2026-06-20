//! Guard-rail tests proving that every module exposes exactly its five official
//! modes — to the host (via the `Enum` trait), to the UI selector, to the
//! defaults, and to the internal presets — and that projects saved with an old
//! removed mode migrate onto a valid official mode.

use std::collections::BTreeMap;

use nih_plug::prelude::{Enum, PluginState};
use nih_plug::wrapper::state::ParamValue;

use crate::dsp::character::CharacterMode;
use crate::dsp::diffusion::DiffusionMode;
use crate::dsp::movement::MovementMode;
use crate::dsp::texture::TextureMode;
use crate::migration::migrate_legacy_state;
use crate::params::Cc22Params;
use crate::presets::internal_presets;
use crate::ui::module_card::{
    CHARACTER_MODE_OPTIONS, DIFFUSION_MODE_OPTIONS, MOVEMENT_MODE_OPTIONS, TEXTURE_MODE_OPTIONS,
};

// ─── Exact exposure: each module has exactly five official modes ────────────

#[test]
fn character_exposes_exactly_drive_sweeten_fuzz_howl_swell() {
    assert_eq!(CharacterMode::variants().len(), 5);
    assert_eq!(
        CharacterMode::variants(),
        &["Drive", "Sweeten", "Fuzz", "Howl", "Swell"][..]
    );
    assert_eq!(
        CharacterMode::ids(),
        Some(&["drive", "sweet", "fuzz", "howl", "swell"][..])
    );
    assert_eq!(CharacterMode::PRODUCT_MODES.len(), 5);
}

#[test]
fn character_does_not_expose_clean_saturation_cassette() {
    for name in ["Clean", "Saturation", "Cassette"] {
        assert!(
            !CharacterMode::variants().contains(&name),
            "Character must not expose mode name {name}"
        );
    }
    for id in ["clean", "saturation", "cassette"] {
        assert!(
            !CharacterMode::ids().unwrap().contains(&id),
            "Character must not expose mode id {id}"
        );
    }
}

#[test]
fn movement_exposes_exactly_doubler_vibrato_phaser_tremolo_pitch() {
    assert_eq!(MovementMode::variants().len(), 5);
    assert_eq!(
        MovementMode::variants(),
        &["Doubler", "Vibrato", "Phaser", "Tremolo", "Pitch"][..]
    );
    assert_eq!(
        MovementMode::ids(),
        Some(&["doubler", "vibrato", "phaser", "tremolo", "pitch"][..])
    );
    assert_eq!(MovementMode::PRODUCT_MODES.len(), 5);
}

#[test]
fn movement_does_not_expose_off_chorus() {
    for name in ["Off", "Chorus"] {
        assert!(
            !MovementMode::variants().contains(&name),
            "Movement must not expose mode name {name}"
        );
    }
    for id in ["off", "chorus"] {
        assert!(
            !MovementMode::ids().unwrap().contains(&id),
            "Movement must not expose mode id {id}"
        );
    }
}

#[test]
fn diffusion_exposes_exactly_cascade_reels_space_collage_reverse() {
    assert_eq!(DiffusionMode::variants().len(), 5);
    assert_eq!(
        DiffusionMode::variants(),
        &["Cascade", "Reels", "Space", "Collage", "Reverse"][..]
    );
    assert_eq!(
        DiffusionMode::ids(),
        Some(&["cascade", "reels", "space", "collage", "reverse"][..])
    );
    assert_eq!(DiffusionMode::PRODUCT_MODES.len(), 5);
}

#[test]
fn diffusion_does_not_expose_off_delay_slap_reverb() {
    for name in ["Off", "Delay", "Slap", "Reverb"] {
        assert!(
            !DiffusionMode::variants().contains(&name),
            "Diffusion must not expose mode name {name}"
        );
    }
    for id in ["off", "delay", "slap", "reverb"] {
        assert!(
            !DiffusionMode::ids().unwrap().contains(&id),
            "Diffusion must not expose mode id {id}"
        );
    }
}

#[test]
fn texture_exposes_exactly_filter_squash_cassette_broken_interference() {
    assert_eq!(TextureMode::variants().len(), 5);
    assert_eq!(
        TextureMode::variants(),
        &["Filter", "Squash", "Cassette", "Broken", "Interference"][..]
    );
    assert_eq!(
        TextureMode::ids(),
        Some(&["filter", "squash", "cassette", "broken", "interference"][..])
    );
    assert_eq!(TextureMode::PRODUCT_MODES.len(), 5);
}

#[test]
fn texture_does_not_expose_off_wowflutter_noise_tape() {
    // Note: "Cassette" is a valid Texture mode, so it is intentionally not listed.
    for name in ["Off", "WowFlutter", "Wow/Flutter", "Noise", "Tape"] {
        assert!(
            !TextureMode::variants().contains(&name),
            "Texture must not expose mode name {name}"
        );
    }
    for id in ["off", "wow-flutter", "noise", "tape"] {
        assert!(
            !TextureMode::ids().unwrap().contains(&id),
            "Texture must not expose mode id {id}"
        );
    }
}

// ─── Defaults are valid official modes ──────────────────────────────────────

#[test]
fn module_defaults_are_valid_first_official_modes() {
    let params = Cc22Params::default();

    assert_eq!(params.character.mode.value(), CharacterMode::Drive);
    assert_eq!(params.movement.mode.value(), MovementMode::Doubler);
    assert_eq!(params.diffusion.mode.value(), DiffusionMode::Cascade);
    assert_eq!(params.texture.mode.value(), TextureMode::Filter);

    assert!(CharacterMode::PRODUCT_MODES.contains(&params.character.mode.value()));
    assert!(MovementMode::PRODUCT_MODES.contains(&params.movement.mode.value()));
    assert!(DiffusionMode::PRODUCT_MODES.contains(&params.diffusion.mode.value()));
    assert!(TextureMode::PRODUCT_MODES.contains(&params.texture.mode.value()));
}

// ─── Internal presets only use official modes ───────────────────────────────

#[test]
fn internal_presets_never_use_removed_modes() {
    for preset in internal_presets() {
        let values = preset.values;
        assert!(
            CharacterMode::PRODUCT_MODES.contains(&values.character.mode),
            "preset {} uses a non-official Character mode",
            preset.name
        );
        assert!(
            MovementMode::PRODUCT_MODES.contains(&values.movement.mode),
            "preset {} uses a non-official Movement mode",
            preset.name
        );
        assert!(
            DiffusionMode::PRODUCT_MODES.contains(&values.diffusion.mode),
            "preset {} uses a non-official Diffusion mode",
            preset.name
        );
        assert!(
            TextureMode::PRODUCT_MODES.contains(&values.texture.mode),
            "preset {} uses a non-official Texture mode",
            preset.name
        );
    }
}

// ─── Loading an old project migrates removed modes to valid official modes ──

fn migrated_mode_id(mode_key: &str, old_id: &str) -> Option<String> {
    let mut params = BTreeMap::new();
    params.insert(mode_key.to_string(), ParamValue::String(old_id.to_string()));
    let mut state = PluginState {
        version: "legacy".to_string(),
        params,
        fields: BTreeMap::new(),
    };
    migrate_legacy_state(&mut state);
    match state.params.get(mode_key) {
        Some(ParamValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn assert_removed_ids_migrate_to_official(mode_key: &str, official: &[&str], removed: &[&str]) {
    for &old in removed {
        // Sanity: the old id really is gone from the official set.
        assert!(
            !official.contains(&old),
            "{old} should not be an official id for {mode_key}"
        );
        let new_id = migrated_mode_id(mode_key, old)
            .unwrap_or_else(|| panic!("{mode_key}: migrating {old} dropped the mode id"));
        assert!(
            official.contains(&new_id.as_str()),
            "{mode_key}: removed id {old} migrated to non-official id {new_id}"
        );
    }
}

#[test]
fn old_projects_migrate_every_removed_mode_to_an_official_mode() {
    assert_removed_ids_migrate_to_official(
        "character_mode",
        CharacterMode::ids().unwrap(),
        &["clean", "saturation", "cassette"],
    );
    assert_removed_ids_migrate_to_official(
        "movement_mode",
        MovementMode::ids().unwrap(),
        &["off", "chorus"],
    );
    assert_removed_ids_migrate_to_official(
        "diffusion_mode",
        DiffusionMode::ids().unwrap(),
        &["off", "delay", "slap", "reverb"],
    );
    assert_removed_ids_migrate_to_official(
        "texture_mode",
        TextureMode::ids().unwrap(),
        &["off", "wow-flutter", "noise", "tape"],
    );
}

#[test]
fn surviving_mode_ids_are_left_untouched_by_migration() {
    for &id in CharacterMode::ids().unwrap() {
        assert_eq!(migrated_mode_id("character_mode", id).as_deref(), Some(id));
    }
    for &id in MovementMode::ids().unwrap() {
        assert_eq!(migrated_mode_id("movement_mode", id).as_deref(), Some(id));
    }
    for &id in DiffusionMode::ids().unwrap() {
        assert_eq!(migrated_mode_id("diffusion_mode", id).as_deref(), Some(id));
    }
    for &id in TextureMode::ids().unwrap() {
        assert_eq!(migrated_mode_id("texture_mode", id).as_deref(), Some(id));
    }
}

// ─── UI selector exposes exactly the product modes, nothing hidden/extra ────

fn option_modes<T: Copy>(options: &[(T, &str)]) -> Vec<T> {
    options.iter().map(|(mode, _)| *mode).collect()
}

fn option_labels<T>(options: &[(T, &'static str)]) -> Vec<&'static str> {
    options.iter().map(|(_, label)| *label).collect()
}

#[test]
fn ui_selectors_show_exactly_the_five_product_modes_in_order() {
    assert_eq!(CHARACTER_MODE_OPTIONS.len(), 5);
    assert_eq!(
        option_modes(&CHARACTER_MODE_OPTIONS),
        CharacterMode::PRODUCT_MODES.to_vec()
    );

    assert_eq!(MOVEMENT_MODE_OPTIONS.len(), 5);
    assert_eq!(
        option_modes(&MOVEMENT_MODE_OPTIONS),
        MovementMode::PRODUCT_MODES.to_vec()
    );

    assert_eq!(DIFFUSION_MODE_OPTIONS.len(), 5);
    assert_eq!(
        option_modes(&DIFFUSION_MODE_OPTIONS),
        DiffusionMode::PRODUCT_MODES.to_vec()
    );

    assert_eq!(TEXTURE_MODE_OPTIONS.len(), 5);
    assert_eq!(
        option_modes(&TEXTURE_MODE_OPTIONS),
        TextureMode::PRODUCT_MODES.to_vec()
    );
}

#[test]
fn ui_selector_labels_match_official_names_and_hide_nothing() {
    assert_eq!(
        option_labels(&CHARACTER_MODE_OPTIONS),
        ["DRIVE", "SWEETEN", "FUZZ", "HOWL", "SWELL"]
    );
    assert_eq!(
        option_labels(&MOVEMENT_MODE_OPTIONS),
        ["DOUBLER", "VIBRATO", "PHASER", "TREMOLO", "PITCH"]
    );
    assert_eq!(
        option_labels(&DIFFUSION_MODE_OPTIONS),
        ["CASCADE", "REELS", "SPACE", "COLLAGE", "REVERSE"]
    );
    assert_eq!(
        option_labels(&TEXTURE_MODE_OPTIONS),
        ["FILTER", "SQUASH", "CASSETTE", "BROKEN", "INTERFERENCE"]
    );

    // No selector should surface a removed mode under any casing.
    let removed = [
        "CLEAN",
        "SATURATION",
        "OFF",
        "CHORUS",
        "DELAY",
        "SLAP",
        "REVERB",
        "TAPE",
        "NOISE",
        "WOWFLUTTER",
    ];
    for label in option_labels(&CHARACTER_MODE_OPTIONS)
        .into_iter()
        .chain(option_labels(&MOVEMENT_MODE_OPTIONS))
        .chain(option_labels(&DIFFUSION_MODE_OPTIONS))
        .chain(option_labels(&TEXTURE_MODE_OPTIONS))
    {
        assert!(
            !removed.contains(&label),
            "UI selector exposes removed mode label {label}"
        );
    }
}
