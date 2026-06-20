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
    meters::UiState,
    signal_flow::{
        card_shadow, compute_drop_slot, drag_handle, drop_indicator_x, final_index_from_drop_slot,
        paint_drop_indicator, paint_floating_card, position_badge,
    },
    theme::{
        Look, ModuleColors, Theme, CARD_HEIGHT, CARD_WIDTH, FONT_MODULE_TITLE, FONT_SECONDARY,
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

                    module_header(ui, setter, spec, theme, hovered);
                    render_module_content(ui, setter, spec, params, theme);

                    if hovered {
                        handle_resp.on_hover_text("\u{2194} Drag handle to reorder");
                    }
                });
        },
    );

    card_rect
}

fn module_header(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    spec: &ModuleCardSpec<'_>,
    theme: Theme,
    hovered: bool,
) {
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
    let power_rect = Rect::from_center_size(
        Pos2::new(line.left() + 16.0, line.center().y),
        Vec2::new(28.0, 14.0),
    );
    let bypassed = spec.bypass.value();
    let power = ui.interact(
        power_rect,
        egui::Id::new(("module-power", spec.title)),
        Sense::click(),
    );
    if power.clicked() {
        set_param(setter, spec.bypass, !bypassed);
    }
    ui.painter().rect_filled(
        power_rect,
        CornerRadius::same(7),
        if bypassed {
            Color32::from_rgb(184, 177, 165)
        } else {
            spec.accent.gamma_multiply(0.9)
        },
    );
    ui.painter().text(
        power_rect.center(),
        egui::Align2::CENTER_CENTER,
        if bypassed {
            "BYP"
        } else if spec.active {
            "ON"
        } else {
            "OFF"
        },
        FontId::monospace(7.0),
        if bypassed {
            theme.muted_dark
        } else {
            Color32::WHITE
        },
    );
    ui.add_space(22.0);
}

/// The exact set of modes the Character selector exposes, in display order.
/// This is the single source of truth for the card's mode list, so tests can
/// assert that the UI never shows a hidden or removed mode.
pub(crate) const CHARACTER_MODE_OPTIONS: [(CharacterMode, &str); 5] = [
    (CharacterMode::Drive, "DRIVE"),
    (CharacterMode::Sweet, "SWEETEN"),
    (CharacterMode::Fuzz, "FUZZ"),
    (CharacterMode::Howl, "HOWL"),
    (CharacterMode::Swell, "SWELL"),
];

/// Modes the Movement selector exposes, in display order.
pub(crate) const MOVEMENT_MODE_OPTIONS: [(MovementMode, &str); 5] = [
    (MovementMode::Doubler, "DOUBLER"),
    (MovementMode::Vibrato, "VIBRATO"),
    (MovementMode::Phaser, "PHASER"),
    (MovementMode::Tremolo, "TREMOLO"),
    (MovementMode::Pitch, "PITCH"),
];

/// Modes the Diffusion selector exposes, in display order.
pub(crate) const DIFFUSION_MODE_OPTIONS: [(DiffusionMode, &str); 5] = [
    (DiffusionMode::Cascade, "CASCADE"),
    (DiffusionMode::Reels, "REELS"),
    (DiffusionMode::Space, "SPACE"),
    (DiffusionMode::Collage, "COLLAGE"),
    (DiffusionMode::Reverse, "REVERSE"),
];

/// Modes the Texture selector exposes, in display order.
pub(crate) const TEXTURE_MODE_OPTIONS: [(TextureMode, &str); 5] = [
    (TextureMode::Filter, "FILTER"),
    (TextureMode::Squash, "SQUASH"),
    (TextureMode::Cassette, "CASSETTE"),
    (TextureMode::Broken, "BROKEN"),
    (TextureMode::Interference, "INTERFERENCE"),
];

