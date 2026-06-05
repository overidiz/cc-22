use std::{sync::Arc, time::Duration};

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self, Align, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke,
        StrokeKind, UiBuilder, Vec2,
    },
    resizable_window::ResizableWindow,
};

use crate::{
    dsp::{
        character::CharacterMode, diffusion::DiffusionMode, eq::EqMode, movement::MovementMode,
        texture::TextureMode,
    },
    meters::Meters,
    params::Cc22Params,
    presets::internal_presets,
    Cc22,
};

const CARD_HEIGHT: f32 = 342.0;
const CARD_WIDTH: f32 = 226.0;
const KNOB_SIZE: f32 = 58.0;

#[derive(Debug, Clone, Copy)]
struct Theme {
    background: Color32,
    paper: Color32,
    paper_alt: Color32,
    card: Color32,
    card_dim: Color32,
    card_edge: Color32,
    text_dark: Color32,
    text_light: Color32,
    muted: Color32,
    muted_dark: Color32,
    warning: Color32,
    shadow: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color32::from_rgb(218, 211, 196),
            paper: Color32::from_rgb(239, 234, 222),
            paper_alt: Color32::from_rgb(229, 223, 210),
            card: Color32::from_rgb(236, 231, 220),
            card_dim: Color32::from_rgb(224, 219, 208),
            card_edge: Color32::from_rgb(190, 183, 169),
            text_dark: Color32::from_rgb(35, 31, 27),
            text_light: Color32::from_rgb(35, 31, 27),
            muted: Color32::from_rgb(150, 143, 130),
            muted_dark: Color32::from_rgb(91, 84, 73),
            warning: Color32::from_rgb(235, 85, 72),
            shadow: Color32::from_rgba_premultiplied(50, 42, 31, 28),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ModuleColors {
    character: Color32,
    movement: Color32,
    diffusion: Color32,
    texture: Color32,
    eq: Color32,
    master: Color32,
}

#[derive(Debug, Clone, Copy)]
struct Look {
    colors: ModuleColors,
    theme: Theme,
}

struct ModuleCardSpec<'a> {
    title: &'static str,
    accent: Color32,
    active: bool,
    bypass: &'a BoolParam,
}

impl Default for ModuleColors {
    fn default() -> Self {
        Self {
            character: Color32::from_rgb(245, 84, 72),
            movement: Color32::from_rgb(245, 180, 45),
            diffusion: Color32::from_rgb(76, 210, 126),
            texture: Color32::from_rgb(63, 190, 224),
            eq: Color32::from_rgb(250, 158, 48),
            master: Color32::from_rgb(238, 229, 207),
        }
    }
}

#[derive(Default)]
struct UiState {
    selected_preset: usize,
    random_seed: u32,
    input_meter: MeterBallistics,
    output_meter: MeterBallistics,
    input_clip_events: u32,
    output_clip_events: u32,
    input_clip_until: f64,
    output_clip_until: f64,
    last_meter_time: Option<f64>,
}

pub fn create_editor(
    params: Arc<Cc22Params>,
    meters: Arc<Meters>,
    _async_executor: AsyncExecutor<Cc22>,
) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();

    create_egui_editor(
        editor_state.clone(),
        UiState {
            random_seed: 0xCC22_2026,
            ..UiState::default()
        },
        |ctx, _state| {
            let theme = Theme::default();
            let mut style = (*ctx.style()).clone();
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.window_margin = egui::Margin::same(0);
            style.visuals.window_fill = theme.background;
            style.visuals.panel_fill = theme.background;
            style.visuals.extreme_bg_color = theme.background;
            style.visuals.widgets.inactive.bg_fill = theme.paper;
            style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(250, 246, 235);
            style.visuals.widgets.active.bg_fill = Color32::from_rgb(226, 220, 207);
            style.visuals.selection.bg_fill = Color32::from_rgb(39, 40, 43);
            style.visuals.override_text_color = Some(theme.text_dark);
            ctx.set_style(style);
        },
        move |ctx, setter, state| {
            let theme = Theme::default();
            let colors = ModuleColors::default();
            let look = Look { colors, theme };
            let now = ctx.input(|input| input.time);
            ctx.request_repaint_after(Duration::from_millis(33));

            ResizableWindow::new("CC-22")
                .min_size(Vec2::new(960.0, 720.0))
                .show(ctx, editor_state.as_ref(), |ui| {
                    egui::Frame::new()
                        .fill(theme.background)
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                top_bar(ui, setter, state, &params, colors, theme);
                                ui.add_space(8.0);
                                center_modules(ui, setter, state, &params, &meters, now, look);
                                ui.add_space(8.0);
                                bottom_macro_row(ui, setter, state, &params, colors, theme);
                            });
                        });
                });
        },
    )
}

fn top_bar(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    rounded_panel(
        ui,
        theme.paper_alt,
        theme.text_dark,
        CornerRadius::same(18),
        |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        brand_mark(ui, colors, theme);
                        ui.label(
                            RichText::new("CC-22")
                                .font(FontId::proportional(31.0))
                                .strong()
                                .color(theme.text_dark),
                        );
                    });
                    ui.label(
                        RichText::new("MODULAR COLOR PROCESSOR")
                            .font(FontId::monospace(10.0))
                            .color(theme.muted_dark),
                    );
                });

                ui.add_space(18.0);
                preset_selector(ui, setter, state, params, theme);

                if compact_button(ui, "PREV", theme, colors.master).clicked() {
                    previous_preset(setter, state, params);
                }
                if compact_button(ui, "NEXT", theme, colors.master).clicked() {
                    next_preset(setter, state, params);
                }
                let _ = compact_button(ui, "SAVE", theme, colors.master)
                    .on_hover_text("Host preset save");
                if compact_button(ui, "RND", theme, colors.texture)
                    .on_hover_text("Randomize musical control values")
                    .clicked()
                {
                    randomize_controls(setter, state, params);
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    global_bypass_button(ui, setter, &params.global_bypass, theme);
                    colored_knob(
                        ui,
                        setter,
                        &params.dry_wet,
                        "INTENSITY",
                        colors.master,
                        theme,
                        58.0,
                    );
                });
            });
        },
    );
}

