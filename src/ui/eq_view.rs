use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2,
};

use crate::params::Cc22Params;

use super::{
    theme::{ModuleColors, Theme},
    widgets::{colored_knob, draw_eq_icon, eq_active, eq_mode_selector, value_string},
};

const EQ_DISPLAY_SAMPLE_RATE: f32 = 48_000.0;
const EQ_DISPLAY_MIN_HZ: f32 = 20.0;
const EQ_DISPLAY_MAX_HZ: f32 = 20_000.0;
const EQ_DISPLAY_DB_RANGE: f32 = 24.0;
const EQ_CURVE_POINTS: usize = 96;

pub(crate) fn eq_workbench(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    let eq_on = eq_active(params);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(690.0, 166.0), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(theme.paper)
                .stroke(Stroke::new(1.0, theme.card_edge))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(670.0, 146.0));
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("EQUALIZER")
                                        .font(FontId::monospace(14.0))
                                        .strong()
                                        .color(theme.text_dark),
                                );
                                ui.label(
                                    RichText::new(if eq_on { "5-BAND  ON" } else { "5-BAND  OFF" })
                                        .font(FontId::monospace(9.0))
                                        .strong()
                                        .color(theme.muted_dark),
                                );
                                ui.add_space(14.0);
                                eq_mode_selector(ui, setter, &params.eq.mode, colors.eq, theme);
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    let (icon_rect, _) = ui
                                        .allocate_exact_size(Vec2::new(38.0, 24.0), Sense::hover());
                                    draw_eq_icon(
                                        ui,
                                        icon_rect,
                                        if eq_on { colors.eq } else { theme.muted },
                                    );
                                });
                            });
                            ui.add_space(3.0);
                            eq_canvas(ui, params, colors, theme);
                        });

                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            band_tabs(ui, colors, theme);
                            ui.add_space(5.0);
                            ui.label(
                                RichText::new("BAND 3  PEAK")
                                    .font(FontId::monospace(11.0))
                                    .strong()
                                    .color(theme.text_dark),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} / {} / Q {}",
                                    value_string(&params.eq.mid_frequency),
                                    value_string(&params.eq.mid_gain),
                                    value_string(&params.eq.mid_q)
                                ))
                                .font(FontId::monospace(9.0))
                                .color(theme.muted_dark),
                            );
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                colored_knob(
                                    ui,
                                    setter,
                                    &params.eq.mid_frequency,
                                    "FREQ",
                                    colors.eq,
                                    theme,
                                    43.0,
                                );
                                colored_knob(
                                    ui,
                                    setter,
                                    &params.eq.mid_gain,
                                    "GAIN",
                                    colors.eq,
                                    theme,
                                    43.0,
                                );
                                colored_knob(
                                    ui,
                                    setter,
                                    &params.eq.mid_q,
                                    "Q",
                                    colors.eq,
                                    theme,
                                    43.0,
                                );
                            });
                        });
                    });
                });
        },
    );
}

