use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2,
};

use crate::{dsp::eq::EqMode, params::Cc22Params};

use super::{
    theme::{ModuleColors, Theme},
    widgets::{eq_active, handle_float_drag, paint_colored_knob, set_param, value_string},
};

const EQ_DISPLAY_SAMPLE_RATE: f32 = 48_000.0;
const EQ_DISPLAY_MIN_HZ: f32 = 10.0;
const EQ_DISPLAY_MAX_HZ: f32 = 20_000.0;
const EQ_DISPLAY_DB_RANGE: f32 = 24.0;
const EQ_CURVE_POINTS: usize = 144;
const EQ_NODE_COUNT: usize = 5;
const EQ_WORKBENCH_HEIGHT: f32 = 166.0;
const EQ_CANVAS_HEIGHT: f32 = 112.0;
const EQ_INSPECTOR_WIDTH: f32 = 260.0;
const EQ_MIN_INSPECTOR_WIDTH: f32 = 230.0;
const EQ_MIN_CANVAS_WIDTH: f32 = 360.0;
const EQ_CONTENT_GAP: f32 = 12.0;
const EQ_RIGHT_MARGIN: f32 = 10.0;
const EQ_NODE_EDGE_INSET: f32 = 16.0;
const EQ_GAIN_MIN_DB: f32 = -18.0;
const EQ_GAIN_MAX_DB: f32 = 18.0;

pub(crate) fn eq_workbench(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
) {
    *selected_eq_band = (*selected_eq_band).min(EQ_NODE_COUNT - 1);
    let workbench_width = (ui.available_width() - EQ_RIGHT_MARGIN).max(0.0);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(workbench_width, EQ_WORKBENCH_HEIGHT),
        Sense::hover(),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(theme.paper)
                .stroke(Stroke::new(1.0, theme.card_edge))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width((workbench_width - 12.0).max(0.0));
                    ui.set_min_height(EQ_WORKBENCH_HEIGHT - 14.0);
                    eq_header(ui, setter, params, selected_eq_band, colors, theme);
                    ui.add_space(3.0);
                    eq_separator(ui, theme);
                    ui.add_space(5.0);
                    ui.horizontal_top(|ui| {
                        let available_width = ui.available_width();
                        let inspector_width = inspector_width_for(available_width);
                        let canvas_width =
                            (available_width - inspector_width - EQ_CONTENT_GAP - EQ_RIGHT_MARGIN)
                                .max(0.0);
                        ui.spacing_mut().item_spacing.x = EQ_CONTENT_GAP;
                        eq_canvas(
                            ui,
                            setter,
                            params,
                            selected_eq_band,
                            colors,
                            theme,
                            canvas_width,
                        );
                        eq_controls(
                            ui,
                            setter,
                            params,
                            selected_eq_band,
                            colors,
                            theme,
                            inspector_width,
                        );
                    });
                });
        },
    );
}

fn inspector_width_for(available_width: f32) -> f32 {
    let room_after_min_canvas = available_width - EQ_MIN_CANVAS_WIDTH - EQ_CONTENT_GAP;
    let max_without_overflow = (available_width - EQ_CONTENT_GAP - EQ_RIGHT_MARGIN).max(0.0);
    room_after_min_canvas
        .min(EQ_INSPECTOR_WIDTH)
        .max(EQ_MIN_INSPECTOR_WIDTH)
        .min(max_without_overflow)
}

fn eq_header(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            RichText::new("EQUALIZER")
                .font(FontId::monospace(13.0))
                .strong()
                .color(theme.text_dark),
        );
        ui.add_space(6.0);
        let active = eq_active(params);
        if eq_toggle_button(ui, active, colors.eq, theme).clicked() {
            if eq_active(params) {
                set_param(setter, &params.eq.bypass, true);
            } else {
                set_param(setter, &params.eq.mode, EqMode::On);
                set_param(setter, &params.eq.bypass, false);
            }
        }
        eq_toolbar_divider(ui, theme);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for band in 0..EQ_NODE_COUNT {
                let selected = band == *selected_eq_band;
                if eq_band_tab_button(
                    ui,
                    eq_band_tab_label(band),
                    selected,
                    eq_band_color(band, colors),
                    theme,
                )
                .clicked()
                {
                    *selected_eq_band = band;
                }
            }
        });
        eq_toolbar_divider(ui, theme);
        if eq_reset_button(ui, colors.master, theme).clicked() {
            reset_eq_to_defaults(setter, params);
            *selected_eq_band = 0;
        }
    });
}