fn center_modules(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    meters: &Meters,
    now: f64,
    look: Look,
) {
    let colors = look.colors;
    let theme = look.theme;
    let meter_reading = state.next_meter_reading(meters, now);

    ui.horizontal_top(|ui| {
        module_card(
            ui,
            setter,
            theme,
            ModuleCardSpec {
                title: "CHARACTER",
                accent: colors.character,
                active: character_active(params),
                bypass: &params.character.bypass,
            },
            |ui| {
                colored_knob(
                    ui,
                    setter,
                    &params.character.drive,
                    "DRIVE",
                    colors.character,
                    theme,
                    KNOB_SIZE,
                );
                colored_knob(
                    ui,
                    setter,
                    &params.character.tone,
                    "TONE",
                    colors.character,
                    theme,
                    KNOB_SIZE,
                );
                module_mode_summary(
                    ui,
                    character_mode_label(params.character.mode.value()),
                    &["Clean", "Saturation", "Cassette"],
                    colors.character,
                    theme,
                );
                character_mode_selector(
                    ui,
                    setter,
                    &params.character.mode,
                    colors.character,
                    theme,
                );
            },
        );

        module_card(
            ui,
            setter,
            theme,
            ModuleCardSpec {
                title: "MOVEMENT",
                accent: colors.movement,
                active: movement_active(params),
                bypass: &params.movement.bypass,
            },
            |ui| {
                colored_knob(
                    ui,
                    setter,
                    &params.movement.rate,
                    "RATE",
                    colors.movement,
                    theme,
                    KNOB_SIZE,
                );
                colored_knob(
                    ui,
                    setter,
                    &params.movement.depth,
                    "DEPTH",
                    colors.movement,
                    theme,
                    KNOB_SIZE,
                );
                module_mode_summary(
                    ui,
                    movement_mode_label(params.movement.mode.value()),
                    &["Off", "Chorus", "Vibrato", "Tremolo"],
                    colors.movement,
                    theme,
                );
                movement_mode_selector(ui, setter, &params.movement.mode, colors.movement, theme);
            },
        );

        module_card(
            ui,
            setter,
            theme,
            ModuleCardSpec {
                title: "DIFFUSION",
                accent: colors.diffusion,
                active: diffusion_active(params),
                bypass: &params.diffusion.bypass,
            },
            |ui| {
                colored_knob(
                    ui,
                    setter,
                    &params.diffusion.time,
                    "TIME",
                    colors.diffusion,
                    theme,
                    KNOB_SIZE,
                );
                colored_knob(
                    ui,
                    setter,
                    &params.diffusion.feedback,
                    "FDBK",
                    colors.diffusion,
                    theme,
                    KNOB_SIZE,
                );
                module_mode_summary(
                    ui,
                    diffusion_mode_label(params.diffusion.mode.value()),
                    &["Off", "Delay", "Slap", "Reverb"],
                    colors.diffusion,
                    theme,
                );
                diffusion_mode_selector(
                    ui,
                    setter,
                    &params.diffusion.mode,
                    colors.diffusion,
                    theme,
                );
            },
        );

        module_card(
            ui,
            setter,
            theme,
            ModuleCardSpec {
                title: "TEXTURE",
                accent: colors.texture,
                active: texture_active(params),
                bypass: &params.texture.bypass,
            },
            |ui| {
                colored_knob(
                    ui,
                    setter,
                    &params.texture.wow_depth,
                    "WOW",
                    colors.texture,
                    theme,
                    KNOB_SIZE,
                );
                colored_knob(
                    ui,
                    setter,
                    &params.texture.flutter_depth,
                    "FLTR",
                    colors.texture,
                    theme,
                    KNOB_SIZE,
                );
                module_mode_summary(
                    ui,
                    texture_mode_label(params.texture.mode.value()),
                    &["Off", "WowFlutter", "Noise"],
                    colors.texture,
                    theme,
                );
                texture_mode_selector(ui, setter, &params.texture.mode, colors.texture, theme);
            },
        );
    });

    ui.add_space(8.0);
    ui.horizontal_top(|ui| {
        eq_workbench(ui, setter, params, colors, theme);
        master_strip(ui, setter, params, meter_reading, colors.master, theme);
    });
}

