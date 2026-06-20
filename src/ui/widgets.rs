use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};

use crate::params::Cc22Params;

use super::theme::{
    ModuleColors, Theme, FONT_CONTROL_LABEL, FONT_HINT, FONT_SECONDARY, FONT_VALUE_LABEL,
};

pub(crate) fn brand_mark(ui: &mut egui::Ui, colors: ModuleColors, theme: Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(33.0, 24.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), theme.text_dark);

    let dots = [
        (colors.character, 0.22, 0.34),
        (colors.movement, 0.46, 0.66),
        (colors.diffusion, 0.70, 0.34),
        (colors.texture, 0.34, 0.78),
        (colors.eq, 0.76, 0.72),
    ];

    for (color, x, y) in dots {
        ui.painter().circle_filled(
            Pos2::new(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            2.6,
            color,
        );
    }
}

pub(crate) fn small_strip_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
) {
    ui.vertical_centered(|ui| {
        ui.set_min_width(46.0);
        let size = 30.0;
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
        handle_float_drag(ui, setter, param, &response);
        let center = rect.center();
        let normalized = param.unmodulated_normalized_value().clamp(0.0, 1.0);
        ui.painter()
            .circle_filled(center, 12.0, Color32::from_rgb(238, 232, 216));
        ui.painter().circle_stroke(
            center,
            13.0,
            Stroke::new(1.5, Color32::from_rgb(92, 84, 72)),
        );
        let start = core::f32::consts::PI * 0.75;
        let end = core::f32::consts::PI * 2.25;
        let current = start + ((end - start) * normalized);
        paint_arc(ui, center, 16.0, start, current, accent, 2.3);
        ui.painter().line_segment(
            [
                center,
                Pos2::new(
                    center.x + current.cos() * 8.0,
                    center.y + current.sin() * 8.0,
                ),
            ],
            Stroke::new(1.8, accent),
        );
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

    {
        let painter = ui.painter();
        painter.circle_filled(
            center + Vec2::new(1.5, 2.0),
            radius + 7.0,
            Color32::from_rgba_premultiplied(62, 52, 40, 38),
        );
        painter.circle_filled(center, radius + 7.0, Color32::from_rgb(189, 181, 166));
        painter.circle_filled(center, radius + 3.0, Color32::from_rgb(245, 241, 232));
        painter.circle_stroke(
            center,
            radius + 6.0,
            Stroke::new(1.1, Color32::from_rgb(155, 145, 129)),
        );
        painter.circle_filled(
            Pos2::new(center.x - radius * 0.22, center.y - radius * 0.25),
            radius * 0.14,
            Color32::from_rgba_premultiplied(255, 255, 255, 90),
        );
    }

    let start = core::f32::consts::PI * 0.72;
    let end = core::f32::consts::PI * 2.28;
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
            .line_segment([inner, outer], Stroke::new(0.75, theme.muted_dark));
    }

    let indicator = Pos2::new(
        center.x + current.cos() * radius * 0.66,
        center.y + current.sin() * radius * 0.66,
    );
    let painter = ui.painter();
    painter.line_segment([center, indicator], Stroke::new(3.0, accent));
    painter.circle_filled(center, radius * 0.10, theme.text_dark);
}

pub(crate) fn paint_mini_slider(
    ui: &mut egui::Ui,
    rect: Rect,
    normalized: f32,
    accent: Color32,
    theme: Theme,
) {
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(5),
        Color32::from_rgb(246, 241, 229),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, theme.card_edge),
        StrokeKind::Outside,
    );
    let fill = Rect::from_min_max(
        rect.left_top(),
        Pos2::new(
            rect.left() + rect.width() * normalized.clamp(0.0, 1.0),
            rect.bottom(),
        ),
    )
    .shrink(2.0);
    painter.rect_filled(fill, CornerRadius::same(4), accent);
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
    let bypassed = param.value();
    let response = ui.add(
        egui::Button::new(
            RichText::new(if bypassed { "BYPASSED" } else { "GLOBAL ON" })
                .font(FontId::monospace(FONT_SECONDARY))
                .strong(),
        )
        .fill(if bypassed { theme.warning } else { theme.paper })
        .stroke(Stroke::new(1.0, theme.text_dark))
        .corner_radius(CornerRadius::same(10))
        .min_size(Vec2::new(94.0, 30.0)),
    );
    if response.clicked() {
        set_param(setter, param, !bypassed);
    }
}

pub(crate) fn compact_button(
    ui: &mut egui::Ui,
    label: &'static str,
    theme: Theme,
    accent: Color32,
) -> egui::Response {
    ui.add(
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
    )
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
