use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2,
};

use crate::{
    dsp::eq::{EqBandType, EqMode},
    params::{Cc22Params, EqParamRefs},
};

use super::{
    meters::{EqBandSelection, EqPositionSelection, EqTargetSelection},
    theme::{ModuleColors, Theme},
    widgets::{mini_slider, set_param, value_string},
};

const EQ_DISPLAY_SAMPLE_RATE: f32 = 48_000.0;
const EQ_DISPLAY_MIN_HZ: f32 = 10.0;
const EQ_DISPLAY_MAX_HZ: f32 = 20_000.0;
const EQ_DISPLAY_DB_RANGE: f32 = 24.0;
const EQ_CURVE_POINTS: usize = 144;
const EQ_NODE_COUNT: usize = 5;
pub(crate) const EQ_WORKBENCH_HEIGHT: f32 = 180.0;
const EQ_CANVAS_HEIGHT: f32 = 128.0;
const EQ_INNER_PADDING: i8 = 8;
const EQ_NODE_EDGE_INSET: f32 = 20.0;
const EQ_GAIN_MIN_DB: f32 = -24.0;
const EQ_GAIN_MAX_DB: f32 = 24.0;
const EQ_Q_MIN: f32 = 0.1;
const EQ_Q_MAX: f32 = 12.0;
const EQ_RESET_FREQUENCIES: [f32; EQ_NODE_COUNT] = [80.0, 250.0, 1_000.0, 4_000.0, 12_000.0];

fn target_color(target: EqTargetSelection, colors: ModuleColors) -> Color32 {
    match target {
        EqTargetSelection::Global => colors.eq,
        EqTargetSelection::Character => colors.character,
        EqTargetSelection::Movement => colors.movement,
        EqTargetSelection::Diffusion => colors.diffusion,
        EqTargetSelection::Texture => colors.texture,
    }
}

pub(crate) fn selected_eq_params<'a>(
    params: &'a Cc22Params,
    target: EqTargetSelection,
    position: EqPositionSelection,
) -> &'a dyn EqParamRefs {
    match (target, position) {
        (EqTargetSelection::Global, EqPositionSelection::Pre) => &params.global_pre_eq,
        (EqTargetSelection::Global, EqPositionSelection::Post) => &params.global_post_eq,
        (EqTargetSelection::Character, EqPositionSelection::Pre) => &params.character_pre_eq,
        (EqTargetSelection::Character, EqPositionSelection::Post) => &params.character_post_eq,
        (EqTargetSelection::Movement, EqPositionSelection::Pre) => &params.movement_pre_eq,
        (EqTargetSelection::Movement, EqPositionSelection::Post) => &params.movement_post_eq,
        (EqTargetSelection::Diffusion, EqPositionSelection::Pre) => &params.diffusion_pre_eq,
        (EqTargetSelection::Diffusion, EqPositionSelection::Post) => &params.diffusion_post_eq,
        (EqTargetSelection::Texture, EqPositionSelection::Pre) => &params.texture_pre_eq,
        (EqTargetSelection::Texture, EqPositionSelection::Post) => &params.texture_post_eq,
    }
}

fn eq_active(params: &dyn EqParamRefs) -> bool {
    !params.bypass().value() && params.mode().value() == EqMode::On
}

pub(crate) fn eq_workbench(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_target: &mut EqTargetSelection,
    selected_eq_position: &mut EqPositionSelection,
    selected_eq_band: &mut EqBandSelection,
    advanced_open: &mut bool,
    colors: ModuleColors,
    theme: Theme,
    available_width: f32,
) {
    let workbench_width = available_width.max(0.0);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(workbench_width, EQ_WORKBENCH_HEIGHT),
        Sense::hover(),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            egui::Frame::new()
                .fill(theme.paper)
                .stroke(Stroke::new(1.0, theme.card_edge))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(EQ_INNER_PADDING))
                .show(ui, |ui| {
                    let content_width =
                        (workbench_width - f32::from(EQ_INNER_PADDING) * 2.0).max(0.0);
                    ui.set_width(content_width);
                    ui.set_min_height(EQ_WORKBENCH_HEIGHT - f32::from(EQ_INNER_PADDING) * 2.0);
                    eq_header(
                        ui,
                        setter,
                        params,
                        selected_eq_target,
                        selected_eq_position,
                        selected_eq_band,
                        advanced_open,
                        colors,
                        theme,
                    );
                    ui.add_space(3.0);
                    eq_separator(ui, theme);
                    ui.add_space(4.0);
                    let eq_params =
                        selected_eq_params(params, *selected_eq_target, *selected_eq_position);
                    eq_body(ui, setter, eq_params, selected_eq_band, colors, theme);
                });
        },
    );
}