fn module_mode_summary(
    ui: &mut egui::Ui,
    active_label: &'static str,
    modes: &[&'static str],
    accent: Color32,
    theme: Theme,
) {
    ui.add_space(2.0);
    let (bar_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
    ui.painter().rect_filled(
        bar_rect,
        CornerRadius::same(4),
        Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 62),
    );
    ui.painter().text(
        bar_rect.center(),
        egui::Align2::CENTER_CENTER,
        active_label,
        FontId::monospace(9.0),
        theme.text_dark,
    );

    ui.add_space(4.0);
    for mode in modes.iter().take(5) {
        ui.horizontal(|ui| {
            let selected = *mode == active_label;
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().circle_filled(
                dot_rect.center(),
                if selected { 2.6 } else { 1.8 },
                if selected { accent } else { theme.muted },
            );
            ui.label(
                RichText::new(*mode)
                    .font(FontId::monospace(8.0))
                    .strong()
                    .color(if selected {
                        theme.text_dark
                    } else {
                        theme.muted
                    }),
            );
        });
    }
}

fn eq_workbench(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    let eq_on = eq_active(params);
    egui::Frame::new()
        .fill(theme.paper)
        .stroke(Stroke::new(1.0, theme.card_edge))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(690.0, 180.0));
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("EQUALIZER")
                                .font(FontId::monospace(14.0))
                                .strong()
                                .color(theme.text_dark),
                        );
                        ui.label(
                            RichText::new(if eq_on { "5-BAND  ON" } else { "5-BAND  OFF" })
                                .font(FontId::monospace(9.0))
                                .strong()
                                .color(theme.muted_dark),
                        );
                        ui.add_space(14.0);
                        eq_mode_selector(ui, setter, &params.eq.mode, colors.eq, theme);
                    });
                    ui.add_space(5.0);
                    eq_canvas(ui, params, colors, theme);
                });

                ui.add_space(10.0);
                ui.vertical(|ui| {
                    band_tabs(ui, colors, theme);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("BAND 3  PEAK")
                            .font(FontId::monospace(11.0))
                            .strong()
                            .color(theme.text_dark),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} / {} / Q {}",
                            value_string(&params.eq.mid_frequency),
                            value_string(&params.eq.mid_gain),
                            value_string(&params.eq.mid_q)
                        ))
                        .font(FontId::monospace(9.0))
                        .color(theme.muted_dark),
                    );
                    ui.add_space(7.0);
                    ui.horizontal(|ui| {
                        colored_knob(
                            ui,
                            setter,
                            &params.eq.mid_frequency,
                            "FREQ",
                            colors.eq,
                            theme,
                            48.0,
                        );
                        colored_knob(
                            ui,
                            setter,
                            &params.eq.mid_gain,
                            "GAIN",
                            colors.eq,
                            theme,
                            48.0,
                        );
                        colored_knob(ui, setter, &params.eq.mid_q, "Q", colors.eq, theme, 48.0);
                    });
                });
            });
        });
}

fn eq_canvas(ui: &mut egui::Ui, params: &Cc22Params, colors: ModuleColors, theme: Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(485.0, 130.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(2),
        Color32::from_rgb(244, 240, 230),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, theme.card_edge),
        StrokeKind::Outside,
    );

    for index in 0..18 {
        let x = rect.left() + rect.width() * index as f32 / 17.0;
        let color = if index % 3 == 0 {
            Color32::from_rgb(207, 200, 186)
        } else {
            Color32::from_rgb(225, 219, 206)
        };
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(0.7, color),
        );
    }
    for index in 0..11 {
        let y = rect.top() + rect.height() * index as f32 / 10.0;
        let color = if index == 5 {
            Color32::from_rgb(170, 162, 148)
        } else {
            Color32::from_rgb(225, 219, 206)
        };
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(if index == 5 { 1.0 } else { 0.7 }, color),
        );
    }

    let points = [
        (
            0.12,
            params.eq.low_shelf_gain.unmodulated_normalized_value(),
            colors.character,
        ),
        (0.30, 0.50, colors.movement),
        (
            0.50,
            params.eq.mid_gain.unmodulated_normalized_value(),
            colors.eq,
        ),
        (0.66, 0.50, colors.diffusion),
        (
            0.82,
            params.eq.high_shelf_gain.unmodulated_normalized_value(),
            colors.texture,
        ),
        (0.92, 0.50, colors.texture),
    ];

    let mut curve = Vec::with_capacity(points.len());
    for (x, y_norm, _) in points.iter().copied() {
        let y = rect.center().y - ((y_norm - 0.5) * rect.height() * 0.75);
        curve.push(Pos2::new(rect.left() + rect.width() * x, y));
    }
    painter.add(egui::Shape::line(curve, Stroke::new(2.0, colors.eq)));

    for (index, (x, y_norm, color)) in points.iter().enumerate() {
        let center = Pos2::new(
            rect.left() + rect.width() * x,
            rect.center().y - ((y_norm - 0.5) * rect.height() * 0.75),
        );
        painter.circle_filled(center, if index == 2 { 8.5 } else { 5.0 }, *color);
        painter.circle_stroke(
            center,
            if index == 2 { 12.0 } else { 6.5 },
            Stroke::new(1.2, theme.paper),
        );
        painter.text(
            center + Vec2::new(0.0, -19.0),
            egui::Align2::CENTER_CENTER,
            format!("{}", index + 1),
            FontId::monospace(8.0),
            theme.muted_dark,
        );
    }
}

fn band_tabs(ui: &mut egui::Ui, colors: ModuleColors, theme: Theme) {
    ui.horizontal(|ui| {
        let band_colors = [
            colors.character,
            colors.movement,
            colors.eq,
            Color32::from_rgb(220, 188, 78),
            colors.diffusion,
            colors.texture,
        ];
        for (index, color) in band_colors.into_iter().enumerate() {
            let selected = index == 2;
            ui.add(
                egui::Button::new(
                    RichText::new(format!("{}", index + 1))
                        .font(FontId::monospace(10.0))
                        .strong()
                        .color(if selected {
                            Color32::WHITE
                        } else {
                            theme.text_dark
                        }),
                )
                .fill(if selected {
                    color
                } else {
                    Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 82)
                })
                .stroke(Stroke::new(1.0, theme.card_edge))
                .corner_radius(CornerRadius::same(6))
                .min_size(Vec2::new(49.0, 20.0)),
            );
        }
    });
}

