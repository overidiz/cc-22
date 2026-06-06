use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, UiBuilder,
    Vec2,
};

use crate::{meters::Meters, params::Cc22Params};

use super::{
    eq_view::eq_workbench,
    master_strip::master_strip,
    meters::UiState,
    theme::{Look, Theme, CARD_HEIGHT, CARD_WIDTH, KNOB_SIZE},
    widgets::{
        character_active, character_mode_label, character_mode_selector, colored_knob,
        diffusion_active, diffusion_mode_label, diffusion_mode_selector, draw_curve_icon,
        draw_lfo_icon, draw_noise_icon, draw_reflection_icon, movement_active, movement_mode_label,
        movement_mode_selector, set_param, texture_active, texture_mode_label,
        texture_mode_selector, toggle_button,
    },
};

struct ModuleCardSpec<'a> {
    title: &'static str,
    accent: Color32,
    active: bool,
    bypass: &'a BoolParam,
}

pub(crate) fn center_modules(
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
            now,
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
            now,
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
            now,
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
                    "FEEDBACK",
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
            now,
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
                    "FLUTTER",
                    colors.texture,
                    theme,
                    KNOB_SIZE,
                );
                module_mode_summary(
                    ui,
                    texture_mode_label(params.texture.mode.value()),
                    &["Off", "WowFlutter", "Noise", "Tape"],
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

fn module_card<R>(
    ui: &mut egui::Ui,
    setter: &ParamSetter<'_>,
    theme: Theme,
    now: f64,
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

                    let icon_rect = Rect::from_min_size(
                        Pos2::new(rect.right() - 58.0, rect.top() + 16.0),
                        Vec2::new(46.0, 38.0),
                    );
                    let icon_accent = if spec.active {
                        spec.accent
                    } else {
                        theme.muted
                    };
                    match spec.title {
                        "CHARACTER" => draw_curve_icon(ui, icon_rect, icon_accent),
                        "MOVEMENT" => draw_lfo_icon(ui, icon_rect, icon_accent, now),
                        "DIFFUSION" => draw_reflection_icon(ui, icon_rect, icon_accent),
                        "TEXTURE" => draw_noise_icon(ui, icon_rect, icon_accent),
                        _ => {}
                    }
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
