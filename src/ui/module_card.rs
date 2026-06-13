use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, LayerId, Order, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, UiBuilder, Vec2,
};

use crate::{
    dsp::{
        chain::ChainModule,
        character::CharacterMode,
        diffusion::DiffusionMode,
        movement::{LfoShape, MovementMode},
        texture::TextureMode,
    },
    params::Cc22Params,
};

use super::{
    eq_view::eq_workbench,
    meters::UiState,
    signal_flow::{
        card_shadow, compute_drop_slot, drag_handle, drop_indicator_x, final_index_from_drop_slot,
        paint_drop_indicator, paint_floating_card, position_badge, signal_flow_arrow,
    },
    theme::{
        Look, ModuleColors, Theme, CARD_HEIGHT, CARD_WIDTH, FONT_MODULE_TITLE, FONT_SECONDARY,
        KNOB_SIZE,
    },
    widgets::{
        character_active, colored_knob, diffusion_active, mini_slider, movement_active, set_param,
        texture_active,
    },
};

const LIFT_AMOUNT: f32 = 3.0;
struct ModuleCardSpec<'a> {
    title: &'static str,
    accent: Color32,
    active: bool,
    bypass: &'a BoolParam,
    module: ChainModule,
}

// ── public entry point ──────────────────────────────────────────────────

