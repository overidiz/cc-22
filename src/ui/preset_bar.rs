use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, RichText, Sense, UiBuilder, Vec2,
};

use crate::{params::Cc22Params, presets::internal_presets};

use super::{
    meters::UiState,
    theme::{ModuleColors, Theme},
    widgets::{
        brand_orb, disabled_mini_control, disabled_nav_button, global_bypass_button, rounded_panel,
        set_float_normalized, small_strip_knob,
    },
};

pub(crate) fn bottom_macro_row(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    rounded_panel(
        ui,
        theme.paper,
        theme.text_dark,
        CornerRadius::same(18),
        |ui| {
            ui.horizontal_top(|ui| {
                let (strip_rect, _) =
                    ui.allocate_exact_size(Vec2::new(385.0, 50.0), Sense::hover());
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(strip_rect)
                        .layout(egui::Layout::top_down(Align::Min)),
                    |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(31, 26, 21))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                ui.set_min_size(Vec2::new(369.0, 34.0));
                                ui.horizontal(|ui| {
                                    brand_orb(ui, colors);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new("CC-22")
                                                .font(FontId::proportional(20.0))
                                                .strong()
                                                .color(Color32::from_rgb(245, 237, 218)),
                                        );
                                        ui.label(
                                            RichText::new("MULTI-FX")
                                                .font(FontId::monospace(8.0))
                                                .strong()
                                                .color(colors.eq),
                                        );
                                    });
                                    ui.add_space(10.0);
                                    disabled_nav_button(ui, "<", theme).on_hover_text(
                                        "Placeholder: use the preset controls in the top bar",
                                    );
                                    disabled_nav_button(ui, "INIT", theme).on_hover_text(
                                        "Placeholder: init/reset preset is not wired yet",
                                    );
                                    disabled_nav_button(ui, ">", theme).on_hover_text(
                                        "Placeholder: use the preset controls in the top bar",
                                    );
                                    ui.add_space(12.0);
                                    small_strip_knob(
                                        ui,
                                        setter,
                                        &params.character.mix,
                                        "CHAR MIX",
                                        colors.character,
                                        theme,
                                    );
                                    small_strip_knob(
                                        ui,
                                        setter,
                                        &params.output_gain,
                                        "OUT",
                                        colors.master,
                                        theme,
                                    );
                                    small_strip_knob(
                                        ui,
                                        setter,
                                        &params.dry_wet,
                                        "DRY/WET",
                                        colors.eq,
                                        theme,
                                    );
                                    small_strip_knob(
                                        ui,
                                        setter,
                                        &params.diffusion.mix,
                                        "DIFF MIX",
                                        colors.diffusion,
                                        theme,
                                    );
                                    small_strip_knob(
                                        ui,
                                        setter,
                                        &params.movement.rate,
                                        "MOV RATE",
                                        colors.movement,
                                        theme,
                                    );
                                });
                            });
                    },
                );

                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        preset_selector_with_id(
                            ui,
                            setter,
                            state,
                            params,
                            theme,
                            "bottom-preset-selector",
                            150.0,
                        );
                        global_bypass_button(ui, setter, &params.global_bypass, theme);
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        disabled_mini_control(ui, "FILTER", "Tilt", theme)
                            .on_hover_text("Placeholder: no Filter macro parameter exists yet");
                        disabled_mini_control(ui, "OS", "2x", theme)
                            .on_hover_text("Placeholder: oversampling control is not implemented");
                        disabled_mini_control(ui, "HQ", "Eco", theme)
                            .on_hover_text("Placeholder: quality mode control is not implemented");
                        disabled_mini_control(ui, "M/S", "Off", theme)
                            .on_hover_text("Placeholder: mid/side control is not implemented");
                    });
                });
            });
        },
    );
}

pub(crate) fn preset_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    theme: Theme,
) {
    preset_selector_with_id(ui, setter, state, params, theme, "preset-selector", 220.0);
}

pub(crate) fn preset_selector_with_id(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    theme: Theme,
    id: &'static str,
    width: f32,
) {
    let presets = internal_presets();
    state.selected_preset = state.selected_preset.min(presets.len().saturating_sub(1));

    ui.vertical(|ui| {
        ui.label(
            RichText::new("PRESET")
                .font(FontId::monospace(10.0))
                .color(theme.muted_dark),
        );
        egui::ComboBox::from_id_salt(id)
            .selected_text(presets[state.selected_preset].name)
            .width(width)
            .show_ui(ui, |ui| {
                for (index, preset) in presets.iter().enumerate() {
                    if ui
                        .selectable_label(index == state.selected_preset, preset.name)
                        .clicked()
                    {
                        state.selected_preset = index;
                        preset.apply_with_setter(setter, params);
                        ui.close_menu();
                    }
                }
            });
    });
}

pub(crate) fn previous_preset(setter: &ParamSetter<'_>, state: &mut UiState, params: &Cc22Params) {
    let presets = internal_presets();
    state.selected_preset = if state.selected_preset == 0 {
        presets.len().saturating_sub(1)
    } else {
        state.selected_preset - 1
    };
    presets[state.selected_preset].apply_with_setter(setter, params);
}

pub(crate) fn next_preset(setter: &ParamSetter<'_>, state: &mut UiState, params: &Cc22Params) {
    let presets = internal_presets();
    state.selected_preset = (state.selected_preset + 1) % presets.len().max(1);
    presets[state.selected_preset].apply_with_setter(setter, params);
}

pub(crate) fn randomize_controls(
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
) {
    let mut next = || {
        state.random_seed ^= state.random_seed << 13;
        state.random_seed ^= state.random_seed >> 17;
        state.random_seed ^= state.random_seed << 5;
        state.random_seed as f32 / u32::MAX as f32
    };

    set_float_normalized(setter, &params.character.drive, 0.12 + next() * 0.55);
    set_float_normalized(setter, &params.character.tone, 0.25 + next() * 0.65);
    set_float_normalized(setter, &params.character.mix, 0.25 + next() * 0.65);
    set_float_normalized(setter, &params.movement.rate, 0.15 + next() * 0.55);
    set_float_normalized(setter, &params.movement.depth, 0.10 + next() * 0.70);
    set_float_normalized(setter, &params.movement.mix, 0.10 + next() * 0.55);
    set_float_normalized(setter, &params.diffusion.time, 0.12 + next() * 0.55);
    set_float_normalized(setter, &params.diffusion.feedback, next() * 0.48);
    set_float_normalized(setter, &params.diffusion.mix, 0.08 + next() * 0.42);
    set_float_normalized(setter, &params.texture.wow_depth, next() * 0.42);
    set_float_normalized(setter, &params.texture.random_drift, next() * 0.35);
    set_float_normalized(setter, &params.texture.mix, next() * 0.45);
    set_float_normalized(setter, &params.dry_wet, 0.72 + next() * 0.28);
}
