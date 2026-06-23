use nih_plug_egui::egui::{self, Pos2, Vec2};

use crate::meters::Meters;

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_preset: 0,
            random_seed: 0,
            save_notice_until: 0.0,
            save_failed: false,
            drag_source: None,
            drag_drop_slot: None,
            drag_grab_offset: Vec2::ZERO,
            drag_visual_pos: None,
            drag_lift: 0.0,
            drag_indicator_x: None,
            card_anim_x: None,
            input_meter: MeterBallistics::default(),
            output_meter: MeterBallistics::default(),
            input_clip_events: 0,
            output_clip_events: 0,
            input_clip_until: 0.0,
            output_clip_until: 0.0,
            last_meter_time: None,
            logo_texture: None,
            grain_texture: None,
        }
    }
}

pub(crate) struct UiState {
    pub(crate) selected_preset: usize,
    pub(crate) random_seed: u32,
    pub(crate) save_notice_until: f64,
    pub(crate) save_failed: bool,
    pub(crate) drag_source: Option<usize>,
    pub(crate) drag_drop_slot: Option<usize>,
    /// Offset between the dragged card's centre and the pointer at grab time, so
    /// the floating proxy tracks the cursor at its original relative position
    /// instead of jumping its centre under the pointer.
    pub(crate) drag_grab_offset: Vec2,
    /// Smoothed centre of the floating card (spring-followed toward its target)
    /// for a soft, controlled drag rather than a rigid snap.
    pub(crate) drag_visual_pos: Option<Pos2>,
    /// Animated "pick-up" lift, eased in on grab so the card rises gently.
    pub(crate) drag_lift: f32,
    /// Smoothed x of the insertion gap so it glides between slots.
    pub(crate) drag_indicator_x: Option<f32>,
    /// Animated left-x of each of the 4 module slots. While dragging, the
    /// non-dragged cards ease toward shifted slots to open a gap; on drop they
    /// ease into their final positions (soft reflow + settle, no snap).
    pub(crate) card_anim_x: Option<[f32; 4]>,
    pub(crate) input_meter: MeterBallistics,
    pub(crate) output_meter: MeterBallistics,
    pub(crate) input_clip_events: u32,
    pub(crate) output_clip_events: u32,
    pub(crate) input_clip_until: f64,
    pub(crate) output_clip_until: f64,
    pub(crate) last_meter_time: Option<f64>,
    /// Lazily-uploaded CC-22 logo texture (decoded RGBA embedded at build time).
    pub(crate) logo_texture: Option<egui::TextureHandle>,
    /// Lazily-generated tileable monochrome grain used for the subtle vintage
    /// matte/painted-metal finish across surfaces.
    pub(crate) grain_texture: Option<egui::TextureHandle>,
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