fn eq_header(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_target: &mut EqTargetSelection,
    selected_eq_position: &mut EqPositionSelection,
    selected_eq_band: &mut EqBandSelection,
    advanced_open: &mut bool,
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
        eq_selection_badge(
            ui,
            *selected_eq_target,
            *selected_eq_position,
            colors,
            theme,
        );
        if eq_advanced_button(ui, *advanced_open, theme).clicked() {
            *advanced_open = !*advanced_open;
        }
        if *advanced_open {
            eq_toolbar_divider(ui, theme);
            eq_target_tabs(ui, selected_eq_target, colors, theme);
            let eq_accent = target_color(*selected_eq_target, colors);
            eq_toolbar_divider(ui, theme);
            eq_position_tabs(ui, selected_eq_position, eq_accent, colors, theme);
        } else {
            eq_toolbar_divider(ui, theme);
            let eq_accent = target_color(*selected_eq_target, colors);
            let eq_params = selected_eq_params(params, *selected_eq_target, *selected_eq_position);
            let active = eq_active(eq_params);
            if eq_toggle_button(ui, active, eq_accent, theme).clicked() {
                if active {
                    set_param(setter, eq_params.bypass(), true);
                } else {
                    set_param(setter, eq_params.mode(), EqMode::On);
                    set_param(setter, eq_params.bypass(), false);
                }
            }
            eq_toolbar_divider(ui, theme);
            eq_band_tabs(ui, selected_eq_band, colors, theme);
            eq_toolbar_divider(ui, theme);
            if eq_reset_button(ui, eq_accent, theme).clicked() {
                reset_eq_params_to_defaults(
                    setter,
                    params,
                    *selected_eq_target,
                    *selected_eq_position,
                );
                *selected_eq_band = EqBandSelection::Band1;
            }
        }
    });
}

fn eq_selection_badge(
    ui: &mut egui::Ui,
    target: EqTargetSelection,
    position: EqPositionSelection,
    colors: ModuleColors,
    theme: Theme,
) {
    let accent = target_color(target, colors);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(88.0, 20.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(6),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 34),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, accent.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{} · {}", target.label(), position.label()),
        FontId::monospace(7.5),
        theme.text_dark,
    );
}

fn eq_advanced_button(ui: &mut egui::Ui, open: bool, theme: Theme) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(82.0, 20.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(6),
        if open || response.hovered() {
            Color32::from_rgb(52, 48, 42)
        } else {
            Color32::from_rgb(225, 219, 207)
        },
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if open { "CLOSE TARGET" } else { "EQ TARGET" },
        FontId::monospace(7.2),
        if open || response.hovered() {
            Color32::WHITE
        } else {
            theme.muted_dark
        },
    );
    response
}

fn eq_toggle_button(
    ui: &mut egui::Ui,
    active: bool,
    accent: Color32,
    theme: Theme,
) -> egui::Response {
    let label = if active { "ON" } else { "OFF" };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(46.0, 20.0), Sense::click());
    let fill = if active {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 220)
    } else if response.hovered() {
        Color32::from_rgb(230, 224, 213)
    } else {
        Color32::from_rgb(220, 214, 203)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
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
    let knob_x = if active {
        rect.right() - 9.5
    } else {
        rect.left() + 9.5
    };
    ui.painter().circle_filled(
        Pos2::new(knob_x, rect.center().y),
        7.0,
        Color32::from_rgb(250, 247, 240),
    );
    ui.painter().circle_stroke(
        Pos2::new(knob_x, rect.center().y),
        7.0,
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.6)),
    );
    ui.painter().text(
        Pos2::new(
            if active {
                rect.left() + 14.0
            } else {
                rect.right() - 14.0
            },
            rect.center().y,
        ),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::monospace(8.0),
        if active {
            Color32::WHITE
        } else {
            theme.text_dark.gamma_multiply(0.72)
        },
    );
    response
}

fn eq_band_tabs(
    ui: &mut egui::Ui,
    selected_eq_band: &mut EqBandSelection,
    colors: ModuleColors,
    theme: Theme,
) {
    let tabs_width = ui.available_width().min(232.0);
    let (group_rect, _) = ui.allocate_exact_size(Vec2::new(tabs_width, 22.0), Sense::hover());
    ui.painter().rect_filled(
        group_rect,
        CornerRadius::same(7),
        Color32::from_rgb(232, 226, 215),
    );
    ui.painter().rect_stroke(
        group_rect,
        CornerRadius::same(7),
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.48)),
        StrokeKind::Inside,
    );

    let mut x = group_rect.left() + 3.0;
    for band in EqBandSelection::ALL {
        let label = eq_band_tab_label(band);
        let width = eq_band_tab_width(label);
        let rect =
            egui::Rect::from_min_size(Pos2::new(x, group_rect.top() + 3.0), Vec2::new(width, 16.0));
        let response = ui.interact(
            rect,
            ui.make_persistent_id(("eq_tab", band.index())),
            Sense::click(),
        );
        if response.clicked() {
            *selected_eq_band = band;
        }
        paint_eq_band_tab(
            ui,
            rect,
            label,
            band == *selected_eq_band,
            response.hovered(),
            eq_band_color(band, colors),
            theme,
        );
        x += width + 3.0;
    }
}

fn eq_target_tabs(
    ui: &mut egui::Ui,
    selected_eq_target: &mut EqTargetSelection,
    colors: ModuleColors,
    theme: Theme,
) {
    for target in EqTargetSelection::ALL {
        let active = *selected_eq_target == target;
        let label = target.label();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(48.0, 20.0), Sense::click());
        if response.clicked() {
            *selected_eq_target = target;
        }
        let fill = if active {
            target_color(target, colors)
        } else if response.hovered() {
            Color32::from_rgba_premultiplied(
                target_color(target, colors).r(),
                target_color(target, colors).g(),
                target_color(target, colors).b(),
                58,
            )
        } else {
            Color32::from_rgb(244, 239, 229)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(7),
            Stroke::new(1.0, theme.card_edge.gamma_multiply(0.72)),
            StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::monospace(7.5),
            if active {
                Color32::WHITE
            } else {
                theme.muted_dark
            },
        );
    }
}