pub(crate) fn center_modules(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    look: Look,
) {
    let colors = look.colors;
    let theme = look.theme;
    let chain_order = params.chain_order();
    let module_row_width = (CARD_WIDTH * 4.0) + (ui.spacing().item_spacing.x * 3.0);
    let card_specs: [ModuleCardSpec<'_>; 4] = chain_order.map(|m| module_spec(m, params, colors));

    // ── compute card layout ──────────────────────────────────────────
    let mut card_rects: [Rect; 4] = [Rect::NOTHING; 4];
    let row_start = ui.cursor().min.x + ((ui.available_width() - module_row_width).max(0.0) * 0.5);
    let gaps = ui.spacing().item_spacing.x;
    for pos in 0..4 {
        let x = row_start + pos as f32 * (CARD_WIDTH + gaps);
        card_rects[pos] = Rect::from_min_size(
            Pos2::new(x, ui.cursor().min.y),
            Vec2::new(CARD_WIDTH, CARD_HEIGHT),
        );
    }

    // ── detect pointer & drag state ────────────────────────────────────
    let pointer = ui.ctx().input(|i| i.pointer.latest_pos());
    let pointer_x = pointer.map(|p| p.x);
    let mut drag_finished = None;

    if let Some(source) = state.drag_source {
        if ui.ctx().input(|i| i.pointer.primary_released()) {
            if let Some(drop_slot) = state.drag_drop_slot {
                let final_index = final_index_from_drop_slot(source, drop_slot);
                drag_finished = Some((source, final_index));
            }
            state.drag_source = None;
            state.drag_drop_slot = None;
        } else if let Some(px) = pointer_x {
            state.drag_drop_slot = Some(compute_drop_slot(px, &card_rects, row_start));
        }
    }

    let just_finished = drag_finished.is_some();

    // ── compute hover states for each card ─────────────────────────────
    let mut card_hovered = [false; 4];
    for pos in 0..4 {
        card_hovered[pos] =
            pointer.map_or(false, |p| card_rects[pos].contains(p)) && state.drag_source.is_none();
    }

    // ── section header ─────────────────────────────────────────────────
    // ── render cards ───────────────────────────────────────────────────
    ui.horizontal_top(|ui| {
        center_fixed_width_row(ui, module_row_width);
        for pos in 0..4 {
            let is_dragged = state.drag_source == Some(pos);
            let spec = &card_specs[pos];
            let hovered = card_hovered[pos];

            let rect = if is_dragged {
                let (r, _) =
                    ui.allocate_exact_size(Vec2::new(CARD_WIDTH, CARD_HEIGHT), Sense::hover());
                let ghost_color = Color32::from_rgba_premultiplied(
                    spec.accent.r(),
                    spec.accent.g(),
                    spec.accent.b(),
                    28,
                );
                ui.painter()
                    .rect_filled(r, CornerRadius::same(14), ghost_color);
                ui.painter().rect_stroke(
                    r,
                    CornerRadius::same(14),
                    Stroke::new(1.0, spec.accent.gamma_multiply(0.25)),
                    StrokeKind::Inside,
                );
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", pos + 1),
                    FontId::monospace(20.0),
                    Color32::from_rgba_premultiplied(
                        spec.accent.r(),
                        spec.accent.g(),
                        spec.accent.b(),
                        60,
                    ),
                );
                r
            } else {
                render_module_card(
                    ui,
                    setter,
                    theme,
                    spec,
                    pos + 1,
                    hovered,
                    !just_finished,
                    state,
                    params,
                )
            };
            card_rects[pos] = rect;
        }
    });

    // ── draw signal flow arrows ────────────────────────────────────────
    let painter = ui.painter().clone();
    for i in 0..3 {
        let from = card_rects[i];
        let to = card_rects[i + 1];
        if from.is_positive() && to.is_positive() {
            let from_right = Pos2::new(from.right(), from.center().y);
            let to_left = Pos2::new(to.left(), to.center().y);
            let near_drop = state.drag_source.is_some() && state.drag_drop_slot == Some(i + 1);
            signal_flow_arrow(
                &painter,
                from_right,
                to_left,
                module_color(chain_order[i], colors),
                near_drop,
            );
        }
    }

    // ── drop indicator + floating card ─────────────────────────────────
    if let Some(source) = state.drag_source {
        if let Some(drop_slot) = state.drag_drop_slot {
            if card_rects[0].is_positive() {
                let overlay = ui.ctx().layer_painter(LayerId::new(
                    Order::Foreground,
                    egui::Id::new("drop-indicator"),
                ));
                let ix = drop_indicator_x(drop_slot, &card_rects, row_start, gaps);
                paint_drop_indicator(
                    &overlay,
                    ix,
                    card_rects[0].top() - 2.0,
                    CARD_HEIGHT + 4.0,
                    module_color(chain_order[source], colors),
                );
            }
        }
        if let Some(ptr) = pointer {
            let fp = ui.ctx().layer_painter(LayerId::new(
                Order::Foreground,
                egui::Id::new("floating-card"),
            ));
            let fw = CARD_WIDTH * 1.02;
            let fh = CARD_HEIGHT * 1.02;
            let float_rect =
                Rect::from_center_size(ptr - Vec2::new(0.0, CARD_HEIGHT * 0.25), Vec2::new(fw, fh));
            let spec = &card_specs[source];
            paint_floating_card(&fp, float_rect, spec.accent, spec.title, source + 1);
        }
    }

    // ── process drag completion ────────────────────────────────────────
    if let Some((source, target)) = drag_finished {
        if source != target {
            let new_order = crate::dsp::chain::reorder_module(chain_order, source, target);
            set_chain_params(setter, params, &new_order);
        }
    }

    ui.add_space(8.0);
    let post_module_width = module_row_width
        .min(ui.available_width())
        .max(0.0)
        .floor();
    ui.horizontal_top(|ui| {
        center_fixed_width_row(ui, post_module_width);
        ui.vertical(|ui| {
            ui.set_width(post_module_width);
            eq_workbench(
                ui,
                setter,
                params,
                &mut state.selected_eq_band,
                colors,
                theme,
                post_module_width,
            );
        });
    });
}

// ── helpers ─────────────────────────────────────────────────────────────

fn set_chain_params(setter: &ParamSetter<'_>, params: &Cc22Params, order: &[ChainModule; 4]) {
    for (i, &module) in order.iter().enumerate() {
        let param = match i {
            0 => &params.chain_slot_0,
            1 => &params.chain_slot_1,
            2 => &params.chain_slot_2,
            _ => &params.chain_slot_3,
        };
        set_param(setter, param, module as i32);
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

fn module_spec<'a>(
    module: ChainModule,
    params: &'a Cc22Params,
    colors: ModuleColors,
) -> ModuleCardSpec<'a> {
    match module {
        ChainModule::Character => ModuleCardSpec {
            title: "CHARACTER",
            accent: colors.character,
            active: character_active(params),
            bypass: &params.character.bypass,
            module,
        },
        ChainModule::Movement => ModuleCardSpec {
            title: "MOVEMENT",
            accent: colors.movement,
            active: movement_active(params),
            bypass: &params.movement.bypass,
            module,
        },
        ChainModule::Diffusion => ModuleCardSpec {
            title: "DIFFUSION",
            accent: colors.diffusion,
            active: diffusion_active(params),
            bypass: &params.diffusion.bypass,
            module,
        },
        ChainModule::Texture => ModuleCardSpec {
            title: "TEXTURE",
            accent: colors.texture,
            active: texture_active(params),
            bypass: &params.texture.bypass,
            module,
        },
    }
}

