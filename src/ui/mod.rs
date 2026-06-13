use std::{sync::Arc, time::Duration};

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, CentralPanel, Color32, Vec2},
};

use crate::{meters::Meters, params::Cc22Params, Cc22};

mod eq_view;
mod master_strip;
mod meters;
mod module_card;
mod preset_bar;
mod signal_flow;
mod theme;
mod top_bar;
mod widgets;

use eq_view::reset_eq_to_defaults;
use meters::UiState;
use module_card::center_modules;
use preset_bar::bottom_macro_row;
use theme::{Look, ModuleColors, Theme, BASE_HEIGHT, BASE_WIDTH};
use top_bar::top_bar;

fn fixed_editor_size() -> (u32, u32) {
    (BASE_WIDTH.round() as u32, BASE_HEIGHT.round() as u32)
}

pub fn create_editor(
    params: Arc<Cc22Params>,
    meters: Arc<Meters>,
    _async_executor: AsyncExecutor<Cc22>,
) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    editor_state.set_requested_size(fixed_editor_size());

    create_egui_editor(
        editor_state.clone(),
        UiState::with_random_seed(0xCC22_2026),
        move |ctx, _state| {
            let theme = Theme::default();
            let mut style = (*ctx.style()).clone();
            style.spacing.item_spacing = Vec2::new(7.0, 6.0);
            style.spacing.window_margin = egui::Margin::same(0);
            style.visuals.window_fill = theme.background;
            style.visuals.panel_fill = theme.background;
            style.visuals.extreme_bg_color = theme.background;
            style.visuals.widgets.inactive.bg_fill = theme.paper;
            style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(250, 246, 235);
            style.visuals.widgets.active.bg_fill = Color32::from_rgb(226, 220, 207);
            style.visuals.selection.bg_fill = Color32::from_rgb(39, 40, 43);
            style.visuals.override_text_color = Some(theme.text_dark);
            ctx.set_style(style);
        },
        {
            move |ctx, setter, state| {
                let theme = Theme::default();
                let colors = ModuleColors::default();
                let look = Look { colors, theme };
                let now = ctx.input(|input| input.time);
                ctx.request_repaint_after(Duration::from_millis(33));

                if !state.eq_open_reset_done {
                    reset_eq_to_defaults(setter, &params);
                    state.selected_eq_band = 0;
                    state.eq_open_reset_done = true;
                }

                CentralPanel::default().show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(theme.background)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                top_bar(ui, setter, state, &params, colors, theme);
                                ui.add_space(2.0);
                                center_modules(ui, setter, state, &params, &meters, now, look);
                                ui.add_space(2.0);
                                bottom_macro_row(ui, setter, state, &params, colors, theme);
                            });
                        });
                });
            }
        },
    )
}