fn eq_position_tabs(
    ui: &mut egui::Ui,
    selected_eq_position: &mut EqPositionSelection,
    eq_accent: Color32,
    colors: ModuleColors,
    theme: Theme,
) {
    let _ = colors;
    for position in EqPositionSelection::ALL {
        let active = *selected_eq_position == position;
        let label = position.label();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(32.0, 20.0), Sense::click());
        if response.clicked() {
            *selected_eq_position = position;
        }
        if response.secondary_clicked() {
            *selected_eq_position = position.toggle();
        }
        let fill = if active {
            Color32::from_rgba_premultiplied(eq_accent.r(), eq_accent.g(), eq_accent.b(), 200)
        } else if response.hovered() {
            Color32::from_rgb(230, 224, 213)
        } else {
            Color32::from_rgb(244, 239, 229)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, theme.card_edge.gamma_multiply(0.72)),
            StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::monospace(7.2),
            if active {
                Color32::WHITE
            } else {
                theme.muted_dark
            },
        );
    }
}

fn eq_band_tab_width(label: &'static str) -> f32 {
    match label.len() {
        0..=2 => 30.0,
        3..=4 => 42.0,
        _ => 52.0,
    }
}

fn paint_eq_band_tab(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &'static str,
    active: bool,
    hovered: bool,
    accent: Color32,
    theme: Theme,
) {
    let fill = if active {
        accent
    } else if hovered {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 58)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
    if active || hovered {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, theme.card_edge.gamma_multiply(0.36)),
            StrokeKind::Inside,
        );
    }
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
}

fn eq_reset_button(ui: &mut egui::Ui, accent: Color32, theme: Theme) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(62.0, 20.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(10),
        if response.hovered() {
            Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 26)
        } else {
            Color32::from_rgb(244, 239, 229)
        },
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.72)),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "RESET EQ",
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

