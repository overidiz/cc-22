use nih_plug_egui::egui;

use crate::meters::Meters;

use super::spectrum::SpectrumState;

impl Default for UiState {
    fn default() -> Self {
        Self {
            spectrum: SpectrumState::default(),
            selected_preset: 0,
            random_seed: 0,
            selected_eq_position: EqPositionSelection::Post,
            selected_eq_band: EqBandSelection::Band1,
            save_notice_until: 0.0,
            save_failed: false,
            drag_source: None,
            drag_drop_slot: None,
            input_meter: MeterBallistics::default(),
            output_meter: MeterBallistics::default(),
            input_clip_events: 0,
            output_clip_events: 0,
            input_clip_until: 0.0,
            output_clip_until: 0.0,
            last_meter_time: None,
            logo_texture: None,
        }
    }
}

pub(crate) struct UiState {
    pub(crate) spectrum: SpectrumState,
    pub(crate) selected_preset: usize,
    pub(crate) random_seed: u32,
    /// The EQ workbench only edits one bank at a time — Pre or Post. Both run in
    /// the DSP simultaneously; this just chooses which one the UI shows.
    pub(crate) selected_eq_position: EqPositionSelection,
    pub(crate) selected_eq_band: EqBandSelection,
    pub(crate) save_notice_until: f64,
    pub(crate) save_failed: bool,
    pub(crate) drag_source: Option<usize>,
    pub(crate) drag_drop_slot: Option<usize>,
    pub(crate) input_meter: MeterBallistics,
    pub(crate) output_meter: MeterBallistics,
    pub(crate) input_clip_events: u32,
    pub(crate) output_clip_events: u32,
    pub(crate) input_clip_until: f64,
    pub(crate) output_clip_until: f64,
    pub(crate) last_meter_time: Option<f64>,
    /// Lazily-uploaded CC-22 logo texture (decoded RGBA embedded at build time).
    pub(crate) logo_texture: Option<egui::TextureHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EqPositionSelection {
    Pre,
    Post,
}

impl EqPositionSelection {
    pub(crate) const ALL: [Self; 2] = [Self::Pre, Self::Post];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Pre => "PRE",
            Self::Post => "POST",
        }
    }

    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Pre => Self::Post,
            Self::Post => Self::Pre,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EqBandSelection {
    Band1,
    Band2,
    Band3,
    Band4,
    Band5,
}

impl EqBandSelection {
    pub(crate) const ALL: [Self; 5] = [
        Self::Band1,
        Self::Band2,
        Self::Band3,
        Self::Band4,
        Self::Band5,
    ];

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Band1 => 0,
            Self::Band2 => 1,
            Self::Band3 => 2,
            Self::Band4 => 3,
            Self::Band5 => 4,
        }
    }

    pub(crate) fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Band1,
            1 => Self::Band2,
            2 => Self::Band3,
            3 => Self::Band4,
            _ => Self::Band5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MeterReading {
    pub(crate) input: MeterSnapshot,
    pub(crate) output: MeterSnapshot,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MeterSnapshot {
    level: f32,
    pub(crate) clipped: bool,
}

impl MeterSnapshot {
    pub(crate) fn level(self) -> f32 {
        self.level
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MeterBallistics {
    level: f32,
}

impl UiState {
    pub(crate) fn with_random_seed(random_seed: u32) -> Self {
        Self {
            random_seed,
            ..Self::default()
        }
    }

    pub(crate) fn next_meter_reading(&mut self, meters: &Meters, now: f64) -> MeterReading {
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
