use std::{sync::Arc, time::Duration};

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Color32, Vec2},
    resizable_window::ResizableWindow,
};

use crate::{meters::Meters, params::Cc22Params, Cc22};

mod eq_view;
mod master_strip;
mod meters;
mod module_card;
mod preset_bar;
mod theme;
mod top_bar;
mod widgets;

use meters::UiState;
use module_card::center_modules;
use preset_bar::bottom_macro_row;
use theme::{Look, ModuleColors, Theme};
use top_bar::top_bar;

pub fn create_editor(
    params: Arc<Cc22Params>,
    meters: Arc<Meters>,
    _async_executor: AsyncExecutor<Cc22>,
) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();

    create_egui_editor(
        editor_state.clone(),
        UiState::with_random_seed(0xCC22_2026),
        |ctx, _state| {
            let theme = Theme::default();
            let mut style = (*ctx.style()).clone();
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
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
        move |ctx, setter, state| {
            let theme = Theme::default();
            let colors = ModuleColors::default();
            let look = Look { colors, theme };
            let now = ctx.input(|input| input.time);
            ctx.request_repaint_after(Duration::from_millis(33));

            ResizableWindow::new("CC-22")
                .min_size(Vec2::new(960.0, 720.0))
                .show(ctx, editor_state.as_ref(), |ui| {
                    egui::Frame::new()
                        .fill(theme.background)
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                top_bar(ui, setter, state, &params, colors, theme);
                                ui.add_space(8.0);
                                center_modules(ui, setter, state, &params, &meters, now, look);
                                ui.add_space(8.0);
                                bottom_macro_row(ui, setter, state, &params, colors, theme);
                            });
                        });
                });
        },
    )
}