fn eq_toggle_button(
    ui: &mut egui::Ui,
    active: bool,
    accent: Color32,
    theme: Theme,
) -> egui::Response {
    let label = if active { "ON" } else { "OFF" };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(42.0, 18.0), Sense::click());
    let fill = if active {
        accent
    } else if response.hovered() {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 44)
    } else {
        Color32::from_rgb(238, 232, 221)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(
            1.0,
            if active {
                accent.gamma_multiply(0.75)
            } else {
                theme.card_edge
            },
        ),
        StrokeKind::Inside,
    );
    let dot_center = Pos2::new(rect.left() + 8.0, rect.center().y);
    ui.painter().circle_filled(
        dot_center,
        2.5,
        if active {
            Color32::WHITE
        } else {
            theme.muted_dark
        },
    );
    ui.painter().text(
        Pos2::new(rect.center().x + 4.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::monospace(8.2),
        if active {
            Color32::WHITE
        } else {
            theme.text_dark.gamma_multiply(0.72)
        },
    );
    response
}

fn eq_band_tab_button(
    ui: &mut egui::Ui,
    label: &'static str,
    active: bool,
    accent: Color32,
    theme: Theme,
) -> egui::Response {
    let width = match label.len() {
        0..=2 => 30.0,
        3..=4 => 44.0,
        _ => 54.0,
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click());
    let fill = if active {
        accent
    } else if response.hovered() {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 58)
    } else {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 28)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.42)),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::monospace(8.5),
        if active {
            Color32::WHITE
        } else {
            theme.text_dark.gamma_multiply(0.78)
        },
    );
    response
}

fn eq_reset_button(ui: &mut egui::Ui, accent: Color32, theme: Theme) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(54.0, 18.0), Sense::click());
    let fill = if response.hovered() {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 34)
    } else {
        Color32::from_rgb(244, 239, 229)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, theme.card_edge),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "RESET",
        FontId::monospace(8.0),
        theme.muted_dark,
    );
    response
}

fn eq_toolbar_divider(ui: &mut egui::Ui, theme: Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 16.0), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.72)),
    );
}

fn eq_separator(ui: &mut egui::Ui, theme: Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.55)),
    );
}

