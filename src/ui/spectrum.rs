//! Input spectrum analyzer for the EQ display.
//!
//! The audio thread fills a lock-free ring in [`crate::meters::Meters`]; here, on
//! the UI thread, we take a windowed snapshot, run a small radix-2 FFT (no extra
//! dependency), and fold the magnitude bins into log-spaced display columns that
//! line up with the EQ frequency axis. All of the cost lives on the UI thread —
//! the audio thread only ever does relaxed atomic stores.

use crate::meters::{Meters, ANALYZER_FFT_SIZE};

/// Number of log-spaced columns drawn across the frequency axis.
pub(crate) const SPECTRUM_COLUMNS: usize = 160;

const DB_FLOOR: f32 = -78.0;
const DB_CEIL: f32 = 6.0;

pub(crate) struct SpectrumState {
    samples: Box<[f32; ANALYZER_FFT_SIZE]>,
    re: Box<[f32; ANALYZER_FFT_SIZE]>,
    im: Box<[f32; ANALYZER_FFT_SIZE]>,
    window: Box<[f32; ANALYZER_FFT_SIZE]>,
    /// Smoothed, normalized (0..1) column heights ready to draw.
    columns: [f32; SPECTRUM_COLUMNS],
}

impl Default for SpectrumState {
    fn default() -> Self {
        // Hann window keeps tonal peaks readable instead of smearing into skirts.
        let mut window = Box::new([0.0_f32; ANALYZER_FFT_SIZE]);
        let denom = (ANALYZER_FFT_SIZE - 1) as f32;
        for (i, w) in window.iter_mut().enumerate() {
            *w = 0.5 - 0.5 * (core::f32::consts::TAU * i as f32 / denom).cos();
        }
        Self {
            samples: Box::new([0.0; ANALYZER_FFT_SIZE]),
            re: Box::new([0.0; ANALYZER_FFT_SIZE]),
            im: Box::new([0.0; ANALYZER_FFT_SIZE]),
            window,
            columns: [0.0; SPECTRUM_COLUMNS],
        }
    }
}

impl SpectrumState {
    /// Refresh the analyzer from the latest captured audio. `min_hz`/`max_hz`
    /// must match the EQ axis so the overlay lines up with the curve.
    pub(crate) fn update(&mut self, meters: &Meters, min_hz: f32, max_hz: f32) {
        meters.analyzer_snapshot(&mut self.samples);
        let sample_rate = meters.analyzer_sample_rate();

        for i in 0..ANALYZER_FFT_SIZE {
            self.re[i] = self.samples[i] * self.window[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re[..], &mut self.im[..]);

        let half = ANALYZER_FFT_SIZE / 2;
        let bin_hz = sample_rate / ANALYZER_FFT_SIZE as f32;
        // Hann coherent gain is 0.5, so a full-scale sine peaks near this norm = 1.
        let norm = 4.0 / ANALYZER_FFT_SIZE as f32;
        let ratio = (max_hz / min_hz).max(1.0);

        for (c, column) in self.columns.iter_mut().enumerate() {
            // Frequency span this column covers (log-spaced, matching the axis).
            let t0 = (c as f32 - 0.5) / (SPECTRUM_COLUMNS - 1) as f32;
            let t1 = (c as f32 + 0.5) / (SPECTRUM_COLUMNS - 1) as f32;
            let f_lo = min_hz * ratio.powf(t0.clamp(0.0, 1.0));
            let f_hi = min_hz * ratio.powf(t1.clamp(0.0, 1.0));

            let bin_lo = ((f_lo / bin_hz).floor() as usize).clamp(1, half - 1);
            let bin_hi = ((f_hi / bin_hz).ceil() as usize).clamp(bin_lo, half - 1);

            let mut mag = 0.0_f32;
            for bin in bin_lo..=bin_hi {
                let m = (self.re[bin] * self.re[bin] + self.im[bin] * self.im[bin]).sqrt();
                mag = mag.max(m);
            }

            let db = 20.0 * (mag * norm + 1e-9).log10();
            let target = ((db - DB_FLOOR) / (DB_CEIL - DB_FLOOR)).clamp(0.0, 1.0);

            // Fast rise, slow fall — a lively but readable analyzer.
            let coeff = if target > *column { 0.5 } else { 0.16 };
            *column += (target - *column) * coeff;
        }
    }

    pub(crate) fn columns(&self) -> &[f32; SPECTRUM_COLUMNS] {
        &self.columns
    }
}

/// In-place iterative radix-2 Cooley-Tukey FFT. `re`/`im` must be the same
/// power-of-two length.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());
    debug_assert_eq!(n, im.len());

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let angle = -core::f32::consts::TAU / len as f32;
        let (wlen_re, wlen_im) = (angle.cos(), angle.sin());
        let mut i = 0;
        while i < n {
            let (mut w_re, mut w_im) = (1.0_f32, 0.0_f32);
            for k in 0..len / 2 {
                let a = i + k;
                let b = a + len / 2;
                let v_re = re[b] * w_re - im[b] * w_im;
                let v_im = re[b] * w_im + im[b] * w_re;
                re[b] = re[a] - v_re;
                im[b] = im[a] - v_im;
                re[a] += v_re;
                im[a] += v_im;
                let nw_re = w_re * wlen_re - w_im * wlen_im;
                w_im = w_re * wlen_im + w_im * wlen_re;
                w_re = nw_re;
            }
            i += len;
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meters::Meters;

    #[test]
    fn fft_of_a_pure_bin_peaks_at_that_bin() {
        let n = 64;
        let bin = 5;
        let mut re = vec![0.0_f32; n];
        let mut im = vec![0.0_f32; n];
        for (i, r) in re.iter_mut().enumerate() {
            *r = (core::f32::consts::TAU * bin as f32 * i as f32 / n as f32).sin();
        }
        fft(&mut re, &mut im);
        let mags: Vec<f32> = (0..n / 2)
            .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt())
            .collect();
        let peak = (0..n / 2)
            .max_by(|a, b| mags[*a].partial_cmp(&mags[*b]).unwrap())
            .unwrap();
        assert_eq!(peak, bin, "FFT peak bin mismatch: {mags:?}");
    }

