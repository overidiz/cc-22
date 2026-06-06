use nih_plug_egui::egui::{
    self, pos2, Align2, Color32, CornerRadius, FontId, LayerId, Order, Pos2, Rect, Stroke,
    StrokeKind, Vec2,
};

use super::theme::UI_SCALE_OPTIONS;

// ── palette ──────────────────────────────────────────────────────────
const OVERLAY_BG: Color32 = Color32::from_rgba_premultiplied(8, 8, 12, 185);
const PANEL_BG: Color32 = Color32::from_rgb(28, 28, 32);
const PANEL_STROKE: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 22);
const DIVIDER: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 14);

const TITLE_COLOR: Color32 = Color32::from_rgb(235, 235, 240);
const LABEL_COLOR: Color32 = Color32::from_rgb(145, 145, 152);
const CLOSE_COLOR: Color32 = Color32::from_rgb(130, 130, 138);
const CLOSE_HOVER: Color32 = Color32::from_rgb(210, 210, 216);

const BTN_IDLE_BG: Color32 = Color32::from_rgb(42, 42, 48);
const BTN_IDLE_STROKE: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 10);
const BTN_HOVER_BG: Color32 = Color32::from_rgb(52, 52, 60);
const BTN_HOVER_STROKE: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 18);
const BTN_ACTIVE_BG: Color32 = Color32::from_rgb(82, 130, 225);
const BTN_ACTIVE_STROKE: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 30);
const BTN_IDLE_TEXT: Color32 = Color32::from_rgb(185, 185, 192);
const BTN_HOVER_TEXT: Color32 = Color32::from_rgb(220, 220, 226);
const BTN_ACTIVE_TEXT: Color32 = Color32::from_rgb(255, 255, 255);

// ── helpers ───────────────────────────────────────────────────────────
#[inline]
fn pointer_pos(ctx: &egui::Context) -> Option<Pos2> {
    ctx.input(|i| i.pointer.latest_pos())
}

fn rect_contains(rect: Rect, pos: Option<Pos2>) -> bool {
    pos.map_or(false, |p| rect.contains(p))
}

fn hovered(ctx: &egui::Context, rect: Rect) -> bool {
    rect_contains(rect, pointer_pos(ctx))
}

fn bg_clicked(ctx: &egui::Context, panel: Rect) -> bool {
    ctx.input(|i| {
        i.pointer.primary_clicked() && i.pointer.latest_pos().map_or(false, |p| !panel.contains(p))
    })
}

fn rect_clicked(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.input(|i| {
        i.pointer.primary_clicked() && i.pointer.latest_pos().map_or(false, |p| rect.contains(p))
    })
}

// ── paint helpers ─────────────────────────────────────────────────────
fn btn_colors(hovered: bool, selected: bool) -> (Color32, Color32, Color32) {
    if selected {
        (BTN_ACTIVE_BG, BTN_ACTIVE_STROKE, BTN_ACTIVE_TEXT)
    } else if hovered {
        (BTN_HOVER_BG, BTN_HOVER_STROKE, BTN_HOVER_TEXT)
    } else {
        (BTN_IDLE_BG, BTN_IDLE_STROKE, BTN_IDLE_TEXT)
    }
}

fn paint_x(painter: &egui::Painter, center: Pos2, arm: f32, color: Color32) {
    let s = Stroke::new(1.8, color);
    painter.line_segment(
        [
            pos2(center.x - arm, center.y - arm),
            pos2(center.x + arm, center.y + arm),
        ],
        s,
    );
    painter.line_segment(
        [
            pos2(center.x + arm, center.y - arm),
            pos2(center.x - arm, center.y + arm),
        ],
        s,
    );
}