pub(crate) fn eq_canvas(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
    canvas_width: f32,
) {
    let (rect, canvas_response) = ui.allocate_exact_size(
        Vec2::new(canvas_width, EQ_CANVAS_HEIGHT),
        Sense::click_and_drag(),
    );
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(4),
        Color32::from_rgb(248, 245, 239),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, Color32::from_rgb(218, 211, 199)),
        StrokeKind::Outside,
    );
    let plot_rect = egui::Rect::from_min_max(
        rect.min + Vec2::new(13.0, 8.0),
        rect.max - Vec2::new(13.0, 18.0),
    );

    for octave in 1..=10 {
        for multiple in 2..10 {
            let frequency = multiple as f32 * 10.0_f32.powi(octave);
            if !(EQ_DISPLAY_MIN_HZ..=EQ_DISPLAY_MAX_HZ).contains(&frequency) {
                continue;
            }
            let x = plot_rect.left() + plot_rect.width() * x_from_frequency(frequency);
            painter.line_segment(
                [
                    Pos2::new(x, plot_rect.top()),
                    Pos2::new(x, plot_rect.bottom()),
                ],
                Stroke::new(0.45, Color32::from_rgb(237, 232, 224)),
            );
        }
    }

    let frequency_labels = [
        (20.0, "20"),
        (50.0, "50"),
        (100.0, "100"),
        (200.0, "200"),
        (500.0, "500"),
        (1_000.0, "1k"),
        (2_000.0, "2k"),
        (5_000.0, "5k"),
        (10_000.0, "10k"),
        (20_000.0, "20k"),
    ];
    for (frequency, label) in frequency_labels {
        let x = plot_rect.left() + plot_rect.width() * x_from_frequency(frequency);
        painter.line_segment(
            [
                Pos2::new(x, plot_rect.top()),
                Pos2::new(x, plot_rect.bottom()),
            ],
            Stroke::new(0.7, Color32::from_rgb(222, 214, 203)),
        );
        let align = if frequency <= 20.0 {
            egui::Align2::LEFT_CENTER
        } else if frequency >= 20_000.0 {
            egui::Align2::RIGHT_CENTER
        } else {
            egui::Align2::CENTER_CENTER
        };
        painter.text(
            Pos2::new(x, rect.bottom() - 7.0),
            align,
            label,
            FontId::monospace(8.2),
            Color32::from_rgb(124, 116, 106),
        );
    }

    let gain_labels = [
        (12.0, "+12"),
        (9.0, "+9"),
        (6.0, "+6"),
        (3.0, "+3"),
        (0.0, "0"),
        (-3.0, "-3"),
        (-6.0, "-6"),
        (-9.0, "-9"),
        (-12.0, "-12"),
    ];
    for (gain_db, label) in gain_labels {
        let y = y_from_gain_db(plot_rect, gain_db);
        let is_zero = gain_db == 0.0;
        painter.line_segment(
            [
                Pos2::new(plot_rect.left(), y),
                Pos2::new(plot_rect.right(), y),
            ],
            Stroke::new(
                if is_zero { 1.2 } else { 0.45 },
                if is_zero {
                    Color32::from_rgb(194, 184, 170)
                } else {
                    Color32::from_rgb(234, 228, 219)
                },
            ),
        );
        painter.text(
            Pos2::new(plot_rect.right() - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            label,
            FontId::monospace(7.6),
            Color32::from_rgb(136, 127, 116),
        );
    }

    let eq_response = EqDisplayResponse::from_params(params, eq_active(params));
    let mut curve = Vec::with_capacity(EQ_CURVE_POINTS);
    for index in 0..EQ_CURVE_POINTS {
        let normalized = index as f32 / (EQ_CURVE_POINTS - 1) as f32;
        let frequency = frequency_from_x(normalized);
        let gain_db = eq_response.gain_db_at(frequency);
        curve.push(Pos2::new(
            plot_rect.left() + plot_rect.width() * normalized,
            y_from_gain_db(plot_rect, gain_db),
        ));
    }

    let curve_color = if eq_active(params) {
        Color32::from_rgb(255, 139, 42)
    } else {
        theme.muted
    };
    if eq_active(params) {
        let fill_color = Color32::from_rgba_premultiplied(255, 140, 50, 12);
        for segment in curve.windows(2) {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    segment[0],
                    segment[1],
                    Pos2::new(segment[1].x, plot_rect.bottom()),
                    Pos2::new(segment[0].x, plot_rect.bottom()),
                ],
                fill_color,
                Stroke::NONE,
            ));
        }
    }
    painter.add(egui::Shape::line(
        curve,
        Stroke::new(if eq_active(params) { 2.35 } else { 1.45 }, curve_color),
    ));

    let node_specs = eq_node_specs(params, colors, plot_rect);
    for node in node_specs {
        let hit_radius = if node.index == *selected_eq_band {
            13.0
        } else {
            11.0
        };
        let node_rect = egui::Rect::from_center_size(node.center, Vec2::splat(hit_radius * 2.0));
        let node_response = ui.interact(
            node_rect,
            ui.make_persistent_id(("eq_node", node.index)),
            Sense::click_and_drag(),
        );
        if node_response.clicked() {
            *selected_eq_band = node.index;
        }
        let (scroll_y, fine_scroll) =
            ui.input(|input| (input.raw_scroll_delta.y, input.modifiers.shift));
        if node_response.hovered() && scroll_y.abs() > 0.0 {
            *selected_eq_band = node.index;
            scroll_eq_band_width(setter, params, node.index, scroll_y, fine_scroll);
        }
        if node_response.double_clicked() {
            *selected_eq_band = node.index;
            reset_eq_band(setter, params, node.index);
        }
        if node_response.drag_started() {
            *selected_eq_band = node.index;
            begin_selected_band_setter(setter, params, node.index);
        }
        if node_response.dragged() {
            let fine = ui.input(|input| input.modifiers.shift);
            if fine {
                let delta = ui.input(|input| input.pointer.delta());
                offset_selected_band_from_delta(setter, params, node.index, rect, delta, 0.22);
            } else if let Some(pos) = ui.input(|input| input.pointer.interact_pos()) {
                set_selected_band_from_pos(setter, params, node.index, rect, pos);
            }
        }
        if node_response.drag_stopped() {
            end_selected_band_setter(setter, params, node.index);
        }

        let hovered = node_response.hovered();
        let selected = node.index == *selected_eq_band;
        let glow_radius = if selected {
            18.0
        } else if hovered {
            14.0
        } else {
            9.5
        };
        painter.circle_filled(
            node.center,
            glow_radius,
            Color32::from_rgba_premultiplied(
                node.color.r(),
                node.color.g(),
                node.color.b(),
                if selected {
                    78
                } else if hovered {
                    56
                } else {
                    20
                },
            ),
        );
        if selected {
            painter.circle_stroke(
                node.center,
                14.5,
                Stroke::new(2.2, Color32::from_rgba_premultiplied(255, 255, 255, 185)),
            );
            painter.circle_stroke(
                node.center,
                17.0,
                Stroke::new(
                    1.1,
                    Color32::from_rgba_premultiplied(
                        node.color.r(),
                        node.color.g(),
                        node.color.b(),
                        160,
                    ),
                ),
            );
        }
        painter.circle_filled(
            node.center,
            if selected {
                8.8
            } else if hovered {
                6.4
            } else {
                5.4
            },
            node.color,
        );
        painter.circle_stroke(
            node.center,
            if selected {
                11.0
            } else if hovered {
                8.5
            } else {
                7.0
            },
            Stroke::new(
                if selected { 2.0 } else { 1.5 },
                Color32::from_rgb(250, 247, 240),
            ),
        );
        if hovered || selected {
            let label = eq_node_label(node.index);
            let label_size = Vec2::new(34.0, 13.0);
            let label_pos = Pos2::new(
                node.center.x.clamp(
                    rect.left() + label_size.x * 0.5,
                    rect.right() - label_size.x * 0.5,
                ),
                (node.center.y - 18.0).clamp(
                    rect.top() + label_size.y * 0.5,
                    rect.bottom() - label_size.y * 0.5,
                ),
            );
            let label_rect = egui::Rect::from_center_size(label_pos, label_size);
            painter.rect_filled(
                label_rect,
                CornerRadius::same(4),
                Color32::from_rgba_premultiplied(38, 33, 28, 205),
            );
            painter.text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::monospace(8.0),
                Color32::from_rgb(248, 241, 226),
            );
        }
    }
    let _ = canvas_response;
}