fn bottom_macro_row(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    colors: ModuleColors,
    theme: Theme,
) {
    rounded_panel(
        ui,
        theme.paper,
        theme.text_dark,
        CornerRadius::same(18),
        |ui| {
            ui.horizontal_top(|ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(31, 26, 21))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_min_size(Vec2::new(385.0, 54.0));
                        ui.horizontal(|ui| {
                            brand_orb(ui, colors);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("CC-22")
                                        .font(FontId::proportional(22.0))
                                        .strong()
                                        .color(Color32::from_rgb(245, 237, 218)),
                                );
                                ui.label(
                                    RichText::new("MULTI-FX")
                                        .font(FontId::monospace(8.0))
                                        .strong()
                                        .color(colors.eq),
                                );
                            });
                            ui.add_space(10.0);
                            compact_nav_button(ui, "<", theme);
                            compact_nav_button(ui, "INIT", theme);
                            compact_nav_button(ui, ">", theme);
                            ui.add_space(12.0);
                            small_strip_knob(
                                ui,
                                setter,
                                &params.character.mix,
                                "MIX",
                                colors.character,
                                theme,
                            );
                            small_strip_knob(
                                ui,
                                setter,
                                &params.output_gain,
                                "OUT",
                                colors.master,
                                theme,
                            );
                            small_strip_knob(ui, setter, &params.dry_wet, "CLIP", colors.eq, theme);
                            small_strip_knob(
                                ui,
                                setter,
                                &params.diffusion.mix,
                                "SPACE",
                                colors.diffusion,
                                theme,
                            );
                            small_strip_knob(
                                ui,
                                setter,
                                &params.movement.rate,
                                "RATE",
                                colors.movement,
                                theme,
                            );
                        });
                    });

                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        preset_selector_with_id(
                            ui,
                            setter,
                            state,
                            params,
                            theme,
                            "bottom-preset-selector",
                            150.0,
                        );
                        global_bypass_button(ui, setter, &params.global_bypass, theme);
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        mini_control_combo(ui, "FILTER", "Tilt", theme);
                        mini_control_combo(ui, "OS", "2x", theme);
                        mini_control_combo(ui, "HQ", "Eco", theme);
                        mini_control_combo(ui, "M/S", "Off", theme);
                    });
                });
            });
        },
    );
}

fn master_strip(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    params: &Cc22Params,
    meter_reading: MeterReading,
    accent: Color32,
    theme: Theme,
) {
    egui::Frame::new()
        .fill(Color32::from_rgb(32, 27, 22))
        .stroke(Stroke::new(1.0, Color32::from_rgb(74, 64, 54)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(220.0, 180.0));
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("MASTER")
                        .font(FontId::monospace(14.0))
                        .strong()
                        .color(Color32::from_rgb(245, 237, 218)),
                );
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    level_meter(ui, "IN", meter_reading.input, accent, theme);
                    level_meter(ui, "OUT", meter_reading.output, accent, theme);
                });
                clip_indicator(
                    ui,
                    "CLIP",
                    meter_reading.input.clipped || meter_reading.output.clipped,
                    theme,
                );
                ui.add_space(8.0);
                colored_knob(ui, setter, &params.input_gain, "INPUT", accent, theme, 45.0);
                colored_knob(
                    ui,
                    setter,
                    &params.output_gain,
                    "OUTPUT",
                    accent,
                    theme,
                    45.0,
                );
                mini_slider(ui, setter, &params.dry_wet, "DRY/WET", accent, theme);
            });
        });
}

fn module_card<R>(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    theme: Theme,
    spec: ModuleCardSpec<'_>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    let fill = if spec.active {
        theme.card
    } else {
        theme.card_dim
    };

    let (rect, _) = ui.allocate_exact_size(Vec2::new(CARD_WIDTH, CARD_HEIGHT), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
        |ui| {
            egui::Frame::new()
                .fill(fill)
                .stroke(Stroke::new(
                    1.2,
                    if spec.active {
                        spec.accent
                    } else {
                        theme.card_edge
                    },
                ))
                .corner_radius(CornerRadius::same(18))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH - 20.0);
                    ui.set_min_height(CARD_HEIGHT - 20.0);
                    let header_rect = Rect::from_min_size(
                        ui.available_rect_before_wrap().min,
                        Vec2::new(ui.available_width(), 30.0),
                    );
                    ui.painter().rect_filled(
                        header_rect,
                        CornerRadius::same(7),
                        if spec.active {
                            spec.accent
                        } else {
                            theme.card_edge
                        },
                    );
                    let cap_rect = Rect::from_min_size(
                        ui.available_rect_before_wrap().min,
                        Vec2::new(ui.available_width(), 4.0),
                    );
                    ui.painter().rect_filled(
                        cap_rect,
                        CornerRadius::same(3),
                        if spec.active {
                            spec.accent
                        } else {
                            theme.card_edge
                        },
                    );
                    ui.add_space(8.0);
                    module_header(
                        ui,
                        spec.title,
                        spec.accent,
                        spec.active,
                        spec.bypass,
                        setter,
                        theme,
                    );
                    ui.add_space(6.0);
                    add_contents(ui);
                });
        },
    );
}

fn module_header(
    ui: &mut egui::Ui,
    title: &'static str,
    accent: Color32,
    active: bool,
    bypass: &BoolParam,
    setter: &ParamSetter<'_>,
    theme: Theme,
) {
    ui.horizontal(|ui| {
        let led_color = if active { accent } else { theme.muted_dark };
        let (led_rect, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
        ui.painter()
            .circle_filled(led_rect.center(), 4.0, led_color);
        ui.painter()
            .circle_stroke(led_rect.center(), 4.7, Stroke::new(1.0, Color32::BLACK));

        if ui
            .add(
                egui::Label::new(
                    RichText::new(title)
                        .font(FontId::monospace(13.0))
                        .strong()
                        .color(if active {
                            Color32::WHITE
                        } else {
                            theme.text_dark
                        }),
                )
                .sense(Sense::click()),
            )
            .on_hover_text("Click module name to enable/bypass")
            .clicked()
        {
            set_param(setter, bypass, !bypass.value());
        }

        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            toggle_button(ui, setter, bypass, active, accent, theme);
        });
    });
    ui.add_space(3.0);
    let line = Rect::from_min_size(
        Pos2::new(ui.min_rect().left(), ui.cursor().min.y),
        Vec2::new(ui.available_width(), 1.0),
    );
    ui.painter().rect_filled(
        line,
        CornerRadius::same(1),
        Color32::from_rgba_premultiplied(
            accent.r(),
            accent.g(),
            accent.b(),
            if active { 95 } else { 28 },
        ),
    );
    ui.add_space(5.0);
}