fn render_premium_mode_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    spec: &ModuleCardSpec<'_>,
    params: &Cc22Params,
    theme: Theme,
) {
    match spec.module {
        ChainModule::Character => render_mode_list(
            ui,
            setter,
            &params.character.mode,
            params.character.mode.value(),
            spec.bypass,
            &CHARACTER_MODE_OPTIONS,
            spec.accent,
            theme,
        ),
        ChainModule::Movement => render_mode_list(
            ui,
            setter,
            &params.movement.mode,
            params.movement.mode.value(),
            spec.bypass,
            &MOVEMENT_MODE_OPTIONS,
            spec.accent,
            theme,
        ),
        ChainModule::Diffusion => render_mode_list(
            ui,
            setter,
            &params.diffusion.mode,
            params.diffusion.mode.value(),
            spec.bypass,
            &DIFFUSION_MODE_OPTIONS,
            spec.accent,
            theme,
        ),
        ChainModule::Texture => render_mode_list(
            ui,
            setter,
            &params.texture.mode,
            params.texture.mode.value(),
            spec.bypass,
            &TEXTURE_MODE_OPTIONS,
            spec.accent,
            theme,
        ),
    }

    ui.label(
        RichText::new(current_mode_description(spec.module, params))
            .font(FontId::monospace(8.5))
            .color(theme.muted_dark),
    );
}

/// Renders the module's five product modes as a fixed list of selectable rows.
/// Every card therefore shows exactly five items, in product order, with the
/// active mode highlighted in the module accent color.
fn render_mode_list<T>(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<T>,
    current: T,
    bypass: &BoolParam,
    options: &[(T, &'static str)],
    accent: Color32,
    theme: Theme,
) where
    T: Enum + Copy + PartialEq + 'static,
{
    // `current` is the canonical (product) mode, so it always resolves to one of
    // the five options even when the raw parameter still holds a legacy value
    // loaded from an older project.
    let row_height = 15.0;
    let radius = CornerRadius::same(5);

    for (value, label) in options {
        let selected = current == *value && !bypass.value();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click());

        // Active row: solid accent fill. Hovered (inactive) row: faint accent wash.
        if selected {
            ui.painter().rect_filled(rect, radius, accent);
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, accent.gamma_multiply(0.6)),
                StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect,
                radius,
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 30),
            );
        }

        // Left indicator square echoes the rest of the card's visual language.
        let square = Rect::from_center_size(
            Pos2::new(rect.left() + 9.0, rect.center().y),
            Vec2::splat(5.0),
        );
        ui.painter().rect_filled(
            square,
            CornerRadius::same(1),
            if selected {
                theme.text_dark
            } else {
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 150)
            },
        );

        // Dark text on the bright accent reads clearly for every module color.
        ui.painter().text(
            Pos2::new(rect.left() + 18.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::monospace(10.0),
            if selected {
                theme.text_dark
            } else {
                theme.muted_dark
            },
        );

        if response.clicked() {
            set_param(setter, bypass, false);
            set_param(setter, param, *value);
        }
        response.on_hover_text("Click to select this mode");
    }
}

fn current_mode_description(module: ChainModule, params: &Cc22Params) -> &'static str {
    match module {
        ChainModule::Character => match params.character.mode.value() {
            CharacterMode::Drive => "FOCUSED ANALOG PUSH",
            CharacterMode::Sweet => "SOFT PRESENCE & SHINE",
            CharacterMode::Fuzz => "DENSE BROKEN GRAIN",
            CharacterMode::Howl => "VOCAL RESONANT COLOR",
            CharacterMode::Swell => "BLOOMING ATTACK SHAPE",
        },
        ChainModule::Movement => match params.movement.mode.value() {
            MovementMode::Doubler => "SHORT STEREO DOUBLE",
            MovementMode::Vibrato => "PURE PITCH WOBBLE",
            MovementMode::Phaser => "RESONANT PHASE SWEEP",
            MovementMode::Tremolo => "RHYTHMIC LEVEL PULSE",
            MovementMode::Pitch => "DRIFTING MICRO-PITCH",
        },
        ChainModule::Diffusion => match params.diffusion.mode.value() {
            DiffusionMode::Cascade => "MULTI-TAP ECHO CLOUD",
            DiffusionMode::Reels => "UNSTABLE TAPE REPEATS",
            DiffusionMode::Space => "WIDE MODULATED SPACE",
            DiffusionMode::Collage => "FRAGMENTED DELAY FIELD",
            DiffusionMode::Reverse => "REVERSED CAPTURE BLOOM",
        },
        ChainModule::Texture => match params.texture.mode.value() {
            TextureMode::Filter => "TILTED TONAL COLOR",
            TextureMode::Squash => "DENSE DYNAMIC GRAIN",
            TextureMode::Cassette => "COMPACT TAPE DAMAGE",
            TextureMode::Broken => "DROPOUTS & DIGITAL WEAR",
            TextureMode::Interference => "ELECTRIC PARASITE TONE",
        },
    }
}