fn eq_controls(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
    inspector_width: f32,
) {
    ui.allocate_ui(Vec2::new(inspector_width, EQ_CANVAS_HEIGHT), |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.label(
            RichText::new(format!(
                "{}  -  {}",
                eq_band_tab_label(*selected_eq_band),
                eq_band_shape_name(*selected_eq_band)
            ))
            .font(FontId::monospace(10.5))
            .strong()
            .color(theme.text_dark),
        );
        ui.label(
            RichText::new(eq_band_value_text(params, *selected_eq_band))
                .font(FontId::monospace(9.0))
                .strong()
                .color(theme.muted_dark),
        );
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let band_color = eq_band_color(*selected_eq_band, colors);
            eq_param_knob(
                ui,
                setter,
                selected_frequency_param(params, *selected_eq_band),
                "FREQ",
                "FREQ",
                band_color,
                theme,
            );
            if let Some(gain) = selected_gain_param(params, *selected_eq_band) {
                eq_param_knob(ui, setter, gain, "GAIN", "GAIN", band_color, theme);
            }
            if let Some(q) = selected_q_param(params, *selected_eq_band) {
                eq_param_knob(ui, setter, q, "Q", "Q", band_color, theme);
            }
        });
    });
}

fn eq_param_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    sublabel: &'static str,
    accent: Color32,
    theme: Theme,
) {
    ui.allocate_ui(Vec2::new(60.0, 70.0), |ui| {
        ui.vertical_centered(|ui| {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::splat(42.0), Sense::click_and_drag());
            handle_float_drag(ui, setter, param, &response);
            paint_colored_knob(
                ui,
                rect,
                param.unmodulated_normalized_value(),
                accent,
                theme,
            );
            ui.label(
                RichText::new(label)
                    .font(FontId::monospace(8.5))
                    .strong()
                    .color(theme.text_dark),
            );
            ui.label(
                RichText::new(sublabel)
                    .font(FontId::monospace(7.0))
                    .strong()
                    .color(theme.muted_dark),
            );
            response.on_hover_text(format!("{}: {}", param.name(), value_string(param)));
        });
    });
}

