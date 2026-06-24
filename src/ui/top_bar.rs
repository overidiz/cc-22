use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, RichText, Sense, Stroke, UiBuilder, Vec2,
};

use crate::params::Cc22Params;

use super::{
    meters::UiState,
    preset_bar::{
        next_preset, preset_selector_with_id, previous_preset, randomize_controls,
        save_user_snapshot,
    },
    theme::{tooltip_text, ModuleColors, Theme},
    widgets::{compact_button, global_bypass_button},
};

pub(crate) const TOP_BAR_HEIGHT: f32 = 76.0;

/// CC-22 logo, pre-decoded to raw RGBA at build time (96×96) so no image-decoder
/// dependency is needed at runtime.
const LOGO_RGBA: &[u8] = include_bytes!("logo_rgba.bin");
const LOGO_SIZE: usize = 80;

/// Lazily upload the logo texture (once) and paint it into a square slot.
fn draw_logo(ui: &mut egui::Ui, state: &mut UiState, side: f32) {
    let texture = state.logo_texture.get_or_insert_with(|| {
        let image = egui::ColorImage::from_rgba_unmultiplied([LOGO_SIZE, LOGO_SIZE], LOGO_RGBA);
        ui.ctx()
            .load_texture("cc22-logo", image, egui::TextureOptions::LINEAR)
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

pub(crate) fn top_bar(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
    now: f64,
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TOP_BAR_HEIGHT),
        Sense::hover(),
    );
    let shadow_rect = rect.translate(Vec2::new(2.0, 3.0));
    ui.painter()
        .rect_filled(shadow_rect, CornerRadius::same(16), theme.shadow);

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(theme.paper_alt)
                .stroke(Stroke::new(1.0, theme.card_edge))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.painter().line_segment(
                        [
                            rect.left_top() + Vec2::new(16.0, 1.5),
                            rect.right_top() + Vec2::new(-16.0, 1.5),
                        ],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 150)),
                    );
                    ui.set_min_size(Vec2::new(rect.width() - 16.0, TOP_BAR_HEIGHT - 16.0));
                    ui.horizontal_centered(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                draw_logo(ui, state, 40.0);
                                ui.label(
                                    RichText::new("CC-22")
                                        .font(FontId::proportional(24.0))
                                        .strong()
                                        .color(theme.text_dark),
                                );
                            });
                            ui.label(
                                RichText::new("MODULAR COLOR PROCESSOR")
                                    .font(FontId::monospace(9.0))
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

                        if compact_button(ui, "\u{25C0}", theme, colors.master)
                            .on_hover_text(tooltip_text("Previous preset"))
                            .clicked()
                        {
                            previous_preset(setter, state, params);
                        }
                        if compact_button(ui, "\u{25B6}", theme, colors.master)
                            .on_hover_text(tooltip_text("Next preset"))
                            .clicked()
                        {
                            next_preset(setter, state, params);
                        }
                        if compact_button(ui, "RND", theme, colors.texture)
                            .on_hover_text(tooltip_text("Randomize the musical controls"))
                            .clicked()
                        {
                            randomize_controls(setter, state, params);
                        }
                        if compact_button(ui, "SAVE", theme, colors.diffusion)
                            .on_hover_text(tooltip_text(
                                "Save a portable snapshot to the CC-22 user folder",
                            ))
                            .clicked()
                        {
                            state.save_failed = save_user_snapshot(params).is_err();
                            state.save_notice_until = now + 2.0;
                        }

                        if now < state.save_notice_until {
                            ui.label(
                                RichText::new(if state.save_failed {
                                    "SAVE ERR"
                                } else {
                                    "SAVED"
                                })
                                .font(FontId::monospace(8.0))
                                .strong()
                                .color(if state.save_failed {
                                    theme.warning
                                } else {
                                    colors.diffusion
                                }),
                            );
                        }

                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            global_bypass_button(ui, setter, &params.global_bypass, theme);
                        });
                    });
                });
        },
    );
}