fn preset_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    theme: Theme,
) {
    preset_selector_with_id(ui, setter, state, params, theme, "preset-selector", 220.0);
}

fn preset_selector_with_id(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    state: &mut UiState,
    params: &Cc22Params,
    theme: Theme,
    id: &'static str,
    width: f32,
) {
    let presets = internal_presets();
    state.selected_preset = state.selected_preset.min(presets.len().saturating_sub(1));

    ui.vertical(|ui| {
        ui.label(
            RichText::new("PRESET")
                .font(FontId::monospace(10.0))
                .color(theme.muted_dark),
        );
        egui::ComboBox::from_id_salt(id)
            .selected_text(presets[state.selected_preset].name)
            .width(width)
            .show_ui(ui, |ui| {
                for (index, preset) in presets.iter().enumerate() {
                    if ui
                        .selectable_label(index == state.selected_preset, preset.name)
                        .clicked()
                    {
                        state.selected_preset = index;
                        preset.apply_with_setter(setter, params);
                        ui.close_menu();
                    }
                }
            });
    });
}

fn brand_mark(ui: &mut egui::Ui, colors: ModuleColors, theme: Theme) {
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

fn brand_orb(ui: &mut egui::Ui, colors: ModuleColors) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
    let center = rect.center();
    ui.painter()
        .circle_filled(center, 16.0, Color32::from_rgb(244, 238, 220));
    ui.painter()
        .circle_filled(center + Vec2::new(-3.0, -4.0), 13.0, colors.character);
    ui.painter()
        .circle_filled(center + Vec2::new(4.0, 3.0), 13.0, colors.texture);
    ui.painter().circle_stroke(
        center,
        16.0,
        Stroke::new(1.5, Color32::from_rgb(245, 237, 218)),
    );
}

fn compact_nav_button(ui: &mut egui::Ui, label: &'static str, theme: Theme) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .font(FontId::monospace(10.0))
                .strong()
                .color(theme.text_dark),
        )
        .fill(Color32::from_rgb(244, 238, 220))
        .stroke(Stroke::new(1.0, Color32::from_rgb(188, 177, 158)))
        .corner_radius(CornerRadius::same(7))
        .min_size(Vec2::new(if label == "INIT" { 36.0 } else { 26.0 }, 18.0)),
    )
}

fn small_strip_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
) {
    ui.vertical_centered(|ui| {
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
                .font(FontId::monospace(7.5))
                .strong()
                .color(Color32::from_rgb(245, 237, 218)),
        );
        response.on_hover_text(format!("{}: {}", param.name(), value_string(param)));
        let _ = theme;
    });
}

fn mini_control_combo(ui: &mut egui::Ui, label: &'static str, value: &'static str, theme: Theme) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(8.0))
                .strong()
                .color(theme.muted_dark),
        );
        egui::ComboBox::from_id_salt(label)
            .selected_text(value)
            .width(88.0)
            .show_ui(ui, |ui| {
                ui.label(value);
            });
    });
}

fn previous_preset(setter: &ParamSetter<'_>, state: &mut UiState, params: &Cc22Params) {
    let presets = internal_presets();
    state.selected_preset = if state.selected_preset == 0 {
        presets.len().saturating_sub(1)
    } else {
        state.selected_preset - 1
    };
    presets[state.selected_preset].apply_with_setter(setter, params);
}

fn next_preset(setter: &ParamSetter<'_>, state: &mut UiState, params: &Cc22Params) {
    let presets = internal_presets();
    state.selected_preset = (state.selected_preset + 1) % presets.len().max(1);
    presets[state.selected_preset].apply_with_setter(setter, params);
}

fn randomize_controls(setter: &ParamSetter<'_>, state: &mut UiState, params: &Cc22Params) {
    let mut next = || {
        state.random_seed ^= state.random_seed << 13;
        state.random_seed ^= state.random_seed >> 17;
        state.random_seed ^= state.random_seed << 5;
        state.random_seed as f32 / u32::MAX as f32
    };

    set_float_normalized(setter, &params.character.drive, 0.12 + next() * 0.55);
    set_float_normalized(setter, &params.character.tone, 0.25 + next() * 0.65);
    set_float_normalized(setter, &params.character.mix, 0.25 + next() * 0.65);
    set_float_normalized(setter, &params.movement.rate, 0.15 + next() * 0.55);
    set_float_normalized(setter, &params.movement.depth, 0.10 + next() * 0.70);
    set_float_normalized(setter, &params.movement.mix, 0.10 + next() * 0.55);
    set_float_normalized(setter, &params.diffusion.time, 0.12 + next() * 0.55);
    set_float_normalized(setter, &params.diffusion.feedback, next() * 0.48);
    set_float_normalized(setter, &params.diffusion.mix, 0.08 + next() * 0.42);
    set_float_normalized(setter, &params.texture.wow_depth, next() * 0.42);
    set_float_normalized(setter, &params.texture.random_drift, next() * 0.35);
    set_float_normalized(setter, &params.texture.mix, next() * 0.45);
    set_float_normalized(setter, &params.dry_wet, 0.72 + next() * 0.28);
}

fn colored_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
    size: f32,
) -> egui::Response {
    ui.allocate_ui(Vec2::new(size + 20.0, size + 34.0), |ui| {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(label)
                    .font(FontId::monospace(9.5))
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
                    .font(FontId::monospace(9.0))
                    .color(value_color),
            );
            response.on_hover_text(format!("{}: {}", param.name(), value_string(param)))
        })
        .inner
    })
    .inner
}