pub(crate) fn eq_canvas(
    ui: &mut egui::Ui,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(430.0, 114.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(2),
        Color32::from_rgb(244, 240, 230),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, theme.card_edge),
        StrokeKind::Outside,
    );

    for index in 0..18 {
        let x = rect.left() + rect.width() * index as f32 / 17.0;
        let color = if index % 3 == 0 {
            Color32::from_rgb(207, 200, 186)
        } else {
            Color32::from_rgb(225, 219, 206)
        };
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(0.7, color),
        );
    }
    for index in 0..11 {
        let y = rect.top() + rect.height() * index as f32 / 10.0;
        let color = if index == 5 {
            Color32::from_rgb(170, 162, 148)
        } else {
            Color32::from_rgb(225, 219, 206)
        };
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(if index == 5 { 1.0 } else { 0.7 }, color),
        );
    }

    let response = EqDisplayResponse::from_params(params, eq_active(params));
    let mut curve = Vec::with_capacity(EQ_CURVE_POINTS);
    for index in 0..EQ_CURVE_POINTS {
        let normalized = index as f32 / (EQ_CURVE_POINTS - 1) as f32;
        let frequency = frequency_from_x(normalized);
        let gain_db = response.gain_db_at(frequency);
        curve.push(Pos2::new(
            rect.left() + rect.width() * normalized,
            y_from_gain_db(rect, gain_db),
        ));
    }

    let curve_color = if eq_active(params) {
        colors.eq
    } else {
        theme.muted
    };
    painter.add(egui::Shape::line(curve, Stroke::new(2.0, curve_color)));

    let bands = [
        (
            params.eq.low_cut_frequency.value(),
            response.gain_db_at(params.eq.low_cut_frequency.value()),
            colors.character,
            "LC",
        ),
        (
            params.eq.low_shelf_frequency.value(),
            response.gain_db_at(params.eq.low_shelf_frequency.value()),
            Color32::from_rgb(220, 188, 78),
            "LS",
        ),
        (
            params.eq.mid_frequency.value(),
            response.gain_db_at(params.eq.mid_frequency.value()),
            colors.eq,
            "MID",
        ),
        (
            params.eq.high_shelf_frequency.value(),
            response.gain_db_at(params.eq.high_shelf_frequency.value()),
            colors.diffusion,
            "HS",
        ),
        (
            params.eq.high_cut_frequency.value(),
            response.gain_db_at(params.eq.high_cut_frequency.value()),
            colors.texture,
            "HC",
        ),
    ];

    for (frequency, gain_db, color, label) in bands {
        let center = Pos2::new(
            rect.left() + rect.width() * x_from_frequency(frequency),
            y_from_gain_db(rect, gain_db),
        );
        painter.circle_filled(center, if label == "MID" { 7.5 } else { 5.0 }, color);
        painter.circle_stroke(
            center,
            if label == "MID" { 10.0 } else { 6.5 },
            Stroke::new(1.2, theme.paper),
        );
        painter.text(
            center + Vec2::new(0.0, -17.0),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::monospace(8.0),
            theme.muted_dark,
        );
    }
}

pub(crate) fn band_tabs(ui: &mut egui::Ui, colors: ModuleColors, theme: Theme) {
    ui.horizontal(|ui| {
        let band_colors = [
            colors.character,
            Color32::from_rgb(220, 188, 78),
            colors.eq,
            colors.diffusion,
            colors.texture,
        ];
        let labels = ["LCUT", "LOW", "MID", "HIGH", "HCUT"];
        for (index, (label, color)) in labels.into_iter().zip(band_colors).enumerate() {
            let selected = index == 2;
            ui.add_enabled(
                false,
                egui::Button::new(
                    RichText::new(label)
                        .font(FontId::monospace(8.0))
                        .strong()
                        .color(if selected {
                            Color32::WHITE
                        } else {
                            theme.text_dark
                        }),
                )
                .fill(if selected {
                    color
                } else {
                    Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 82)
                })
                .stroke(Stroke::new(1.0, theme.card_edge))
                .corner_radius(CornerRadius::same(6))
                .min_size(Vec2::new(42.0, 20.0)),
            )
            .on_hover_text("Placeholder: band selection is visual only; Mid controls are active");
        }
    });
}

#[derive(Debug, Clone, Copy)]
struct EqDisplayResponse {
    active: bool,
    filters: [DisplayBiquad; 5],
}

