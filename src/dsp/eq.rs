use nih_plug::prelude::*;

use crate::params::EqParamRefs;

use super::{
    chain::{sanitize_sample, ModuleCore},
    smoothing::LinearSmoother,
};

const MAX_CHANNELS: usize = 2;
const NUM_BANDS: usize = 5;
const COEFFICIENT_FREQUENCY_EPSILON: f32 = 0.01;
const COEFFICIENT_GAIN_EPSILON: f32 = 0.000_5;
const COEFFICIENT_Q_EPSILON: f32 = 0.000_1;
const COEFFICIENT_SAMPLE_RATE_EPSILON: f32 = 0.01;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqMode {
    #[id = "off"]
    Off,

    #[id = "on"]
    On,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqBandType {
    #[id = "off"]
    Off,
    #[id = "bell"]
    Bell,
    #[id = "low_shelf"]
    LowShelf,
    #[id = "high_shelf"]
    HighShelf,
    #[id = "high_pass"]
    HighPass,
    #[id = "low_pass"]
    LowPass,
}

#[derive(Debug, Clone)]
pub struct Eq {
    core: ModuleCore,
    sample_rate: f32,
    filters: [[Biquad; NUM_BANDS]; MAX_CHANNELS],
    current_mode: EqMode,
    mode_crossfade: LinearSmoother,
    coefficient_state: Option<EqCoefficientState>,
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct EqFrame {
    pub mode: EqMode,
    pub active_mix: f32,
    pub mode_fade: f32,
    pub bands: [EqBandFrame; NUM_BANDS],
}

#[derive(Debug, Clone, Copy)]
pub struct EqBandFrame {
    pub enabled: bool,
    pub band_type: EqBandType,
    pub frequency: f32,
    pub gain: f32,
    pub q: f32,
}

#[derive(Debug, Clone, Copy)]
struct Biquad {
    coefficients: BiquadCoefficients,
    z1: f32,
    z2: f32,
}

#[derive(Debug, Clone, Copy)]
struct BiquadCoefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Debug, Clone, Copy)]
struct EqCoefficientState {
    sample_rate: f32,
    bands: [EqBandFrame; NUM_BANDS],
}