#[allow(dead_code)]
fn macro_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    active: bool,
    theme: Theme,
) -> egui::Response {
    ui.allocate_ui(Vec2::new(144.0, 120.0), |ui| {
        ui.vertical_centered(|ui| {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::splat(50.0), Sense::click_and_drag());
            handle_float_drag(ui, setter, param, &response);
            paint_colored_knob(
                ui,
                rect,
                param.unmodulated_normalized_value(),
                accent,
                theme,
            );

            let led_rect = Rect::from_center_size(
                Pos2::new(rect.center().x, rect.bottom() + 10.0),
                Vec2::splat(8.0),
            );
            ui.painter().circle_filled(
                led_rect.center(),
                3.8,
                if active { accent } else { theme.muted_dark },
            );

            ui.add_space(8.0);
            ui.label(
                RichText::new(label)
                    .font(FontId::monospace(11.0))
                    .strong()
                    .color(theme.text_dark),
            );
            ui.label(
                RichText::new(value_string(param))
                    .font(FontId::monospace(9.0))
                    .color(if response.dragged() {
                        accent
                    } else {
                        theme.muted_dark
                    }),
            );
            response.on_hover_text(format!("Macro mapped to {}", param.name()))
        })
        .inner
    })
    .inner
}

fn mini_slider(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &FloatParam,
    label: &'static str,
    accent: Color32,
    theme: Theme,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(label)
                    .font(FontId::monospace(9.0))
                    .color(theme.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(value_string(param))
                        .font(FontId::monospace(8.0))
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
        response.on_hover_text(format!("{}: {}", param.name(), value_string(param)));
    });
}

fn handle_float_drag(
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

fn handle_float_drag_horizontal(
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

fn paint_colored_knob(
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
        painter.circle_filled(center, radius + 7.0, Color32::from_rgb(207, 199, 184));
        painter.circle_filled(center, radius + 3.0, Color32::from_rgb(238, 234, 224));
        painter.circle_stroke(
            center,
            radius + 6.0,
            Stroke::new(1.0, Color32::from_rgb(178, 169, 153)),
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
            .line_segment([inner, outer], Stroke::new(0.9, theme.muted_dark));
    }

    let indicator = Pos2::new(
        center.x + current.cos() * radius * 0.66,
        center.y + current.sin() * radius * 0.66,
    );
    let painter = ui.painter();
    painter.line_segment([center, indicator], Stroke::new(3.0, accent));
    painter.circle_filled(center, radius * 0.10, theme.text_dark);
}

fn paint_mini_slider(
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

fn paint_arc(
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

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum IconKind {
    Curve,
    Lfo,
    Reflections,
    Noise,
    Eq,
}

#[allow(dead_code)]
fn icon_display(ui: &mut egui::Ui, kind: IconKind, accent: Color32, theme: Theme, now: f64) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(8),
        Color32::from_rgb(246, 241, 229),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, theme.card_edge),
        StrokeKind::Outside,
    );

    match kind {
        IconKind::Curve => draw_curve_icon(ui, rect, accent),
        IconKind::Lfo => draw_lfo_icon(ui, rect, accent, now),
        IconKind::Reflections => draw_reflection_icon(ui, rect, accent),
        IconKind::Noise => draw_noise_icon(ui, rect, accent),
        IconKind::Eq => draw_eq_icon(ui, rect, accent),
    }
}

#[allow(dead_code)]
fn draw_curve_icon(ui: &mut egui::Ui, rect: Rect, accent: Color32) {
    let mut points = Vec::with_capacity(24);
    for index in 0..24 {
        let x = index as f32 / 23.0;
        let y = 0.5 - ((x * 5.0 - 2.5).tanh() * 0.36);
        points.push(Pos2::new(
            rect.left() + 10.0 + x * (rect.width() - 20.0),
            rect.top() + y * rect.height(),
        ));
    }
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(2.0, accent)));
}

#[allow(dead_code)]
fn draw_lfo_icon(ui: &mut egui::Ui, rect: Rect, accent: Color32, now: f64) {
    let phase = (now as f32 * 0.35).fract();
    let mut points = Vec::with_capacity(32);
    for index in 0..32 {
        let x = index as f32 / 31.0;
        let y = 0.5 + (((x + phase) * core::f32::consts::TAU * 2.0).sin() * 0.25);
        points.push(Pos2::new(
            rect.left() + 8.0 + x * (rect.width() - 16.0),
            rect.top() + y * rect.height(),
        ));
    }
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(2.0, accent)));
}

#[allow(dead_code)]
fn draw_reflection_icon(ui: &mut egui::Ui, rect: Rect, accent: Color32) {
    for index in 0..5 {
        let x = rect.left() + 12.0 + index as f32 * 23.0;
        let alpha = 210_u8.saturating_sub(index as u8 * 32);
        ui.painter().line_segment(
            [
                Pos2::new(x, rect.top() + 12.0),
                Pos2::new(x + 16.0, rect.bottom() - 12.0),
            ],
            Stroke::new(
                2.0,
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha),
            ),
        );
    }
}

#[allow(dead_code)]
fn draw_noise_icon(ui: &mut egui::Ui, rect: Rect, accent: Color32) {
    for index in 0..22 {
        let x = rect.left() + 8.0 + index as f32 * ((rect.width() - 16.0) / 21.0);
        let y = rect.center().y + (((index * 17 % 11) as f32 - 5.0) * 2.2);
        ui.painter().circle_filled(Pos2::new(x, y), 1.7, accent);
    }
}

#[allow(dead_code)]
fn draw_eq_icon(ui: &mut egui::Ui, rect: Rect, accent: Color32) {
    let points = [
        Pos2::new(rect.left() + 8.0, rect.bottom() - 14.0),
        Pos2::new(rect.left() + 35.0, rect.bottom() - 20.0),
        Pos2::new(rect.left() + 63.0, rect.top() + 15.0),
        Pos2::new(rect.left() + 92.0, rect.top() + 19.0),
        Pos2::new(rect.right() - 8.0, rect.bottom() - 15.0),
    ];
    ui.painter()
        .add(egui::Shape::line(points.to_vec(), Stroke::new(2.0, accent)));
}