#[derive(Debug, Clone, Copy)]
struct DisplayBiquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl EqDisplayResponse {
    fn from_params(params: &Cc22Params, active: bool) -> Self {
        Self {
            active,
            filters: [
                DisplayBiquad::high_pass(
                    params.eq.low_cut_frequency.value(),
                    0.707,
                    EQ_DISPLAY_SAMPLE_RATE,
                ),
                DisplayBiquad::low_shelf(
                    params.eq.low_shelf_frequency.value(),
                    params.eq.low_shelf_gain.value(),
                    EQ_DISPLAY_SAMPLE_RATE,
                ),
                DisplayBiquad::peaking(
                    params.eq.mid_frequency.value(),
                    params.eq.mid_gain.value(),
                    params.eq.mid_q.value(),
                    EQ_DISPLAY_SAMPLE_RATE,
                ),
                DisplayBiquad::high_shelf(
                    params.eq.high_shelf_frequency.value(),
                    params.eq.high_shelf_gain.value(),
                    EQ_DISPLAY_SAMPLE_RATE,
                ),
                DisplayBiquad::low_pass(
                    params.eq.high_cut_frequency.value(),
                    0.707,
                    EQ_DISPLAY_SAMPLE_RATE,
                ),
            ],
        }
    }

    fn gain_db_at(self, frequency: f32) -> f32 {
        if !self.active {
            return 0.0;
        }

        let magnitude = self
            .filters
            .iter()
            .map(|filter| filter.magnitude_at(frequency, EQ_DISPLAY_SAMPLE_RATE))
            .product::<f32>()
            .max(0.000_001);

        (20.0 * magnitude.log10()).clamp(-EQ_DISPLAY_DB_RANGE, EQ_DISPLAY_DB_RANGE)
    }
}

impl DisplayBiquad {
    fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    fn high_pass(frequency: f32, q: f32, sample_rate: f32) -> Self {
        let omega = omega(frequency, sample_rate);
        let alpha = omega.sin() / (2.0 * q.max(0.1));
        let cos = omega.cos();
        let b0 = (1.0 + cos) * 0.5;
        let b1 = -(1.0 + cos);
        let b2 = (1.0 + cos) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;

        normalize_biquad(b0, b1, b2, a0, a1, a2)
    }

    fn low_pass(frequency: f32, q: f32, sample_rate: f32) -> Self {
        let omega = omega(frequency, sample_rate);
        let alpha = omega.sin() / (2.0 * q.max(0.1));
        let cos = omega.cos();
        let b0 = (1.0 - cos) * 0.5;
        let b1 = 1.0 - cos;
        let b2 = (1.0 - cos) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;

        normalize_biquad(b0, b1, b2, a0, a1, a2)
    }

    fn peaking(frequency: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        if gain_db.abs() < 0.000_1 {
            return Self::identity();
        }

        let omega = omega(frequency, sample_rate);
        let alpha = omega.sin() / (2.0 * q.clamp(0.1, 10.0));
        let cos = omega.cos();
        let a = 10.0_f32.powf(gain_db / 40.0);
        let b0 = 1.0 + (alpha * a);
        let b1 = -2.0 * cos;
        let b2 = 1.0 - (alpha * a);
        let a0 = 1.0 + (alpha / a);
        let a1 = -2.0 * cos;
        let a2 = 1.0 - (alpha / a);

        normalize_biquad(b0, b1, b2, a0, a1, a2)
    }

    fn low_shelf(frequency: f32, gain_db: f32, sample_rate: f32) -> Self {
        shelf_biquad(frequency, gain_db, sample_rate, false)
    }

    fn high_shelf(frequency: f32, gain_db: f32, sample_rate: f32) -> Self {
        shelf_biquad(frequency, gain_db, sample_rate, true)
    }

    fn magnitude_at(self, frequency: f32, sample_rate: f32) -> f32 {
        let omega = omega(frequency, sample_rate);
        let z1_re = omega.cos();
        let z1_im = -omega.sin();
        let z2_re = (2.0 * omega).cos();
        let z2_im = -(2.0 * omega).sin();

        let numerator_re = self.b0 + (self.b1 * z1_re) + (self.b2 * z2_re);
        let numerator_im = (self.b1 * z1_im) + (self.b2 * z2_im);
        let denominator_re = 1.0 + (self.a1 * z1_re) + (self.a2 * z2_re);
        let denominator_im = (self.a1 * z1_im) + (self.a2 * z2_im);

        let numerator = (numerator_re * numerator_re) + (numerator_im * numerator_im);
        let denominator =
            ((denominator_re * denominator_re) + (denominator_im * denominator_im)).max(0.000_001);

        (numerator / denominator).sqrt()
    }

