use crate::meters::Meters;
use nih_plug_egui::egui::{
    self, Color32, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, StrokeKind, Vec2,
};

use super::theme::Theme;

#[derive(Default)]
pub(crate) struct UiState {
    pub(crate) selected_preset: usize,
    pub(crate) random_seed: u32,
    input_meter: MeterBallistics,
    output_meter: MeterBallistics,
    input_clip_events: u32,
    output_clip_events: u32,
    input_clip_until: f64,
    output_clip_until: f64,
    last_meter_time: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MeterReading {
    pub(crate) input: MeterSnapshot,
    pub(crate) output: MeterSnapshot,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MeterSnapshot {
    level: f32,
    pub(crate) clipped: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct MeterBallistics {
    level: f32,
}

impl UiState {
    pub(crate) fn with_random_seed(random_seed: u32) -> Self {
        Self {
            random_seed,
            ..Self::default()
        }
    }

    pub(crate) fn next_meter_reading(&mut self, meters: &Meters, now: f64) -> MeterReading {
        let dt = self
            .last_meter_time
            .map(|last| (now - last).clamp(1.0 / 240.0, 0.1) as f32)
            .unwrap_or(1.0 / 60.0);
        self.last_meter_time = Some(now);

        let input_peak = meters.take_input_peak();
        let output_peak = meters.take_output_peak();
        let input_clip_events = meters.input_clip_events();
        let output_clip_events = meters.output_clip_events();

        if input_clip_events != self.input_clip_events {
            self.input_clip_events = input_clip_events;
            self.input_clip_until = now + 0.75;
        }

        if output_clip_events != self.output_clip_events {
            self.output_clip_events = output_clip_events;
            self.output_clip_until = now + 0.75;
        }

        MeterReading {
            input: MeterSnapshot {
                level: self.input_meter.next(peak_to_meter_level(input_peak), dt),
                clipped: now < self.input_clip_until,
            },
            output: MeterSnapshot {
                level: self.output_meter.next(peak_to_meter_level(output_peak), dt),
                clipped: now < self.output_clip_until,
            },
        }
    }
}

impl MeterBallistics {
    fn next(&mut self, target: f32, dt: f32) -> f32 {
        let time_constant = if target > self.level { 0.012 } else { 0.320 };
        let coefficient = 1.0 - (-dt / time_constant).exp();
        self.level += (target - self.level) * coefficient.clamp(0.0, 1.0);
        self.level = self.level.clamp(0.0, 1.0);
        self.level
    }
}

pub(crate) fn level_meter(
    ui: &mut egui::Ui,
    label: &'static str,
    reading: MeterSnapshot,
    accent: Color32,
    theme: Theme,
) {
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(9.0))
                .color(theme.muted),
        );
        let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 62.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_rgb(10, 11, 13));
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, theme.card_edge),
            StrokeKind::Outside,
        );

        let fill_height = rect.height() * reading.level;
        let fill_rect = egui::Rect::from_min_max(
            Pos2::new(rect.left() + 3.0, rect.bottom() - fill_height + 3.0),
            Pos2::new(rect.right() - 3.0, rect.bottom() - 3.0),
        );
        ui.painter().rect_filled(
            fill_rect,
            CornerRadius::same(3),
            if reading.clipped {
                theme.warning
            } else {
                accent
            },
        );
    });
}

pub(crate) fn clip_indicator(ui: &mut egui::Ui, label: &'static str, clipped: bool, theme: Theme) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter().circle_filled(
            rect.center(),
            4.5,
            if clipped {
                theme.warning
            } else {
                theme.card_edge
            },
        );
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(9.0))
                .color(if clipped { theme.warning } else { theme.muted }),
        );
    });
}

fn peak_to_meter_level(peak: f32) -> f32 {
    ((linear_to_db(peak) + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn linear_to_db(peak: f32) -> f32 {
    20.0 * peak.max(0.000_001).log10()
}
