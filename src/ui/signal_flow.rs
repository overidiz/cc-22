use nih_plug_egui::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Stroke, StrokeKind, Vec2,
};

use super::theme::Theme;

pub(crate) fn card_shadow(
    painter: &egui::Painter,
    card_rect: egui::Rect,
    lift: f32,
    accent: Option<Color32>,
) {
    let shadow_offset = 4.0 + lift * 1.5;
    let shadow_alpha = (40.0 + lift * 30.0) as u8;

    let shadow_rect = card_rect.translate(Vec2::new(shadow_offset, shadow_offset));

    let color = if let Some(accent) = accent {
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), shadow_alpha)
    } else {
        Color32::from_rgba_premultiplied(28, 23, 18, shadow_alpha)
    };

    painter.rect_filled(shadow_rect, CornerRadius::same(14), color);
}

// ── signal flow arrows ──────────────────────────────────────────────────

pub(crate) fn position_badge(ui: &mut egui::Ui, pos: Pos2, number: usize, accent: Color32) {
    // A discreet chain-order marker ("01".."04"): a tiny accent tick plus a
    // muted two-digit number, deliberately understated so it reads as "order"
    // rather than an unread-notification badge.
    let tick = egui::Rect::from_min_size(Pos2::new(pos.x - 12.0, pos.y - 4.0), Vec2::new(3.0, 8.0));
    ui.painter().rect_filled(
        tick,
        CornerRadius::same(1),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 150),
    );
    ui.painter().text(
        Pos2::new(pos.x - 5.0, pos.y),
        Align2::LEFT_CENTER,
        format!("{number:02}"),
        FontId::monospace(9.5),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 175),
    );
}

// ── drag handle ─────────────────────────────────────────────────────────

pub(crate) fn drag_handle(
    ui: &mut egui::Ui,
    card_rect: egui::Rect,
    color: Color32,
    position_index: usize,
    hovered: bool,
) -> egui::Response {
    let handle_h = 16.0;
    let handle_rect = egui::Rect::from_min_size(
        Pos2::new(card_rect.max.x - 32.0, card_rect.min.y + 6.0),
        Vec2::new(26.0, handle_h),
    );

    let alpha: u8 = if hovered { 210 } else { 80 };

    if hovered {
        let bg_rect = handle_rect.expand(3.0);
        ui.painter().rect_filled(
            bg_rect,
            CornerRadius::same(5),
            Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 45),
        );
    }

    let dots = [
        Pos2::new(handle_rect.min.x + 6.0, handle_rect.center().y - 3.0),
        Pos2::new(handle_rect.min.x + 13.0, handle_rect.center().y - 3.0),
        Pos2::new(handle_rect.min.x + 6.0, handle_rect.center().y + 3.0),
        Pos2::new(handle_rect.min.x + 13.0, handle_rect.center().y + 3.0),
    ];

    for dot in dots {
        ui.painter().circle_filled(
            dot,
            1.5,
            Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha),
        );
    }

    let id = egui::Id::new(format!("drag-handle-{}", position_index));
    ui.interact(handle_rect, id, egui::Sense::click_and_drag())
}

// ── drop indicator bar ──────────────────────────────────────────────────

pub(crate) fn paint_drop_indicator(
    painter: &egui::Painter,
    x: f32,
    top: f32,
    height: f32,
    accent: Color32,
) {
    let glow_rect =
        egui::Rect::from_min_size(Pos2::new(x - 6.0, top - 4.0), Vec2::new(12.0, height + 8.0));
    let bar_rect =
        egui::Rect::from_min_size(Pos2::new(x - 2.5, top - 2.0), Vec2::new(5.0, height + 4.0));

    let glow = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 50);
    let solid = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 220);

    painter.rect_filled(glow_rect, CornerRadius::same(6), glow);
    painter.rect_filled(bar_rect, CornerRadius::same(3), solid);
}

// ── floating card proxy ─────────────────────────────────────────────────

