use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Color32, CornerRadius, FontId, RichText, Stroke, Vec2};

use crate::params::Cc22Params;

use super::{
    meters::{clip_indicator, level_meter, MeterReading},
    theme::Theme,
    widgets::{colored_knob, mini_slider},
};

pub(crate) fn master_strip(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    meter_reading: MeterReading,
    accent: Color32,
    theme: Theme,
) {
    egui::Frame::new()
        .fill(Color32::from_rgb(32, 27, 22))
        .stroke(Stroke::new(1.0, Color32::from_rgb(74, 64, 54)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(220.0, 180.0));
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("MASTER")
                        .font(FontId::monospace(14.0))
                        .strong()
                        .color(Color32::from_rgb(245, 237, 218)),
                );
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    level_meter(ui, "IN", meter_reading.input, accent, theme);
                    level_meter(ui, "OUT", meter_reading.output, accent, theme);
                });
                clip_indicator(
                    ui,
                    "CLIP",
                    meter_reading.input.clipped || meter_reading.output.clipped,
                    theme,
                );
                ui.add_space(8.0);
                colored_knob(ui, setter, &params.input_gain, "INPUT", accent, theme, 45.0);
                colored_knob(
                    ui,
                    setter,
                    &params.output_gain,
                    "OUTPUT",
                    accent,
                    theme,
                    45.0,
                );
                mini_slider(ui, setter, &params.dry_wet, "DRY/WET", accent, theme);
            });
        });
}
