use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, UiBuilder, Vec2,
};

use crate::{params::Cc22Params, presets::internal_presets};

use super::{
    meters::{MeterReading, UiState},
    theme::{ModuleColors, Theme},
    widgets::{set_float_normalized, small_strip_knob},
};

pub(crate) const MASTER_ROW_HEIGHT: f32 = 82.0;

pub(crate) fn bottom_macro_row(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    _state: &mut UiState,
    params: &Cc22Params,
    meter_reading: MeterReading,
    colors: ModuleColors,
    theme: Theme,
) {
    let row_width = ui.available_width().max(0.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(row_width, MASTER_ROW_HEIGHT), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            egui::Frame::new()
                .fill(Color32::from_rgb(34, 29, 23))
                .stroke(Stroke::new(1.0, theme.text_dark))
                .corner_radius(CornerRadius::same(12))
                .show(ui, |ui| {
                    ui.set_clip_rect(rect.intersect(ui.clip_rect()));
                    paint_master_row(ui, setter, params, meter_reading, colors, theme, rect);
                });
        },
    );
}

fn paint_master_row(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    meter_reading: MeterReading,
    colors: ModuleColors,
    theme: Theme,
    rect: egui::Rect,
) {
    let clip = rect.intersect(ui.clip_rect());
    let content = rect.shrink2(Vec2::new(12.0, 8.0));
    let knob_w = 58.0;
    let knob_gap = 8.0;
    let knob_total = (knob_w * 3.0) + (knob_gap * 2.0);
    let knob_x = content.right() - knob_total;
    let y = content.top();
    let h = content.height();

    fixed_child_ui(ui, clip, content.left(), y, 82.0, h, |ui| {
        master_label(ui, colors.eq);
    });

    let mut x = content.left() + 94.0;
    fixed_child_ui(ui, clip, x, y + 12.0, 48.0, h - 24.0, |ui| {
        clip_indicator(
            ui,
            meter_reading.input.clipped || meter_reading.output.clipped,
            theme,
        );
    });
    x += 58.0;
    fixed_child_ui(ui, clip, x, y + 4.0, 34.0, h - 8.0, |ui| {
        bottom_meter(
            ui,
            "IN",
            meter_reading.input.level(),
            meter_reading.input.clipped,
        );
    });
    x += 40.0;
    fixed_child_ui(ui, clip, x, y + 4.0, 34.0, h - 8.0, |ui| {
        bottom_meter(
            ui,
            "OUT",
            meter_reading.output.level(),
            meter_reading.output.clipped,
        );
    });

    if x + 46.0 < knob_x {
        fixed_child_ui(ui, clip, knob_x - 18.0, y + 7.0, 1.0, h - 14.0, |ui| {
            bottom_strip_divider(ui);
        });
    }

    fixed_child_ui(ui, clip, knob_x, y, knob_w, h, |ui| {
        small_strip_knob(
            ui,
            setter,
            &params.input_gain,
            "INPUT",
            colors.master,
            theme,
        );
    });
    fixed_child_ui(ui, clip, knob_x + knob_w + knob_gap, y, knob_w, h, |ui| {
        small_strip_knob(
            ui,
            setter,
            &params.output_gain,
            "OUTPUT",
            colors.master,
            theme,
        );
    });
    fixed_child_ui(
        ui,
        clip,
        knob_x + (knob_w + knob_gap) * 2.0,
        y,
        knob_w,
        h,
        |ui| {
            small_strip_knob(ui, setter, &params.dry_wet, "DRY/WET", colors.eq, theme);
        },
    );
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

fn master_label(ui: &mut egui::Ui, accent: Color32) {
    ui.vertical_centered(|ui| {
        ui.add_space(13.0);
        ui.label(
            RichText::new("MASTER")
                .font(FontId::monospace(13.0))
                .strong()
                .color(Color32::from_rgb(245, 237, 218)),
        );
        ui.label(
            RichText::new("FINAL OUT")
                .font(FontId::monospace(8.0))
                .strong()
                .color(accent),
        );
    });
}

fn clip_indicator(ui: &mut egui::Ui, clipped: bool, theme: Theme) {
    ui.horizontal_centered(|ui| {
        let color = if clipped {
            theme.warning
        } else {
            Color32::from_rgb(111, 101, 86)
        };
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, color);
        ui.label(
            RichText::new("CLIP")
                .font(FontId::monospace(8.0))
                .strong()
                .color(if clipped {
                    theme.warning
                } else {
                    Color32::from_rgb(170, 158, 137)
                }),
        );
    });
}

fn bottom_strip_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, ui.available_height()), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        egui::Stroke::new(1.0, Color32::from_rgb(83, 74, 61)),
    );
}

fn fixed_child_ui(
    ui: &mut egui::Ui,
    clip: egui::Rect,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let rect = egui::Rect::from_min_size(Pos2::new(x, y), Vec2::new(width.max(0.0), height));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            ui.set_clip_rect(rect.intersect(clip));
            add_contents(ui);
        },
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