pub(crate) fn paint_floating_card(
    painter: &egui::Painter,
    rect: egui::Rect,
    accent: Color32,
    title: &str,
    position_num: usize,
    theme: Theme,
) {
    // A clean, solid replica of the real card — looks like the actual module has
    // been lifted off the row. The elevation is conveyed by a soft, close shadow
    // and a slim accent edge, not by a glaring glow.
    let radius = CornerRadius::same(14);

    // Soft, close drop shadow (small offset = low perceived elevation).
    let shadow = rect.translate(Vec2::new(2.0, 4.0));
    painter.rect_filled(
        shadow,
        radius,
        Color32::from_rgba_premultiplied(18, 14, 10, 60),
    );

    // Solid card body in the light "paper" surface, with a restrained accent edge.
    painter.rect_filled(rect, radius, theme.card);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.2, accent.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );

    // Slim top accent bar — the card's colour identity, matching the resting card.
    let accent_bar = egui::Rect::from_min_size(
        Pos2::new(rect.left() + 16.0, rect.top() + 8.0),
        Vec2::new(rect.width() - 32.0, 3.0),
    );
    painter.rect_filled(accent_bar, CornerRadius::same(2), accent);

    // Title.
    painter.text(
        Pos2::new(rect.left() + 16.0, rect.top() + 26.0),
        Align2::LEFT_CENTER,
        title,
        FontId::monospace(super::theme::FONT_MODULE_TITLE),
        theme.text_dark,
    );

    // Discreet chain-order badge (bottom-right), like the resting card.
    let badge = Pos2::new(rect.right() - 18.0, rect.bottom() - 16.0);
    let tick = egui::Rect::from_min_size(
        Pos2::new(badge.x - 12.0, badge.y - 4.0),
        Vec2::new(3.0, 8.0),
    );
    painter.rect_filled(
        tick,
        CornerRadius::same(1),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 160),
    );
    painter.text(
        Pos2::new(badge.x - 5.0, badge.y),
        Align2::LEFT_CENTER,
        format!("{position_num:02}"),
        FontId::monospace(9.5),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 185),
    );

    // Quiet grip dots at top-right echo the drag handle.
    let dots_x = rect.right() - 28.0;
    let dots_y = rect.top() + 14.0;
    let dot = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 150);
    painter.circle_filled(Pos2::new(dots_x, dots_y), 1.5, dot);
    painter.circle_filled(Pos2::new(dots_x + 6.0, dots_y), 1.5, dot);
    painter.circle_filled(Pos2::new(dots_x, dots_y + 5.0), 1.5, dot);
    painter.circle_filled(Pos2::new(dots_x + 6.0, dots_y + 5.0), 1.5, dot);
}

// ── drag reorder helpers ─────────────────────────────────────────────────

/// Compute the visual drop slot (0..=4) from pointer x and card layout.
/// 0 = before first card, 4 = after last card.
pub(crate) fn compute_drop_slot(
    pointer_x: f32,
    card_rects: &[egui::Rect; 4],
    row_start: f32,
) -> usize {
    let slot_center = |s: usize| -> f32 {
        match s {
            0 => row_start,
            4 => card_rects[3].right(),
            _ => (card_rects[s - 1].right() + card_rects[s].left()) * 0.5,
        }
    };

    for i in 0..4 {
        let boundary = (slot_center(i) + slot_center(i + 1)) * 0.5;
        if pointer_x < boundary {
            return i;
        }
    }
    4
}

/// Convert a visual drop slot (0..=4) to the final destination index (0..=3) for `reorder_module`.
pub(crate) fn final_index_from_drop_slot(source: usize, drop_slot: usize) -> usize {
    if drop_slot <= source {
        drop_slot
    } else {
        drop_slot.saturating_sub(1)
    }
}

/// Compute the x position for the drop indicator bar.
pub(crate) fn drop_indicator_x(
    drop_slot: usize,
    card_rects: &[egui::Rect; 4],
    row_start: f32,
    gaps: f32,
) -> f32 {
    match drop_slot {
        0 => row_start - gaps * 0.5,
        4 => card_rects[3].right() + gaps * 0.5,
        s => (card_rects[s - 1].right() + card_rects[s].left()) * 0.5,
    }
}