fn center_fixed_width_row(ui: &mut egui::Ui, target_width: f32) {
    let extra = ui.available_width() - target_width;
    if extra > 0.0 {
        ui.add_space(extra * 0.5);
    }
}

// ── reorder arrow buttons ───────────────────────────────────────────────

fn reorder_arrows(
    ui: &mut egui::Ui,
    card_rect: Rect,
    position_num: usize,
    accent: Color32,
    theme: Theme,
    hovered: bool,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
) -> bool {
    if !hovered {
        return false;
    }

    let btn_w = 18.0;
    let btn_h = 14.0;
    let y = card_rect.min.y + 10.0;
    let gap = 4.0;

    let left_center = Pos2::new(card_rect.min.x + 36.0, y + btn_h * 0.5);
    let right_center = Pos2::new(card_rect.min.x + 36.0 + btn_w + gap, y + btn_h * 0.5);

    let left_rect = Rect::from_center_size(left_center, Vec2::new(btn_w, btn_h));
    let right_rect = Rect::from_center_size(right_center, Vec2::new(btn_w, btn_h));

    let can_left = position_num > 1;
    let can_right = position_num < 4;

    let left_color = if can_left { accent } else { theme.muted };
    let right_color = if can_right { accent } else { theme.muted };

    // left arrow
    ui.painter().rect_filled(
        left_rect,
        CornerRadius::same(4),
        Color32::from_rgba_premultiplied(
            left_color.r(),
            left_color.g(),
            left_color.b(),
            if can_left { 50 } else { 20 },
        ),
    );
    ui.painter().text(
        left_rect.center(),
        egui::Align2::CENTER_CENTER,
        "\u{25C0}",
        FontId::monospace(8.0),
        left_color,
    );

    // right arrow
    ui.painter().rect_filled(
        right_rect,
        CornerRadius::same(4),
        Color32::from_rgba_premultiplied(
            right_color.r(),
            right_color.g(),
            right_color.b(),
            if can_right { 50 } else { 20 },
        ),
    );
    ui.painter().text(
        right_rect.center(),
        egui::Align2::CENTER_CENTER,
        "\u{25B6}",
        FontId::monospace(8.0),
        right_color,
    );

    let left_id = egui::Id::new(format!("reorder-left-{}", position_num));
    let right_id = egui::Id::new(format!("reorder-right-{}", position_num));

    let left_clicked = ui.interact(left_rect, left_id, Sense::click()).clicked() && can_left;
    let right_clicked = ui.interact(right_rect, right_id, Sense::click()).clicked() && can_right;

    if left_clicked || right_clicked {
        let order = params.chain_order();
        let src = position_num - 1;
        let dst = if left_clicked {
            src.saturating_sub(1)
        } else {
            (src + 1).min(3)
        };
        let new_order = crate::dsp::chain::reorder_module(order, src, dst);
        set_chain_params(setter, params, &new_order);
        return true;
    }
    false
}

// ── card rendering ──────────────────────────────────────────────────────