fn eq_canvas(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    selected_eq_band: &mut EqBandSelection,
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
        CornerRadius::same(6),
        Color32::from_rgb(248, 245, 238),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, Color32::from_rgb(214, 207, 194)),
        StrokeKind::Inside,
    );
    let plot_rect = egui::Rect::from_min_max(
        rect.min + Vec2::new(16.0, 9.0),
        rect.max - Vec2::new(16.0, 18.0),
    );

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

    for octave in 1..=4 {
        for multiple in 2..10 {
            let frequency = multiple as f32 * 10.0_f32.powi(octave);
            if !(EQ_DISPLAY_MIN_HZ..=EQ_DISPLAY_MAX_HZ).contains(&frequency)
                || frequency_labels
                    .iter()
                    .any(|(major, _)| (frequency - *major).abs() < f32::EPSILON)
            {
                continue;
            }
            let x = plot_rect.left() + plot_rect.width() * x_from_frequency(frequency);
            painter.line_segment(
                [
                    Pos2::new(x, plot_rect.top()),
                    Pos2::new(x, plot_rect.bottom()),
                ],
                Stroke::new(0.35, Color32::from_rgba_premultiplied(128, 116, 96, 34)),
            );
        }
    }

    for (frequency, label) in frequency_labels {
        let x = plot_rect.left() + plot_rect.width() * x_from_frequency(frequency);
        painter.line_segment(
            [
                Pos2::new(x, plot_rect.top()),
                Pos2::new(x, plot_rect.bottom()),
            ],
            Stroke::new(0.65, Color32::from_rgba_premultiplied(119, 106, 88, 64)),
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
            Color32::from_rgb(108, 99, 87),
        );
    }

    let gain_labels = [
        (12.0, "+12"),
        (6.0, "+6"),
        (0.0, "0"),
        (-6.0, "-6"),
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
                    Color32::from_rgb(171, 158, 138)
                } else {
                    Color32::from_rgba_premultiplied(116, 104, 88, 42)
                },
            ),
        );
        painter.text(
            Pos2::new(plot_rect.right() - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            label,
            FontId::monospace(7.6),
            if is_zero {
                Color32::from_rgb(72, 64, 54)
            } else {
                Color32::from_rgb(128, 118, 104)
            },
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
    let zero_y = y_from_gain_db(plot_rect, 0.0);
    let fill_color = if eq_active(params) {
        Color32::from_rgba_premultiplied(255, 141, 45, 15)
    } else {
        Color32::from_rgba_premultiplied(theme.muted.r(), theme.muted.g(), theme.muted.b(), 9)
    };
    for segment in curve.windows(2) {
        painter.add(egui::Shape::convex_polygon(
            vec![
                segment[0],
                segment[1],
                Pos2::new(segment[1].x, zero_y),
                Pos2::new(segment[0].x, zero_y),
            ],
            fill_color,
            Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::line(
        curve,
        Stroke::new(if eq_active(params) { 2.4 } else { 2.0 }, curve_color),
    ));

    let node_specs = eq_node_specs(params, colors, plot_rect);
    for node in node_specs {
        let node_band = EqBandSelection::from_index(node.index);
        let hit_radius = if node_band == *selected_eq_band {
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
            *selected_eq_band = node_band;
        }
        if node_response.secondary_clicked() {
            *selected_eq_band = node_band;
            if let Some(band_type) = direct_right_click_band_type(node.index) {
                set_eq_band_type(setter, params, node.index, band_type);
            }
        }
        node_response.context_menu(|ui| {
            *selected_eq_band = node_band;
            let default_for_band = direct_right_click_band_type(node.index);
            let ordered_types = eq_band_type_menu_order(default_for_band);
            for band_type in ordered_types {
                let label = if Some(band_type) == default_for_band {
                    format!("{} (default)", eq_band_type_menu_label(band_type))
                } else {
                    eq_band_type_menu_label(band_type).to_string()
                };
                if ui.button(label).clicked() {
                    set_eq_band_type(setter, params, node.index, band_type);
                    ui.close_menu();
                }
            }
        });
        let (scroll_y, fine_scroll) =
            ui.input(|input| (input.raw_scroll_delta.y, input.modifiers.shift));
        if node_response.hovered() && scroll_y.abs() > 0.0 {
            *selected_eq_band = node_band;
            scroll_eq_band_width(setter, params, node.index, scroll_y, fine_scroll);
        }
        if node_response.double_clicked() {
            *selected_eq_band = node_band;
            reset_eq_band(setter, params, node.index);
        }
        if node_response.drag_started() && node.draggable {
            *selected_eq_band = node_band;
            begin_selected_band_setter(setter, params, node.index);
        }
        if node_response.dragged() && node.draggable {
            let fine = ui.input(|input| input.modifiers.shift);
            if fine {
                let delta = ui.input(|input| input.pointer.delta());
                offset_selected_band_from_delta(setter, params, node.index, plot_rect, delta, 0.22);
            } else if let Some(pos) = ui.input(|input| input.pointer.interact_pos()) {
                set_selected_band_from_pos(setter, params, node.index, plot_rect, pos);
            }
        }
        if node_response.drag_stopped() && node.draggable {
            end_selected_band_setter(setter, params, node.index);
        }

        let hovered = node_response.hovered();
        let selected = node_band == *selected_eq_band;
        let glow_radius = if selected {
            18.0
        } else if hovered {
            13.5
        } else {
            0.0
        };
        if glow_radius > 0.0 {
            painter.circle_filled(
                node.center,
                glow_radius,
                Color32::from_rgba_premultiplied(
                    node.color.r(),
                    node.color.g(),
                    node.color.b(),
                    if selected { 72 } else { 42 },
                ),
            );
        }
        if selected {
            painter.circle_stroke(
                node.center,
                14.0,
                Stroke::new(2.0, Color32::from_rgba_premultiplied(255, 255, 255, 210)),
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
                8.4
            } else if hovered {
                6.2
            } else {
                4.8
            },
            if node.draggable {
                node.color
            } else {
                Color32::from_rgba_premultiplied(node.color.r(), node.color.g(), node.color.b(), 78)
            },
        );
        painter.circle_stroke(
            node.center,
            if selected {
                10.8
            } else if hovered {
                8.4
            } else {
                6.4
            },
            Stroke::new(
                if selected { 1.9 } else { 1.2 },
                Color32::from_rgb(255, 252, 244),
            ),
        );
        if hovered || selected {
            let label = node.label;
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

fn eq_body(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    selected_eq_band: &mut EqBandSelection,
    colors: ModuleColors,
    theme: Theme,
) {
    let width = ui.available_width().max(0.0);
    let gap = 8.0;
    let inspector_width = (width * 0.22)
        .clamp(178.0, 220.0)
        .min((width - gap).max(0.0));
    let canvas_width = (width - inspector_width - gap).max(0.0);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        eq_canvas(
            ui,
            setter,
            params,
            selected_eq_band,
            colors,
            theme,
            canvas_width,
        );
        eq_inspector(
            ui,
            setter,
            params,
            *selected_eq_band,
            colors,
            theme,
            inspector_width,
        );
    });
}

fn eq_inspector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    band: EqBandSelection,
    colors: ModuleColors,
    theme: Theme,
    width: f32,
) {
    let accent = eq_band_color(band, colors);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, EQ_CANVAS_HEIGHT), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(
                RichText::new(eq_band_name(band))
                    .font(FontId::monospace(10.0))
                    .strong()
                    .color(theme.text_dark),
            );
            ui.label(
                RichText::new(eq_band_summary(params, band))
                    .font(FontId::monospace(8.2))
                    .strong()
                    .color(theme.muted_dark),
            );
            ui.add_space(2.0);
            let band_index = band.index();
            eq_band_enable_button(ui, setter, params.band_enabled(band_index), accent, theme);
            eq_band_type_buttons(ui, setter, params.band_type(band_index), accent, theme);

            let band_type = params.band_type(band_index).value();
            if band_type != EqBandType::Off && params.band_enabled(band_index).value() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let count = 1
                        + usize::from(eq_type_uses_gain(band_type))
                        + usize::from(eq_type_uses_q(band_type));
                    let control_width = ((ui.available_width()
                        - 6.0 * (count.saturating_sub(1)) as f32)
                        / count as f32)
                        .max(42.0);
                    for (param, label) in [
                        (Some(params.band_frequency(band_index)), "FREQ"),
                        (
                            eq_type_uses_gain(band_type).then(|| params.band_gain(band_index)),
                            "GAIN",
                        ),
                        (
                            eq_type_uses_q(band_type).then(|| params.band_q(band_index)),
                            "Q",
                        ),
                    ] {
                        if let Some(param) = param {
                            ui.allocate_ui(Vec2::new(control_width, 32.0), |ui| {
                                eq_inspector_slider(ui, setter, param, label, accent, theme);
                            });
                        }
                    }
                });
            }
        },
    );
}

