use std::{env, fs, io, path::PathBuf};

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
                    ui.painter().line_segment(
                        [
                            rect.left_top() + Vec2::new(14.0, 1.5),
                            rect.right_top() + Vec2::new(-14.0, 1.5),
                        ],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(244, 239, 228, 56)),
                    );
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
    let y = content.top();
    let h = content.height();
    let identity_w = 150.0;
    let controls_left = content.left() + identity_w;
    let section_w = (content.right() - controls_left) / 3.0;
    let clipped = meter_reading.input.clipped || meter_reading.output.clipped;

    fixed_child_ui(ui, clip, content.left(), y, identity_w - 16.0, h, |ui| {
        master_label(ui, colors.accent, clipped, theme);
    });

    for boundary in [
        controls_left,
        controls_left + section_w,
        controls_left + section_w * 2.0,
    ] {
        fixed_child_ui(ui, clip, boundary, y + 7.0, 1.0, h - 14.0, |ui| {
            bottom_strip_divider(ui);
        });
    }

    let input_center = controls_left + section_w * 0.5;
    fixed_child_ui(ui, clip, controls_left, y, section_w, 13.0, |ui| {
        section_title(ui, "INPUT STAGE", colors.master);
    });
    fixed_child_ui(
        ui,
        clip,
        input_center - 51.0,
        y + 13.0,
        30.0,
        h - 13.0,
        |ui| {
            bottom_meter(ui, meter_reading.input.level(), meter_reading.input.clipped);
        },
    );
    fixed_child_ui(
        ui,
        clip,
        input_center - 5.0,
        y + 13.0,
        70.0,
        h - 13.0,
        |ui| {
            small_strip_knob(
                ui,
                setter,
                &params.input_gain,
                "GAIN",
                colors.master,
                theme,
                30.0,
            );
        },
    );

    let output_left = controls_left + section_w;
    let output_center = output_left + section_w * 0.5;
    fixed_child_ui(ui, clip, output_left, y, section_w, 13.0, |ui| {
        section_title(ui, "OUTPUT STAGE", colors.master);
    });
    fixed_child_ui(
        ui,
        clip,
        output_center - 51.0,
        y + 13.0,
        30.0,
        h - 13.0,
        |ui| {
            bottom_meter(
                ui,
                meter_reading.output.level(),
                meter_reading.output.clipped,
            );
        },
    );
    fixed_child_ui(
        ui,
        clip,
        output_center - 5.0,
        y + 13.0,
        70.0,
        h - 13.0,
        |ui| {
            small_strip_knob(
                ui,
                setter,
                &params.output_gain,
                "GAIN",
                colors.master,
                theme,
                30.0,
            );
        },
    );

    let blend_left = controls_left + section_w * 2.0;
    fixed_child_ui(ui, clip, blend_left, y, section_w, 13.0, |ui| {
        section_title(ui, "MASTER BLEND", colors.accent)
    });
    fixed_child_ui(
        ui,
        clip,
        blend_left + (section_w - 82.0) * 0.5,
        y + 10.0,
        82.0,
        h - 10.0,
        |ui| {
            small_strip_knob(
                ui,
                setter,
                &params.dry_wet,
                "DRY/WET",
                colors.accent,
                theme,
                38.0,
            );
        },
    );
}

fn section_title(ui: &mut egui::Ui, label: &'static str, color: Color32) {
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(8.0))
                .strong()
                .color(color.gamma_multiply(0.72)),
        );
    });
}

fn bottom_meter(ui: &mut egui::Ui, level: f32, clipped: bool) {
    ui.vertical_centered(|ui| {
        ui.add_space(2.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(18.0, 34.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(18, 15, 12));
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, Color32::from_rgb(83, 74, 61)),
            egui::StrokeKind::Inside,
        );
        ui.painter().line_segment(
            [
                rect.left_top() + Vec2::new(3.0, 2.0),
                rect.right_top() + Vec2::new(-3.0, 2.0),
            ],
            Stroke::new(0.8, Color32::from_rgba_unmultiplied(244, 239, 228, 36)),
        );
        let fill_bounds = rect.shrink2(Vec2::new(2.5, 2.5));
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
                    Color32::from_rgb(239, 192, 0)
                },
            );
        }
    });
}