#[allow(dead_code)]
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
            egui::ComboBox::from_id_salt("character-mode")
                .width(ui.available_width() - 4.0)
                .selected_text(character_mode_label(current))
                .show_ui(ui, |ui| {
                    for mode in &CharacterMode::PRODUCT_MODES {
                        let label = character_mode_label(*mode);
                        if ui
                            .selectable_label(current == *mode && !spec.bypass.value(), label)
                            .clicked()
                        {
                            set_param(setter, spec.bypass, false);
                            set_param(setter, &params.character.mode, *mode);
                        }
                    }
                });
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
            egui::ComboBox::from_id_salt("texture-mode")
                .width(ui.available_width() - 4.0)
                .selected_text(texture_mode_label(current))
                .show_ui(ui, |ui| {
                    for mode in &TextureMode::PRODUCT_MODES {
                        let label = texture_mode_label(*mode);
                        if ui
                            .selectable_label(current == *mode && !spec.bypass.value(), label)
                            .clicked()
                        {
                            set_param(setter, spec.bypass, false);
                            set_param(setter, &params.texture.mode, *mode);
                        }
                    }
                });
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
    render_premium_mode_selector(ui, setter, spec, params, theme);
    ui.add_space(6.0);

    match spec.module {
        ChainModule::Character => {
            let mode = params.character.mode.value();
            let (first_label, first_param, first_tip) = match mode {
                CharacterMode::Howl => (
                    "RESO",
                    &params.character.drive,
                    Some(
                        "RESO controls the real Drive parameter: input push, resonant howl, and feedback-like aggression.",
                    ),
                ),
                CharacterMode::Swell => (
                    "ATTACK",
                    &params.character.drive,
                    Some(
                        "ATTACK controls the real Drive parameter: swell attack time, transient removal, and retrigger sensitivity.",
                    ),
                ),
                _ => ("DRIVE", &params.character.drive, None),
            };
            let (second_label, second_param, second_tip) = match mode {
                CharacterMode::Howl => (
                    "FORMANT",
                    &params.character.tone,
                    Some(
                        "FORMANT controls the real Tone parameter: vocal formant center and final brightness.",
                    ),
                ),
                CharacterMode::Swell => (
                    "TONE",
                    &params.character.tone,
                    Some("TONE controls the real Tone parameter for post-swell brightness."),
                ),
                _ => ("TONE", &params.character.tone, None),
            };
            let controls = vec![
                FloatControlSpec {
                    label: first_label,
                    param: first_param,
                    tip: first_tip,
                },
                FloatControlSpec {
                    label: second_label,
                    param: second_param,
                    tip: second_tip,
                },
            ];
            primary_control_row(ui, setter, &controls, spec.accent, theme);
            ui.add_space(2.0);
            secondary_slider_pair(
                ui,
                setter,
                (&params.character.mix, "MIX", None),
                (&params.character.output_trim, "LEVEL", None),
                spec.accent,
                theme,
            );
        }
        ChainModule::Movement => {
            let mode = params.movement.mode.value();
            let first = movement_first_control(mode, params);
            let second = movement_second_control(mode, params);
            let third = movement_third_control(mode, params);
            let controls = vec![first, second, third];
            primary_control_row(ui, setter, &controls, spec.accent, theme);
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
            let third = diffusion_third_control(mode, params);
            let controls = vec![first, second, third];
            primary_control_row(ui, setter, &controls, spec.accent, theme);
            ui.add_space(2.0);
            secondary_slider_pair(
                ui,
                setter,
                (&params.diffusion.mix, "MIX", None),
                (&params.diffusion.width, "SPACE", None),
                spec.accent,
                theme,
            );
        }
        ChainModule::Texture => {
            let mode = params.texture.mode.value();
            let first = texture_first_control(mode, params);
            let second = texture_second_control(mode, params);
            let third = texture_third_control(mode, params);
            let controls = vec![first, second, third];
            primary_control_row(ui, setter, &controls, spec.accent, theme);
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

fn primary_control_row(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    controls: &[FloatControlSpec<'_>],
    accent: Color32,
    theme: Theme,
) {
    if controls.is_empty() {
        ui.allocate_space(Vec2::new(ui.available_width(), 68.0));
        return;
    }
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for control in controls {
            let response = colored_knob(
                ui,
                setter,
                control.param,
                control.label,
                accent,
                theme,
                42.0,
            );
            if let Some(tip) = control.tip {
                response.on_hover_text(tip);
            }
        }
    });
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

fn movement_third_control<'a>(mode: MovementMode, params: &'a Cc22Params) -> FloatControlSpec<'a> {
    match mode {
        MovementMode::Doubler | MovementMode::Pitch => FloatControlSpec {
            label: "DEPTH",
            param: &params.movement.depth,
            tip: None,
        },
        MovementMode::Phaser => FloatControlSpec {
            label: "RESO",
            param: &params.movement.feedback,
            tip: Some("RESO controls the phaser feedback path."),
        },
        _ => FloatControlSpec {
            label: "WIDTH",
            param: &params.movement.width,
            tip: None,
        },
    }
}