fn eq_band_shape_name(band: usize) -> &'static str {
    match band {
        0 => "LOW CUT",
        1 => "LOW SHELF",
        2 => "PEAK",
        3 => "HIGH SHELF",
        _ => "HIGH CUT",
    }
}

fn eq_band_tab_label(band: usize) -> &'static str {
    match band {
        0 => "LCUT",
        1 => "LOW",
        2 => "MID",
        3 => "HIGH",
        _ => "HCUT",
    }
}

fn eq_node_label(band: usize) -> &'static str {
    match band {
        0 => "LC",
        1 => "LS",
        2 => "MID",
        3 => "HS",
        _ => "HC",
    }
}

fn eq_band_value_text(params: &Cc22Params, band: usize) -> String {
    let freq = selected_frequency_param(params, band).value();
    let gain = selected_gain_param(params, band)
        .map(|param| format!(" / {:+.1} dB", param.value()))
        .unwrap_or_default();
    let q = selected_q_param(params, band)
        .map(|param| format!(" / Q {:.2}", param.value()))
        .unwrap_or_default();
    format!("{}{}{}", format_frequency(freq), gain, q)
}

fn format_frequency(frequency: f32) -> String {
    if frequency >= 1_000.0 {
        format!("{:.2} kHz", frequency / 1_000.0)
    } else {
        format!("{:.0} Hz", frequency)
    }
}

fn selected_frequency_param(params: &Cc22Params, band: usize) -> &FloatParam {
    match band {
        0 => &params.eq.low_cut_frequency,
        1 => &params.eq.low_shelf_frequency,
        2 => &params.eq.mid_frequency,
        3 => &params.eq.high_shelf_frequency,
        _ => &params.eq.high_cut_frequency,
    }
}

fn selected_gain_param(params: &Cc22Params, band: usize) -> Option<&FloatParam> {
    match band {
        1 => Some(&params.eq.low_shelf_gain),
        2 => Some(&params.eq.mid_gain),
        3 => Some(&params.eq.high_shelf_gain),
        _ => None,
    }
}

fn selected_q_param(params: &Cc22Params, band: usize) -> Option<&FloatParam> {
    match band {
        2 => Some(&params.eq.mid_q),
        _ => None,
    }
}

fn eq_band_color(band: usize, colors: ModuleColors) -> Color32 {
    match band {
        0 => Color32::from_rgb(255, 90, 55),
        1 => Color32::from_rgb(255, 150, 60),
        2 => Color32::from_rgb(255, 175, 65),
        3 => Color32::from_rgb(100, 210, 160),
        _ => colors.texture,
    }
}