fn render_module_card(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    theme: Theme,
    spec: &ModuleCardSpec<'_>,
    position_num: usize,
    hovered: bool,
    detect_drag: bool,
    state: &mut UiState,
    params: &Cc22Params,
) -> Rect {
    let fill = if spec.active {
        theme.card
    } else {
        theme.card_dim
    };
    let lift = if hovered { LIFT_AMOUNT } else { 0.0 };
    let alloc_h = CARD_HEIGHT + 6.0;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(CARD_WIDTH, alloc_h), Sense::hover());
    let card_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y - lift),
        Vec2::new(CARD_WIDTH, CARD_HEIGHT),
    );

    let shadow_accent = if hovered { Some(spec.accent) } else { None };
    card_shadow(ui.painter(), card_rect, lift, shadow_accent);

    let border_alpha: u8 = if hovered { 255 } else { 180 };
    let border_color = if spec.active {
        Color32::from_rgba_premultiplied(
            spec.accent.r(),
            spec.accent.g(),
            spec.accent.b(),
            border_alpha,
        )
    } else {
        theme.card_edge
    };
    let card_bg = if hovered && !spec.active {
        theme.card
    } else {
        fill
    };

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(card_rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(card_bg)
                .stroke(Stroke::new(1.4, border_color))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH - 16.0);
                    ui.set_min_height(CARD_HEIGHT - 16.0);

                    position_badge(
                        ui,
                        Pos2::new(card_rect.right() - 20.0, card_rect.bottom() - 24.0),
                        position_num,
                        spec.accent,
                    );

                    reorder_arrows(
                        ui,
                        card_rect,
                        position_num,
                        spec.accent,
                        theme,
                        hovered,
                        setter,
                        params,
                    );

                    let handle_resp =
                        drag_handle(ui, card_rect, spec.accent, position_num, hovered);
                    if detect_drag && handle_resp.drag_started() {
                        state.drag_source = Some(position_num - 1);
                        state.drag_drop_slot = None;
                    }

                    module_header(ui, spec, theme, hovered);
                    render_module_content(ui, setter, spec, params, theme);

                    if hovered {
                        handle_resp.on_hover_text("\u{2194} Drag handle to reorder");
                    }
                });
        },
    );

    card_rect
}

fn module_header(ui: &mut egui::Ui, spec: &ModuleCardSpec<'_>, theme: Theme, hovered: bool) {
    let line = Rect::from_min_size(
        Pos2::new(ui.min_rect().left(), ui.cursor().min.y),
        Vec2::new(ui.available_width(), 18.0),
    );
    let line_alpha: u8 = if spec.active {
        38
    } else if hovered {
        24
    } else {
        16
    };
    ui.painter().rect_filled(
        line,
        CornerRadius::same(3),
        Color32::from_rgba_premultiplied(
            spec.accent.r(),
            spec.accent.g(),
            spec.accent.b(),
            line_alpha,
        ),
    );
    ui.painter().text(
        line.center(),
        egui::Align2::CENTER_CENTER,
        spec.title,
        FontId::monospace(FONT_MODULE_TITLE),
        if spec.active {
            theme.text_dark
        } else {
            theme.muted_dark
        },
    );
    ui.add_space(22.0);
}