fn eq_band_enable_button(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &BoolParam,
    accent: Color32,
    theme: Theme,
) {
    let enabled = param.value();
    let label = if enabled { "ENABLED" } else { "DISABLED" };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 15.0), Sense::click());
    if response.clicked() {
        set_param(setter, param, !enabled);
    }
    let fill = if enabled {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 58)
    } else {
        Color32::from_rgb(228, 222, 212)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, if enabled { accent } else { theme.card_edge }),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::monospace(8.0),
        if enabled {
            theme.text_dark
        } else {
            theme.muted_dark
        },
    );
}

fn eq_band_type_buttons(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<EqBandType>,
    accent: Color32,
    theme: Theme,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for band_type in [
            EqBandType::Off,
            EqBandType::Bell,
            EqBandType::LowShelf,
            EqBandType::HighShelf,
            EqBandType::HighPass,
            EqBandType::LowPass,
        ] {
            let label = eq_band_type_label(band_type);
            let width = if matches!(band_type, EqBandType::LowShelf | EqBandType::HighShelf) {
                22.0
            } else {
                20.0
            };
            let active = param.value() == band_type;
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 15.0), Sense::click());
            if response.clicked() {
                set_param(setter, param, band_type);
            }
            let fill = if active {
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 220)
            } else if response.hovered() {
                Color32::from_rgb(236, 231, 221)
            } else {
                Color32::from_rgb(226, 220, 210)
            };
            ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::monospace(7.2),
                if active {
                    Color32::WHITE
                } else {
                    theme.text_dark
                },
            );
        }
    });
}

fn eq_inspector_slider(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
) {
    mini_slider(ui, setter, param, label, accent, theme);
}

fn eq_band_tab_label(band: EqBandSelection) -> &'static str {
    match band {
        EqBandSelection::Band1 => "B1",
        EqBandSelection::Band2 => "B2",
        EqBandSelection::Band3 => "B3",
        EqBandSelection::Band4 => "B4",
        EqBandSelection::Band5 => "B5",
    }
}

fn eq_band_name(band: EqBandSelection) -> &'static str {
    match band {
        EqBandSelection::Band1 => "BAND 1",
        EqBandSelection::Band2 => "BAND 2",
        EqBandSelection::Band3 => "BAND 3",
        EqBandSelection::Band4 => "BAND 4",
        EqBandSelection::Band5 => "BAND 5",
    }
}

fn eq_band_summary(params: &dyn EqParamRefs, band: EqBandSelection) -> String {
    let index = band.index();
    let band_type = params.band_type(index).value();
    if !params.band_enabled(index).value() || band_type == EqBandType::Off {
        return format!(
            "{} / {}",
            eq_band_type_label(band_type),
            value_string(params.band_frequency(index))
        );
    }

    if eq_type_uses_gain(band_type) && eq_type_uses_q(band_type) {
        format!(
            "{} / {} / {} / Q {}",
            eq_band_type_label(band_type),
            value_string(params.band_frequency(index)),
            value_string(params.band_gain(index)),
            value_string(params.band_q(index))
        )
    } else if eq_type_uses_gain(band_type) {
        format!(
            "{} / {} / {}",
            eq_band_type_label(band_type),
            value_string(params.band_frequency(index)),
            value_string(params.band_gain(index))
        )
    } else if eq_type_uses_q(band_type) {
        format!(
            "{} / {} / Q {}",
            eq_band_type_label(band_type),
            value_string(params.band_frequency(index)),
            value_string(params.band_q(index))
        )
    } else {
        format!(
            "{} / {}",
            eq_band_type_label(band_type),
            value_string(params.band_frequency(index))
        )
    }
}

fn eq_band_type_label(band_type: EqBandType) -> &'static str {
    match band_type {
        EqBandType::Off => "OFF",
        EqBandType::Bell => "BEL",
        EqBandType::LowShelf => "LS",
        EqBandType::HighShelf => "HS",
        EqBandType::HighPass => "HP",
        EqBandType::LowPass => "LP",
    }
}

fn eq_band_type_menu_label(band_type: EqBandType) -> &'static str {
    match band_type {
        EqBandType::Off => "Off",
        EqBandType::Bell => "Bell",
        EqBandType::LowShelf => "Low Shelf",
        EqBandType::HighShelf => "High Shelf",
        EqBandType::HighPass => "High Pass",
        EqBandType::LowPass => "Low Pass",
    }
}

fn eq_type_uses_gain(band_type: EqBandType) -> bool {
    matches!(
        band_type,
        EqBandType::Bell | EqBandType::LowShelf | EqBandType::HighShelf
    )
}

fn eq_type_uses_q(band_type: EqBandType) -> bool {
    matches!(
        band_type,
        EqBandType::Bell
            | EqBandType::LowShelf
            | EqBandType::HighShelf
            | EqBandType::HighPass
            | EqBandType::LowPass
    )
}

