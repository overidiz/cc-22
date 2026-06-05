use nih_plug::prelude::*;

use crate::params::EqParams;

use super::{
    chain::{sanitize_sample, ModuleCore},
    smoothing::LinearSmoother,
};

const MAX_CHANNELS: usize = 2;
const NUM_BANDS: usize = 5;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqMode {
    #[id = "off"]
    Off,

    #[id = "on"]
    On,
}

#[derive(Debug, Clone)]
pub struct Eq {
    core: ModuleCore,
    sample_rate: f32,
    filters: [[Biquad; NUM_BANDS]; MAX_CHANNELS],
    current_mode: EqMode,
    mode_crossfade: LinearSmoother,
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct EqFrame {
    mode: EqMode,
    active_mix: f32,
    mode_fade: f32,
    low_cut_frequency: f32,
    low_shelf_gain: f32,
    low_shelf_frequency: f32,
    mid_gain: f32,
    mid_frequency: f32,
    mid_q: f32,
    high_shelf_gain: f32,
    high_shelf_frequency: f32,
    high_cut_frequency: f32,
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

impl Default for Eq {
    fn default() -> Self {
        let mut eq = Self {
            core: ModuleCore::default(),
            sample_rate: 44_100.0,
            filters: [[Biquad::default(); NUM_BANDS]; MAX_CHANNELS],
            current_mode: EqMode::On,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
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

    pub fn next_frame(&mut self, params: &EqParams) -> EqFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let sample_rate = self.sample_rate;
        let frame = EqFrame {
            mode,
            active_mix: self
                .core
                .next_frame(params.bypass.value(), 1.0, 1.0, 0.0)
                .active_mix,
            mode_fade: self.mode_crossfade.next_value().clamp(0.0, 1.0),
            low_cut_frequency: clamp_frequency(
                params.low_cut_frequency.smoothed.next(),
                20.0,
                500.0,
                sample_rate,
            ),
            low_shelf_gain: params.low_shelf_gain.smoothed.next().clamp(-18.0, 18.0),
            low_shelf_frequency: clamp_frequency(
                params.low_shelf_frequency.smoothed.next(),
                40.0,
                500.0,
                sample_rate,
            ),
            mid_gain: params.mid_gain.smoothed.next().clamp(-18.0, 18.0),
            mid_frequency: clamp_frequency(
                params.mid_frequency.smoothed.next(),
                100.0,
                8_000.0,
                sample_rate,
            ),
            mid_q: params.mid_q.smoothed.next().clamp(0.1, 10.0),
            high_shelf_gain: params.high_shelf_gain.smoothed.next().clamp(-18.0, 18.0),
            high_shelf_frequency: clamp_frequency(
                params.high_shelf_frequency.smoothed.next(),
                1_000.0,
                16_000.0,
                sample_rate,
            ),
            high_cut_frequency: clamp_frequency(
                params.high_cut_frequency.smoothed.next(),
                2_000.0,
                20_000.0,
                sample_rate,
            ),
        };
        self.update_coefficients(&frame);
        frame
    }

    pub fn process_block(&mut self, buffer: &mut Buffer, params: &EqParams) {
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
        let coefficients = [
            BiquadCoefficients::high_pass(frame.low_cut_frequency, 0.707, self.sample_rate),
            BiquadCoefficients::low_shelf(
                frame.low_shelf_frequency,
                frame.low_shelf_gain,
                self.sample_rate,
            ),
            BiquadCoefficients::peaking(
                frame.mid_frequency,
                frame.mid_gain,
                frame.mid_q,
                self.sample_rate,
            ),
            BiquadCoefficients::high_shelf(
                frame.high_shelf_frequency,
                frame.high_shelf_gain,
                self.sample_rate,
            ),
            BiquadCoefficients::low_pass(frame.high_cut_frequency, 0.707, self.sample_rate),
        ];

        for channel in &mut self.filters {
            for (filter, coefficients) in channel.iter_mut().zip(coefficients) {
                filter.set_coefficients(coefficients);
            }
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
            low_cut_frequency: 20.0,
            low_shelf_gain: 0.0,
            low_shelf_frequency: 120.0,
            mid_gain: 0.0,
            mid_frequency: 1_000.0,
            mid_q: 1.0,
            high_shelf_gain: 0.0,
            high_shelf_frequency: 8_000.0,
            high_cut_frequency: clamp_frequency(20_000.0, 2_000.0, 20_000.0, sample_rate),
        }
    }
}

#[inline]
fn linear_crossfade(dry: f32, wet: f32, amount: f32) -> f32 {
    let amount = amount.clamp(0.0, 1.0);
    sanitize_sample((dry * (1.0 - amount)) + (wet * amount))
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

    fn low_shelf(frequency: f32, gain_db: f32, sample_rate: f32) -> Self {
        shelf(frequency, gain_db, sample_rate, false)
    }

    fn high_shelf(frequency: f32, gain_db: f32, sample_rate: f32) -> Self {
        shelf(frequency, gain_db, sample_rate, true)
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

fn shelf(frequency: f32, gain_db: f32, sample_rate: f32, high: bool) -> BiquadCoefficients {
    if gain_db.abs() < 0.000_1 {
        return BiquadCoefficients::identity();
    }

    let omega = omega(frequency, sample_rate);
    let sin = omega.sin();
    let cos = omega.cos();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let sqrt_a = a.sqrt();
    let alpha = sin * core::f32::consts::FRAC_1_SQRT_2;
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
    use super::{clamp_frequency, Biquad, BiquadCoefficients, Eq, EqFrame, EqMode};

    #[test]
    fn clamps_frequency_below_nyquist() {
        assert!(clamp_frequency(20_000.0, 2_000.0, 20_000.0, 44_100.0) < 22_050.0);
    }

    #[test]
    fn biquad_coefficients_are_finite() {
        let filters = [
            BiquadCoefficients::high_pass(20.0, 0.707, 48_000.0),
            BiquadCoefficients::low_shelf(120.0, 18.0, 48_000.0),
            BiquadCoefficients::peaking(1_000.0, -18.0, 0.1, 48_000.0),
            BiquadCoefficients::high_shelf(8_000.0, 18.0, 48_000.0),
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
    fn biquad_output_stays_finite() {
        let mut filter = Biquad::default();
        filter.set_coefficients(BiquadCoefficients::peaking(1_000.0, 18.0, 10.0, 48_000.0));

        let mut sample = 1.0;
        for _ in 0..4_000 {
            sample = filter.process(sample);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn eq_chain_output_stays_finite() {
        let mut eq = Eq::default();
        eq.prepare(48_000.0);
        let frame = EqFrame {
            mode: EqMode::On,
            active_mix: 1.0,
            mode_fade: 1.0,
            low_cut_frequency: 40.0,
            low_shelf_gain: 12.0,
            low_shelf_frequency: 100.0,
            mid_gain: -6.0,
            mid_frequency: 1_200.0,
            mid_q: 1.2,
            high_shelf_gain: 6.0,
            high_shelf_frequency: 8_000.0,
            high_cut_frequency: 18_000.0,
        };
        eq.update_coefficients(&frame);

        let mut sample = 0.5;
        for _ in 0..4_000 {
            sample = eq.process_sample_for_channel(0, sample, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }
}
