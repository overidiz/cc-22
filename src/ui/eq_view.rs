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
    meters::{EqBandSelection, EqPositionSelection},
    theme::{ModuleColors, Theme},
    widgets::set_param,
};

const EQ_DISPLAY_SAMPLE_RATE: f32 = 48_000.0;
pub(crate) const EQ_DISPLAY_MIN_HZ: f32 = 10.0;
pub(crate) const EQ_DISPLAY_MAX_HZ: f32 = 20_000.0;
const EQ_DISPLAY_DB_RANGE: f32 = 24.0;
const EQ_CURVE_POINTS: usize = 240;
const EQ_NODE_COUNT: usize = 5;
pub(crate) const EQ_WORKBENCH_HEIGHT: f32 = 190.0;
const EQ_CANVAS_HEIGHT: f32 = 126.0;
const EQ_INNER_PADDING: i8 = 8;
const EQ_NODE_EDGE_INSET: f32 = 20.0;
const EQ_GAIN_MIN_DB: f32 = -24.0;
const EQ_GAIN_MAX_DB: f32 = 24.0;
const EQ_Q_MIN: f32 = 0.1;
const EQ_Q_MAX: f32 = 12.0;
const EQ_RESET_FREQUENCIES: [f32; EQ_NODE_COUNT] = [80.0, 250.0, 1_000.0, 4_000.0, 12_000.0];

/// The EQ has a single accent colour now (no per-module target).
fn eq_accent_color(colors: ModuleColors) -> Color32 {
    colors.eq
}

/// Resolve the parameter bank for the position the UI is currently editing. Both
/// banks always run in the DSP; this only picks which one the workbench shows.
pub(crate) fn selected_eq_params(
    params: &Cc22Params,
    position: EqPositionSelection,
) -> &dyn EqParamRefs {
    match position {
        EqPositionSelection::Pre => &params.pre_eq,
        EqPositionSelection::Post => &params.post_eq,
    }
}

fn eq_active(params: &dyn EqParamRefs) -> bool {
    !params.bypass().value() && params.mode().value() == EqMode::On
}

pub(crate) fn eq_workbench(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_position: &mut EqPositionSelection,
    selected_eq_band: &mut EqBandSelection,
    spectrum: &[f32],
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
                        selected_eq_position,
                        selected_eq_band,
                        colors,
                        theme,
                    );
                    ui.add_space(3.0);
                    eq_separator(ui, theme);
                    ui.add_space(4.0);
                    let eq_params = selected_eq_params(params, *selected_eq_position);
                    eq_body(
                        ui,
                        setter,
                        eq_params,
                        selected_eq_band,
                        spectrum,
                        colors,
                        theme,
                    );
                });
        },
    );
}