impl Default for Eq {
    fn default() -> Self {
        let mut eq = Self {
            core: ModuleCore::default(),
            sample_rate: 44_100.0,
            filters: [[Biquad::default(); NUM_BANDS]; MAX_CHANNELS],
            current_mode: EqMode::On,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            coefficient_state: None,
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
        };
        eq.prepare(44_100.0);
        eq
    }
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            coefficients: BiquadCoefficients::identity(),
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl BiquadCoefficients {
    fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

impl Eq {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.core.prepare(self.sample_rate);
        self.mode_crossfade.prepare(self.sample_rate);
        self.coefficient_state = None;
        self.update_coefficients(&EqFrame::neutral(self.sample_rate));
    }

    pub fn reset(&mut self) {
        self.core.reset();
        for channel in &mut self.filters {
            for filter in channel {
                filter.reset();
            }
        }
        self.mode_crossfade.reset(1.0);
        self.last_output = [0.0; MAX_CHANNELS];
        self.has_processed = false;
    }

    pub fn next_frame(&mut self, params: &impl EqParamRefs) -> EqFrame {
        let mode = params.mode().value();
        self.set_mode(mode);
        let sample_rate = self.sample_rate;
        let frame = EqFrame {
            mode,
            active_mix: self
                .core
                .next_frame(params.bypass().value(), 1.0, 0.0)
                .active_mix,
            mode_fade: self.mode_crossfade.next_value().clamp(0.0, 1.0),
            bands: core::array::from_fn(|band| {
                let band_params = params.band(band);
                EqBandFrame {
                    enabled: band_params.enabled.value(),
                    band_type: band_params.band_type.value(),
                    frequency: clamp_frequency(
                        band_params.frequency.smoothed.next(),
                        20.0,
                        20_000.0,
                        sample_rate,
                    ),
                    gain: band_params.gain.smoothed.next().clamp(-24.0, 24.0),
                    q: band_params.q.smoothed.next().clamp(0.1, 12.0),
                }
            }),
        };
        self.update_coefficients_if_needed(&frame);
        frame
    }

    pub fn process_block(&mut self, buffer: &mut Buffer, params: &impl EqParamRefs) {
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params);
            for (channel_index, sample) in channel_samples.into_iter().enumerate() {
                *sample = self.process_sample_for_channel(channel_index, *sample, &frame);
            }
        }
    }

    pub fn process_sample(&mut self, sample: f32, frame: &EqFrame) -> f32 {
        self.process_sample_for_channel(0, sample, frame)
    }

    pub fn process_sample_for_channel(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &EqFrame,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let dry = sanitize_sample(sample);
        let mut wet = dry;

        if frame.mode == EqMode::On {
            for filter in &mut self.filters[index] {
                wet = filter.process(wet);
            }
        }

        let wet = self.smooth_mode_transition(index, wet, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, wet, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    fn update_coefficients(&mut self, frame: &EqFrame) {
        let state = EqCoefficientState::from_frame(frame, self.sample_rate);
        let coefficients = frame
            .bands
            .map(|band| coefficients_for_band(band, self.sample_rate));

        for channel in &mut self.filters {
            for (filter, coefficients) in channel.iter_mut().zip(coefficients) {
                filter.set_coefficients(coefficients);
            }
        }

        self.coefficient_state = Some(state);
    }

    fn update_coefficients_if_needed(&mut self, frame: &EqFrame) {
        let next_state = EqCoefficientState::from_frame(frame, self.sample_rate);
        if self
            .coefficient_state
            .is_none_or(|last_state| last_state.differs_from(next_state))
        {
            self.update_coefficients(frame);
        }
    }

    fn set_mode(&mut self, mode: EqMode) {
        if mode != self.current_mode {
            self.current_mode = mode;
            if self.has_processed {
                self.mode_crossfade.reset(0.0);
                self.mode_crossfade.set_target(1.0);
            } else {
                self.mode_crossfade.reset(1.0);
            }
        }
    }

    fn smooth_mode_transition(&self, channel: usize, next_output: f32, mode_fade: f32) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        if mode_fade >= 0.999_999 {
            sanitize_sample(next_output)
        } else {
            linear_crossfade(self.last_output[index], next_output, mode_fade)
        }
    }
}

impl EqFrame {
    fn neutral(sample_rate: f32) -> Self {
        Self {
            mode: EqMode::On,
            active_mix: 1.0,
            mode_fade: 1.0,
            bands: [
                EqBandFrame::off(120.0),
                EqBandFrame::off(400.0),
                EqBandFrame::off(1_000.0),
                EqBandFrame::off(3_000.0),
                EqBandFrame::off(clamp_frequency(8_000.0, 20.0, 20_000.0, sample_rate)),
            ],
        }
    }
}

impl EqCoefficientState {
    fn from_frame(frame: &EqFrame, sample_rate: f32) -> Self {
        Self {
            sample_rate,
            bands: frame.bands,
        }
    }

    fn differs_from(self, other: Self) -> bool {
        value_changed(
            self.sample_rate,
            other.sample_rate,
            COEFFICIENT_SAMPLE_RATE_EPSILON,
        ) || self
            .bands
            .iter()
            .zip(other.bands)
            .any(|(a, b)| band_frame_changed(*a, b))
    }
}

impl EqBandFrame {
    fn off(frequency: f32) -> Self {
        Self {
            enabled: false,
            band_type: EqBandType::Off,
            frequency,
            gain: 0.0,
            q: 1.0,
        }
    }
}

fn coefficients_for_band(band: EqBandFrame, sample_rate: f32) -> BiquadCoefficients {
    if !band.enabled || band.band_type == EqBandType::Off {
        return BiquadCoefficients::identity();
    }

    match band.band_type {
        EqBandType::Off => BiquadCoefficients::identity(),
        EqBandType::Bell => {
            BiquadCoefficients::peaking(band.frequency, band.gain, band.q, sample_rate)
        }
        EqBandType::LowShelf => {
            BiquadCoefficients::low_shelf(band.frequency, band.gain, band.q, sample_rate)
        }
        EqBandType::HighShelf => {
            BiquadCoefficients::high_shelf(band.frequency, band.gain, band.q, sample_rate)
        }
        EqBandType::HighPass => BiquadCoefficients::high_pass(band.frequency, band.q, sample_rate),
        EqBandType::LowPass => BiquadCoefficients::low_pass(band.frequency, band.q, sample_rate),
    }
}