fn mode_selector<R>(
    ui: &mut egui::Ui,
    id: &'static str,
    selected_text: &'static str,
    accent: Color32,
    theme: Theme,
    add_options: impl FnOnce(&mut egui::Ui) -> R,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("MODE")
                .font(FontId::monospace(9.0))
                .color(theme.muted),
        );
        egui::ComboBox::from_id_salt(id)
            .selected_text(RichText::new(selected_text).color(accent))
            .width(92.0)
            .show_ui(ui, add_options);
    });
}

fn character_mode_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<CharacterMode>,
    accent: Color32,
    theme: Theme,
) {
    let current = param.value();
    mode_selector(
        ui,
        "character-mode",
        character_mode_label(current),
        accent,
        theme,
        |ui| {
            enum_option(ui, setter, param, current, CharacterMode::Clean, "Clean");
            enum_option(
                ui,
                setter,
                param,
                current,
                CharacterMode::Saturation,
                "Saturation",
            );
            enum_option(
                ui,
                setter,
                param,
                current,
                CharacterMode::Cassette,
                "Cassette",
            );
        },
    );
}

fn movement_mode_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<MovementMode>,
    accent: Color32,
    theme: Theme,
) {
    let current = param.value();
    mode_selector(
        ui,
        "movement-mode",
        movement_mode_label(current),
        accent,
        theme,
        |ui| {
            enum_option(ui, setter, param, current, MovementMode::Off, "Off");
            enum_option(ui, setter, param, current, MovementMode::Chorus, "Chorus");
            enum_option(ui, setter, param, current, MovementMode::Vibrato, "Vibrato");
            enum_option(ui, setter, param, current, MovementMode::Tremolo, "Tremolo");
        },
    );
}

fn diffusion_mode_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<DiffusionMode>,
    accent: Color32,
    theme: Theme,
) {
    let current = param.value();
    mode_selector(
        ui,
        "diffusion-mode",
        diffusion_mode_label(current),
        accent,
        theme,
        |ui| {
            enum_option(ui, setter, param, current, DiffusionMode::Off, "Off");
            enum_option(ui, setter, param, current, DiffusionMode::Delay, "Delay");
            enum_option(ui, setter, param, current, DiffusionMode::Slap, "Slap");
            enum_option(ui, setter, param, current, DiffusionMode::Reverb, "Reverb");
        },
    );
}

fn texture_mode_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<TextureMode>,
    accent: Color32,
    theme: Theme,
) {
    let current = param.value();
    mode_selector(
        ui,
        "texture-mode",
        texture_mode_label(current),
        accent,
        theme,
        |ui| {
            enum_option(ui, setter, param, current, TextureMode::Off, "Off");
            enum_option(
                ui,
                setter,
                param,
                current,
                TextureMode::WowFlutter,
                "WowFlutter",
            );
            enum_option(ui, setter, param, current, TextureMode::Noise, "Noise");
        },
    );
}

fn eq_mode_selector(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<EqMode>,
    accent: Color32,
    theme: Theme,
) {
    let current = param.value();
    mode_selector(ui, "eq-mode", eq_mode_label(current), accent, theme, |ui| {
        enum_option(ui, setter, param, current, EqMode::Off, "Off");
        enum_option(ui, setter, param, current, EqMode::On, "On");
    });
}

fn enum_option<T>(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &EnumParam<T>,
    current: T,
    value: T,
    label: &'static str,
) where
    T: Enum + Copy + PartialEq,
{
    if ui.selectable_label(current == value, label).clicked() {
        set_param(setter, param, value);
        ui.close_menu();
    }
}

fn toggle_button(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &BoolParam,
    active: bool,
    accent: Color32,
    theme: Theme,
) -> egui::Response {
    let label = if param.value() { "OFF" } else { "ON" };
    let fill = if active { accent } else { theme.card_edge };
    let response = ui.add(
        egui::Button::new(
            RichText::new(label)
                .font(FontId::monospace(9.0))
                .strong()
                .color(if active { theme.text_dark } else { theme.muted }),
        )
        .fill(fill)
        .stroke(Stroke::new(1.0, fill))
        .corner_radius(CornerRadius::same(8))
        .min_size(Vec2::new(34.0, 20.0)),
    );

    if response.clicked() {
        set_param(setter, param, !param.value());
    }

    response.on_hover_text("Enable/bypass module")
}

fn global_bypass_button(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    param: &BoolParam,
    theme: Theme,
) {
    let bypassed = param.value();
    let response = ui.add(
        egui::Button::new(
            RichText::new(if bypassed { "BYPASSED" } else { "GLOBAL ON" })
                .font(FontId::monospace(10.0))
                .strong(),
        )
        .fill(if bypassed {
            theme.warning
        } else {
            theme.paper_alt
        })
        .stroke(Stroke::new(1.0, theme.text_dark))
        .corner_radius(CornerRadius::same(10))
        .min_size(Vec2::new(94.0, 30.0)),
    );
    if response.clicked() {
        set_param(setter, param, !bypassed);
    }
}

fn compact_button(
    ui: &mut egui::Ui,
    label: &'static str,
    theme: Theme,
    accent: Color32,
) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .font(FontId::monospace(10.0))
                .strong()
                .color(theme.text_dark),
        )
        .fill(theme.paper_alt)
        .stroke(Stroke::new(1.0, accent))
        .corner_radius(CornerRadius::same(9))
        .min_size(Vec2::new(48.0, 30.0)),
    )
}

fn rounded_panel<R>(
    ui: &mut egui::Ui,
    fill: Color32,
    stroke: Color32,
    radius: CornerRadius,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let rect = ui.available_rect_before_wrap();
    let shadow_rect = Rect::from_min_size(
        rect.min + Vec2::new(4.0, 5.0),
        Vec2::new(rect.width().min(1_100.0), 54.0),
    );
    ui.painter()
        .rect_filled(shadow_rect, radius, Theme::default().shadow);
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(radius)
        .inner_margin(egui::Margin::same(12))
        .show(ui, add_contents)
        .inner
}

