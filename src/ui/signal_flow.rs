use nih_plug_egui::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Sense, Stroke, StrokeKind, Vec2,
};

use crate::{dsp::chain::ChainModule, params::Cc22Params};

use super::theme::{ModuleColors, Theme};

pub(crate) const SIGNAL_CHAIN_HEIGHT: f32 = 46.0;

pub(crate) fn signal_chain_row(
    ui: &mut egui::Ui,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SIGNAL_CHAIN_HEIGHT),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(39, 36, 32));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, Color32::from_rgb(80, 73, 63)),
        StrokeKind::Inside,
    );

    let order = params.chain_order();
    let widths = [40.0, 64.0, 120.0, 120.0, 120.0, 120.0, 64.0, 40.0];
    let gap = 18.0;
    let total = widths.iter().sum::<f32>() + gap * 7.0;
    let mut x = rect.center().x - total * 0.5;
    let y = rect.center().y - 15.0;
    let mut nodes = [egui::Rect::NOTHING; 8];
    let mut accents = [theme.muted; 8];

    nodes[0] = paint_chain_node(
        ui.painter(),
        x,
        y + 4.0,
        widths[0],
        22.0,
        "IN",
        None,
        theme.muted,
    );
    x += widths[0] + gap;
    accents[1] = colors.eq;
    nodes[1] = paint_chain_node(
        ui.painter(),
        x,
        y,
        widths[1],
        30.0,
        "PRE EQ",
        Some("GLOBAL"),
        colors.eq,
    );
    x += widths[1] + gap;

    for (index, module) in order.into_iter().enumerate() {
        let node_index = index + 2;
        let accent = module_color(module, colors);
        accents[node_index] = accent;
        nodes[node_index] = paint_chain_node(
            ui.painter(),
            x,
            y,
            widths[node_index],
            30.0,
            module_name(module),
            Some("PRE EQ · POST EQ"),
            accent,
        );
        x += widths[node_index] + gap;
    }

    accents[6] = colors.eq;
    nodes[6] = paint_chain_node(
        ui.painter(),
        x,
        y,
        widths[6],
        30.0,
        "POST EQ",
        Some("GLOBAL"),
        colors.eq,
    );
    x += widths[6] + gap;
    nodes[7] = paint_chain_node(
        ui.painter(),
        x,
        y + 4.0,
        widths[7],
        22.0,
        "OUT",
        None,
        theme.muted,
    );

    for index in 0..7 {
        signal_flow_arrow(
            ui.painter(),
            Pos2::new(nodes[index].right() + 3.0, nodes[index].center().y),
            Pos2::new(nodes[index + 1].left() - 3.0, nodes[index].center().y),
            accents[index],
            false,
        );
    }
}

fn paint_chain_node(
    painter: &egui::Painter,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    label: &str,
    eq_label: Option<&str>,
    accent: Color32,
) -> egui::Rect {
    let node = egui::Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
    painter.rect_filled(
        node,
        CornerRadius::same(8),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 48),
    );
    painter.rect_stroke(
        node,
        CornerRadius::same(8),
        Stroke::new(1.0, accent.gamma_multiply(0.85)),
        StrokeKind::Inside,
    );
    painter.text(
        Pos2::new(
            node.center().x,
            node.center().y - if eq_label.is_some() { 4.5 } else { 0.0 },
        ),
        Align2::CENTER_CENTER,
        label,
        FontId::monospace(if width >= 100.0 { 9.0 } else { 7.5 }),
        Color32::from_rgb(244, 237, 221),
    );
    if let Some(eq_label) = eq_label {
        painter.text(
            Pos2::new(node.center().x, node.center().y + 6.0),
            Align2::CENTER_CENTER,
            eq_label,
            FontId::monospace(6.5),
            accent.gamma_multiply(1.15),
        );
    }
    node
}

fn module_name(module: ChainModule) -> &'static str {
    match module {
        ChainModule::Character => "CHARACTER",
        ChainModule::Movement => "MOVEMENT",
        ChainModule::Diffusion => "DIFFUSION",
        ChainModule::Texture => "TEXTURE",
    }
}

fn module_color(module: ChainModule, colors: ModuleColors) -> Color32 {
    match module {
        ChainModule::Character => colors.character,
        ChainModule::Movement => colors.movement,
        ChainModule::Diffusion => colors.diffusion,
        ChainModule::Texture => colors.texture,
    }
}