fn eq_header(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    selected_eq_position: &mut EqPositionSelection,
    selected_eq_band: &mut EqBandSelection,
    colors: ModuleColors,
    theme: Theme,
) {
    // EQUALIZER   [PRE] [POST]   [ON]   B1–B5   [RESET]
    let accent = eq_accent_color(colors);
    let eq_params = selected_eq_params(params, *selected_eq_position);
    let active = eq_active(eq_params);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            RichText::new("EQUALIZER")
                .font(FontId::monospace(13.0))
                .strong()
                .color(theme.text_dark),
        );
        ui.add_space(4.0);
        eq_position_tabs(ui, selected_eq_position, accent, theme);
        eq_toolbar_divider(ui, theme);
        if eq_toggle_button(ui, active, accent, theme)
            .on_hover_text(
                "Enable/bypass this EQ. Pre and Post both run; this toggles the one shown.",
            )
            .clicked()
        {
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
        let reset_tip = format!(
            "Reset the {} EQ back to flat defaults.",
            selected_eq_position.label()
        );
        if eq_reset_button(ui, accent, theme)
            .on_hover_text(reset_tip)
            .clicked()
        {
            reset_eq_params_to_defaults(setter, eq_params);
            *selected_eq_band = EqBandSelection::Band1;
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

fn eq_position_tabs(
    ui: &mut egui::Ui,
    selected_eq_position: &mut EqPositionSelection,
    eq_accent: Color32,
    theme: Theme,
) {
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
    spectrum: &[f32],
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
                Stroke::new(0.3, Color32::from_rgba_premultiplied(128, 116, 96, 20)),
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
            Stroke::new(0.55, Color32::from_rgba_premultiplied(119, 106, 88, 40)),
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
        (3.0, "+3"),
        (0.0, "0"),
        (-3.0, "-3"),
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
                if is_zero { 1.0 } else { 0.4 },
                if is_zero {
                    Color32::from_rgba_premultiplied(171, 158, 138, 200)
                } else {
                    Color32::from_rgba_premultiplied(116, 104, 88, 28)
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

    // Input spectrum overlay: a soft, signal-following analyzer drawn behind the
    // curve so the EQ shape stays the focus. Columns are log-spaced to line up
    // with the frequency axis; heights come pre-smoothed from the UI thread.
    if spectrum.len() >= 2 {
        let cols = spectrum.len();
        // Keep this a faint, translucent wash so it never veils the curve/grid:
        // a low-alpha fill and a thin soft top line, not a solid block.
        let spectrum_fill = Color32::from_rgba_unmultiplied(112, 134, 152, 20);
        let spectrum_edge = Color32::from_rgba_unmultiplied(118, 146, 168, 55);
        let base_y = plot_rect.bottom();
        let plot_h = plot_rect.height();
        let mut prev: Option<Pos2> = None;
        for (c, mag) in spectrum.iter().enumerate() {
            let t = c as f32 / (cols - 1) as f32;
            let x = plot_rect.left() + plot_rect.width() * t;
            let y = base_y - mag.clamp(0.0, 1.0) * plot_h;
            let top = Pos2::new(x, y);
            if let Some(p) = prev {
                // Skip near-silent columns so the floor stays perfectly clean.
                if (base_y - top.y) > 0.5 || (base_y - p.y) > 0.5 {
                    painter.add(egui::Shape::convex_polygon(
                        vec![p, top, Pos2::new(top.x, base_y), Pos2::new(p.x, base_y)],
                        spectrum_fill,
                        Stroke::NONE,
                    ));
                    painter.line_segment([p, top], Stroke::new(0.8, spectrum_edge));
                }
            }
            prev = Some(top);
        }
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

    // Elegant warm-orange curve (chroma-like): a thin stroke over a very soft
    // fill to zero, so the shape reads clearly without veiling the grid.
    let curve_color = if eq_active(params) {
        Color32::from_rgb(255, 140, 50)
    } else {
        theme.muted
    };
    let zero_y = y_from_gain_db(plot_rect, 0.0);
    let fill_color = if eq_active(params) {
        Color32::from_rgba_premultiplied(255, 140, 50, 12)
    } else {
        Color32::from_rgba_premultiplied(theme.muted.r(), theme.muted.g(), theme.muted.b(), 7)
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
        Stroke::new(if eq_active(params) { 1.7 } else { 1.4 }, curve_color),
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

        // Chroma-style "bubble" node: a soft colored halo, a white ring, the
        // colored dot, and a white outline — clearest when selected, dimmed and
        // discreet when the band is Off (not draggable).
        let halo = if selected {
            12.0
        } else if hovered {
            9.0
        } else {
            7.0
        };
        let halo_alpha: u8 = if selected {
            90
        } else if hovered {
            56
        } else {
            28
        };
        let dot_r = if selected {
            6.5
        } else if hovered {
            6.0
        } else {
            5.0
        };
        let dot_color = if node.draggable {
            node.color
        } else {
            Color32::from_rgba_premultiplied(node.color.r(), node.color.g(), node.color.b(), 80)
        };

        painter.circle_filled(
            node.center,
            halo,
            Color32::from_rgba_premultiplied(
                node.color.r(),
                node.color.g(),
                node.color.b(),
                halo_alpha,
            ),
        );
        painter.circle_filled(
            node.center,
            dot_r + 1.2,
            Color32::from_rgba_premultiplied(255, 255, 255, 190),
        );
        painter.circle_filled(node.center, dot_r, dot_color);
        painter.circle_stroke(
            node.center,
            dot_r,
            Stroke::new(
                if selected { 1.6 } else { 1.0 },
                Color32::from_rgba_premultiplied(255, 255, 255, if selected { 235 } else { 150 }),
            ),
        );
        // Band number above the node (always visible, like the old chroma EQ).
        painter.text(
            Pos2::new(node.center.x, node.center.y - dot_r - 7.0),
            egui::Align2::CENTER_CENTER,
            format!("{}", node.index + 1),
            FontId::monospace(7.5),
            Color32::from_rgba_premultiplied(
                theme.text_dark.r(),
                theme.text_dark.g(),
                theme.text_dark.b(),
                if selected { 235 } else { 150 },
            ),
        );
    }
    let _ = canvas_response;
}

fn eq_body(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    selected_eq_band: &mut EqBandSelection,
    spectrum: &[f32],
    colors: ModuleColors,
    theme: Theme,
) {
    // The curve is the instrument: bands are edited directly on the graph — drag a
    // node for frequency + gain, scroll for Q, right-click for the band type (Off
    // disables it), double-click to reset. A single compact readout line below
    // summarises the selected band (the "inspector", chroma-style).
    let canvas_width = ui.available_width().max(0.0);
    eq_canvas(
        ui,
        setter,
        params,
        selected_eq_band,
        spectrum,
        colors,
        theme,
        canvas_width,
    );

    ui.add_space(4.0);
    eq_band_readout(ui, setter, params, *selected_eq_band, colors, theme);
}

/// Compact, *editable* inspector for the selected band: a colored dot, the band
/// label/type, and Freq / Gain / Q drag-values (click to type for precision;
/// gain hidden for HP/LP, which ignore it). The graph stays the primary way to
/// edit; these give exact numeric entry.
fn eq_band_readout(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &dyn EqParamRefs,
    selected_eq_band: EqBandSelection,
    colors: ModuleColors,
    theme: Theme,
) {
    let index = selected_eq_band.index();
    let band_type = params.band_type(index).value();
    let dot = eq_band_color(selected_eq_band, colors);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 14.0), Sense::hover());
        ui.painter().circle_filled(dot_rect.center(), 3.5, dot);
        ui.label(
            RichText::new(format!("B{}", index + 1))
                .font(FontId::monospace(9.5))
                .strong()
                .color(theme.text_dark),
        );
        ui.label(
            RichText::new(eq_band_type_menu_label(band_type))
                .font(FontId::monospace(9.0))
                .color(theme.muted_dark),
        );

        // Freq (typeable). Coarse drag here; the graph is the fine control.
        let mut freq = params.band_frequency(index).value();
        if ui
            .add(
                egui::DragValue::new(&mut freq)
                    .speed(2.0)
                    .range(20.0..=20_000.0)
                    .max_decimals(0)
                    .suffix(" Hz"),
            )
            .on_hover_text("Centre frequency (click to type)")
            .changed()
        {
            set_param(
                setter,
                params.band_frequency(index),
                freq.clamp(20.0, 20_000.0),
            );
        }

        // Gain only for types that use it.
        if eq_type_uses_gain(band_type) {
            let mut gain = params.band_gain(index).value();
            if ui
                .add(
                    egui::DragValue::new(&mut gain)
                        .speed(0.1)
                        .range(-24.0..=24.0)
                        .max_decimals(1)
                        .suffix(" dB"),
                )
                .on_hover_text("Gain (click to type)")
                .changed()
            {
                set_param(setter, params.band_gain(index), gain.clamp(-24.0, 24.0));
            }
        }

        // Q / bandwidth.
        let mut q = params.band_q(index).value();
        if ui
            .add(
                egui::DragValue::new(&mut q)
                    .speed(0.02)
                    .range(0.1..=12.0)
                    .max_decimals(2)
                    .prefix("Q "),
            )
            .on_hover_text("Q / bandwidth (click to type)")
            .changed()
        {
            set_param(setter, params.band_q(index), q.clamp(0.1, 12.0));
        }

        // Discreet gesture hint (right-aligned) so the canvas interactions stay
        // discoverable.
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(
                    "drag \u{00B7} scroll=Q \u{00B7} dbl-click=reset \u{00B7} right-click=type",
                )
                .font(FontId::monospace(8.0))
                .color(theme.muted),
            );
        });
    });
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
        }
    })
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

