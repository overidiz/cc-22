use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2,
};

use crate::{dsp::eq::EqMode, params::Cc22Params};

use super::{
    theme::{ModuleColors, Theme, FONT_HINT, FONT_SECONDARY, FONT_VALUE_LABEL},
    widgets::{colored_knob, eq_active, set_param, value_string},
};

const EQ_DISPLAY_SAMPLE_RATE: f32 = 48_000.0;
const EQ_DISPLAY_MIN_HZ: f32 = 20.0;
const EQ_DISPLAY_MAX_HZ: f32 = 20_000.0;
const EQ_DISPLAY_DB_RANGE: f32 = 24.0;
const EQ_CURVE_POINTS: usize = 144;
const EQ_WORKBENCH_WIDTH: f32 = 690.0;
const EQ_WORKBENCH_HEIGHT: f32 = 166.0;
const EQ_CANVAS_WIDTH: f32 = 448.0;
const EQ_CANVAS_HEIGHT: f32 = 112.0;
const EQ_INSPECTOR_WIDTH: f32 = 220.0;
const EQ_NODE_EDGE_INSET: f32 = 12.0;
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
    *selected_eq_band = (*selected_eq_band).min(4);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(EQ_WORKBENCH_WIDTH, EQ_WORKBENCH_HEIGHT),
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
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(7))
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(
                        EQ_WORKBENCH_WIDTH - 14.0,
                        EQ_WORKBENCH_HEIGHT - 14.0,
                    ));
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            eq_toolbar(ui, setter, params, selected_eq_band, colors, theme);
                            ui.add_space(2.0);
                            eq_canvas(ui, setter, params, selected_eq_band, colors, theme);
                        });

                        ui.add_space(8.0);
                        band_inspector(ui, setter, params, selected_eq_band, colors, theme);
                    });
                });
        },
    );
}

fn eq_toolbar(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new("EQUALIZER")
                .font(FontId::monospace(14.0))
                .strong()
                .color(theme.text_dark),
        );

        let mode_on = params.eq.mode.value() == EqMode::On;
        let toggle_label = if mode_on { "ON" } else { "OFF" };
        if toolbar_button(
            ui,
            toggle_label,
            Vec2::new(32.0, 16.0),
            mode_on,
            colors.eq,
            theme,
        )
        .clicked()
        {
            set_param(
                setter,
                &params.eq.mode,
                if mode_on { EqMode::Off } else { EqMode::On },
            );
        }

        ui.add_space(2.0);
        band_tabs(ui, selected_eq_band, colors, theme);
        ui.add_space(2.0);

        if toolbar_button(ui, "RESET", Vec2::new(45.0, 16.0), false, colors.eq, theme).clicked() {
            reset_eq_to_defaults(setter, params);
        }
    });
}