fn diffusion_first_control<'a>(
    mode: DiffusionMode,
    params: &'a Cc22Params,
) -> FloatControlSpec<'a> {
    match mode {
        DiffusionMode::Reels => FloatControlSpec {
            label: "TIME",
            param: &params.diffusion.time,
            tip: Some(
                "TIME controls the real Time parameter: smoothed base delay time for the tape echo.",
            ),
        },
        DiffusionMode::Reverse => FloatControlSpec {
            label: "TIME",
            param: &params.diffusion.time,
            tip: Some(
                "TIME controls the real Time parameter: reverse capture window and delay feel.",
            ),
        },
        DiffusionMode::Space | DiffusionMode::Collage => FloatControlSpec {
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
        DiffusionMode::Reels => FloatControlSpec {
            label: "WOW/DRIFT",
            param: &params.diffusion.size,
            tip: Some(
                "WOW/DRIFT controls the real Size parameter: tape wow, flutter, and random drift amount.",
            ),
        },
        DiffusionMode::Reverse => FloatControlSpec {
            label: "LENGTH",
            param: &params.diffusion.size,
            tip: Some(
                "LENGTH controls the real Size parameter: reverse segment length and density.",
            ),
        },
        DiffusionMode::Space => FloatControlSpec {
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
            label: "REPEATS",
            param: &params.diffusion.feedback,
            tip: None,
        },
    }
}

fn diffusion_third_control<'a>(
    mode: DiffusionMode,
    params: &'a Cc22Params,
) -> FloatControlSpec<'a> {
    match mode {
        DiffusionMode::Reels | DiffusionMode::Reverse => FloatControlSpec {
            label: "REPEATS",
            param: &params.diffusion.feedback,
            tip: None,
        },
        DiffusionMode::Space | DiffusionMode::Collage => FloatControlSpec {
            label: "TONE",
            param: &params.diffusion.tone,
            tip: None,
        },
        _ => FloatControlSpec {
            label: "TONE",
            param: &params.diffusion.tone,
            tip: None,
        },
    }
}