fn band_frame_changed(a: EqBandFrame, b: EqBandFrame) -> bool {
    a.enabled != b.enabled
        || a.band_type != b.band_type
        || value_changed(a.frequency, b.frequency, COEFFICIENT_FREQUENCY_EPSILON)
        || value_changed(a.gain, b.gain, COEFFICIENT_GAIN_EPSILON)
        || value_changed(a.q, b.q, COEFFICIENT_Q_EPSILON)
}

#[inline]
fn linear_crossfade(dry: f32, wet: f32, amount: f32) -> f32 {
    let amount = amount.clamp(0.0, 1.0);
    sanitize_sample((dry * (1.0 - amount)) + (wet * amount))
}

#[inline]
fn value_changed(previous: f32, next: f32, epsilon: f32) -> bool {
    (previous - next).abs() > epsilon
}

impl Biquad {
    fn set_coefficients(&mut self, coefficients: BiquadCoefficients) {
        self.coefficients = coefficients.sanitized();
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    fn process(&mut self, sample: f32) -> f32 {
        let input = sanitize_sample(sample);
        let output = (self.coefficients.b0 * input) + self.z1;
        self.z1 = (self.coefficients.b1 * input) - (self.coefficients.a1 * output) + self.z2;
        self.z2 = (self.coefficients.b2 * input) - (self.coefficients.a2 * output);

        if !self.z1.is_finite() || !self.z2.is_finite() {
            self.reset();
            return input;
        }

        sanitize_sample(output)
    }
}

impl BiquadCoefficients {
    fn high_pass(frequency: f32, q: f32, sample_rate: f32) -> Self {
        let omega = omega(frequency, sample_rate);
        let alpha = omega.sin() / (2.0 * q.max(0.1));
        let cos = omega.cos();
        let b0 = (1.0 + cos) * 0.5;
        let b1 = -(1.0 + cos);
        let b2 = (1.0 + cos) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;

        normalize(b0, b1, b2, a0, a1, a2)
    }

    fn low_pass(frequency: f32, q: f32, sample_rate: f32) -> Self {
        let omega = omega(frequency, sample_rate);
        let alpha = omega.sin() / (2.0 * q.max(0.1));
        let cos = omega.cos();
        let b0 = (1.0 - cos) * 0.5;
        let b1 = 1.0 - cos;
        let b2 = (1.0 - cos) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;

        normalize(b0, b1, b2, a0, a1, a2)
    }

    fn peaking(frequency: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        if gain_db.abs() < 0.000_1 {
            return Self::identity();
        }

        let omega = omega(frequency, sample_rate);
        let alpha = omega.sin() / (2.0 * q.clamp(0.1, 10.0));
        let cos = omega.cos();
        let a = 10.0_f32.powf(gain_db / 40.0);
        let b0 = 1.0 + (alpha * a);
        let b1 = -2.0 * cos;
        let b2 = 1.0 - (alpha * a);
        let a0 = 1.0 + (alpha / a);
        let a1 = -2.0 * cos;
        let a2 = 1.0 - (alpha / a);

        normalize(b0, b1, b2, a0, a1, a2)
    }

    fn low_shelf(frequency: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        shelf(frequency, gain_db, q, sample_rate, false)
    }

    fn high_shelf(frequency: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        shelf(frequency, gain_db, q, sample_rate, true)
    }