pub(crate) fn eq_canvas(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
) {
    let (rect, canvas_response) = ui.allocate_exact_size(
        Vec2::new(EQ_CANVAS_WIDTH, EQ_CANVAS_HEIGHT),
        Sense::click_and_drag(),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(34, 31, 29));
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, Color32::from_rgb(94, 84, 74)),
        StrokeKind::Outside,
    );
    painter.line_segment(
        [
            rect.left_top() + Vec2::new(2.0, 1.0),
            rect.right_top() + Vec2::new(-2.0, 1.0),
        ],
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 244, 220, 32)),
    );

    for octave in 1..=10 {
        for multiple in 2..10 {
            let frequency = multiple as f32 * 10.0_f32.powi(octave);
            if !(EQ_DISPLAY_MIN_HZ..=EQ_DISPLAY_MAX_HZ).contains(&frequency) {
                continue;
            }
            let x = rect.left() + rect.width() * x_from_frequency(frequency);
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(0.45, Color32::from_rgba_premultiplied(238, 229, 209, 18)),
            );
        }
    }

    let frequency_labels = [
        (20.0, "20"),
        (100.0, "100"),
        (500.0, "500"),
        (1_000.0, "1k"),
        (5_000.0, "5k"),
        (20_000.0, "20k"),
    ];
    for (frequency, label) in frequency_labels {
        let x = rect.left() + rect.width() * x_from_frequency(frequency);
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(0.75, Color32::from_rgba_premultiplied(238, 229, 209, 38)),
        );
        painter.text(
            Pos2::new(x, rect.bottom() - 9.0),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::monospace(FONT_HINT),
            Color32::from_rgb(166, 153, 132),
        );
    }

    let gain_labels = [(18.0, "+18"), (0.0, "0"), (-18.0, "-18")];
    for (gain_db, label) in gain_labels {
        let y = y_from_gain_db(rect, gain_db);
        let is_zero = gain_db == 0.0;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(
                if is_zero { 1.25 } else { 0.7 },
                if is_zero {
                    Color32::from_rgb(218, 197, 156)
                } else {
                    Color32::from_rgba_premultiplied(238, 229, 209, 32)
                },
            ),
        );
        painter.text(
            Pos2::new(rect.left() + 15.0, y - 7.0),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::monospace(FONT_HINT),
            Color32::from_rgb(166, 153, 132),
        );
    }

    let eq_response = EqDisplayResponse::from_params(params, eq_active(params));
    let mut curve = Vec::with_capacity(EQ_CURVE_POINTS);
    for index in 0..EQ_CURVE_POINTS {
        let normalized = index as f32 / (EQ_CURVE_POINTS - 1) as f32;
        let frequency = frequency_from_x(normalized);
        let gain_db = eq_response.gain_db_at(frequency);
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
    let zero_y = y_from_gain_db(rect, 0.0);
    if eq_active(params) {
        for segment in curve.windows(2) {
            let fill = [
                segment[0],
                segment[1],
                Pos2::new(segment[1].x, zero_y),
                Pos2::new(segment[0].x, zero_y),
            ];
            painter.add(egui::Shape::convex_polygon(
                fill.to_vec(),
                Color32::from_rgba_premultiplied(
                    curve_color.r(),
                    curve_color.g(),
                    curve_color.b(),
                    18,
                ),
                Stroke::NONE,
            ));
        }
    }
    for segment in curve.windows(2) {
        painter.line_segment(
            [
                segment[0] + Vec2::new(0.0, 1.0),
                segment[1] + Vec2::new(0.0, 1.0),
            ],
            Stroke::new(3.2, Color32::from_rgba_premultiplied(0, 0, 0, 65)),
        );
    }
    painter.add(egui::Shape::line(
        curve,
        Stroke::new(if eq_active(params) { 2.25 } else { 1.7 }, curve_color),
    ));

    let node_specs = eq_node_specs(params, colors, rect);
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
            14.0
        } else if hovered {
            12.0
        } else {
            9.0
        };
        painter.circle_filled(
            node.center,
            glow_radius,
            Color32::from_rgba_premultiplied(
                node.color.r(),
                node.color.g(),
                node.color.b(),
                if hovered || selected { 58 } else { 34 },
            ),
        );
        painter.circle_filled(
            node.center,
            if selected {
                7.8
            } else if hovered {
                6.2
            } else {
                5.0
            },
            node.color,
        );
        painter.circle_stroke(
            node.center,
            if selected {
                10.8
            } else if hovered {
                8.2
            } else {
                6.5
            },
            Stroke::new(
                if selected || hovered { 1.8 } else { 1.2 },
                Color32::from_rgb(248, 239, 218),
            ),
        );
        painter.text(
            node_label_pos(rect, node.center),
            egui::Align2::CENTER_CENTER,
            node.label,
            FontId::monospace(FONT_HINT),
            Color32::from_rgb(210, 196, 170),
        );
    }
    let _ = canvas_response;
}

#[derive(Clone, Copy)]
struct EqNodeSpec {
    index: usize,
    label: &'static str,
    center: Pos2,
    color: Color32,
}