fn master_label(ui: &mut egui::Ui, accent: Color32, clipped: bool, theme: Theme) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new("MASTER")
                .font(FontId::monospace(14.0))
                .strong()
                .color(Color32::from_rgb(245, 237, 218)),
        );
        ui.label(
            RichText::new("FINAL OUT")
                .font(FontId::monospace(8.0))
                .strong()
                .color(accent),
        );
        ui.add_space(2.0);
        let color = if clipped {
            theme.warning
        } else {
            Color32::from_rgb(111, 101, 86)
        };
        ui.horizontal_centered(|ui| {
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 3.0, color);
            ui.label(
                RichText::new("CLIP")
                    .font(FontId::monospace(7.5))
                    .strong()
                    .color(if clipped {
                        theme.warning
                    } else {
                        Color32::from_rgb(170, 158, 137)
                    }),
            );
        });
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
        // The global theme paints windows/widgets near-black with dark text,
        // which left the preset combo (button + popup) invisible. Give this combo
        // a light "paper" surface locally so the dark text reads. `weak_bg_fill`
        // is what the ComboBox button uses; `window_fill` is the popup frame.
        {
            let visuals = &mut ui.style_mut().visuals;
            visuals.window_fill = theme.paper;
            visuals.window_stroke = Stroke::new(1.0, theme.card_edge);
            visuals.widgets.inactive.weak_bg_fill = theme.paper;
            visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(250, 246, 235);
            visuals.widgets.active.weak_bg_fill = Color32::from_rgb(226, 220, 207);
        }
        egui::ComboBox::from_id_salt(id)
            .selected_text(presets[state.selected_preset].name)
            .width(width)
            .show_ui(ui, |ui| {
                // Inside the popup: dark text on the light frame, with a tinted
                // (not dark) highlight so the selected/hovered row stays readable.
                let visuals = &mut ui.style_mut().visuals;
                visuals.override_text_color = Some(theme.text_dark);
                visuals.selection.bg_fill = Color32::from_rgb(214, 206, 190);
                visuals.selection.stroke = Stroke::new(1.0, theme.card_edge);
                visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(232, 226, 214);
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

pub(crate) fn save_user_snapshot(params: &Cc22Params) -> io::Result<PathBuf> {
    let base = env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let directory = base.join("CC-22").join("Presets");
    fs::create_dir_all(&directory)?;
    let path = directory.join("CC-22 User Snapshot.cc22preset");
    fs::write(&path, user_snapshot_contents(params))?;
    Ok(path)
}

fn user_snapshot_contents(params: &Cc22Params) -> String {
    let mut values = params
        .param_map()
        .into_iter()
        .map(|(id, param, _)| {
            // ParamPtr values remain valid while `params` is alive. This runs on the UI thread.
            let normalized = unsafe { param.unmodulated_normalized_value() };
            (id, normalized)
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.0.cmp(&right.0));

    let mut snapshot = String::from("CC22_PRESET_V1\n");
    for (id, normalized) in values {
        snapshot.push_str(&format!("{id}={normalized:.9}\n"));
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::user_snapshot_contents;
    use crate::params::Cc22Params;

    #[test]
    fn user_snapshot_serializes_all_chain_slots() {
        let snapshot = user_snapshot_contents(&Cc22Params::default());
        for id in ["chain_s0", "chain_s1", "chain_s2", "chain_s3"] {
            assert!(
                snapshot
                    .lines()
                    .any(|line| line.starts_with(&format!("{id}="))),
                "snapshot did not serialize {id}"
            );
        }
    }
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
