use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, RichText, Sense, UiBuilder, Vec2,
};

use crate::{params::Cc22Params, presets::internal_presets};

use super::{
    meters::{MeterReading, UiState},
    theme::{ModuleColors, Theme},
    widgets::{
        brand_orb, global_bypass_button, rounded_panel, set_float_normalized, small_strip_knob,
    },
};

pub(crate) fn bottom_macro_row(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    meter_reading: MeterReading,
    colors: ModuleColors,
    theme: Theme,
) {
    let panel_height = ui.available_height().max(78.0);
    let strip_height = (panel_height - 20.0).clamp(58.0, 96.0);
    rounded_panel(
        ui,
        theme.paper,
        theme.text_dark,
        CornerRadius::same(18),
        |ui| {
            ui.set_min_height(panel_height - 20.0);
            ui.horizontal_top(|ui| {
                let (strip_rect, _) =
                    ui.allocate_exact_size(Vec2::new(620.0, strip_height), Sense::hover());
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
                                ui.set_min_size(Vec2::new(604.0, strip_height - 16.0));
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
                                    ui.add_space(12.0);
                                    bottom_master_section(
                                        ui,
                                        setter,
                                        params,
                                        meter_reading,
                                        colors,
                                        theme,
                                    );
                                    bottom_strip_divider(ui);
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
                });
            });
        },
    );
}

fn bottom_master_section(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    meter_reading: MeterReading,
    colors: ModuleColors,
    theme: Theme,
) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        ui.vertical(|ui| {
            ui.set_width(70.0);
            ui.label(
                RichText::new("MASTER / OUT")
                    .font(FontId::monospace(9.0))
                    .strong()
                    .color(Color32::from_rgb(245, 237, 218)),
            );
            ui.label(
                RichText::new("FINAL")
                    .font(FontId::monospace(7.0))
                    .strong()
                    .color(Color32::from_rgb(165, 154, 132)),
            );
        });
        bottom_meter(ui, "IN", meter_reading.input.level(), meter_reading.input.clipped);
        bottom_meter(
            ui,
            "OUT",
            meter_reading.output.level(),
            meter_reading.output.clipped,
        );
        small_strip_knob(ui, setter, &params.input_gain, "INPUT", colors.master, theme);
        small_strip_knob(ui, setter, &params.output_gain, "OUTPUT", colors.master, theme);
        small_strip_knob(ui, setter, &params.dry_wet, "DRY/WET", colors.eq, theme);
    });
}

fn bottom_meter(ui: &mut egui::Ui, label: &'static str, level: f32, clipped: bool) {
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(7.0))
                .strong()
                .color(Color32::from_rgb(245, 237, 218)),
        );
        let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 22.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(18, 15, 12));
        let fill_bounds = rect.shrink2(Vec2::new(2.0, 2.0));
        let fill_h = fill_bounds.height() * level.clamp(0.0, 1.0);
        let fill = egui::Rect::from_min_max(
            egui::pos2(fill_bounds.left(), fill_bounds.bottom() - fill_h),
            fill_bounds.right_bottom(),
        );
        if fill_h > 0.5 {
            ui.painter().rect_filled(
                fill,
                CornerRadius::same(2),
                if clipped {
                    Color32::from_rgb(225, 64, 52)
                } else {
                    Color32::from_rgb(245, 132, 34)
                },
            );
        }
    });
}

fn bottom_strip_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 34.0), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        egui::Stroke::new(1.0, Color32::from_rgb(83, 74, 61)),
    );
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
