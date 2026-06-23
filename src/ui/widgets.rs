use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, ColorImage, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, TextureHandle, TextureOptions, Vec2,
};

use crate::params::Cc22Params;

use super::meters::UiState;
use super::theme::{Theme, FONT_CONTROL_LABEL, FONT_HINT, FONT_SECONDARY, FONT_VALUE_LABEL};

/// Side of the square, tileable vintage-grain texture (in texels).
const GRAIN_TILE: usize = 128;

/// Lazily build the tileable grain once. Each texel is a faint white or black
/// fleck with a small alpha, so when tiled over any surface it adds both micro
/// highlights and micro shadows — a subtle matte/painted finish, never dirt.
fn grain_texture(ui: &egui::Ui, state: &mut UiState) -> TextureHandle {
    state
        .grain_texture
        .get_or_insert_with(|| {
            let mut pixels = Vec::with_capacity(GRAIN_TILE * GRAIN_TILE);
            let mut seed: u32 = 0x9E37_79B9;
            for _ in 0..GRAIN_TILE * GRAIN_TILE {
                // xorshift32 — deterministic, no allocations.
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let n = (seed >> 8) as f32 / ((1u32 << 24) as f32); // 0..1
                                                                    // Strong cubic bias: most texels invisible, only a sparse
                                                                    // few carry a faint fleck (matte finish, not static).
                let alpha = (n * n * n * 90.0) as u8;
                let light = (seed & 1) == 0;
                pixels.push(if light {
                    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
                } else {
                    Color32::from_rgba_unmultiplied(0, 0, 0, alpha)
                });
            }
            let image = ColorImage {
                size: [GRAIN_TILE, GRAIN_TILE],
                pixels,
            };
            ui.ctx()
                .load_texture("cc22-grain", image, TextureOptions::NEAREST_REPEAT)
        })
        .clone()
}

/// Paint the tiled grain over `rect` at the given strength (`alpha` ≈ 8–24),
/// clipped to the rect. 1 texel ≈ 1 logical px so the fleck stays fine.
pub(crate) fn paint_grain(ui: &egui::Ui, state: &mut UiState, rect: Rect, alpha: u8) {
    let texture = grain_texture(ui, state);
    let uv = Rect::from_min_max(
        Pos2::ZERO,
        Pos2::new(
            rect.width() / GRAIN_TILE as f32,
            rect.height() / GRAIN_TILE as f32,
        ),
    );
    ui.painter().with_clip_rect(rect).image(
        texture.id(),
        rect,
        uv,
        Color32::from_white_alpha(alpha),
    );
}

pub(crate) fn small_strip_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
    size: f32,
) {
    ui.vertical_centered(|ui| {
        ui.set_min_width(size + 16.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
        handle_float_drag(ui, setter, param, &response);
        let center = rect.center();
        let normalized = param.unmodulated_normalized_value().clamp(0.0, 1.0);
        // Inner radii scale with the dial size so a larger knob (Dry/Wet) stays
        // proportional to the small Input/Output dials.
        let s = size / 30.0;
        let pi = core::f32::consts::PI;
        let rim = 13.5 * s;
        let face = 12.0 * s;
        {
            let painter = ui.painter();
            // Soft seat shadow on the dark master strip.
            painter.circle_filled(
                center + Vec2::new(0.6, 1.4),
                rim + 0.5,
                Color32::from_rgba_premultiplied(0, 0, 0, 70),
            );
            // Metal rim + convex cream face (lighter pool offset up = domed).
            painter.circle_filled(center, rim, Color32::from_rgb(120, 110, 96));
            painter.circle_filled(center, face, Color32::from_rgb(224, 217, 201));
            painter.circle_filled(
                center - Vec2::new(0.0, face * 0.12),
                face * 0.92,
                Color32::from_rgb(241, 235, 221),
            );
            painter.circle_filled(
                center - Vec2::new(face * 0.18, face * 0.26),
                face * 0.5,
                Color32::from_rgba_premultiplied(255, 255, 255, 70),
            );
        }
        // Rim bevel: lit top, shaded bottom.
        paint_arc(
            ui,
            center,
            rim - 0.6,
            pi * 1.15,
            pi * 1.95,
            Color32::from_rgba_premultiplied(255, 255, 255, 120),
            1.3,
        );
        paint_arc(
            ui,
            center,
            rim - 0.6,
            pi * 0.12,
            pi * 0.92,
            Color32::from_rgba_premultiplied(0, 0, 0, 90),
            1.3,
        );
        let start = pi * 0.75;
        let end = pi * 2.25;
        let current = start + ((end - start) * normalized);
        paint_arc(ui, center, 16.0 * s, start, current, accent, 2.3 * s);
        let tip = Pos2::new(
            center.x + current.cos() * 8.0 * s,
            center.y + current.sin() * 8.0 * s,
        );
        ui.painter().line_segment(
            [center + Vec2::new(0.5, 0.7), tip + Vec2::new(0.5, 0.7)],
            Stroke::new(2.0 * s, Color32::from_rgba_premultiplied(0, 0, 0, 60)),
        );
        ui.painter()
            .line_segment([center, tip], Stroke::new(1.8 * s, accent));
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(FONT_HINT))
                .strong()
                .color(Color32::from_rgb(245, 237, 218)),
        );
        response.on_hover_text(format!("{}: {}", param.name(), value_string(param)));
        let _ = theme;
    });
}