fn level_meter(
    ui: &mut egui::Ui,
    label: &'static str,
    reading: MeterSnapshot,
    accent: Color32,
    theme: Theme,
) {
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(9.0))
                .color(theme.muted),
        );
        let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 62.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_rgb(10, 11, 13));
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, theme.card_edge),
            StrokeKind::Outside,
        );

        let fill_height = rect.height() * reading.level;
        let fill_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 3.0, rect.bottom() - fill_height + 3.0),
            Pos2::new(rect.right() - 3.0, rect.bottom() - 3.0),
        );
        ui.painter().rect_filled(
            fill_rect,
            CornerRadius::same(3),
            if reading.clipped {
                theme.warning
            } else {
                accent
            },
        );
    });
}

fn clip_indicator(ui: &mut egui::Ui, label: &'static str, clipped: bool, theme: Theme) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter().circle_filled(
            rect.center(),
            4.5,
            if clipped {
                theme.warning
            } else {
                theme.card_edge
            },
        );
        ui.label(
            RichText::new(label)
                .font(FontId::monospace(9.0))
                .color(if clipped { theme.warning } else { theme.muted }),
        );
    });
}

#[derive(Debug, Clone, Copy)]
struct MeterReading {
    input: MeterSnapshot,
    output: MeterSnapshot,
}

#[derive(Debug, Clone, Copy)]
struct MeterSnapshot {
    level: f32,
    clipped: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct MeterBallistics {
    level: f32,
}

impl UiState {
    fn next_meter_reading(&mut self, meters: &Meters, now: f64) -> MeterReading {
        let dt = self
            .last_meter_time
            .map(|last| (now - last).clamp(1.0 / 240.0, 0.1) as f32)
            .unwrap_or(1.0 / 60.0);
        self.last_meter_time = Some(now);

        let input_peak = meters.take_input_peak();
        let output_peak = meters.take_output_peak();
        let input_clip_events = meters.input_clip_events();
        let output_clip_events = meters.output_clip_events();

        if input_clip_events != self.input_clip_events {
            self.input_clip_events = input_clip_events;
            self.input_clip_until = now + 0.75;
        }

        if output_clip_events != self.output_clip_events {
            self.output_clip_events = output_clip_events;
            self.output_clip_until = now + 0.75;
        }

        MeterReading {
            input: MeterSnapshot {
                level: self.input_meter.next(peak_to_meter_level(input_peak), dt),
                clipped: now < self.input_clip_until,
            },
            output: MeterSnapshot {
                level: self.output_meter.next(peak_to_meter_level(output_peak), dt),
                clipped: now < self.output_clip_until,
            },
        }
    }
}

impl MeterBallistics {
    fn next(&mut self, target: f32, dt: f32) -> f32 {
        let time_constant = if target > self.level { 0.012 } else { 0.320 };
        let coefficient = 1.0 - (-dt / time_constant).exp();
        self.level += (target - self.level) * coefficient.clamp(0.0, 1.0);
        self.level = self.level.clamp(0.0, 1.0);
        self.level
    }
}

fn peak_to_meter_level(peak: f32) -> f32 {
    ((linear_to_db(peak) + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn linear_to_db(peak: f32) -> f32 {
    20.0 * peak.max(0.000_001).log10()
}

fn set_float_normalized(setter: &ParamSetter<'_>, param: &FloatParam, normalized: f32) {
    setter.begin_set_parameter(param);
    setter.set_parameter_normalized(param, normalized.clamp(0.0, 1.0));
    setter.end_set_parameter(param);
}

fn set_param<P: Param>(setter: &ParamSetter<'_>, param: &P, value: P::Plain) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

fn value_string(param: &FloatParam) -> String {
    param.normalized_value_to_string(param.unmodulated_normalized_value(), true)
}

fn character_active(params: &Cc22Params) -> bool {
    !params.character.bypass.value() && params.character.mode.value() != CharacterMode::Clean
}

fn movement_active(params: &Cc22Params) -> bool {
    !params.movement.bypass.value() && params.movement.mode.value() != MovementMode::Off
}

fn diffusion_active(params: &Cc22Params) -> bool {
    !params.diffusion.bypass.value() && params.diffusion.mode.value() != DiffusionMode::Off
}

fn texture_active(params: &Cc22Params) -> bool {
    !params.texture.bypass.value() && params.texture.mode.value() != TextureMode::Off
}

fn eq_active(params: &Cc22Params) -> bool {
    !params.eq.bypass.value() && params.eq.mode.value() == EqMode::On
}

fn character_mode_label(mode: CharacterMode) -> &'static str {
    match mode {
        CharacterMode::Clean => "Clean",
        CharacterMode::Saturation => "Saturation",
        CharacterMode::Cassette => "Cassette",
    }
}

fn movement_mode_label(mode: MovementMode) -> &'static str {
    match mode {
        MovementMode::Off => "Off",
        MovementMode::Chorus => "Chorus",
        MovementMode::Vibrato => "Vibrato",
        MovementMode::Tremolo => "Tremolo",
    }
}

fn diffusion_mode_label(mode: DiffusionMode) -> &'static str {
    match mode {
        DiffusionMode::Off => "Off",
        DiffusionMode::Delay => "Delay",
        DiffusionMode::Slap => "Slap",
        DiffusionMode::Reverb => "Reverb",
    }
}

fn texture_mode_label(mode: TextureMode) -> &'static str {
    match mode {
        TextureMode::Off => "Off",
        TextureMode::WowFlutter => "WowFlutter",
        TextureMode::Noise => "Noise",
    }
}

fn eq_mode_label(mode: EqMode) -> &'static str {
    match mode {
        EqMode::Off => "Off",
        EqMode::On => "On",
    }
}
