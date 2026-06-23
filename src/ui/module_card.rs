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
        transport::NoteDivision,
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

/// How far the floating card lifts off its row while dragging. Deliberately
/// small so the card reads as "picked up" without leaping away from its line.
const DRAG_LIFT: f32 = 7.0;

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
            state.drag_visual_pos = None;
            state.drag_lift = 0.0;
            state.drag_indicator_x = None;
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
                // Glide the insertion marker between slots so neighbours appear to
                // open space fluidly instead of the bar teleporting.
                let target_ix = drop_indicator_x(drop_slot, &card_rects, row_start, gaps);
                let ix = match state.drag_indicator_x {
                    Some(prev) => prev + (target_ix - prev) * 0.40,
                    None => target_ix,
                };
                state.drag_indicator_x = Some(ix);
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
            // The card tracks the cursor at 1:1 scale and stays on its original
            // line: target centre = pointer + grab offset, spring-followed for a
            // soft, controlled feel, with a small eased "pick-up" lift.
            let target = ptr + state.drag_grab_offset;
            let smoothed = match state.drag_visual_pos {
                Some(prev) => prev + (target - prev) * 0.38,
                None => target,
            };
            state.drag_visual_pos = Some(smoothed);
            state.drag_lift += (DRAG_LIFT - state.drag_lift) * 0.22;
            let center = smoothed - Vec2::new(0.0, state.drag_lift);
            let float_rect = Rect::from_center_size(center, Vec2::new(CARD_WIDTH, CARD_HEIGHT));
            let spec = &card_specs[source];
            paint_floating_card(&fp, float_rect, spec.accent, spec.title, source + 1, theme);
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
    // Fixed reserved rect — identical in idle / hover / active / drag, so hover is
    // purely visual and never reflows anything below (EQ / Master stay put). Every
    // hover change beyond this point is paint-only.
    let (card_rect, _) = ui.allocate_exact_size(Vec2::new(CARD_WIDTH, CARD_HEIGHT), Sense::hover());

    // Drop shadow (painter-only). Accent-tinted on hover; constant offset so it
    // never reserves space.
    let shadow_accent = if hovered { Some(spec.accent) } else { None };
    card_shadow(ui.painter(), card_rect, 0.0, shadow_accent);

    // Hover glow as a painter overlay — may visually exceed the card but reserves
    // no layout space (replaces the old position "lift").
    if hovered {
        ui.painter().rect_stroke(
            card_rect.expand(2.0),
            CornerRadius::same(16),
            Stroke::new(1.5, spec.accent.gamma_multiply(0.45)),
            StrokeKind::Outside,
        );
    }

    // Neutral, restrained border (no saturated colour edge); the colour identity
    // comes from the slim top accent bar and the mode dots instead.
    let border_color = if hovered {
        theme.card_edge
    } else {
        theme.card_edge.gamma_multiply(0.8)
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
                .stroke(Stroke::new(1.0, border_color))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH - 16.0);
                    ui.set_min_height(CARD_HEIGHT - 16.0);

                    // Slim top accent bar — the card's colour identity, kept as a
                    // tasteful tab rather than a neon outline.
                    let accent_bar = Rect::from_min_size(
                        Pos2::new(card_rect.left() + 16.0, card_rect.top() + 4.0),
                        Vec2::new(CARD_WIDTH - 32.0, 3.0),
                    );
                    let bar_color = if spec.active {
                        spec.accent.gamma_multiply(if hovered { 1.0 } else { 0.9 })
                    } else {
                        Color32::from_rgba_premultiplied(
                            spec.accent.r(),
                            spec.accent.g(),
                            spec.accent.b(),
                            90,
                        )
                    };
                    ui.painter()
                        .rect_filled(accent_bar, CornerRadius::same(2), bar_color);

                    position_badge(
                        ui,
                        Pos2::new(card_rect.right() - 18.0, card_rect.bottom() - 16.0),
                        position_num,
                        spec.accent,
                    );

                    let handle_resp =
                        drag_handle(ui, card_rect, spec.accent, position_num, hovered);
                    if detect_drag && handle_resp.drag_started() {
                        state.drag_source = Some(position_num - 1);
                        state.drag_drop_slot = None;
                        // Remember where on the card the user grabbed, so the
                        // floating proxy stays under the cursor at its original
                        // relative position instead of snapping its centre to it.
                        let ptr = ui
                            .ctx()
                            .input(|i| i.pointer.latest_pos())
                            .unwrap_or(card_rect.center());
                        state.drag_grab_offset = card_rect.center() - ptr;
                        state.drag_visual_pos = None;
                        state.drag_lift = 0.0;
                        state.drag_indicator_x = None;
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
    let top = ui.cursor().min.y;
    let left = ui.min_rect().left();
    let width = ui.available_width();
    let row_h = 20.0;
    let center_y = top + row_h * 0.5;

    // Premium ON/OFF pill switch — a small sliding knob rather than a loose
    // "ON/BYP" badge stuck to the title.
    let bypassed = spec.bypass.value();
    let on = !bypassed;
    let sw = Rect::from_min_size(Pos2::new(left, center_y - 8.0), Vec2::new(30.0, 16.0));
    let power = ui.interact(
        sw,
        egui::Id::new(("module-power", spec.title)),
        Sense::click(),
    );
    if power.clicked() {
        set_param(setter, spec.bypass, !bypassed);
    }
    let track = if on {
        spec.accent.gamma_multiply(0.92)
    } else {
        theme.card_dim
    };
    ui.painter().rect_filled(sw, CornerRadius::same(8), track);
    ui.painter().rect_stroke(
        sw,
        CornerRadius::same(8),
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    let knob_x = if on {
        sw.right() - 8.0
    } else {
        sw.left() + 8.0
    };
    ui.painter().circle_filled(
        Pos2::new(knob_x, sw.center().y),
        6.0,
        Color32::from_rgb(250, 247, 240),
    );
    ui.painter().circle_stroke(
        Pos2::new(knob_x, sw.center().y),
        6.0,
        Stroke::new(1.0, theme.card_edge.gamma_multiply(0.6)),
    );
    power.on_hover_text(if on {
        "Module on \u{2014} click to bypass"
    } else {
        "Bypassed \u{2014} click to enable"
    });

    // Module title with clear hierarchy (largest text on the card).
    ui.painter().text(
        Pos2::new(sw.right() + 10.0, center_y),
        egui::Align2::LEFT_CENTER,
        spec.title,
        FontId::monospace(FONT_MODULE_TITLE),
        if on {
            theme.text_dark
        } else {
            theme.muted_dark
        },
    );

    // Subtle accent underline separates the header from the mode list without a
    // heavy filled band.
    let underline_y = top + row_h + 3.0;
    ui.painter().line_segment(
        [
            Pos2::new(left, underline_y),
            Pos2::new(left + width, underline_y),
        ],
        Stroke::new(1.0, spec.accent.gamma_multiply(if on { 0.5 } else { 0.22 })),
    );
    let _ = hovered;
    ui.add_space(row_h + 8.0);
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
}

/// Renders the module's five product modes as a fixed list of selectable rows.
/// Every card shows exactly five items, in product order, each prefixed by a
/// per-row colored square. The selected mode's name is drawn in bold so it
/// stands out from the other options.
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

    for (index, (value, label)) in options.iter().enumerate() {
        let selected = current == *value && !bypass.value();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click());

        // Subtle row backing: a quiet accent wash on the active row (and a fainter
        // one on hover) so selection reads as a soft highlight, never clutter.
        if selected {
            ui.painter().rect_filled(
                rect,
                radius,
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 22),
            );
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect,
                radius,
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 14),
            );
        }

        // Per-row colored dot ("bolinha"): red, yellow, green, blue, purple by
        // position. These are the plugin's visual identity, so they are always
        // drawn; the active one is a touch larger and brighter with a soft halo,
        // while the others stay legible but quieter.
        let dot_center = Pos2::new(rect.left() + 9.0, rect.center().y);
        let base = mode_option_color(index);
        let dot_radius = if selected { 4.3 } else { 3.4 };
        if selected {
            ui.painter().circle_filled(
                dot_center,
                dot_radius + 2.0,
                Color32::from_rgba_premultiplied(base.r(), base.g(), base.b(), 60),
            );
            ui.painter().circle_filled(dot_center, dot_radius, base);
        } else {
            ui.painter().circle_filled(
                dot_center,
                dot_radius,
                Color32::from_rgba_premultiplied(base.r(), base.g(), base.b(), 135),
            );
        }

        // Selected mode: bold name (a second offset pass thickens the strokes,
        // since egui's monospace family has no dedicated bold weight). Others
        // stay regular weight and slightly muted so the bold one pops.
        let text_color = if selected {
            theme.text_dark
        } else {
            theme.muted_dark
        };
        let offsets: &[f32] = if selected { &[0.0, 0.55] } else { &[0.0] };
        for &dx in offsets {
            ui.painter().text(
                Pos2::new(rect.left() + 22.0 + dx, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                FontId::monospace(10.0),
                text_color,
            );
        }

        if response.clicked() {
            set_param(setter, bypass, false);
            set_param(setter, param, *value);
        }
        response.on_hover_text("Click to select this mode");
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
    ui.add_space(12.0);

    match spec.module {
        ChainModule::Character => {
            let mode = params.character.mode.value();
            let (first_label, first_param, first_tip) = match mode {
                CharacterMode::Howl => (
                    "RESO",
                    &params.character.drive,
                    Some(
                        "RESO - howl intensity: input push, resonant grind and feedback-like aggression.",
                    ),
                ),
                CharacterMode::Swell => (
                    "AMOUNT",
                    &params.character.drive,
                    Some(
                        "AMOUNT - swell length: attack/decay (low = fast, high = long/cinematic).",
                    ),
                ),
                _ => ("DRIVE", &params.character.drive, None),
            };
            let (second_label, second_param, second_tip) = match mode {
                CharacterMode::Howl => (
                    "FORMANT",
                    &params.character.tone,
                    Some(
                        "FORMANT - vowel center and final brightness.",
                    ),
                ),
                CharacterMode::Swell => (
                    "SENSE",
                    &params.character.tone,
                    Some(
                        "SENSE - swell sensitivity / onset threshold (high = lighter touch retriggers).",
                    ),
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
                    label: "AGE",
                    param: &params.character.age,
                    tip: Some(
                        "AGE adds gentle grit and a tape-style high-frequency roll-off (warmth).",
                    ),
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
            ui.add_space(2.0);
            sync_controls(
                ui,
                setter,
                &params.movement.sync_enabled,
                &params.movement.sync_division,
                Some((
                    "LOCK",
                    &params.movement.sync_phase_lock,
                    "Phase-lock the LFO to the bar/beat while the host plays.",
                )),
                spec.accent,
                theme,
                true,
            );
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
            ui.add_space(2.0);
            // The PRE option only governs the Space pre-delay, so it appears only
            // for Space; the other diffusion modes show a clean SYNC chip.
            let pre_extra = if mode == DiffusionMode::Space {
                Some((
                    "PRE",
                    &params.diffusion.sync_pre_delay,
                    "Lock the Space pre-delay to tempo (decay stays free).",
                ))
            } else {
                None
            };
            sync_controls(
                ui,
                setter,
                &params.diffusion.sync_enabled,
                &params.diffusion.sync_division,
                pre_extra,
                spec.accent,
                theme,
                true,
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
            ui.add_space(2.0);
            // Texture only reacts to tempo on its time-smeared modes; Filter and
            // Squash show a disabled SYNC chip with an explanatory tooltip.
            let sync_applicable = matches!(
                mode,
                TextureMode::Cassette | TextureMode::Broken | TextureMode::Interference
            );
            sync_controls(
                ui,
                setter,
                &params.texture.sync_enabled,
                &params.texture.sync_division,
                None,
                spec.accent,
                theme,
                sync_applicable,
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
    // Distribute the knobs across equal-width columns and centre each one, so a
    // two-knob card (Character) spreads evenly across the card instead of
    // bunching to the left and leaving a lopsided gap.
    let cell_width = ui.available_width() / controls.len() as f32;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for control in controls {
            ui.allocate_ui(Vec2::new(cell_width, 68.0), |ui| {
                ui.vertical_centered(|ui| {
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
                });
            });
        }
    });
}

/// Compact, musical sync control for the time-based modules. Renders as a single
/// unified "SYNC · 1/8" chip (left half toggles tempo-sync, right half cycles the
/// division), with any LOCK/PRE option reduced to a small dot indicator rather
/// than a competing button. When `applicable` is false (e.g. Texture modes that
/// ignore tempo) it shows a quiet, disabled "SYNC —" chip with a tooltip.
fn sync_controls(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    enabled: &BoolParam,
    division: &EnumParam<NoteDivision>,
    extra: Option<(&'static str, &BoolParam, &'static str)>,
    accent: Color32,
    theme: Theme,
    applicable: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        if !applicable {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(96.0, 16.0), Sense::hover());
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(8),
                theme.card_dim.gamma_multiply(0.9),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "SYNC \u{2014}",
                FontId::monospace(8.5),
                theme.muted,
            );
            resp.on_hover_text("This mode doesn't use tempo sync.");
            return;
        }

        let on = enabled.value();
        let names = NoteDivision::variants();
        let count = names.len();
        let current = division.value().to_index();

        // One pill, two halves: SYNC (toggle) · division (cycle).
        let pill_w = 98.0;
        let (pill, _) = ui.allocate_exact_size(Vec2::new(pill_w, 16.0), Sense::hover());
        let split_x = pill.left() + 48.0;
        let left_rect = Rect::from_min_max(pill.min, Pos2::new(split_x, pill.bottom()));
        let right_rect = Rect::from_min_max(Pos2::new(split_x, pill.top()), pill.max);

        // Backing: accent-tinted left when armed, neutral paper right.
        ui.painter().rect_filled(
            pill,
            CornerRadius::same(8),
            if on { theme.paper } else { theme.card_dim },
        );
        if on {
            ui.painter().rect_filled(
                left_rect,
                CornerRadius {
                    nw: 8,
                    sw: 8,
                    ne: 0,
                    se: 0,
                },
                accent.gamma_multiply(0.92),
            );
        }
        ui.painter().rect_stroke(
            pill,
            CornerRadius::same(8),
            Stroke::new(1.0, theme.card_edge.gamma_multiply(0.7)),
            StrokeKind::Inside,
        );
        ui.painter().line_segment(
            [
                Pos2::new(split_x, pill.top() + 3.0),
                Pos2::new(split_x, pill.bottom() - 3.0),
            ],
            Stroke::new(1.0, theme.card_edge.gamma_multiply(0.5)),
        );
        ui.painter().text(
            left_rect.center(),
            egui::Align2::CENTER_CENTER,
            "SYNC",
            FontId::monospace(8.0),
            if on { Color32::WHITE } else { theme.muted_dark },
        );
        ui.painter().text(
            right_rect.center(),
            egui::Align2::CENTER_CENTER,
            names[current],
            FontId::monospace(8.5),
            if on {
                theme.text_dark
            } else {
                theme.muted_dark
            },
        );

        let sync_resp = ui.interact(
            left_rect,
            ui.make_persistent_id(("sync-toggle", enabled as *const _ as usize)),
            Sense::click(),
        );
        if sync_resp.clicked() {
            set_param(setter, enabled, !on);
        }
        sync_resp.on_hover_text("Sync uses DAW BPM (falls back to 120).");

        let div_resp = ui.interact(
            right_rect,
            ui.make_persistent_id(("sync-division", division as *const _ as usize)),
            Sense::click(),
        );
        if div_resp.clicked() {
            set_param(
                setter,
                division,
                NoteDivision::from_index((current + 1) % count),
            );
        } else if div_resp.secondary_clicked() {
            set_param(
                setter,
                division,
                NoteDivision::from_index((current + count - 1) % count),
            );
        }
        div_resp.on_hover_text("Click to cycle the sync division (right-click for previous).");

        // Optional LOCK/PRE option as a discreet dot indicator, not a button.
        if let Some((label, param, tip)) = extra {
            let active = param.value();
            let (erect, eresp) = ui.allocate_exact_size(Vec2::new(42.0, 16.0), Sense::click());
            if eresp.clicked() {
                set_param(setter, param, !active);
            }
            let dot = Pos2::new(erect.left() + 6.0, erect.center().y);
            if active {
                ui.painter().circle_filled(dot, 3.0, accent);
            } else {
                ui.painter()
                    .circle_stroke(dot, 3.0, Stroke::new(1.0, theme.muted));
            }
            ui.painter().text(
                Pos2::new(erect.left() + 13.0, erect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                FontId::monospace(8.0),
                if active { theme.text_dark } else { theme.muted },
            );
            eresp.on_hover_text(tip);
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
            tip: Some("TIME - doubler offset (the 'second take' delay)."),
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
            tip: Some("RESO - phaser feedback / resonance."),
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
            tip: Some("TIME - tape echo base delay time."),
        },
        DiffusionMode::Reverse => FloatControlSpec {
            label: "TIME",
            param: &params.diffusion.time,
            tip: Some("TIME - playback speed/pitch: left = down, centre = normal, right = up."),
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
            tip: Some("WOW/DRIFT - tape wow, flutter and random drift."),
        },
        DiffusionMode::Reverse => FloatControlSpec {
            label: "LENGTH",
            param: &params.diffusion.size,
            tip: Some("LENGTH - reverse segment length / density."),
        },
        DiffusionMode::Space => FloatControlSpec {
            label: "DECAY",
            param: &params.diffusion.decay,
            tip: None,
        },
        DiffusionMode::Collage => FloatControlSpec {
            label: "DENSITY",
            param: &params.diffusion.feedback,
            tip: Some("DENSITY - Collage fragment density."),
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
        tip: Some("MIX - dry/wet blend."),
    };

    match mode {
        DiffusionMode::Reels => (
            mix,
            FloatControlSpec {
                label: "REPEATS",
                param: &params.diffusion.feedback,
                tip: Some("REPEATS - amount of colored tape echo fed back."),
            },
        ),
        DiffusionMode::Reverse => (
            mix,
            FloatControlSpec {
                label: "REPEATS",
                param: &params.diffusion.feedback,
                tip: Some("REPEATS - how much reversed signal feeds the repeats."),
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
            tip: Some("COLOR - filter character / tilt."),
        },
        TextureMode::Squash => FloatControlSpec {
            label: "WEAR",
            param: &params.texture.degrade,
            tip: Some("WEAR - squash intensity."),
        },
        TextureMode::Broken => FloatControlSpec {
            label: "WEAR",
            param: &params.texture.degrade,
            tip: None,
        },
        TextureMode::Interference => FloatControlSpec {
            label: "NOISE",
            param: &params.texture.noise_amount,
            tip: Some("NOISE - interference level."),
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
            tip: Some("WEAR - filter drive / resonance."),
        },
        TextureMode::Squash => FloatControlSpec {
            label: "COLOR",
            param: &params.texture.noise_color,
            tip: Some("COLOR - squash detector color/tone."),
        },
        TextureMode::Cassette => FloatControlSpec {
            label: "NOISE",
            param: &params.texture.noise_amount,
            tip: Some("NOISE - tape hiss amount."),
        },
        TextureMode::Broken => FloatControlSpec {
            label: "DRIFT",
            param: &params.texture.random_drift,
            tip: Some("DRIFT - random instability."),
        },
        TextureMode::Interference => FloatControlSpec {
            label: "COLOR",
            param: &params.texture.noise_color,
            tip: Some("COLOR - interference frequency & color."),
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

        response.on_hover_text("SHAPE - tremolo LFO waveform.")
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
