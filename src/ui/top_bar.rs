use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, CornerRadius, FontId, RichText, Sense, Stroke, UiBuilder, Vec2,
};

use crate::params::Cc22Params;

use super::{
    meters::UiState,
    preset_bar::{next_preset, preset_selector_with_id, previous_preset, randomize_controls},
    theme::{ModuleColors, Theme},
    widgets::{brand_mark, colored_knob, compact_button, global_bypass_button},
};

const TOP_BAR_HEIGHT: f32 = 86.0;

pub(crate) fn top_bar(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TOP_BAR_HEIGHT),
        Sense::hover(),
    );
    let shadow_rect = egui::Rect::from_min_size(
        rect.min + Vec2::new(4.0, 5.0),
        Vec2::new(rect.width().min(1_100.0), TOP_BAR_HEIGHT - 20.0),
    );
    ui.painter()
        .rect_filled(shadow_rect, CornerRadius::same(16), theme.shadow);

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(theme.paper_alt)
                .stroke(Stroke::new(1.0, theme.text_dark))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(rect.width() - 16.0, TOP_BAR_HEIGHT - 16.0));
                    ui.horizontal_centered(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                brand_mark(ui, colors, theme);
                                ui.label(
                                    RichText::new("CC-22")
                                        .font(FontId::proportional(24.0))
                                        .strong()
                                        .color(theme.text_dark),
                                );
                            });
                            ui.label(
                                RichText::new("MODULAR COLOR PROCESSOR")
                                    .font(FontId::monospace(8.5))
                                    .color(theme.muted_dark),
                            );
                        });

                        ui.add_space(14.0);
                        preset_selector_with_id(
                            ui,
                            setter,
                            state,
                            params,
                            theme,
                            "preset-selector",
                            280.0,
                        );

                        if compact_button(ui, "PREV", theme, colors.master).clicked() {
                            previous_preset(setter, state, params);
                        }
                        if compact_button(ui, "NEXT", theme, colors.master).clicked() {
                            next_preset(setter, state, params);
                        }
                        if compact_button(ui, "RND", theme, colors.texture)
                            .on_hover_text("Randomize musical control values")
                            .clicked()
                        {
                            randomize_controls(setter, state, params);
                        }

                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            global_bypass_button(ui, setter, &params.global_bypass, theme);
                            colored_knob(
                                ui,
                                setter,
                                &params.dry_wet,
                                "DRY/WET",
                                colors.master,
                                theme,
                                38.0,
                            );
                        });
                    });
                });
        },
    );
}
