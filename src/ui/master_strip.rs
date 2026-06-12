use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, RichText, Sense, Stroke, UiBuilder, Vec2,
};

use crate::params::Cc22Params;

use super::{
    meters::{clip_indicator, level_meter, MeterReading},
    theme::Theme,
    widgets::small_strip_knob,
};

const MASTER_STRIP_HEIGHT: f32 = 78.0;
const MASTER_RIGHT_MARGIN: f32 = 10.0;

pub(crate) fn master_strip(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    meter_reading: MeterReading,
    accent: Color32,
    theme: Theme,
) {
    let strip_width = (ui.available_width() - MASTER_RIGHT_MARGIN).max(0.0);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(strip_width, MASTER_STRIP_HEIGHT), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(38, 32, 26))
                .stroke(Stroke::new(1.0, Color32::from_rgb(78, 68, 58)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width((strip_width - 20.0).max(0.0));
                    ui.set_min_height(MASTER_STRIP_HEIGHT - 16.0);
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        ui.vertical(|ui| {
                            ui.set_width(82.0);
                            ui.label(
                                RichText::new("MASTER")
                                    .font(FontId::monospace(12.0))
                                    .strong()
                                    .color(Color32::from_rgb(245, 237, 218)),
                            );
                        });

                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            level_meter(ui, "IN", meter_reading.input, accent, theme);
                            level_meter(ui, "OUT", meter_reading.output, accent, theme);
                            clip_indicator(
                                ui,
                                "CLIP",
                                meter_reading.input.clipped || meter_reading.output.clipped,
                                theme,
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            small_strip_knob(ui, setter, &params.dry_wet, "DRY/WET", accent, theme);
                            small_strip_knob(
                                ui,
                                setter,
                                &params.output_gain,
                                "OUTPUT",
                                accent,
                                theme,
                            );
                            small_strip_knob(
                                ui,
                                setter,
                                &params.input_gain,
                                "INPUT",
                                accent,
                                theme,
                            );
                        });
                    });
                });
        },
    );
}