#[derive(Clone, Copy)]
struct EqNodeSpec {
    index: usize,
    center: Pos2,
    color: Color32,
}

fn eq_node_specs(
    params: &Cc22Params,
    colors: ModuleColors,
    rect: egui::Rect,
) -> [EqNodeSpec; EQ_NODE_COUNT] {
    [
        EqNodeSpec {
            index: 0,
            center: node_pos(rect, params.eq.low_cut_frequency.value(), 0.0),
            color: eq_band_color(0, colors),
        },
        EqNodeSpec {
            index: 1,
            center: node_pos(
                rect,
                params.eq.low_shelf_frequency.value(),
                params.eq.low_shelf_gain.value(),
            ),
            color: eq_band_color(1, colors),
        },
        EqNodeSpec {
            index: 2,
            center: node_pos(
                rect,
                params.eq.mid_frequency.value(),
                params.eq.mid_gain.value(),
            ),
            color: eq_band_color(2, colors),
        },
        EqNodeSpec {
            index: 3,
            center: node_pos(
                rect,
                params.eq.high_shelf_frequency.value(),
                params.eq.high_shelf_gain.value(),
            ),
            color: eq_band_color(3, colors),
        },
        EqNodeSpec {
            index: 4,
            center: node_pos(rect, params.eq.high_cut_frequency.value(), 0.0),
            color: eq_band_color(4, colors),
        },
    ]
}

fn node_pos(rect: egui::Rect, frequency: f32, gain_db: f32) -> Pos2 {
    let pos = Pos2::new(
        rect.left() + rect.width() * x_from_frequency(frequency),
        y_from_real_gain_db(rect, gain_db),
    );
    Pos2::new(
        pos.x.clamp(
            rect.left() + EQ_NODE_EDGE_INSET,
            rect.right() - EQ_NODE_EDGE_INSET,
        ),
        pos.y.clamp(
            rect.top() + EQ_NODE_EDGE_INSET,
            rect.bottom() - EQ_NODE_EDGE_INSET,
        ),
    )
}

fn begin_selected_band_setter(setter: &ParamSetter<'_>, params: &Cc22Params, band: usize) {
    match band {
        0 => setter.begin_set_parameter(&params.eq.low_cut_frequency),
        1 => {
            setter.begin_set_parameter(&params.eq.low_shelf_frequency);
            setter.begin_set_parameter(&params.eq.low_shelf_gain);
        }
        2 => {
            setter.begin_set_parameter(&params.eq.mid_frequency);
            setter.begin_set_parameter(&params.eq.mid_gain);
        }
        3 => {
            setter.begin_set_parameter(&params.eq.high_shelf_frequency);
            setter.begin_set_parameter(&params.eq.high_shelf_gain);
        }
        _ => setter.begin_set_parameter(&params.eq.high_cut_frequency),
    }
}

fn end_selected_band_setter(setter: &ParamSetter<'_>, params: &Cc22Params, band: usize) {
    match band {
        0 => setter.end_set_parameter(&params.eq.low_cut_frequency),
        1 => {
            setter.end_set_parameter(&params.eq.low_shelf_frequency);
            setter.end_set_parameter(&params.eq.low_shelf_gain);
        }
        2 => {
            setter.end_set_parameter(&params.eq.mid_frequency);
            setter.end_set_parameter(&params.eq.mid_gain);
        }
        3 => {
            setter.end_set_parameter(&params.eq.high_shelf_frequency);
            setter.end_set_parameter(&params.eq.high_shelf_gain);
        }
        _ => setter.end_set_parameter(&params.eq.high_cut_frequency),
    }
}