// ── shadows & lift ──────────────────────────────────────────────────────

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

pub(crate) fn signal_flow_arrow(
    painter: &egui::Painter,
    from_right: Pos2,
    to_left: Pos2,
    color: Color32,
    glow: bool,
) {
    let mid_x = (from_right.x + to_left.x) * 0.5;
    let arrow_size = 5.0;

    let line_alpha: u8 = if glow { 220 } else { 120 };
    let line_color = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), line_alpha);

    if glow {
        let glow_color = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 60);
        painter.line_segment(
            [from_right, Pos2::new(to_left.x, from_right.y)],
            Stroke::new(3.5, glow_color),
        );
    }

    painter.line_segment(
        [from_right, Pos2::new(mid_x - arrow_size, from_right.y)],
        Stroke::new(1.2, line_color),
    );

    let tip = Pos2::new(mid_x, from_right.y);
    painter.line_segment(
        [Pos2::new(mid_x - arrow_size, from_right.y - 3.5), tip],
        Stroke::new(1.2, line_color),
    );
    painter.line_segment(
        [Pos2::new(mid_x - arrow_size, from_right.y + 3.5), tip],
        Stroke::new(1.2, line_color),
    );

    painter.line_segment(
        [Pos2::new(mid_x + arrow_size, from_right.y), to_left],
        Stroke::new(1.2, line_color),
    );
}

// ── position badge ──────────────────────────────────────────────────────

pub(crate) fn position_badge(ui: &mut egui::Ui, pos: Pos2, number: usize, accent: Color32) {
    let badge_rect = egui::Rect::from_center_size(pos, Vec2::new(18.0, 18.0));
    ui.painter().rect_filled(
        badge_rect,
        CornerRadius::same(5),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 70),
    );
    ui.painter().rect_stroke(
        badge_rect,
        CornerRadius::same(5),
        Stroke::new(1.0, accent),
        StrokeKind::Inside,
    );
    ui.painter().text(
        badge_rect.center(),
        Align2::CENTER_CENTER,
        format!("{}", number),
        FontId::monospace(10.0),
        accent,
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
) {
    // shadow
    let shadow = rect.translate(Vec2::new(6.0, 8.0));
    painter.rect_filled(
        shadow,
        CornerRadius::same(14),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 35),
    );

    // body
    painter.rect_filled(
        rect,
        CornerRadius::same(14),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 28),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(1.6, accent.gamma_multiply(0.55)),
        StrokeKind::Inside,
    );

    // header strip
    let header = egui::Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0),
        Vec2::new(rect.width() - 16.0, 26.0),
    );
    painter.rect_filled(
        header,
        CornerRadius::same(7),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 140),
    );

    // title text
    let text_pos = Pos2::new(rect.min.x + 26.0, rect.min.y + 11.0);
    painter.text(
        text_pos,
        Align2::LEFT_TOP,
        title,
        FontId::monospace(12.0),
        Color32::WHITE,
    );

    // position badge
    let badge = egui::Rect::from_center_size(
        Pos2::new(rect.min.x + 16.0, rect.min.y + 12.0),
        Vec2::new(18.0, 18.0),
    );
    painter.rect_filled(
        badge,
        CornerRadius::same(5),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 100),
    );
    painter.rect_stroke(
        badge,
        CornerRadius::same(5),
        Stroke::new(1.0, Color32::WHITE),
        StrokeKind::Inside,
    );
    painter.text(
        badge.center(),
        Align2::CENTER_CENTER,
        format!("{}", position_num),
        FontId::monospace(10.0),
        Color32::WHITE,
    );

    // drag dots on top
    let dots_x = rect.max.x - 18.0;
    let dots_y = rect.min.y + 10.0;
    painter.circle_filled(Pos2::new(dots_x, dots_y), 1.8, Color32::WHITE);
    painter.circle_filled(Pos2::new(dots_x + 7.0, dots_y), 1.8, Color32::WHITE);
    painter.circle_filled(Pos2::new(dots_x, dots_y + 6.0), 1.8, Color32::WHITE);
    painter.circle_filled(Pos2::new(dots_x + 7.0, dots_y + 6.0), 1.8, Color32::WHITE);
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