    fn sanitized(self) -> Self {
        if self.b0.is_finite()
            && self.b1.is_finite()
            && self.b2.is_finite()
            && self.a1.is_finite()
            && self.a2.is_finite()
        {
            self
        } else {
            Self::identity()
        }
    }
}

fn shelf_biquad(frequency: f32, gain_db: f32, sample_rate: f32, high: bool) -> DisplayBiquad {
    if gain_db.abs() < 0.000_1 {
        return DisplayBiquad::identity();
    }

    let omega = omega(frequency, sample_rate);
    let sin = omega.sin();
    let cos = omega.cos();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let sqrt_a = a.sqrt();
    let alpha = sin * core::f32::consts::FRAC_1_SQRT_2;
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let (b0, b1, b2, a0, a1, a2) = if high {
        (
            a * ((a + 1.0) + ((a - 1.0) * cos) + two_sqrt_a_alpha),
            -2.0 * a * ((a - 1.0) + ((a + 1.0) * cos)),
            a * ((a + 1.0) + ((a - 1.0) * cos) - two_sqrt_a_alpha),
            (a + 1.0) - ((a - 1.0) * cos) + two_sqrt_a_alpha,
            2.0 * ((a - 1.0) - ((a + 1.0) * cos)),
            (a + 1.0) - ((a - 1.0) * cos) - two_sqrt_a_alpha,
        )
    } else {
        (
            a * ((a + 1.0) - ((a - 1.0) * cos) + two_sqrt_a_alpha),
            2.0 * a * ((a - 1.0) - ((a + 1.0) * cos)),
            a * ((a + 1.0) - ((a - 1.0) * cos) - two_sqrt_a_alpha),
            (a + 1.0) + ((a - 1.0) * cos) + two_sqrt_a_alpha,
            -2.0 * ((a - 1.0) + ((a + 1.0) * cos)),
            (a + 1.0) + ((a - 1.0) * cos) - two_sqrt_a_alpha,
        )
    };

    normalize_biquad(b0, b1, b2, a0, a1, a2)
}

fn normalize_biquad(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> DisplayBiquad {
    let a0 = if a0.abs() < 0.000_001 { 1.0 } else { a0 };
    DisplayBiquad {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
    .sanitized()
}

fn omega(frequency: f32, sample_rate: f32) -> f32 {
    core::f32::consts::TAU * clamp_display_frequency(frequency) / sample_rate.max(1.0)
}

fn clamp_display_frequency(frequency: f32) -> f32 {
    frequency.clamp(
        EQ_DISPLAY_MIN_HZ,
        EQ_DISPLAY_MAX_HZ.min(EQ_DISPLAY_SAMPLE_RATE * 0.49),
    )
}

fn frequency_from_x(x: f32) -> f32 {
    let min_log = EQ_DISPLAY_MIN_HZ.log10();
    let max_log = EQ_DISPLAY_MAX_HZ.log10();
    10.0_f32.powf(min_log + ((max_log - min_log) * x.clamp(0.0, 1.0)))
}

fn x_from_frequency(frequency: f32) -> f32 {
    let min_log = EQ_DISPLAY_MIN_HZ.log10();
    let max_log = EQ_DISPLAY_MAX_HZ.log10();
    ((clamp_display_frequency(frequency).log10() - min_log) / (max_log - min_log)).clamp(0.0, 1.0)
}

fn y_from_gain_db(rect: egui::Rect, gain_db: f32) -> f32 {
    let normalized = 0.5
        - (gain_db.clamp(-EQ_DISPLAY_DB_RANGE, EQ_DISPLAY_DB_RANGE) / (EQ_DISPLAY_DB_RANGE * 2.0));
    rect.top() + (rect.height() * normalized)
}
