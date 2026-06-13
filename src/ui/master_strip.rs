use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Color32, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, StrokeKind, UiBuilder,
    Vec2,
};

use crate::params::Cc22Params;

use super::{
    meters::{clip_indicator, level_meter, MeterReading},
    theme::Theme,
    widgets::{handle_float_drag, paint_arc, value_string},
};

const MASTER_STRIP_HEIGHT: f32 = 58.0;
const MASTER_PADDING_X: i8 = 12;
const MASTER_PADDING_Y: i8 = 8;

pub(crate) fn master_strip(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    meter_reading: MeterReading,
    _accent: Color32,
    theme: Theme,
    available_width: f32,
) {
    let strip_width = available_width.max(0.0);
    let final_accent = Color32::from_rgb(82, 75, 64);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(strip_width, MASTER_STRIP_HEIGHT), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), Color32::from_rgb(231, 225, 212));
    painter.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, theme.card_edge),
        StrokeKind::Inside,
    );

    let inner = rect.shrink2(Vec2::new(
        f32::from(MASTER_PADDING_X),
        f32::from(MASTER_PADDING_Y),
    ));
    let clip = rect.intersect(ui.clip_rect());
    let mut x = inner.left();
    let y = inner.top();

    fixed_child_ui(ui, clip, x, y, 148.0, inner.height(), |ui| {
        master_label(ui, final_accent, theme);
    });
    x += 158.0;
    fixed_child_ui(ui, clip, x, y, 86.0, inner.height(), |ui| {
        master_meters(ui, meter_reading, final_accent, theme);
    });
    x += 92.0;
    fixed_child_ui(ui, clip, x, y + 13.0, 54.0, 18.0, |ui| {
        clip_indicator(
            ui,
            "CLIP",
            meter_reading.input.clipped || meter_reading.output.clipped,
            theme,
        );
    });
    x += 64.0;
    fixed_child_ui(ui, clip, x, y, 50.0, inner.height(), |ui| {
        master_strip_knob(ui, setter, &params.input_gain, "INPUT", final_accent, theme);
    });
    x += 58.0;
    fixed_child_ui(ui, clip, x, y, 54.0, inner.height(), |ui| {
        master_strip_knob(
            ui,
            setter,
            &params.output_gain,
            "OUTPUT",
            final_accent,
            theme,
        );
    });
    x += 62.0;
    fixed_child_ui(ui, clip, x, y, 58.0, inner.height(), |ui| {
        master_strip_knob(ui, setter, &params.dry_wet, "DRY/WET", final_accent, theme);
    });
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
    let rect = egui::Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.set_clip_rect(rect.intersect(clip));
            add_contents(ui);
        },
    );
}

fn master_label(ui: &mut egui::Ui, accent: Color32, theme: Theme) {
    ui.horizontal(|ui| {
        let (rail, _) = ui.allocate_exact_size(Vec2::new(3.0, 34.0), Sense::hover());
        ui.painter()
            .rect_filled(rail, CornerRadius::same(2), accent);
        ui.vertical(|ui| {
            ui.set_width(96.0);
            ui.label(
                RichText::new("MASTER / OUT")
                    .font(FontId::monospace(11.0))
                    .strong()
                    .color(theme.text_dark),
            );
            ui.label(
                RichText::new("FINAL")
                    .font(FontId::monospace(8.0))
                    .strong()
                    .color(theme.muted),
            );
        });
    });
}

fn master_meters(ui: &mut egui::Ui, meter_reading: MeterReading, accent: Color32, theme: Theme) {
    ui.allocate_ui(Vec2::new(86.0, 40.0), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            level_meter(ui, "IN", meter_reading.input, accent, theme);
            level_meter(ui, "OUT", meter_reading.output, accent, theme);
        });
    });
}

fn master_strip_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
) {
    ui.vertical_centered(|ui| {
        ui.set_min_width(46.0);
        let size = 28.0;
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
        handle_float_drag(ui, setter, param, &response);
        let center = rect.center();
        let normalized = param.unmodulated_normalized_value().clamp(0.0, 1.0);
        ui.painter()
            .circle_filled(center, 11.0, Color32::from_rgb(246, 241, 230));
        ui.painter().circle_stroke(
            center,
            12.0,
            Stroke::new(1.3, Color32::from_rgb(113, 104, 89)),
        );
        let start = core::f32::consts::PI * 0.75;
        let end = core::f32::consts::PI * 2.25;
        let current = start + ((end - start) * normalized);
        paint_arc(ui, center, 14.5, start, current, accent, 2.0);
        ui.painter().line_segment(
            [
                center,
                Pos2::new(
                    center.x + current.cos() * 7.2,
                    center.y + current.sin() * 7.2,
                ),
            ],
            Stroke::new(1.6, accent),
        );
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(8.0))
                .strong()
                .color(theme.muted_dark),
        );
        response.on_hover_text(format!("{}: {}", param.name(), value_string(param)));
    });
}