fn set_selected_band_from_pos(
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    band: usize,
    rect: egui::Rect,
    pos: Pos2,
) {
    let x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let y = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    let frequency = frequency_from_x(x);
    let gain_db = gain_from_y(y);

    match band {
        0 => setter.set_parameter(&params.eq.low_cut_frequency, frequency.clamp(10.0, 500.0)),
        1 => {
            setter.set_parameter(&params.eq.low_shelf_frequency, frequency.clamp(10.0, 500.0));
            setter.set_parameter(&params.eq.low_shelf_gain, gain_db);
        }
        2 => {
            setter.set_parameter(&params.eq.mid_frequency, frequency.clamp(100.0, 8_000.0));
            setter.set_parameter(&params.eq.mid_gain, gain_db);
        }
        3 => {
            setter.set_parameter(
                &params.eq.high_shelf_frequency,
                frequency.clamp(1_000.0, 16_000.0),
            );
            setter.set_parameter(&params.eq.high_shelf_gain, gain_db);
        }
        _ => setter.set_parameter(
            &params.eq.high_cut_frequency,
            frequency.clamp(2_000.0, 20_000.0),
        ),
    }
}

fn offset_selected_band_from_delta(
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    band: usize,
    rect: egui::Rect,
    delta: Vec2,
    scale: f32,
) {
    let current_frequency = match band {
        0 => params.eq.low_cut_frequency.value(),
        1 => params.eq.low_shelf_frequency.value(),
        2 => params.eq.mid_frequency.value(),
        3 => params.eq.high_shelf_frequency.value(),
        _ => params.eq.high_cut_frequency.value(),
    };
    let current_gain = match band {
        1 => params.eq.low_shelf_gain.value(),
        2 => params.eq.mid_gain.value(),
        3 => params.eq.high_shelf_gain.value(),
        _ => 0.0,
    };

    let x =
        (x_from_frequency(current_frequency) + (delta.x / rect.width()) * scale).clamp(0.0, 1.0);
    let frequency = frequency_from_x(x);
    let gain_db = (current_gain
        - (delta.y / rect.height()) * (EQ_GAIN_MAX_DB - EQ_GAIN_MIN_DB) * scale)
        .clamp(EQ_GAIN_MIN_DB, EQ_GAIN_MAX_DB);

    match band {
        0 => setter.set_parameter(&params.eq.low_cut_frequency, frequency.clamp(10.0, 500.0)),
        1 => {
            setter.set_parameter(&params.eq.low_shelf_frequency, frequency.clamp(10.0, 500.0));
            setter.set_parameter(&params.eq.low_shelf_gain, gain_db);
        }
        2 => {
            setter.set_parameter(&params.eq.mid_frequency, frequency.clamp(100.0, 8_000.0));
            setter.set_parameter(&params.eq.mid_gain, gain_db);
        }
        3 => {
            setter.set_parameter(
                &params.eq.high_shelf_frequency,
                frequency.clamp(1_000.0, 16_000.0),
            );
            setter.set_parameter(&params.eq.high_shelf_gain, gain_db);
        }
        _ => setter.set_parameter(
            &params.eq.high_cut_frequency,
            frequency.clamp(2_000.0, 20_000.0),
        ),
    }
}

fn scroll_eq_band_width(
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    band: usize,
    scroll_y: f32,
    fine: bool,
) {
    let amount = scroll_y.abs() * if fine { 0.000_45 } else { 0.001_8 };
    let factor = amount.exp().clamp(1.001, 1.35);

    match band {
        0 => set_param(
            setter,
            &params.eq.low_cut_frequency,
            scroll_frequency(
                params.eq.low_cut_frequency.value(),
                scroll_y,
                factor,
                10.0,
                500.0,
                false,
            ),
        ),
        1 => set_param(
            setter,
            &params.eq.low_shelf_frequency,
            scroll_frequency(
                params.eq.low_shelf_frequency.value(),
                scroll_y,
                factor,
                10.0,
                500.0,
                false,
            ),
        ),
        2 => {
            let q = if scroll_y > 0.0 {
                params.eq.mid_q.value() * factor
            } else {
                params.eq.mid_q.value() / factor
            };
            set_param(setter, &params.eq.mid_q, q.clamp(0.1, 10.0));
        }
        3 => set_param(
            setter,
            &params.eq.high_shelf_frequency,
            scroll_frequency(
                params.eq.high_shelf_frequency.value(),
                scroll_y,
                factor,
                1_000.0,
                16_000.0,
                true,
            ),
        ),
        _ => set_param(
            setter,
            &params.eq.high_cut_frequency,
            scroll_frequency(
                params.eq.high_cut_frequency.value(),
                scroll_y,
                factor,
                2_000.0,
                20_000.0,
                true,
            ),
        ),
    }
}