fn eq_band_color(band: EqBandSelection, colors: ModuleColors) -> Color32 {
    match band {
        EqBandSelection::Band1 => Color32::from_rgb(255, 90, 55),
        EqBandSelection::Band2 => Color32::from_rgb(255, 150, 60),
        EqBandSelection::Band3 => Color32::from_rgb(255, 175, 65),
        EqBandSelection::Band4 => Color32::from_rgb(100, 210, 160),
        EqBandSelection::Band5 => colors.texture,
    }
}

#[derive(Clone, Copy)]
struct EqNodeSpec {
    index: usize,
    center: Pos2,
    color: Color32,
    draggable: bool,
    label: &'static str,
}

fn eq_node_specs(
    params: &dyn EqParamRefs,
    colors: ModuleColors,
    rect: egui::Rect,
) -> [EqNodeSpec; EQ_NODE_COUNT] {
    core::array::from_fn(|index| {
        let band = EqBandSelection::from_index(index);
        let band_type = params.band_type(index).value();
        let enabled = params.band_enabled(index).value();
        let draggable = enabled && band_type != EqBandType::Off;
        let gain = if eq_type_uses_gain(band_type) && draggable {
            params.band_gain(index).value()
        } else {
            0.0
        };
        EqNodeSpec {
            index,
            center: node_pos(rect, params.band_frequency(index).value(), gain),
            color: eq_band_color(band, colors),
            draggable,
            label: eq_node_type_label(band_type),
        }
    })
}

fn eq_node_type_label(band_type: EqBandType) -> &'static str {
    match band_type {
        EqBandType::Off => "OFF",
        EqBandType::Bell => "BEL",
        EqBandType::LowShelf => "LS",
        EqBandType::HighShelf => "HS",
        EqBandType::HighPass => "HP",
        EqBandType::LowPass => "LP",
    }
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

pub(crate) fn direct_right_click_band_type(band: usize) -> Option<EqBandType> {
    match band {
        0 => Some(EqBandType::HighPass),
        4 => Some(EqBandType::LowPass),
        _ => None,
    }
}

fn eq_band_type_menu_order(default: Option<EqBandType>) -> [EqBandType; 6] {
    let all = [
        EqBandType::Bell,
        EqBandType::LowShelf,
        EqBandType::HighShelf,
        EqBandType::HighPass,
        EqBandType::LowPass,
        EqBandType::Off,
    ];
    let mut result = all;
    if let Some(d) = default {
        if let Some(pos) = result.iter().position(|&t| t == d) {
            result[0..=pos].rotate_right(1);
        }
    }
    result
}

fn set_eq_band_type(
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    band: usize,
    band_type: EqBandType,
) {
    set_param(setter, params.band_type(band), band_type);
    set_param(
        setter,
        params.band_enabled(band),
        band_type != EqBandType::Off,
    );
    if band_type == EqBandType::Off {
        set_param(setter, params.band_gain(band), 0.0);
    }
}

fn begin_selected_band_setter(setter: &ParamSetter<'_>, params: &dyn EqParamRefs, band: usize) {
    if params.band_type(band).value() == EqBandType::Off || !params.band_enabled(band).value() {
        return;
    }
    setter.begin_set_parameter(params.band_frequency(band));
    if eq_type_uses_gain(params.band_type(band).value()) {
        setter.begin_set_parameter(params.band_gain(band));
    }
    if eq_type_uses_q(params.band_type(band).value()) {
        setter.begin_set_parameter(params.band_q(band));
    }
}

fn end_selected_band_setter(setter: &ParamSetter<'_>, params: &dyn EqParamRefs, band: usize) {
    if params.band_type(band).value() == EqBandType::Off || !params.band_enabled(band).value() {
        return;
    }
    setter.end_set_parameter(params.band_frequency(band));
    if eq_type_uses_gain(params.band_type(band).value()) {
        setter.end_set_parameter(params.band_gain(band));
    }
    if eq_type_uses_q(params.band_type(band).value()) {
        setter.end_set_parameter(params.band_q(band));
    }
}

fn set_selected_band_from_pos(
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    band: usize,
    rect: egui::Rect,
    pos: Pos2,
) {
    let x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let y = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    let frequency = frequency_from_x(x);
    let gain_db = gain_from_y(y);
    let band_type = params.band_type(band).value();

    setter.set_parameter(params.band_frequency(band), frequency.clamp(20.0, 20_000.0));
    if eq_type_uses_gain(band_type) {
        setter.set_parameter(params.band_gain(band), gain_db);
    } else if matches!(band_type, EqBandType::HighPass | EqBandType::LowPass) {
        setter.set_parameter(params.band_q(band), q_from_y(y));
    }
}

fn q_from_y(y: f32) -> f32 {
    (EQ_Q_MIN + ((1.0 - y.clamp(0.0, 1.0)) * (EQ_Q_MAX - EQ_Q_MIN))).clamp(EQ_Q_MIN, EQ_Q_MAX)
}

