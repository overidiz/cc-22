use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, RichText, Sense, Stroke, UiBuilder, Vec2,
};

use crate::params::Cc22Params;

use super::{
    meters::{clip_indicator, level_meter, MeterReading},
    theme::Theme,
    widgets::{mini_slider, small_strip_knob},
};

const MASTER_STRIP_WIDTH: f32 = 925.0;
const MASTER_STRIP_HEIGHT: f32 = 62.0;

pub(crate) fn master_strip(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    meter_reading: MeterReading,
    accent: Color32,
    theme: Theme,
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(MASTER_STRIP_WIDTH, MASTER_STRIP_HEIGHT),
        Sense::hover(),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(32, 27, 22))
                .stroke(Stroke::new(1.0, Color32::from_rgb(74, 64, 54)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(7))
                .show(ui, |ui| {
                    ui.set_width(MASTER_STRIP_WIDTH - 14.0);
                    ui.set_min_height(48.0);
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.set_width(118.0);
                            ui.label(
                                RichText::new("MASTER")
                                    .font(FontId::monospace(12.0))
                                    .strong()
                                    .color(Color32::from_rgb(245, 237, 218)),
                            );
                            clip_indicator(
                                ui,
                                "CLIP",
                                meter_reading.input.clipped || meter_reading.output.clipped,
                                theme,
                            );
                        });

                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.set_width(50.0);
                            level_meter(ui, "IN", meter_reading.input, accent, theme);
                            level_meter(ui, "OUT", meter_reading.output, accent, theme);
                        });
                        ui.add_space(22.0);
                        small_strip_knob(ui, setter, &params.input_gain, "INPUT", accent, theme);
                        small_strip_knob(ui, setter, &params.output_gain, "OUTPUT", accent, theme);
                        ui.add_space(26.0);
                        ui.vertical(|ui| {
                            ui.set_width(320.0);
                            mini_slider(ui, setter, &params.dry_wet, "DRY/WET", accent, theme);
                        });
                    });
                });
        },
    );
}
