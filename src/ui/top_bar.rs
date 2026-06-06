use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Align, CornerRadius, FontId, RichText};

use crate::params::Cc22Params;

use super::{
    meters::UiState,
    preset_bar::{next_preset, preset_selector, previous_preset, randomize_controls},
    theme::{ModuleColors, Theme},
    widgets::{
        brand_mark, colored_knob, compact_button, disabled_compact_button, global_bypass_button,
        rounded_panel,
    },
};

pub(crate) fn top_bar(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    rounded_panel(
        ui,
        theme.paper_alt,
        theme.text_dark,
        CornerRadius::same(18),
        |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        brand_mark(ui, colors, theme);
                        ui.label(
                            RichText::new("CC-22")
                                .font(FontId::proportional(29.0))
                                .strong()
                                .color(theme.text_dark),
                        );
                    });
                    ui.label(
                        RichText::new("MODULAR COLOR PROCESSOR")
                            .font(FontId::monospace(10.0))
                            .color(theme.muted_dark),
                    );
                });

                ui.add_space(18.0);
                preset_selector(ui, setter, state, params, theme);

                if compact_button(ui, "PREV", theme, colors.master).clicked() {
                    previous_preset(setter, state, params);
                }
                if compact_button(ui, "NEXT", theme, colors.master).clicked() {
                    next_preset(setter, state, params);
                }
                let _ = disabled_compact_button(ui, "SAVE", theme)
                    .on_hover_text("Placeholder: preset saving is handled by the host for now");
                if compact_button(ui, "RND", theme, colors.texture)
                    .on_hover_text("Randomize musical control values")
                    .clicked()
                {
                    randomize_controls(setter, state, params);
                }
                if compact_button(ui, "SET", theme, colors.eq)
                    .on_hover_text("Open settings")
                    .clicked()
                {
                    state.settings_open = !state.settings_open;
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
                        52.0,
                    );
                });
            });
        },
    );
}