fn offset_selected_band_from_delta(
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    band: usize,
    rect: egui::Rect,
    delta: Vec2,
    scale: f32,
) {
    let current_frequency = params.band_frequency(band).value();
    let band_type = params.band_type(band).value();
    let current_gain = if eq_type_uses_gain(band_type) {
        params.band_gain(band).value()
    } else {
        0.0
    };

    let x =
        (x_from_frequency(current_frequency) + (delta.x / rect.width()) * scale).clamp(0.0, 1.0);
    let frequency = frequency_from_x(x);
    let gain_db = (current_gain
        - (delta.y / rect.height()) * (EQ_GAIN_MAX_DB - EQ_GAIN_MIN_DB) * scale)
        .clamp(EQ_GAIN_MIN_DB, EQ_GAIN_MAX_DB);

    setter.set_parameter(params.band_frequency(band), frequency.clamp(20.0, 20_000.0));
    if eq_type_uses_gain(band_type) {
        setter.set_parameter(params.band_gain(band), gain_db);
    } else if matches!(band_type, EqBandType::HighPass | EqBandType::LowPass) {
        let q = (params.band_q(band).value()
            - (delta.y / rect.height()) * (EQ_Q_MAX - EQ_Q_MIN) * scale)
            .clamp(EQ_Q_MIN, EQ_Q_MAX);
        setter.set_parameter(params.band_q(band), q);
    }
}