fn scroll_frequency(
    current: f32,
    scroll_y: f32,
    factor: f32,
    min: f32,
    max: f32,
    high_side: bool,
) -> f32 {
    let closes_band = scroll_y > 0.0;
    let should_raise = if high_side { !closes_band } else { closes_band };
    if should_raise {
        current * factor
    } else {
        current / factor
    }
    .clamp(min, max)
}

fn reset_eq_band(setter: &ParamSetter<'_>, params: &Cc22Params, band: usize) {
    match band {
        0 => set_param(
            setter,
            &params.eq.low_cut_frequency,
            params.eq.low_cut_frequency.default_plain_value(),
        ),
        1 => {
            set_param(
                setter,
                &params.eq.low_shelf_frequency,
                params.eq.low_shelf_frequency.default_plain_value(),
            );
            set_param(
                setter,
                &params.eq.low_shelf_gain,
                params.eq.low_shelf_gain.default_plain_value(),
            );
        }
        2 => {
            set_param(
                setter,
                &params.eq.mid_frequency,
                params.eq.mid_frequency.default_plain_value(),
            );
            set_param(
                setter,
                &params.eq.mid_gain,
                params.eq.mid_gain.default_plain_value(),
            );
            set_param(
                setter,
                &params.eq.mid_q,
                params.eq.mid_q.default_plain_value(),
            );
        }
        3 => {
            set_param(
                setter,
                &params.eq.high_shelf_frequency,
                params.eq.high_shelf_frequency.default_plain_value(),
            );
            set_param(
                setter,
                &params.eq.high_shelf_gain,
                params.eq.high_shelf_gain.default_plain_value(),
            );
        }
        _ => set_param(
            setter,
            &params.eq.high_cut_frequency,
            params.eq.high_cut_frequency.default_plain_value(),
        ),
    }
}

pub(crate) fn reset_eq_to_defaults(setter: &ParamSetter<'_>, params: &Cc22Params) {
    set_param(
        setter,
        &params.eq.mode,
        params.eq.mode.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.bypass,
        params.eq.bypass.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.low_cut_frequency,
        params.eq.low_cut_frequency.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.low_shelf_gain,
        params.eq.low_shelf_gain.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.low_shelf_frequency,
        params.eq.low_shelf_frequency.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.mid_gain,
        params.eq.mid_gain.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.mid_frequency,
        params.eq.mid_frequency.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.mid_q,
        params.eq.mid_q.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.high_shelf_gain,
        params.eq.high_shelf_gain.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.high_shelf_frequency,
        params.eq.high_shelf_frequency.default_plain_value(),
    );
    set_param(
        setter,
        &params.eq.high_cut_frequency,
        params.eq.high_cut_frequency.default_plain_value(),
    );
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

fn y_from_real_gain_db(rect: egui::Rect, gain_db: f32) -> f32 {
    let normalized = 1.0
        - ((gain_db.clamp(EQ_GAIN_MIN_DB, EQ_GAIN_MAX_DB) - EQ_GAIN_MIN_DB)
            / (EQ_GAIN_MAX_DB - EQ_GAIN_MIN_DB));
    rect.top() + (rect.height() * normalized.clamp(0.0, 1.0))
}

fn gain_from_y(y: f32) -> f32 {
    (EQ_GAIN_MAX_DB - (y.clamp(0.0, 1.0) * (EQ_GAIN_MAX_DB - EQ_GAIN_MIN_DB)))
        .clamp(EQ_GAIN_MIN_DB, EQ_GAIN_MAX_DB)
}