fn render_module_mode_list(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    spec: &ModuleCardSpec<'_>,
    params: &Cc22Params,
    theme: Theme,
) {
    match spec.module {
        ChainModule::Character => {
            let current = params.character.mode.value();
            mode_list_row(
                ui,
                setter,
                &params.character.mode,
                spec.bypass,
                current,
                CharacterMode::Drive,
                "DRIVE",
                mode_option_color(0),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.character.mode,
                spec.bypass,
                current,
                CharacterMode::Sweet,
                "SWEET",
                mode_option_color(1),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.character.mode,
                spec.bypass,
                current,
                CharacterMode::Fuzz,
                "FUZZ",
                mode_option_color(2),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.character.mode,
                spec.bypass,
                current,
                CharacterMode::Howl,
                "HOWL",
                mode_option_color(3),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.character.mode,
                spec.bypass,
                current,
                CharacterMode::Swell,
                "SWELL",
                mode_option_color(4),
                theme,
            );
        }
        ChainModule::Movement => {
            let current = params.movement.mode.value();
            mode_list_row(
                ui,
                setter,
                &params.movement.mode,
                spec.bypass,
                current,
                MovementMode::Doubler,
                "DOUBLER",
                mode_option_color(0),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.movement.mode,
                spec.bypass,
                current,
                MovementMode::Vibrato,
                "VIBRATO",
                mode_option_color(1),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.movement.mode,
                spec.bypass,
                current,
                MovementMode::Phaser,
                "PHASER",
                mode_option_color(2),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.movement.mode,
                spec.bypass,
                current,
                MovementMode::Tremolo,
                "TREMOLO",
                mode_option_color(3),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.movement.mode,
                spec.bypass,
                current,
                MovementMode::Pitch,
                "PITCH",
                mode_option_color(4),
                theme,
            );
        }
        ChainModule::Diffusion => {
            let current = params.diffusion.mode.value();
            mode_list_row(
                ui,
                setter,
                &params.diffusion.mode,
                spec.bypass,
                current,
                DiffusionMode::Cascade,
                "CASCADE",
                mode_option_color(0),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.diffusion.mode,
                spec.bypass,
                current,
                DiffusionMode::Reels,
                "REELS",
                mode_option_color(1),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.diffusion.mode,
                spec.bypass,
                current,
                DiffusionMode::Space,
                "SPACE",
                mode_option_color(2),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.diffusion.mode,
                spec.bypass,
                current,
                DiffusionMode::Collage,
                "COLLAGE",
                mode_option_color(3),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.diffusion.mode,
                spec.bypass,
                current,
                DiffusionMode::Reverse,
                "REVERSE",
                mode_option_color(4),
                theme,
            );
        }
        ChainModule::Texture => {
            let current = params.texture.mode.value();
            mode_list_row(
                ui,
                setter,
                &params.texture.mode,
                spec.bypass,
                current,
                TextureMode::Filter,
                "FILTER",
                mode_option_color(0),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.texture.mode,
                spec.bypass,
                current,
                TextureMode::Squash,
                "SQUASH",
                mode_option_color(1),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.texture.mode,
                spec.bypass,
                current,
                TextureMode::Cassette,
                "CASSETTE",
                mode_option_color(2),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.texture.mode,
                spec.bypass,
                current,
                TextureMode::Broken,
                "BROKEN",
                mode_option_color(3),
                theme,
            );
            mode_list_row(
                ui,
                setter,
                &params.texture.mode,
                spec.bypass,
                current,
                TextureMode::Interference,
                "INTERFERENCE",
                mode_option_color(4),
                theme,
            );
        }
    }
}

fn mode_option_color(index: usize) -> Color32 {
    match index {
        0 => Color32::from_rgb(232, 48, 36),
        1 => Color32::from_rgb(240, 205, 28),
        2 => Color32::from_rgb(24, 184, 70),
        3 => Color32::from_rgb(45, 180, 220),
        _ => Color32::from_rgb(130, 82, 200),
    }
}

fn mode_list_row<T>(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<T>,
    bypass: &BoolParam,
    current: T,
    value: T,
    label: &'static str,
    accent: Color32,
    theme: Theme,
) where
    T: Enum + Copy + PartialEq,
{
    let selected = current == value && !bypass.value();
    let row_height = 13.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click());

    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(3),
            Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 18),
        );
    }

    let square_center = Pos2::new(rect.left() + 6.0, rect.center().y);
    mode_square_at(ui, square_center, accent, selected, 3.6);
    ui.painter().text(
        Pos2::new(rect.left() + 15.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::monospace(FONT_SECONDARY - 0.5),
        if selected {
            theme.text_dark
        } else {
            theme.muted_dark
        },
    );

    if response.clicked() {
        set_param(setter, bypass, false);
        set_param(setter, param, value);
    }
}

fn mode_square_at(ui: &mut egui::Ui, center: Pos2, color: Color32, active: bool, size: f32) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(1),
        Color32::from_rgba_premultiplied(
            color.r(),
            color.g(),
            color.b(),
            if active { 255 } else { 110 },
        ),
    );
    if active {
        ui.painter().rect_stroke(
            rect.expand(1.5),
            CornerRadius::same(2),
            Stroke::new(0.9, color.gamma_multiply(0.55)),
            StrokeKind::Outside,
        );
    }
}