fn reset_eq_params_to_defaults(setter: &ParamSetter<'_>, eq_params: &dyn EqParamRefs) {
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
    use nih_plug::prelude::EnumParam;

    use crate::{params::Cc22Params, ui::meters::EqPositionSelection};

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
    fn pre_and_post_select_distinct_banks() {
        let params = Cc22Params::default();
        let pre =
            selected_eq_params(&params, EqPositionSelection::Pre).band_gain(0) as *const _ as usize;
        let post = selected_eq_params(&params, EqPositionSelection::Post).band_gain(0) as *const _
            as usize;
        assert_ne!(pre, post, "Pre and Post must be independent EQ banks");
        // And they resolve to the actual pre_eq / post_eq params.
        assert_eq!(pre, &params.pre_eq.band1_gain as *const _ as usize);
        assert_eq!(post, &params.post_eq.band1_gain as *const _ as usize);
    }

    #[test]
    fn editing_post_band_does_not_change_pre_band() {
        let mut params = Cc22Params::default();
        let pre_before = params.pre_eq.band5_type.value();
        let right_click_type = direct_right_click_band_type(4).unwrap();
        params.post_eq.band5_type = EnumParam::new("Type", right_click_type);

        assert_eq!(params.post_eq.band5_type.value(), EqBandType::LowPass);
        assert_eq!(
            params.pre_eq.band5_type.value(),
            pre_before,
            "editing Post must not change Pre"
        );
    }
}