pub(crate) fn colored_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
    size: f32,
) -> egui::Response {
    ui.allocate_ui(Vec2::new(size + 18.0, size + 28.0), |ui| {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(label)
                    .font(FontId::monospace(FONT_CONTROL_LABEL))
                    .strong()
                    .color(theme.text_light),
            );
            let (rect, response) =
                ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
            handle_float_drag(ui, setter, param, &response);
            paint_colored_knob(
                ui,
                rect,
                param.unmodulated_normalized_value(),
                accent,
                theme,
            );
            let value_color = if response.dragged() {
                accent
            } else {
                theme.muted
            };
            ui.label(
                RichText::new(value_string(param))
                    .font(FontId::monospace(FONT_VALUE_LABEL))
                    .color(value_color),
            );
            response.on_hover_text(format!("{}: {}", param.name(), value_string(param)))
        })
        .inner
    })
    .inner
}

pub(crate) fn mini_slider(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
) -> egui::Response {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(label)
                    .font(FontId::monospace(FONT_CONTROL_LABEL))
                    .color(theme.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(value_string(param))
                        .font(FontId::monospace(FONT_VALUE_LABEL))
                        .color(theme.muted),
                );
            });
        });

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(80.0), 13.0),
            Sense::click_and_drag(),
        );
        handle_float_drag_horizontal(ui, setter, param, &response, rect);
        paint_mini_slider(
            ui,
            rect,
            param.unmodulated_normalized_value(),
            accent,
            theme,
        );
        response.on_hover_text(format!("{}: {}", param.name(), value_string(param)))
    })
    .inner
}

pub(crate) fn handle_float_drag(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    response: &egui::Response,
) {
    if response.drag_started() {
        setter.begin_set_parameter(param);
    }

    if response.dragged() {
        let (delta_y, fine) = ui.input(|input| (input.pointer.delta().y, input.modifiers.shift));
        let speed = if fine { 0.0015 } else { 0.006 };
        let normalized = (param.unmodulated_normalized_value() - (delta_y * speed)).clamp(0.0, 1.0);
        setter.set_parameter_normalized(param, normalized);
    }

    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }

    if response.double_clicked() {
        set_param(setter, param, param.default_plain_value());
    }
}

pub(crate) fn handle_float_drag_horizontal(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    response: &egui::Response,
    rect: Rect,
) {
    if response.drag_started() {
        setter.begin_set_parameter(param);
    }

    if response.dragged() || response.clicked() {
        if let Some(pos) = ui.input(|input| input.pointer.interact_pos()) {
            let mut normalized = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            if ui.input(|input| input.modifiers.shift) {
                let current = param.unmodulated_normalized_value();
                normalized = current + ((normalized - current) * 0.25);
            }
            setter.set_parameter_normalized(param, normalized);
        }
    }

    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }

    if response.double_clicked() {
        set_param(setter, param, param.default_plain_value());
    }
}