fn render_module_content(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    spec: &ModuleCardSpec<'_>,
    params: &Cc22Params,
    theme: Theme,
) {
    render_module_mode_list(ui, setter, spec, params, theme);
    ui.add_space(5.0);

    match spec.module {
        ChainModule::Character => {
            let mode = params.character.mode.value();
            let (first_label, first_param, first_tip) = match mode {
                CharacterMode::Swell => (
                    "SENS",
                    &params.character.drive,
                    Some(
                        "SENS controls the real Drive parameter as the swell detector sensitivity.",
                    ),
                ),
                CharacterMode::Cassette => (
                    "DRIVE",
                    &params.character.drive,
                    Some("DRIVE controls the real Drive parameter before the cassette stage."),
                ),
                _ => ("DRIVE", &params.character.drive, None),
            };
            let (second_label, second_param, second_tip) = match mode {
                CharacterMode::Cassette => (
                    "AGE",
                    &params.character.age,
                    Some("AGE controls the real Age parameter for cassette wear."),
                ),
                _ => ("TONE", &params.character.tone, None),
            };
            ui.horizontal(|ui| {
                knob_with_tip(
                    ui,
                    setter,
                    first_param,
                    first_label,
                    spec.accent,
                    theme,
                    first_tip,
                );
                knob_with_tip(
                    ui,
                    setter,
                    second_param,
                    second_label,
                    spec.accent,
                    theme,
                    second_tip,
                );
            });
            ui.add_space(2.0);
            secondary_slider_pair(
                ui,
                setter,
                (&params.character.mix, "MIX", None),
                (&params.character.output_trim, "OUTPUT", None),
                spec.accent,
                theme,
            );
        }
        ChainModule::Movement => {
            let mode = params.movement.mode.value();
            let first = movement_first_control(mode, params);
            let second = movement_second_control(mode, params);
            ui.horizontal(|ui| {
                knob_with_tip(
                    ui,
                    setter,
                    first.param,
                    first.label,
                    spec.accent,
                    theme,
                    first.tip,
                );
                knob_with_tip(
                    ui,
                    setter,
                    second.param,
                    second.label,
                    spec.accent,
                    theme,
                    second.tip,
                );
            });
            ui.add_space(2.0);
            match mode {
                MovementMode::Phaser => secondary_slider_pair(
                    ui,
                    setter,
                    (&params.movement.feedback, "FEEDBACK", None),
                    (&params.movement.mix, "MIX", None),
                    spec.accent,
                    theme,
                ),
                MovementMode::Tremolo => {
                    secondary_shape_and_slider(
                        ui,
                        setter,
                        &params.movement.shape,
                        (&params.movement.mix, "MIX", None),
                        spec.accent,
                        theme,
                    );
                }
                _ => secondary_slider_pair(
                    ui,
                    setter,
                    (&params.movement.mix, "MIX", None),
                    (&params.movement.tone, "TONE", None),
                    spec.accent,
                    theme,
                ),
            };
        }
        ChainModule::Diffusion => {
            let mode = params.diffusion.mode.value();
            let first = diffusion_first_control(mode, params);
            let second = diffusion_second_control(mode, params);
            ui.horizontal(|ui| {
                knob_with_tip(
                    ui,
                    setter,
                    first.param,
                    first.label,
                    spec.accent,
                    theme,
                    first.tip,
                );
                knob_with_tip(
                    ui,
                    setter,
                    second.param,
                    second.label,
                    spec.accent,
                    theme,
                    second.tip,
                );
            });
            ui.add_space(2.0);
            secondary_slider_pair(
                ui,
                setter,
                (&params.diffusion.mix, "MIX", None),
                (&params.diffusion.width, "WIDTH", None),
                spec.accent,
                theme,
            );
        }
        ChainModule::Texture => {
            let mode = params.texture.mode.value();
            let first = texture_first_control(mode, params);
            let second = texture_second_control(mode, params);
            ui.horizontal(|ui| {
                knob_with_tip(
                    ui,
                    setter,
                    first.param,
                    first.label,
                    spec.accent,
                    theme,
                    first.tip,
                );
                knob_with_tip(
                    ui,
                    setter,
                    second.param,
                    second.label,
                    spec.accent,
                    theme,
                    second.tip,
                );
            });
            ui.add_space(2.0);
            secondary_slider_pair(
                ui,
                setter,
                (&params.texture.mix, "MIX", None),
                (&params.texture.stereo_spread, "SPREAD", None),
                spec.accent,
                theme,
            );
        }
    }
}