fn eq_node_specs(params: &Cc22Params, colors: ModuleColors, rect: egui::Rect) -> [EqNodeSpec; 5] {
    [
        EqNodeSpec {
            index: 0,
            label: "LC",
            center: node_pos(rect, params.eq.low_cut_frequency.value(), 0.0),
            color: colors.character,
        },
        EqNodeSpec {
            index: 1,
            label: "LS",
            center: node_pos(
                rect,
                params.eq.low_shelf_frequency.value(),
                params.eq.low_shelf_gain.value(),
            ),
            color: Color32::from_rgb(220, 188, 78),
        },
        EqNodeSpec {
            index: 2,
            label: "MID",
            center: node_pos(
                rect,
                params.eq.mid_frequency.value(),
                params.eq.mid_gain.value(),
            ),
            color: colors.eq,
        },
        EqNodeSpec {
            index: 3,
            label: "HS",
            center: node_pos(
                rect,
                params.eq.high_shelf_frequency.value(),
                params.eq.high_shelf_gain.value(),
            ),
            color: colors.diffusion,
        },
        EqNodeSpec {
            index: 4,
            label: "HC",
            center: node_pos(rect, params.eq.high_cut_frequency.value(), 0.0),
            color: colors.texture,
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

fn node_label_pos(rect: egui::Rect, center: Pos2) -> Pos2 {
    if center.y - 17.0 < rect.top() + 7.0 {
        center + Vec2::new(0.0, 17.0)
    } else {
        center + Vec2::new(0.0, -17.0)
    }
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
        0 => setter.set_parameter(&params.eq.low_cut_frequency, frequency.clamp(20.0, 500.0)),
        1 => {
            setter.set_parameter(&params.eq.low_shelf_frequency, frequency.clamp(40.0, 500.0));
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
        0 => setter.set_parameter(&params.eq.low_cut_frequency, frequency.clamp(20.0, 500.0)),
        1 => {
            setter.set_parameter(&params.eq.low_shelf_frequency, frequency.clamp(40.0, 500.0));
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

fn band_inspector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
) {
    *selected_eq_band = (*selected_eq_band).min(4);
    ui.vertical(|ui| {
        ui.set_width(EQ_INSPECTOR_WIDTH);
        let band = (*selected_eq_band).min(4);
        let color = band_color(band, colors);

        ui.label(
            RichText::new(band_name(band))
                .font(FontId::monospace(FONT_SECONDARY))
                .strong()
                .color(color),
        );
        ui.label(
            RichText::new(band_kind(band))
                .font(FontId::monospace(FONT_VALUE_LABEL))
                .color(theme.muted_dark),
        );
        ui.label(
            RichText::new(band_values(params, band))
                .font(FontId::monospace(FONT_VALUE_LABEL))
                .color(theme.text_dark),
        );
        ui.add_space(2.0);
        ui.horizontal(|ui| match band {
            0 => {
                compact_eq_knob(
                    ui,
                    setter,
                    &params.eq.low_cut_frequency,
                    "FREQ",
                    color,
                    theme,
                );
            }
            1 => {
                compact_eq_knob(
                    ui,
                    setter,
                    &params.eq.low_shelf_frequency,
                    "FREQ",
                    color,
                    theme,
                );
                compact_eq_knob(ui, setter, &params.eq.low_shelf_gain, "GAIN", color, theme);
            }
            2 => {
                compact_eq_knob(ui, setter, &params.eq.mid_frequency, "FREQ", color, theme);
                compact_eq_knob(ui, setter, &params.eq.mid_gain, "GAIN", color, theme);
                compact_eq_knob(ui, setter, &params.eq.mid_q, "Q", color, theme);
            }
            3 => {
                compact_eq_knob(
                    ui,
                    setter,
                    &params.eq.high_shelf_frequency,
                    "FREQ",
                    color,
                    theme,
                );
                compact_eq_knob(ui, setter, &params.eq.high_shelf_gain, "GAIN", color, theme);
            }
            _ => {
                compact_eq_knob(
                    ui,
                    setter,
                    &params.eq.high_cut_frequency,
                    "FREQ",
                    color,
                    theme,
                );
            }
        });
    });
}

fn compact_eq_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    color: Color32,
    theme: Theme,
) {
    colored_knob(ui, setter, param, label, color, theme, 28.0);
}

fn toolbar_button(
    ui: &mut egui::Ui,
    label: &'static str,
    size: Vec2,
    active: bool,
    accent: Color32,
    theme: Theme,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if active {
        accent
    } else if response.hovered() {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 104)
    } else {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 58)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(
            if active { 1.1 } else { 0.8 },
            if active {
                Color32::from_rgb(248, 239, 218)
            } else {
                theme.card_edge
            },
        ),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::monospace(FONT_HINT),
        if active {
            Color32::WHITE
        } else {
            theme.text_dark
        },
    );
    response
}

pub(crate) fn band_tabs(
    ui: &mut egui::Ui,
    selected_eq_band: &mut usize,
    colors: ModuleColors,
    theme: Theme,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let band_colors = [
            colors.character,
            Color32::from_rgb(220, 188, 78),
            colors.eq,
            colors.diffusion,
            colors.texture,
        ];
        let labels = ["LCUT", "LOW", "MID", "HIGH", "HCUT"];
        for (index, (label, color)) in labels.into_iter().zip(band_colors).enumerate() {
            let selected = index == *selected_eq_band;
            let (rect, response) = ui.allocate_exact_size(Vec2::new(34.0, 16.0), Sense::click());
            if response.clicked() {
                *selected_eq_band = index;
            }
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(5),
                if selected {
                    color
                } else if response.hovered() {
                    Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 112)
                } else {
                    Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 68)
                },
            );
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(5),
                Stroke::new(
                    if selected { 1.15 } else { 0.8 },
                    if selected {
                        Color32::from_rgb(248, 239, 218)
                    } else {
                        theme.card_edge
                    },
                ),
                StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::monospace(FONT_HINT),
                if selected {
                    Color32::WHITE
                } else {
                    theme.text_dark
                },
            );
        }
    });
}

fn band_color(index: usize, colors: ModuleColors) -> Color32 {
    match index {
        0 => colors.character,
        1 => Color32::from_rgb(220, 188, 78),
        2 => colors.eq,
        3 => colors.diffusion,
        _ => colors.texture,
    }
}

fn band_name(index: usize) -> &'static str {
    match index {
        0 => "LOW CUT",
        1 => "LOW SHELF",
        2 => "MID",
        3 => "HIGH SHELF",
        _ => "HIGH CUT",
    }
}

fn band_kind(index: usize) -> &'static str {
    match index {
        0 => "FILTER",
        1 => "SHELF",
        2 => "PEAK",
        3 => "SHELF",
        _ => "FILTER",
    }
}

fn band_values(params: &Cc22Params, index: usize) -> String {
    match index {
        0 => value_string(&params.eq.low_cut_frequency),
        1 => format!(
            "{} / {}",
            value_string(&params.eq.low_shelf_frequency),
            value_string(&params.eq.low_shelf_gain)
        ),
        2 => format!(
            "{} / {} / Q {}",
            value_string(&params.eq.mid_frequency),
            value_string(&params.eq.mid_gain),
            value_string(&params.eq.mid_q)
        ),
        3 => format!(
            "{} / {}",
            value_string(&params.eq.high_shelf_frequency),
            value_string(&params.eq.high_shelf_gain)
        ),
        _ => value_string(&params.eq.high_cut_frequency),
    }
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
