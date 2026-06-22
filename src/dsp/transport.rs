//! DAW transport / tempo-sync foundation.
//!
//! [`TransportFrame`] is a per-block snapshot of the host transport (tempo,
//! play state, musical position). [`NoteDivision`] plus the `*_for_division`
//! helpers convert a musical division into delay times / rates against the
//! current BPM. Nothing here touches audio — modules opt in to sync later.

use nih_plug::prelude::Enum;

pub const DEFAULT_BPM: f32 = 120.0;
const MIN_BPM: f32 = 20.0;
const MAX_BPM: f32 = 300.0;

/// A snapshot of the host transport for one processing block. Always valid:
/// `bpm` is sanitized, positions are `None` when the host doesn't provide them.
#[derive(Debug, Clone, Copy)]
pub struct TransportFrame {
    pub bpm: f32,
    pub playing: bool,
    pub ppq_position: Option<f64>,
    pub bar_position: Option<f64>,
    pub time_sig_numerator: Option<u32>,
    pub time_sig_denominator: Option<u32>,
    pub sample_rate: f32,
}

impl Default for TransportFrame {
    fn default() -> Self {
        Self {
            bpm: DEFAULT_BPM,
            playing: false,
            ppq_position: None,
            bar_position: None,
            time_sig_numerator: None,
            time_sig_denominator: None,
            sample_rate: 44_100.0,
        }
    }
}

impl TransportFrame {
    /// Sanitize a host-provided BPM: non-finite or non-positive falls back to
    /// 120, otherwise it is clamped to a musically sane `[20, 300]` range.
    pub fn sanitize_bpm(bpm: f32) -> f32 {
        if bpm.is_finite() && bpm > 0.0 {
            bpm.clamp(MIN_BPM, MAX_BPM)
        } else {
            DEFAULT_BPM
        }
    }

    /// Beats per bar from the time signature, defaulting to 4 (4/4) when the host
    /// doesn't report one. Expressed in quarter-note beats.
    pub fn beats_per_bar(&self) -> f32 {
        match (self.time_sig_numerator, self.time_sig_denominator) {
            (Some(num), Some(den)) if num > 0 && den > 0 => num as f32 * 4.0 / den as f32,
            _ => 4.0,
        }
    }
}

/// Musical note divisions usable as a sync rate. Stored by stable `#[id]` so it
/// can be exposed as an `EnumParam` without breaking saved state.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteDivision {
    #[id = "bar1"]
    #[name = "1 Bar"]
    Bar1,
    #[id = "half"]
    #[name = "1/2"]
    Half,
    #[id = "quarter"]
    #[name = "1/4"]
    Quarter,
    #[id = "eighth"]
    #[name = "1/8"]
    Eighth,
    #[id = "sixteenth"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "thirtysecond"]
    #[name = "1/32"]
    ThirtySecond,
    #[id = "sixtyfourth"]
    #[name = "1/64"]
    SixtyFourth,
    #[id = "dotted_quarter"]
    #[name = "1/4 D"]
    DottedQuarter,
    #[id = "dotted_eighth"]
    #[name = "1/8 D"]
    DottedEighth,
    #[id = "dotted_sixteenth"]
    #[name = "1/16 D"]
    DottedSixteenth,
    #[id = "quarter_triplet"]
    #[name = "1/4 T"]
    QuarterTriplet,
    #[id = "eighth_triplet"]
    #[name = "1/8 T"]
    EighthTriplet,
    #[id = "sixteenth_triplet"]
    #[name = "1/16 T"]
    SixteenthTriplet,
}

impl NoteDivision {
    /// Length of the division in quarter-note beats (1 Bar assumes 4/4).
    pub fn beats(self) -> f32 {
        match self {
            Self::Bar1 => 4.0,
            Self::Half => 2.0,
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::ThirtySecond => 0.125,
            Self::SixtyFourth => 0.0625,
            Self::DottedQuarter => 1.5,
            Self::DottedEighth => 0.75,
            Self::DottedSixteenth => 0.375,
            Self::QuarterTriplet => 2.0 / 3.0,
            Self::EighthTriplet => 1.0 / 3.0,
            Self::SixteenthTriplet => 1.0 / 6.0,
        }
    }
}

/// Quarter-note beats spanned by a division.
pub fn beats_for_division(division: NoteDivision) -> f32 {
    division.beats()
}