    fn sanitized(self) -> Self {
        if self.b0.is_finite()
            && self.b1.is_finite()
            && self.b2.is_finite()
            && self.a1.is_finite()
            && self.a2.is_finite()
        {
            self
        } else {
            Self::identity()
        }
    }
}

fn shelf(frequency: f32, gain_db: f32, q: f32, sample_rate: f32, high: bool) -> BiquadCoefficients {
    if gain_db.abs() < 0.000_1 {
        return BiquadCoefficients::identity();
    }

    let omega = omega(frequency, sample_rate);
    let sin = omega.sin();
    let cos = omega.cos();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let sqrt_a = a.sqrt();
    let alpha = sin / (2.0 * q.max(0.1));
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let (b0, b1, b2, a0, a1, a2) = if high {
        (
            a * ((a + 1.0) + ((a - 1.0) * cos) + two_sqrt_a_alpha),
            -2.0 * a * ((a - 1.0) + ((a + 1.0) * cos)),
            a * ((a + 1.0) + ((a - 1.0) * cos) - two_sqrt_a_alpha),
            (a + 1.0) - ((a - 1.0) * cos) + two_sqrt_a_alpha,
            2.0 * ((a - 1.0) - ((a + 1.0) * cos)),
            (a + 1.0) - ((a - 1.0) * cos) - two_sqrt_a_alpha,
        )
    } else {
        (
            a * ((a + 1.0) - ((a - 1.0) * cos) + two_sqrt_a_alpha),
            2.0 * a * ((a - 1.0) - ((a + 1.0) * cos)),
            a * ((a + 1.0) - ((a - 1.0) * cos) - two_sqrt_a_alpha),
            (a + 1.0) + ((a - 1.0) * cos) + two_sqrt_a_alpha,
            -2.0 * ((a - 1.0) + ((a + 1.0) * cos)),
            (a + 1.0) + ((a - 1.0) * cos) - two_sqrt_a_alpha,
        )
    };

    normalize(b0, b1, b2, a0, a1, a2)
}

fn normalize(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> BiquadCoefficients {
    let a0 = if a0.abs() < 0.000_001 { 1.0 } else { a0 };
    BiquadCoefficients {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
    .sanitized()
}

fn omega(frequency: f32, sample_rate: f32) -> f32 {
    let frequency = clamp_frequency(frequency, 20.0, 20_000.0, sample_rate);
    core::f32::consts::TAU * frequency / sample_rate.max(1.0)
}

fn clamp_frequency(frequency: f32, min: f32, max: f32, sample_rate: f32) -> f32 {
    let nyquist_safe = (sample_rate.max(1.0) * 0.49).max(20.0);
    frequency.clamp(min, max.min(nyquist_safe))
}

#[cfg(test)]
mod tests {
    use crate::dsp::chain::EqRack;

    use super::{
        clamp_frequency, coefficients_for_band, Biquad, BiquadCoefficients, Eq, EqBandFrame,
        EqBandType, EqFrame, EqMode, NUM_BANDS,
    };

    #[test]
    fn clamps_frequency_below_nyquist() {
        assert!(clamp_frequency(20_000.0, 2_000.0, 20_000.0, 44_100.0) < 22_050.0);
    }

    #[test]
    fn updating_one_rack_eq_does_not_change_other_coefficient_states() {
        let mut rack = EqRack::default();
        rack.prepare(48_000.0);
        let mut changed = EqFrame::neutral(48_000.0);
        changed.bands[2] = EqBandFrame {
            enabled: true,
            band_type: EqBandType::Bell,
            frequency: 1_200.0,
            gain: 12.0,
            q: 1.5,
        };

        rack.character_pre.update_coefficients(&changed);

        assert_eq!(
            rack.character_pre.coefficient_state.unwrap().bands[2].gain,
            12.0
        );
        for eq in [
            &rack.global_pre,
            &rack.global_post,
            &rack.character_post,
            &rack.movement_pre,
            &rack.movement_post,
            &rack.diffusion_pre,
            &rack.diffusion_post,
            &rack.texture_pre,
            &rack.texture_post,
        ] {
            assert_eq!(eq.coefficient_state.unwrap().bands[2].gain, 0.0);
        }
    }

    #[test]
    fn biquad_coefficients_are_finite() {
        let filters = [
            BiquadCoefficients::high_pass(20.0, 0.707, 48_000.0),
            BiquadCoefficients::low_shelf(120.0, 18.0, 1.0, 48_000.0),
            BiquadCoefficients::peaking(1_000.0, -18.0, 0.1, 48_000.0),
            BiquadCoefficients::high_shelf(8_000.0, 18.0, 1.0, 48_000.0),
            BiquadCoefficients::low_pass(20_000.0, 0.707, 48_000.0),
        ];

        for coefficients in filters {
            assert!(coefficients.b0.is_finite());
            assert!(coefficients.b1.is_finite());
            assert!(coefficients.b2.is_finite());
            assert!(coefficients.a1.is_finite());
            assert!(coefficients.a2.is_finite());
        }
    }

    #[test]
    fn coefficients_are_finite_for_each_eq_band_type() {
        for band_type in [
            EqBandType::Off,
            EqBandType::Bell,
            EqBandType::LowShelf,
            EqBandType::HighShelf,
            EqBandType::HighPass,
            EqBandType::LowPass,
        ] {
            let coefficients = coefficients_for_band(
                EqBandFrame {
                    enabled: true,
                    band_type,
                    frequency: 1_000.0,
                    gain: 6.0,
                    q: 0.707,
                },
                48_000.0,
            );

            assert_coefficients_finite(coefficients);
        }
    }

    #[test]
    fn off_and_disabled_bands_are_identity() {
        let off = coefficients_for_band(
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::Off,
                frequency: 1_000.0,
                gain: 12.0,
                q: 0.5,
            },
            48_000.0,
        );
        let disabled = coefficients_for_band(
            EqBandFrame {
                enabled: false,
                band_type: EqBandType::Bell,
                frequency: 1_000.0,
                gain: 12.0,
                q: 0.5,
            },
            48_000.0,
        );

        assert_coefficients_close(off, BiquadCoefficients::identity());
        assert_coefficients_close(disabled, BiquadCoefficients::identity());
    }

    #[test]
    fn flat_bands_are_identity() {
        for band_type in [
            EqBandType::Bell,
            EqBandType::LowShelf,
            EqBandType::HighShelf,
        ] {
            let coefficients = coefficients_for_band(
                EqBandFrame {
                    enabled: true,
                    band_type,
                    frequency: 1_000.0,
                    gain: 0.0,
                    q: 1.0,
                },
                48_000.0,
            );

            assert_coefficients_close(coefficients, BiquadCoefficients::identity());
        }
    }

    #[test]
    fn high_pass_and_low_pass_ignore_gain() {
        for band_type in [EqBandType::HighPass, EqBandType::LowPass] {
            let low_gain = coefficients_for_band(
                EqBandFrame {
                    enabled: true,
                    band_type,
                    frequency: 1_000.0,
                    gain: -18.0,
                    q: 0.707,
                },
                48_000.0,
            );
            let high_gain = coefficients_for_band(
                EqBandFrame {
                    enabled: true,
                    band_type,
                    frequency: 1_000.0,
                    gain: 18.0,
                    q: 0.707,
                },
                48_000.0,
            );

            assert_coefficients_close(low_gain, high_gain);
        }
    }

    #[test]
    fn high_pass_band_ignores_gain() {
        assert_filter_type_ignores_gain(EqBandType::HighPass);
    }

    #[test]
    fn low_pass_band_ignores_gain() {
        assert_filter_type_ignores_gain(EqBandType::LowPass);
    }

    #[test]
    fn every_eq_band_type_outputs_finite_audio() {
        for band_type in [
            EqBandType::Off,
            EqBandType::Bell,
            EqBandType::LowShelf,
            EqBandType::HighShelf,
            EqBandType::HighPass,
            EqBandType::LowPass,
        ] {
            let coefficients = coefficients_for_band(
                EqBandFrame {
                    enabled: true,
                    band_type,
                    frequency: 1_000.0,
                    gain: 9.0,
                    q: 0.707,
                },
                48_000.0,
            );
            let mut filter = Biquad::default();
            filter.set_coefficients(coefficients);

            for index in 0..512 {
                let input = if index == 0 { 1.0 } else { 0.0 };
                assert!(filter.process(input).is_finite());
            }
        }
    }

    #[test]
    fn biquad_output_stays_finite() {
        let mut filter = Biquad::default();
        filter.set_coefficients(BiquadCoefficients::peaking(1_000.0, 18.0, 10.0, 48_000.0));

        let mut peak = 0.0_f32;
        for index in 0..4_000 {
            let input = if index == 0 { 1.0 } else { 0.0 };
            let sample = filter.process(input);
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }

        assert!(peak < 8.0, "biquad impulse response peak was {peak}");
    }

    #[test]
    fn eq_chain_output_stays_finite() {
        let mut eq = Eq::default();
        eq.prepare(48_000.0);
        let frame = EqFrame {
            mode: EqMode::On,
            active_mix: 1.0,
            mode_fade: 1.0,
            bands: [
                EqBandFrame {
                    enabled: true,
                    band_type: EqBandType::HighPass,
                    frequency: 40.0,
                    gain: 0.0,
                    q: 0.707,
                },
                EqBandFrame {
                    enabled: true,
                    band_type: EqBandType::LowShelf,
                    frequency: 100.0,
                    gain: 12.0,
                    q: 1.0,
                },
                EqBandFrame {
                    enabled: true,
                    band_type: EqBandType::Bell,
                    frequency: 1_200.0,
                    gain: -6.0,
                    q: 1.2,
                },
                EqBandFrame {
                    enabled: true,
                    band_type: EqBandType::HighShelf,
                    frequency: 8_000.0,
                    gain: 6.0,
                    q: 1.0,
                },
                EqBandFrame {
                    enabled: true,
                    band_type: EqBandType::LowPass,
                    frequency: 18_000.0,
                    gain: 0.0,
                    q: 0.707,
                },
            ],
        };
        eq.update_coefficients(&frame);

        let mut peak = 0.0_f32;
        for index in 0..4_000 {
            let input = if index == 0 { 0.5 } else { 0.0 };
            let sample = eq.process_sample_for_channel(0, input, &frame);
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }

        assert!(peak < 8.0, "eq impulse response peak was {peak}");
    }

    #[test]
    fn shelf_q_affects_coefficients() {
        let narrow = coefficients_for_band(
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::LowShelf,
                frequency: 1_000.0,
                gain: 12.0,
                q: 4.0,
            },
            48_000.0,
        );
        let wide = coefficients_for_band(
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::LowShelf,
                frequency: 1_000.0,
                gain: 12.0,
                q: 0.5,
            },
            48_000.0,
        );

        let diff = (narrow.b0 - wide.b0).abs()
            + (narrow.b1 - wide.b1).abs()
            + (narrow.b2 - wide.b2).abs()
            + (narrow.a1 - wide.a1).abs()
            + (narrow.a2 - wide.a2).abs();
        assert!(diff > 0.000_1, "Shelf Q should affect coefficients");
    }