#[allow(dead_code)]
fn diffusion_secondary_controls<'a>(
    mode: DiffusionMode,
    params: &'a Cc22Params,
) -> (FloatControlSpec<'a>, FloatControlSpec<'a>) {
    let mix = FloatControlSpec {
        label: "MIX",
        param: &params.diffusion.mix,
        tip: Some("MIX controls the real Mix parameter: dry/wet blend."),
    };

    match mode {
        DiffusionMode::Reels => (
            mix,
            FloatControlSpec {
                label: "REPEATS",
                param: &params.diffusion.feedback,
                tip: Some(
                    "REPEATS controls the real Feedback parameter: amount of colored tape echo returned to the feedback path.",
                ),
            },
        ),
        DiffusionMode::Reverse => (
            mix,
            FloatControlSpec {
                label: "REPEATS",
                param: &params.diffusion.feedback,
                tip: Some(
                    "REPEATS controls the real Feedback parameter: how much reversed signal is written back for repeats.",
                ),
            },
        ),
        _ => (
            mix,
            FloatControlSpec {
                label: "WIDTH",
                param: &params.diffusion.width,
                tip: None,
            },
        ),
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
            label: "WEAR",
            param: &params.texture.degrade,
            tip: Some("AMOUNT controls the real Degrade parameter for squash intensity."),
        },
        TextureMode::Broken => FloatControlSpec {
            label: "WEAR",
            param: &params.texture.degrade,
            tip: None,
        },
        TextureMode::Interference => FloatControlSpec {
            label: "NOISE",
            param: &params.texture.noise_amount,
            tip: Some("AMOUNT controls the real Noise Amount parameter for interference level."),
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
            label: "WEAR",
            param: &params.texture.degrade,
            tip: Some("RES/DEG controls the real Degrade parameter, mapped to filter drive/resonance."),
        },
        TextureMode::Squash => FloatControlSpec {
            label: "COLOR",
            param: &params.texture.noise_color,
            tip: Some("TONE controls the real Noise Color parameter for squash detector color."),
        },
        TextureMode::Cassette => FloatControlSpec {
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
            label: "COLOR",
            param: &params.texture.noise_color,
            tip: Some("FREQ/COL controls the real Noise Color parameter, mapped to interference frequency and color."),
        },
    }
}

fn texture_third_control<'a>(mode: TextureMode, params: &'a Cc22Params) -> FloatControlSpec<'a> {
    match mode {
        TextureMode::Cassette => FloatControlSpec {
            label: "DRIFT",
            param: &params.texture.random_drift,
            tip: None,
        },
        TextureMode::Filter | TextureMode::Squash => FloatControlSpec {
            label: "WIDTH",
            param: &params.texture.stereo_spread,
            tip: None,
        },
        TextureMode::Broken => FloatControlSpec {
            label: "NOISE",
            param: &params.texture.noise_amount,
            tip: None,
        },
        TextureMode::Interference => FloatControlSpec {
            label: "WEAR",
            param: &params.texture.degrade,
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

fn character_mode_label(mode: CharacterMode) -> &'static str {
    match mode {
        CharacterMode::Drive => "Drive",
        CharacterMode::Sweet => "Sweeten",
        CharacterMode::Fuzz => "Fuzz",
        CharacterMode::Howl => "Howl",
        CharacterMode::Swell => "Swell",
    }
}

fn texture_mode_label(mode: TextureMode) -> &'static str {
    match mode {
        TextureMode::Filter => "Filter",
        TextureMode::Squash => "Squash",
        TextureMode::Cassette => "Cassette",
        TextureMode::Broken => "Broken",
        TextureMode::Interference => "Interference",
    }
}