pub(crate) fn paint_colored_knob(
    ui: &mut egui::Ui,
    rect: Rect,
    normalized: f32,
    accent: Color32,
    theme: Theme,
) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.40;
    let normalized = normalized.clamp(0.0, 1.0);
    let pi = core::f32::consts::PI;
    let rim = radius + 7.0;
    let face = radius + 3.0;

    {
        let painter = ui.painter();
        // Short, soft drop shadow — the knob sits a hair above the card.
        painter.circle_filled(
            center + Vec2::new(0.8, 1.8),
            rim + 1.0,
            Color32::from_rgba_premultiplied(40, 33, 25, 40),
        );
        // Brushed-metal rim.
        painter.circle_filled(center, rim, Color32::from_rgb(196, 188, 172));
    }
    // Rim bevel: lit on the top, shaded underneath (light from top-left).
    paint_arc(
        ui,
        center,
        rim - 0.7,
        pi * 1.15,
        pi * 1.95,
        Color32::from_rgba_premultiplied(255, 255, 255, 150),
        1.7,
    );
    paint_arc(
        ui,
        center,
        rim - 0.7,
        pi * 0.12,
        pi * 0.92,
        Color32::from_rgba_premultiplied(64, 54, 42, 120),
        1.7,
    );
    {
        let painter = ui.painter();
        // Convex face: a darker base with a lighter pool offset upward leaves a
        // soft shaded crescent along the bottom, reading as a domed cap.
        painter.circle_filled(center, face, Color32::from_rgb(231, 226, 214));
        painter.circle_filled(
            center - Vec2::new(0.0, face * 0.12),
            face * 0.95,
            Color32::from_rgb(245, 241, 232),
        );
        // Broad top sheen + a crisp specular highlight.
        painter.circle_filled(
            center - Vec2::new(face * 0.16, face * 0.24),
            face * 0.6,
            Color32::from_rgba_premultiplied(255, 255, 253, 60),
        );
        painter.circle_filled(
            Pos2::new(center.x - radius * 0.24, center.y - radius * 0.28),
            radius * 0.12,
            Color32::from_rgba_premultiplied(255, 255, 255, 120),
        );
        painter.circle_stroke(
            center,
            face,
            Stroke::new(0.8, Color32::from_rgb(170, 160, 144)),
        );
    }

    let start = pi * 0.72;
    let end = pi * 2.28;
    let current = start + ((end - start) * normalized);
    paint_arc(ui, center, radius + 10.0, start, current, accent, 3.2);
    paint_arc(
        ui,
        center,
        radius + 10.0,
        current,
        end,
        Color32::from_rgb(180, 171, 155),
        1.0,
    );

    for tick in 0..=6 {
        let t = tick as f32 / 6.0;
        let angle = start + ((end - start) * t);
        let inner = Pos2::new(
            center.x + angle.cos() * (radius + 14.0),
            center.y + angle.sin() * (radius + 14.0),
        );
        let outer = Pos2::new(
            center.x + angle.cos() * (radius + 17.0),
            center.y + angle.sin() * (radius + 17.0),
        );
        ui.painter()
            .line_segment([inner, outer], Stroke::new(0.6, theme.muted));
    }

    let indicator = Pos2::new(
        center.x + current.cos() * radius * 0.66,
        center.y + current.sin() * radius * 0.66,
    );
    let painter = ui.painter();
    // Pointer with a faint drop shadow under it, an accent shaft and a rounded
    // tip — a small moulded indicator rather than a flat line.
    painter.line_segment(
        [
            center + Vec2::new(0.6, 0.9),
            indicator + Vec2::new(0.6, 0.9),
        ],
        Stroke::new(3.4, Color32::from_rgba_premultiplied(40, 33, 25, 55)),
    );
    painter.line_segment([center, indicator], Stroke::new(3.0, accent));
    painter.circle_filled(indicator, 1.7, accent);
    // Centre hub with a tiny top highlight.
    painter.circle_filled(center, radius * 0.13, theme.text_dark);
    painter.circle_filled(
        center - Vec2::new(0.4, 0.7),
        radius * 0.05,
        Color32::from_rgba_premultiplied(255, 255, 255, 70),
    );
}

pub(crate) fn paint_mini_slider(
    ui: &mut egui::Ui,
    rect: Rect,
    normalized: f32,
    accent: Color32,
    theme: Theme,
) {
    let painter = ui.painter();
    let n = normalized.clamp(0.0, 1.0);
    // Recessed well: slightly darker base with a shaded top edge and a lit
    // bottom edge so the track reads as a shallow groove.
    painter.rect_filled(
        rect,
        CornerRadius::same(5),
        Color32::from_rgb(232, 226, 212),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.9)),
        StrokeKind::Inside,
    );
    let inset = rect.shrink(1.5);
    painter.line_segment(
        [
            Pos2::new(inset.left() + 2.0, inset.top() + 0.6),
            Pos2::new(inset.right() - 2.0, inset.top() + 0.6),
        ],
        Stroke::new(1.0, Color32::from_rgba_premultiplied(60, 52, 42, 45)),
    );
    painter.line_segment(
        [
            Pos2::new(inset.left() + 2.0, inset.bottom() - 0.6),
            Pos2::new(inset.right() - 2.0, inset.bottom() - 0.6),
        ],
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 80)),
    );

    // Clean coloured fill with a faint top gloss.
    let fill = Rect::from_min_max(
        rect.left_top(),
        Pos2::new(rect.left() + rect.width() * n, rect.bottom()),
    )
    .shrink(2.0);
    if fill.width() > 0.5 {
        painter.rect_filled(fill, CornerRadius::same(4), accent);
        painter.line_segment(
            [
                Pos2::new(fill.left() + 1.0, fill.top() + 1.2),
                Pos2::new(fill.right() - 1.0, fill.top() + 1.2),
            ],
            Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 55)),
        );
    }

    // Tactile thumb at the fill edge: a small bevelled grip with a centre notch.
    let tx = (rect.left() + rect.width() * n).clamp(rect.left() + 4.0, rect.right() - 4.0);
    let thumb = Rect::from_center_size(
        Pos2::new(tx, rect.center().y),
        Vec2::new(7.0, rect.height() + 4.0),
    );
    painter.rect_filled(
        thumb.translate(Vec2::new(0.5, 1.0)),
        CornerRadius::same(3),
        Color32::from_rgba_premultiplied(40, 33, 25, 45),
    );
    painter.rect_filled(
        thumb,
        CornerRadius::same(3),
        Color32::from_rgb(248, 244, 236),
    );
    painter.rect_stroke(
        thumb,
        CornerRadius::same(3),
        Stroke::new(1.0, theme.card_edge),
        StrokeKind::Inside,
    );
    painter.line_segment(
        [
            Pos2::new(tx, thumb.top() + 2.5),
            Pos2::new(tx, thumb.bottom() - 2.5),
        ],
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.8)),
    );
}