fn scroll_eq_band_width(
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    band: usize,
    scroll_y: f32,
    fine: bool,
) {
    let amount = scroll_y.abs() * if fine { 0.000_45 } else { 0.001_8 };
    let factor = amount.exp().clamp(1.001, 1.35);

    if eq_type_uses_q(params.band_type(band).value()) {
        let q = if scroll_y > 0.0 {
            params.band_q(band).value() * factor
        } else {
            params.band_q(band).value() / factor
        };
        set_param(setter, params.band_q(band), q.clamp(EQ_Q_MIN, EQ_Q_MAX));
    } else {
        set_param(
            setter,
            params.band_frequency(band),
            scroll_frequency(
                params.band_frequency(band).value(),
                scroll_y,
                factor,
                20.0,
                20_000.0,
                band >= 3,
            ),
        );
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

fn reset_eq_band(setter: &ParamSetter<'_>, params: &dyn EqParamRefs, band: usize) {
    set_param(setter, params.band_enabled(band), true);
    set_param(setter, params.band_type(band), EqBandType::Bell);
    set_param(
        setter,
        params.band_frequency(band),
        safe_eq_reset_frequency(band),
    );
    set_param(setter, params.band_gain(band), 0.0);
    set_param(setter, params.band_q(band), 1.0);
}

fn reset_eq_params_to_defaults(
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    target: EqTargetSelection,
    position: EqPositionSelection,
) {
    let eq_params = selected_eq_params(params, target, position);
    set_param(setter, eq_params.mode(), EqMode::On);
    set_param(setter, eq_params.bypass(), false);
    for band in 0..EQ_NODE_COUNT {
        reset_eq_band(setter, eq_params, band);
    }
}

fn safe_eq_reset_frequency(band: usize) -> f32 {
    EQ_RESET_FREQUENCIES
        .get(band)
        .copied()
        .unwrap_or(EQ_RESET_FREQUENCIES[EQ_RESET_FREQUENCIES.len() - 1])
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
    fn from_params(params: &dyn EqParamRefs, active: bool) -> Self {
        Self {
            active,
            filters: core::array::from_fn(|band| {
                if !params.band_enabled(band).value() {
                    return DisplayBiquad::identity();
                }

                match params.band_type(band).value() {
                    EqBandType::Off => DisplayBiquad::identity(),
                    EqBandType::Bell => DisplayBiquad::peaking(
                        params.band_frequency(band).value(),
                        params.band_gain(band).value(),
                        params.band_q(band).value(),
                        EQ_DISPLAY_SAMPLE_RATE,
                    ),
                    EqBandType::LowShelf => DisplayBiquad::low_shelf(
                        params.band_frequency(band).value(),
                        params.band_gain(band).value(),
                        params.band_q(band).value(),
                        EQ_DISPLAY_SAMPLE_RATE,
                    ),
                    EqBandType::HighShelf => DisplayBiquad::high_shelf(
                        params.band_frequency(band).value(),
                        params.band_gain(band).value(),
                        params.band_q(band).value(),
                        EQ_DISPLAY_SAMPLE_RATE,
                    ),
                    EqBandType::HighPass => DisplayBiquad::high_pass(
                        params.band_frequency(band).value(),
                        params.band_q(band).value(),
                        EQ_DISPLAY_SAMPLE_RATE,
                    ),
                    EqBandType::LowPass => DisplayBiquad::low_pass(
                        params.band_frequency(band).value(),
                        params.band_q(band).value(),
                        EQ_DISPLAY_SAMPLE_RATE,
                    ),
                }
            }),
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

    fn low_shelf(frequency: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        shelf_biquad(frequency, gain_db, q, sample_rate, false)
    }

    fn high_shelf(frequency: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        shelf_biquad(frequency, gain_db, q, sample_rate, true)
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
}

fn shelf_biquad(
    frequency: f32,
    gain_db: f32,
    q: f32,
    sample_rate: f32,
    high: bool,
) -> DisplayBiquad {
    if gain_db.abs() < 0.000_1 {
        return DisplayBiquad::identity();
    }

    let omega = omega(frequency, sample_rate);
    let sin = omega.sin();
    let cos = omega.cos();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let sqrt_a = a.sqrt();
    let alpha = sin / (2.0 * q.max(0.1));
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

impl DisplayBiquad {
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

fn omega(frequency: f32, sample_rate: f32) -> f32 {
    let frequency = frequency.clamp(EQ_DISPLAY_MIN_HZ, EQ_DISPLAY_MAX_HZ);
    core::f32::consts::TAU * frequency / sample_rate.max(1.0)
}

fn x_from_frequency(frequency: f32) -> f32 {
    let clamped = frequency.clamp(EQ_DISPLAY_MIN_HZ, EQ_DISPLAY_MAX_HZ);
    ((clamped / EQ_DISPLAY_MIN_HZ).ln() / (EQ_DISPLAY_MAX_HZ / EQ_DISPLAY_MIN_HZ).ln())
        .clamp(0.0, 1.0)
}

fn frequency_from_x(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    EQ_DISPLAY_MIN_HZ * (EQ_DISPLAY_MAX_HZ / EQ_DISPLAY_MIN_HZ).powf(x)
}

fn gain_from_y(y: f32) -> f32 {
    let y = y.clamp(0.0, 1.0);
    EQ_GAIN_MAX_DB - y * 2.0 * EQ_GAIN_MAX_DB
}

fn y_from_gain_db(rect: egui::Rect, gain_db: f32) -> f32 {
    let gain_db = gain_db.clamp(-EQ_DISPLAY_DB_RANGE, EQ_DISPLAY_DB_RANGE);
    rect.bottom() - ((gain_db + EQ_DISPLAY_DB_RANGE) / (2.0 * EQ_DISPLAY_DB_RANGE)) * rect.height()
}

fn y_from_real_gain_db(rect: egui::Rect, gain_db: f32) -> f32 {
    y_from_gain_db(rect, gain_db.clamp(-EQ_GAIN_MAX_DB, EQ_GAIN_MAX_DB))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nih_plug::prelude::EnumParam;

    use crate::{
        params::Cc22Params,
        ui::meters::{EqPositionSelection, EqTargetSelection},
    };

    use super::{
        direct_right_click_band_type, safe_eq_reset_frequency, selected_eq_params, EqBandType,
    };

    #[test]
    fn direct_right_click_maps_edge_bands_to_filters() {
        assert_eq!(direct_right_click_band_type(0), Some(EqBandType::HighPass));
        assert_eq!(direct_right_click_band_type(4), Some(EqBandType::LowPass));
        assert_eq!(direct_right_click_band_type(2), None);
    }

    #[test]
    fn reset_frequencies_are_in_expected_ranges() {
        for band in 0..5 {
            let frequency = safe_eq_reset_frequency(band);
            assert!((20.0..=20_000.0).contains(&frequency));
        }
    }

    #[test]
    fn every_target_and_position_selects_a_distinct_eq_bank() {
        let params = Cc22Params::default();
        let targets = [
            EqTargetSelection::Global,
            EqTargetSelection::Character,
            EqTargetSelection::Movement,
            EqTargetSelection::Diffusion,
            EqTargetSelection::Texture,
        ];
        let positions = [EqPositionSelection::Pre, EqPositionSelection::Post];
        let expected = [
            &params.global_pre_eq.band1_gain as *const _ as usize,
            &params.global_post_eq.band1_gain as *const _ as usize,
            &params.character_pre_eq.band1_gain as *const _ as usize,
            &params.character_post_eq.band1_gain as *const _ as usize,
            &params.movement_pre_eq.band1_gain as *const _ as usize,
            &params.movement_post_eq.band1_gain as *const _ as usize,
            &params.diffusion_pre_eq.band1_gain as *const _ as usize,
            &params.diffusion_post_eq.band1_gain as *const _ as usize,
            &params.texture_pre_eq.band1_gain as *const _ as usize,
            &params.texture_post_eq.band1_gain as *const _ as usize,
        ];
        let mut band1_gain_addresses = BTreeSet::new();
        let mut index = 0;

        for target in targets {
            for position in positions {
                let eq = selected_eq_params(&params, target, position);
                let address = eq.band_gain(0) as *const _ as usize;
                assert_eq!(
                    address, expected[index],
                    "resolver returned the wrong EQ bank"
                );
                index += 1;
                assert!(
                    band1_gain_addresses.insert(address),
                    "selection reused an EQ parameter bank"
                );
            }
        }

        assert_eq!(band1_gain_addresses.len(), 10);
    }

    #[test]
    fn texture_post_band5_right_click_does_not_change_other_eqs() {
        let mut params = Cc22Params::default();
        let before = [
            params.global_pre_eq.band5_type.value(),
            params.global_post_eq.band5_type.value(),
            params.character_pre_eq.band5_type.value(),
            params.character_post_eq.band5_type.value(),
            params.movement_pre_eq.band5_type.value(),
            params.movement_post_eq.band5_type.value(),
            params.diffusion_pre_eq.band5_type.value(),
            params.diffusion_post_eq.band5_type.value(),
            params.texture_pre_eq.band5_type.value(),
        ];
        let right_click_type = direct_right_click_band_type(4).unwrap();
        params.texture_post_eq.band5_type = EnumParam::new("Type", right_click_type);

        assert_eq!(
            params.texture_post_eq.band5_type.value(),
            EqBandType::LowPass
        );
        let after = [
            params.global_pre_eq.band5_type.value(),
            params.global_post_eq.band5_type.value(),
            params.character_pre_eq.band5_type.value(),
            params.character_post_eq.band5_type.value(),
            params.movement_pre_eq.band5_type.value(),
            params.movement_post_eq.band5_type.value(),
            params.diffusion_pre_eq.band5_type.value(),
            params.diffusion_post_eq.band5_type.value(),
            params.texture_pre_eq.band5_type.value(),
        ];
        assert_eq!(after, before);
    }
}