// ── panel ─────────────────────────────────────────────────────────────
pub(crate) fn settings_panel(
    ctx: &egui::Context,
    ui_scale: &mut u32,
    settings_open: &mut bool,
    mut on_scale_changed: impl FnMut(u32),
) {
    if !*settings_open {
        return;
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *settings_open = false;
        return;
    }

    let layer_id = LayerId::new(Order::Foreground, egui::Id::new("settings-overlay"));
    let screen = ctx.available_rect();

    // ── geometry ──────────────────────────────────────────────────────
    let panel_w = 480.0;
    let panel_h = 144.0;
    let panel = Rect::from_center_size(screen.center(), Vec2::new(panel_w, panel_h));
    let radius = CornerRadius::same(11);

    let pad_x = 22.0;
    let div_y = panel.min.y + 42.0;

    let close_w = 28.0;
    let close = Rect::from_center_size(
        pos2(panel.max.x - pad_x - close_w / 2.0, panel.min.y + 21.0),
        Vec2::splat(close_w),
    );

    let label_y = div_y + 14.0;

    let total = UI_SCALE_OPTIONS.len() as f32;
    let btn_h = 30.0;
    let btn_gap = 7.0;
    let btn_row_w = panel_w - pad_x * 2.0;
    let btn_w = (btn_row_w - btn_gap * (total - 1.0)) / total;
    let btn_row_x = panel.min.x + pad_x;
    let btn_y = label_y + 24.0;

    // ── input ─────────────────────────────────────────────────────────
    if bg_clicked(ctx, panel) {
        *settings_open = false;
        return;
    }

    let close_hover = hovered(ctx, close);
    if rect_clicked(ctx, close) {
        *settings_open = false;
        return;
    }

    let mut changed = None;
    for (i, &opt) in UI_SCALE_OPTIONS.iter().enumerate() {
        let x = btn_row_x + i as f32 * (btn_w + btn_gap);
        let btn = Rect::from_min_size(pos2(x, btn_y), Vec2::new(btn_w, btn_h));
        if rect_clicked(ctx, btn) {
            changed = Some(opt);
        }
    }

    // ── paint ─────────────────────────────────────────────────────────
    let painter = ctx.layer_painter(layer_id);

    // overlay
    painter.rect_filled(screen, CornerRadius::ZERO, OVERLAY_BG);

    // panel body
    painter.rect_filled(panel, radius, PANEL_BG);
    painter.rect_stroke(
        panel,
        radius,
        Stroke::new(1.0, PANEL_STROKE),
        StrokeKind::Inside,
    );

    // divider
    painter.line_segment(
        [
            pos2(panel.min.x + pad_x, div_y),
            pos2(panel.max.x - pad_x, div_y),
        ],
        Stroke::new(1.0, DIVIDER),
    );

    // title
    painter.text(
        pos2(panel.min.x + pad_x, panel.min.y + 14.0),
        Align2::LEFT_TOP,
        "SETTINGS",
        FontId::proportional(15.0),
        TITLE_COLOR,
    );

    // X button
    paint_x(
        &painter,
        close.center(),
        6.0,
        if close_hover {
            CLOSE_HOVER
        } else {
            CLOSE_COLOR
        },
    );

    // label
    painter.text(
        pos2(panel.min.x + pad_x, label_y),
        Align2::LEFT_TOP,
        "UI scaling \u{2014} %",
        FontId::monospace(10.5),
        LABEL_COLOR,
    );

    // scale buttons
    for (i, &opt) in UI_SCALE_OPTIONS.iter().enumerate() {
        let x = btn_row_x + i as f32 * (btn_w + btn_gap);
        let btn = Rect::from_min_size(pos2(x, btn_y), Vec2::new(btn_w, btn_h));
        let selected = opt == *ui_scale;
        let hov = !selected && hovered(ctx, btn);
        let (bg, stroke, text) = btn_colors(hov, selected);

        painter.rect_filled(btn, CornerRadius::same(6), bg);
        painter.rect_stroke(
            btn,
            CornerRadius::same(6),
            Stroke::new(1.0, stroke),
            StrokeKind::Inside,
        );

        painter.text(
            btn.center(),
            Align2::CENTER_CENTER,
            format!("{}", opt),
            FontId::monospace(10.5),
            text,
        );
    }

    if let Some(scale) = changed {
        *ui_scale = scale;
        on_scale_changed(scale);
    }
}