pub(crate) fn paint_arc(
    ui: &mut egui::Ui,
    center: Pos2,
    radius: f32,
    start: f32,
    end: f32,
    color: Color32,
    width: f32,
) {
    let steps = 24;
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = start + ((end - start) * t);
        points.push(Pos2::new(
            center.x + angle.cos() * radius,
            center.y + angle.sin() * radius,
        ));
    }
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(width, color)));
}

pub(crate) fn global_bypass_button(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &BoolParam,
    theme: Theme,
) {
    // Pill switch consistent with the module ON/OFF and EQ ON toggles: a sliding
    // white knob on a colour-coded track — green = processing, amber = bypassed.
    let bypassed = param.value();
    let active = !bypassed;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(92.0, 26.0), Sense::click());
    if response.clicked() {
        set_param(setter, param, !bypassed);
    }
    let track = if active {
        Color32::from_rgb(48, 178, 100)
    } else {
        theme.warning
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(13), track.gamma_multiply(0.92));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(13),
        Stroke::new(1.0, track.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    let knob_x = if active {
        rect.right() - 13.0
    } else {
        rect.left() + 13.0
    };
    ui.painter().circle_filled(
        Pos2::new(knob_x, rect.center().y),
        9.0,
        Color32::from_rgb(250, 247, 240),
    );
    // State label on the side opposite the knob.
    let text_x = if active {
        rect.left() + 30.0
    } else {
        rect.right() - 30.0
    };
    ui.painter().text(
        Pos2::new(text_x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        if active { "GLOBAL" } else { "BYPASS" },
        FontId::monospace(FONT_SECONDARY),
        Color32::WHITE,
    );
    response.on_hover_text("Global bypass — entire plugin");
}

pub(crate) fn compact_button(
    ui: &mut egui::Ui,
    label: &'static str,
    theme: Theme,
    accent: Color32,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new(
            RichText::new(label)
                .font(FontId::monospace(FONT_SECONDARY))
                .strong()
                .color(theme.text_dark),
        )
        .fill(theme.paper)
        .stroke(Stroke::new(1.0, accent))
        .corner_radius(CornerRadius::same(9))
        .min_size(Vec2::new(48.0, 30.0)),
    );
    // Physical finish: a lit top edge and a faint shaded bottom edge.
    let r = response.rect;
    ui.painter().line_segment(
        [
            Pos2::new(r.left() + 5.0, r.top() + 1.4),
            Pos2::new(r.right() - 5.0, r.top() + 1.4),
        ],
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 150)),
    );
    ui.painter().line_segment(
        [
            Pos2::new(r.left() + 5.0, r.bottom() - 1.4),
            Pos2::new(r.right() - 5.0, r.bottom() - 1.4),
        ],
        Stroke::new(1.0, Color32::from_rgba_premultiplied(60, 52, 42, 30)),
    );
    response
}

pub(crate) fn set_float_normalized(setter: &ParamSetter<'_>, param: &FloatParam, normalized: f32) {
    setter.begin_set_parameter(param);
    setter.set_parameter_normalized(param, normalized.clamp(0.0, 1.0));
    setter.end_set_parameter(param);
}

pub(crate) fn set_param<P: Param>(setter: &ParamSetter<'_>, param: &P, value: P::Plain) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

pub(crate) fn value_string(param: &FloatParam) -> String {
    param.normalized_value_to_string(param.unmodulated_normalized_value(), true)
}

// A module is active whenever it is not bypassed — "off" is the bypass now, so
// there is no longer a dedicated off/clean mode to exclude.
pub(crate) fn character_active(params: &Cc22Params) -> bool {
    !params.character.bypass.value()
}

pub(crate) fn movement_active(params: &Cc22Params) -> bool {
    !params.movement.bypass.value()
}

pub(crate) fn diffusion_active(params: &Cc22Params) -> bool {
    !params.diffusion.bypass.value()
}

pub(crate) fn texture_active(params: &Cc22Params) -> bool {
    !params.texture.bypass.value()
}