struct FloatControlSpec<'a> {
    label: &'static str,
    param: &'a FloatParam,
    tip: Option<&'static str>,
}

fn knob_with_tip(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
    tip: Option<&'static str>,
) {
    let response = colored_knob(ui, setter, param, label, accent, theme, KNOB_SIZE);
    if let Some(tip) = tip {
        response.on_hover_text(tip);
    }
}

fn slider_with_tip(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
    tip: Option<&'static str>,
) {
    let response = mini_slider(ui, setter, param, label, accent, theme);
    if let Some(tip) = tip {
        response.on_hover_text(tip);
    }
}

fn secondary_slider_pair(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    first: (&FloatParam, &'static str, Option<&'static str>),
    second: (&FloatParam, &'static str, Option<&'static str>),
    accent: Color32,
    theme: Theme,
) {
    ui.horizontal(|ui| {
        ui.allocate_ui(Vec2::new(96.0, 38.0), |ui| {
            slider_with_tip(ui, setter, first.0, first.1, accent, theme, first.2);
        });
        ui.allocate_ui(Vec2::new(96.0, 38.0), |ui| {
            slider_with_tip(ui, setter, second.0, second.1, accent, theme, second.2);
        });
    });
}

fn secondary_shape_and_slider(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    shape: &EnumParam<LfoShape>,
    slider: (&FloatParam, &'static str, Option<&'static str>),
    accent: Color32,
    theme: Theme,
) {
    ui.horizontal(|ui| {
        ui.allocate_ui(Vec2::new(96.0, 38.0), |ui| {
            let _ = shape_selector(ui, setter, shape, accent, theme);
        });
        ui.allocate_ui(Vec2::new(96.0, 38.0), |ui| {
            slider_with_tip(ui, setter, slider.0, slider.1, accent, theme, slider.2);
        });
    });
}

fn movement_first_control<'a>(mode: MovementMode, params: &'a Cc22Params) -> FloatControlSpec<'a> {
    match mode {
        MovementMode::Doubler => FloatControlSpec {
            label: "TIME",
            param: &params.movement.delay,
            tip: Some("TIME controls the real Delay parameter for the doubler offset."),
        },
        _ => FloatControlSpec {
            label: "RATE",
            param: &params.movement.rate,
            tip: None,
        },
    }
}

fn movement_second_control<'a>(mode: MovementMode, params: &'a Cc22Params) -> FloatControlSpec<'a> {
    match mode {
        MovementMode::Doubler | MovementMode::Pitch => FloatControlSpec {
            label: "WIDTH",
            param: &params.movement.width,
            tip: None,
        },
        _ => FloatControlSpec {
            label: "DEPTH",
            param: &params.movement.depth,
            tip: None,
        },
    }
}

fn diffusion_first_control<'a>(
    mode: DiffusionMode,
    params: &'a Cc22Params,
) -> FloatControlSpec<'a> {
    match mode {
        DiffusionMode::Reverb | DiffusionMode::Space | DiffusionMode::Collage => FloatControlSpec {
            label: "SIZE",
            param: &params.diffusion.size,
            tip: None,
        },
        _ => FloatControlSpec {
            label: "TIME",
            param: &params.diffusion.time,
            tip: None,
        },
    }
}

fn diffusion_second_control<'a>(
    mode: DiffusionMode,
    params: &'a Cc22Params,
) -> FloatControlSpec<'a> {
    match mode {
        DiffusionMode::Reverb | DiffusionMode::Space => FloatControlSpec {
            label: "DECAY",
            param: &params.diffusion.decay,
            tip: None,
        },
        DiffusionMode::Collage => FloatControlSpec {
            label: "DENSITY",
            param: &params.diffusion.feedback,
            tip: Some(
                "DENSITY controls the real Feedback parameter used by Collage fragment density.",
            ),
        },
        _ => FloatControlSpec {
            label: "FEEDBACK",
            param: &params.diffusion.feedback,
            tip: None,
        },
    }
}