    #[test]
    fn silence_keeps_all_columns_at_floor() {
        let meters = Meters::default();
        meters.set_analyzer_sample_rate(48_000.0);
        let mut spectrum = SpectrumState::default();
        for _ in 0..40 {
            spectrum.update(&meters, 10.0, 20_000.0);
        }
        for &c in spectrum.columns() {
            assert!(c < 0.02, "silence should read near zero, got {c}");
        }
    }

    #[test]
    fn a_tone_lifts_the_column_at_its_frequency() {
        let meters = Meters::default();
        let sample_rate = 48_000.0;
        meters.set_analyzer_sample_rate(sample_rate);
        let freq = 1_000.0_f32;
        // Fill the ring with a steady tone.
        let mut phase = 0.0_f32;
        for _ in 0..ANALYZER_FFT_SIZE * 2 {
            phase += freq / sample_rate;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            meters.push_analyzer_sample((phase * core::f32::consts::TAU).sin() * 0.5);
        }
        let mut spectrum = SpectrumState::default();
        for _ in 0..40 {
            spectrum.update(&meters, 10.0, 20_000.0);
        }

        // The column nearest 1 kHz must be clearly elevated, and DC/very-low and
        // very-high columns must stay quiet.
        let ratio = 20_000.0_f32 / 10.0;
        let tone_col =
            (((freq / 10.0).ln() / ratio.ln()) * (SPECTRUM_COLUMNS - 1) as f32).round() as usize;
        let tone_height = spectrum.columns()[tone_col.clamp(1, SPECTRUM_COLUMNS - 2)];
        assert!(
            tone_height > 0.5,
            "1 kHz column should be lifted, got {tone_height}"
        );
        assert!(
            spectrum.columns()[SPECTRUM_COLUMNS - 1] < 0.25,
            "20 kHz column should stay quiet for a 1 kHz tone"
        );
    }

    #[test]
    fn analyzer_does_not_produce_nan_on_extreme_input() {
        let meters = Meters::default();
        meters.set_analyzer_sample_rate(48_000.0);
        for i in 0..ANALYZER_FFT_SIZE {
            meters.push_analyzer_sample(if i % 2 == 0 { 8.0 } else { -8.0 });
        }
        let mut spectrum = SpectrumState::default();
        spectrum.update(&meters, 10.0, 20_000.0);
        for &c in spectrum.columns() {
            assert!(c.is_finite(), "column not finite");
            assert!((0.0..=1.0).contains(&c), "column out of range: {c}");
        }
    }
}
