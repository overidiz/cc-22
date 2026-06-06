use nih_plug_egui::egui::Color32;

pub(crate) const CARD_HEIGHT: f32 = 342.0;
pub(crate) const CARD_WIDTH: f32 = 226.0;
pub(crate) const KNOB_SIZE: f32 = 58.0;

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
            character: Color32::from_rgb(245, 84, 72),
            movement: Color32::from_rgb(245, 180, 45),
            diffusion: Color32::from_rgb(76, 210, 126),
            texture: Color32::from_rgb(63, 190, 224),
            eq: Color32::from_rgb(250, 158, 48),
            master: Color32::from_rgb(238, 229, 207),
        }
    }
}