fn texture_first_control<'a>(mode: TextureMode, params: &'a Cc22Params) -> FloatControlSpec<'a> {
    match mode {
        TextureMode::Filter => FloatControlSpec {
            label: "COLOR",
            param: &params.texture.noise_color,
            tip: Some("COLOR controls the real Noise Color parameter for the filter character."),
        },
        TextureMode::Squash => FloatControlSpec {
            label: "AMOUNT",
            param: &params.texture.degrade,
            tip: Some("AMOUNT controls the real Degrade parameter for squash intensity."),
        },
        TextureMode::Broken => FloatControlSpec {
            label: "DEGRADE",
            param: &params.texture.degrade,
            tip: None,
        },
        TextureMode::Interference => FloatControlSpec {
            label: "AMOUNT",
            param: &params.texture.noise_amount,
            tip: Some("AMOUNT controls the real Noise Amount parameter for interference level."),
        },
        TextureMode::Noise => FloatControlSpec {
            label: "AMOUNT",
            param: &params.texture.noise_amount,
            tip: Some("AMOUNT controls the real Noise Amount parameter."),
        },
        _ => FloatControlSpec {
            label: "WOW",
            param: &params.texture.wow_depth,
            tip: None,
        },
    }
}

fn texture_second_control<'a>(mode: TextureMode, params: &'a Cc22Params) -> FloatControlSpec<'a> {
    match mode {
        TextureMode::Filter => FloatControlSpec {
            label: "RES/DEG",
            param: &params.texture.degrade,
            tip: Some("RES/DEG controls the real Degrade parameter, mapped to filter drive/resonance."),
        },
        TextureMode::Squash => FloatControlSpec {
            label: "TONE",
            param: &params.texture.noise_color,
            tip: Some("TONE controls the real Noise Color parameter for squash detector color."),
        },
        TextureMode::Cassette | TextureMode::Tape => FloatControlSpec {
            label: "NOISE",
            param: &params.texture.noise_amount,
            tip: Some("NOISE controls the real Noise Amount parameter."),
        },
        TextureMode::Broken => FloatControlSpec {
            label: "DRIFT",
            param: &params.texture.random_drift,
            tip: Some("DRIFT controls the real Random Drift parameter."),
        },
        TextureMode::Interference => FloatControlSpec {
            label: "FREQ/COL",
            param: &params.texture.noise_color,
            tip: Some("FREQ/COL controls the real Noise Color parameter, mapped to interference frequency and color."),
        },
        TextureMode::Noise => FloatControlSpec {
            label: "COLOR",
            param: &params.texture.noise_color,
            tip: Some("COLOR controls the real Noise Color parameter."),
        },
        _ => FloatControlSpec {
            label: "FLUTTER",
            param: &params.texture.flutter_depth,
            tip: None,
        },
    }
}

fn shape_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<LfoShape>,
    accent: Color32,
    theme: Theme,
) -> egui::Response {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("SHAPE")
                    .font(FontId::monospace(super::theme::FONT_CONTROL_LABEL))
                    .color(theme.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(lfo_shape_label(param.value()))
                        .font(FontId::monospace(super::theme::FONT_VALUE_LABEL))
                        .color(theme.muted),
                );
            });
        });

        let response = egui::ComboBox::from_id_salt("movement-shape-card")
            .selected_text(lfo_shape_label(param.value()))
            .width(ui.available_width().max(80.0))
            .show_ui(ui, |ui| {
                shape_option(ui, setter, param, LfoShape::Sine, "Sine");
                shape_option(ui, setter, param, LfoShape::Triangle, "Triangle");
                shape_option(ui, setter, param, LfoShape::SquareSmooth, "Square");
            })
            .response;

        let rect = response.rect;
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(7),
            Stroke::new(1.0, accent.gamma_multiply(0.45)),
            StrokeKind::Inside,
        );

        response.on_hover_text("SHAPE controls the real Shape enum parameter for Tremolo.")
    })
    .inner
}

fn shape_option(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<LfoShape>,
    value: LfoShape,
    label: &'static str,
) {
    if ui.selectable_label(param.value() == value, label).clicked() {
        set_param(setter, param, value);
        ui.close_menu();
    }
}

fn lfo_shape_label(shape: LfoShape) -> &'static str {
    match shape {
        LfoShape::Sine => "Sine",
        LfoShape::Triangle => "Triangle",
        LfoShape::SquareSmooth => "Square",
    }
}