    #[test]
    fn high_shelf_q_affects_coefficients() {
        let narrow = coefficients_for_band(
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::HighShelf,
                frequency: 5_000.0,
                gain: 12.0,
                q: 4.0,
            },
            48_000.0,
        );
        let wide = coefficients_for_band(
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::HighShelf,
                frequency: 5_000.0,
                gain: 12.0,
                q: 0.5,
            },
            48_000.0,
        );

        let diff = (narrow.b0 - wide.b0).abs()
            + (narrow.b1 - wide.b1).abs()
            + (narrow.b2 - wide.b2).abs()
            + (narrow.a1 - wide.a1).abs()
            + (narrow.a2 - wide.a2).abs();
        assert!(diff > 0.000_1, "High shelf Q should affect coefficients");
    }

    #[test]
    fn off_band_is_transparent_in_full_eq() {
        let mut eq = Eq::default();
        eq.prepare(48_000.0);
        let frame = EqFrame {
            mode: EqMode::On,
            active_mix: 1.0,
            mode_fade: 1.0,
            bands: [EqBandFrame::off(1_000.0); NUM_BANDS],
        };
        eq.update_coefficients(&frame);

        for index in 0..512 {
            let input = (index as f32 * 0.01).sin();
            let output = eq.process_sample_for_channel(0, input, &frame);
            assert!((output - input).abs() < 0.000_001);
        }
    }

    #[test]
    fn ten_eqs_flat_in_series_are_transparent() {
        let mut eqs: [Eq; 10] = Default::default();
        for eq in &mut eqs {
            eq.prepare(48_000.0);
        }

        let flat_bands: [EqBandFrame; NUM_BANDS] = core::array::from_fn(|_| EqBandFrame {
            enabled: true,
            band_type: EqBandType::Bell,
            frequency: 1_000.0,
            gain: 0.0,
            q: 1.0,
        });

        let frame = EqFrame {
            mode: EqMode::On,
            active_mix: 1.0,
            mode_fade: 1.0,
            bands: flat_bands,
        };

        for eq in &mut eqs {
            eq.update_coefficients(&frame);
        }

        for index in 0..512 {
            let input = (index as f32 * 0.01).sin() * 0.5;
            let mut sample = input;
            for eq in &mut eqs {
                sample = eq.process_sample_for_channel(0, sample, &frame);
            }
            assert!(
                (sample - input).abs() < 0.000_1,
                "10 flat EQs should be transparent. input={input}, output={sample}"
            );
        }
    }

    #[test]
    fn ten_eqs_with_active_bands_dont_explode() {
        let mut eqs: [Eq; 10] = Default::default();
        for eq in &mut eqs {
            eq.prepare(48_000.0);
        }

        let active_bands: [EqBandFrame; NUM_BANDS] = [
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::HighPass,
                frequency: 40.0,
                gain: 0.0,
                q: 0.707,
            },
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::LowShelf,
                frequency: 120.0,
                gain: 6.0,
                q: 1.0,
            },
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::Bell,
                frequency: 800.0,
                gain: -3.0,
                q: 2.0,
            },
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::Bell,
                frequency: 3_000.0,
                gain: 4.0,
                q: 0.7,
            },
            EqBandFrame {
                enabled: true,
                band_type: EqBandType::HighShelf,
                frequency: 8_000.0,
                gain: -2.0,
                q: 1.0,
            },
        ];

        let frame = EqFrame {
            mode: EqMode::On,
            active_mix: 1.0,
            mode_fade: 1.0,
            bands: active_bands,
        };

        for eq in &mut eqs {
            eq.update_coefficients(&frame);
        }

        let mut peak = 0.0_f32;
        for index in 0..4_000 {
            let input = if index == 0 { 1.0 } else { 0.0 };
            let mut sample = input;
            for eq in &mut eqs {
                sample = eq.process_sample_for_channel(0, sample, &frame);
            }
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }

        assert!(peak < 8.0, "10 active EQs peak was {peak}");
    }

    fn assert_coefficients_finite(coefficients: BiquadCoefficients) {
        assert!(coefficients.b0.is_finite());
        assert!(coefficients.b1.is_finite());
        assert!(coefficients.b2.is_finite());
        assert!(coefficients.a1.is_finite());
        assert!(coefficients.a2.is_finite());
    }

    fn assert_coefficients_close(left: BiquadCoefficients, right: BiquadCoefficients) {
        assert!((left.b0 - right.b0).abs() < 0.000_001);
        assert!((left.b1 - right.b1).abs() < 0.000_001);
        assert!((left.b2 - right.b2).abs() < 0.000_001);
        assert!((left.a1 - right.a1).abs() < 0.000_001);
        assert!((left.a2 - right.a2).abs() < 0.000_001);
    }

    fn assert_filter_type_ignores_gain(band_type: EqBandType) {
        let low_gain = coefficients_for_band(
            EqBandFrame {
                enabled: true,
                band_type,
                frequency: 1_000.0,
                gain: -24.0,
                q: 0.707,
            },
            48_000.0,
        );
        let high_gain = coefficients_for_band(
            EqBandFrame {
                enabled: true,
                band_type,
                frequency: 1_000.0,
                gain: 24.0,
                q: 0.707,
            },
            48_000.0,
        );

        assert_coefficients_close(low_gain, high_gain);
    }
}