/// Seconds the division lasts at `bpm` (BPM is sanitized first).
pub fn seconds_for_division(bpm: f32, division: NoteDivision) -> f32 {
    let bpm = TransportFrame::sanitize_bpm(bpm);
    division.beats() * 60.0 / bpm
}

/// Milliseconds the division lasts at `bpm`.
pub fn ms_for_division(bpm: f32, division: NoteDivision) -> f32 {
    seconds_for_division(bpm, division) * 1000.0
}

/// Rate in Hz of one division cycle at `bpm` (e.g. tremolo/LFO rate).
pub fn hz_for_division(bpm: f32, division: NoteDivision) -> f32 {
    let seconds = seconds_for_division(bpm, division);
    if seconds > 0.0 {
        1.0 / seconds
    } else {
        0.0
    }
}

/// Length of the division in samples at `bpm` and `sample_rate`.
pub fn samples_for_division(bpm: f32, division: NoteDivision, sample_rate: f32) -> usize {
    let samples = seconds_for_division(bpm, division) * sample_rate.max(1.0);
    if samples.is_finite() && samples > 0.0 {
        samples.round() as usize
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 0.01, "expected {b}, got {a}");
    }

    #[test]
    fn conversions_at_120_bpm() {
        approx(ms_for_division(120.0, NoteDivision::Quarter), 500.0);
        approx(ms_for_division(120.0, NoteDivision::Eighth), 250.0);
        approx(ms_for_division(120.0, NoteDivision::Sixteenth), 125.0);
        approx(ms_for_division(120.0, NoteDivision::Half), 1000.0);
        approx(ms_for_division(120.0, NoteDivision::Bar1), 2000.0);
        approx(ms_for_division(120.0, NoteDivision::DottedEighth), 375.0);
        approx(
            ms_for_division(120.0, NoteDivision::QuarterTriplet),
            333.333,
        );
        approx(ms_for_division(120.0, NoteDivision::EighthTriplet), 166.667);
    }

    #[test]
    fn conversions_at_60_and_90_bpm() {
        approx(ms_for_division(60.0, NoteDivision::Quarter), 1000.0);
        approx(ms_for_division(60.0, NoteDivision::Eighth), 500.0);
        approx(ms_for_division(90.0, NoteDivision::Quarter), 666.667);
    }

    #[test]
    fn hz_conversion() {
        // 120 BPM, 1/8 = 250 ms = 4 Hz.
        approx(hz_for_division(120.0, NoteDivision::Eighth), 4.0);
        // 120 BPM, 1/4 = 500 ms = 2 Hz.
        approx(hz_for_division(120.0, NoteDivision::Quarter), 2.0);
    }

    #[test]
    fn samples_conversion() {
        // 120 BPM quarter = 500 ms = 24000 samples at 48 kHz.
        assert_eq!(
            samples_for_division(120.0, NoteDivision::Quarter, 48_000.0),
            24_000
        );
    }

    #[test]
    fn invalid_bpm_falls_back_or_clamps() {
        assert_eq!(TransportFrame::sanitize_bpm(f32::NAN), DEFAULT_BPM);
        assert_eq!(TransportFrame::sanitize_bpm(f32::INFINITY), DEFAULT_BPM);
        assert_eq!(TransportFrame::sanitize_bpm(0.0), DEFAULT_BPM);
        assert_eq!(TransportFrame::sanitize_bpm(-50.0), DEFAULT_BPM);
        assert_eq!(TransportFrame::sanitize_bpm(5.0), MIN_BPM);
        assert_eq!(TransportFrame::sanitize_bpm(9000.0), MAX_BPM);
        assert_eq!(TransportFrame::sanitize_bpm(128.0), 128.0);
        // A division using an invalid BPM still produces a finite result.
        assert!(ms_for_division(f32::NAN, NoteDivision::Quarter).is_finite());
    }

    #[test]
    fn default_transport_is_safe() {
        let t = TransportFrame::default();
        assert_eq!(t.bpm, DEFAULT_BPM);
        assert!(!t.playing);
        assert!(t.ppq_position.is_none());
        approx(t.beats_per_bar(), 4.0);
    }

    #[test]
    fn beats_per_bar_uses_time_signature() {
        let mut t = TransportFrame::default();
        t.time_sig_numerator = Some(3);
        t.time_sig_denominator = Some(4);
        approx(t.beats_per_bar(), 3.0);
        t.time_sig_numerator = Some(6);
        t.time_sig_denominator = Some(8);
        approx(t.beats_per_bar(), 3.0);
    }
}
