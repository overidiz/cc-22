use nih_plug_egui::egui::Color32;

pub(crate) const CARD_HEIGHT: f32 = 280.0;
pub(crate) const CARD_WIDTH: f32 = 226.0;
pub(crate) const BASE_WIDTH: f32 = 1040.0;
pub(crate) const BASE_HEIGHT: f32 = 744.0;

pub(crate) const FONT_MODULE_TITLE: f32 = 16.0;
pub(crate) const FONT_CONTROL_LABEL: f32 = 9.5;
pub(crate) const FONT_VALUE_LABEL: f32 = 9.0;
pub(crate) const FONT_SECONDARY: f32 = 9.5;
pub(crate) const FONT_HINT: f32 = 9.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Theme {
    pub(crate) background: Color32,
    pub(crate) paper: Color32,
    pub(crate) paper_alt: Color32,
    pub(crate) card: Color32,
    pub(crate) card_dim: Color32,
    pub(crate) card_edge: Color32,
    pub(crate) text_dark: Color32,
    pub(crate) text_light: Color32,
    pub(crate) muted: Color32,
    pub(crate) muted_dark: Color32,
    pub(crate) warning: Color32,
    pub(crate) shadow: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color32::from_rgb(31, 27, 22),
            paper: Color32::from_rgb(244, 239, 228),
            paper_alt: Color32::from_rgb(235, 229, 216),
            card: Color32::from_rgb(236, 231, 219),
            card_dim: Color32::from_rgb(219, 213, 200),
            card_edge: Color32::from_rgb(153, 145, 130),
            text_dark: Color32::from_rgb(30, 27, 23),
            text_light: Color32::from_rgb(30, 27, 23),
            muted: Color32::from_rgb(126, 118, 104),
            muted_dark: Color32::from_rgb(70, 64, 55),
            warning: Color32::from_rgb(225, 64, 52),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 92),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleColors {
    pub(crate) character: Color32,
    pub(crate) movement: Color32,
    pub(crate) diffusion: Color32,
    pub(crate) texture: Color32,
    pub(crate) eq: Color32,
    pub(crate) master: Color32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Look {
    pub(crate) colors: ModuleColors,
    pub(crate) theme: Theme,
}

impl Default for ModuleColors {
    fn default() -> Self {
        Self {
            character: Color32::from_rgb(235, 74, 64),
            movement: Color32::from_rgb(235, 166, 32),
            diffusion: Color32::from_rgb(48, 198, 112),
            texture: Color32::from_rgb(42, 176, 214),
            eq: Color32::from_rgb(245, 132, 34),
            master: Color32::from_rgb(246, 239, 222),
        }
    }
}
