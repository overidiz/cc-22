use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const CLIP_THRESHOLD: f32 = 0.999;
const MAX_METER_PEAK: f32 = 4.0;

/// Length of the lock-free analyzer capture ring. Power of two so the index can
/// wrap with a cheap bit-mask. At 48 kHz this holds ~43 ms of input, giving the
/// spectrum display ~23 Hz bins — enough low-end resolution for an EQ overlay.
pub const ANALYZER_FFT_SIZE: usize = 2048;

#[derive(Debug)]
pub struct Meters {
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    input_clip_events: AtomicU32,
    output_clip_events: AtomicU32,
    /// Recent input samples (f32 bits) written by the audio thread, read by the
    /// UI thread. Relaxed atomics only: a torn snapshot is harmless for a meter.
    analyzer_ring: [AtomicU32; ANALYZER_FFT_SIZE],
    analyzer_write: AtomicUsize,
    analyzer_sample_rate: AtomicU32,
}

impl Default for Meters {
    fn default() -> Self {
        Self {
            input_peak: AtomicU32::new(0),
            output_peak: AtomicU32::new(0),
            input_clip_events: AtomicU32::new(0),
            output_clip_events: AtomicU32::new(0),
            analyzer_ring: std::array::from_fn(|_| AtomicU32::new(0)),
            analyzer_write: AtomicUsize::new(0),
            analyzer_sample_rate: AtomicU32::new(44_100.0_f32.to_bits()),
        }
    }
}

impl Meters {
    pub fn publish_block(&self, input_peak: f32, output_peak: f32) {
        self.publish_input_peak(input_peak);
        self.publish_output_peak(output_peak);

        if input_peak >= CLIP_THRESHOLD {
            self.input_clip_events.fetch_add(1, Ordering::Relaxed);
        }

        if output_peak >= CLIP_THRESHOLD {
            self.output_clip_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn take_input_peak(&self) -> f32 {
        peak_from_bits(self.input_peak.swap(0, Ordering::Acquire))
    }

    pub fn take_output_peak(&self) -> f32 {
        peak_from_bits(self.output_peak.swap(0, Ordering::Acquire))
    }

    pub fn input_clip_events(&self) -> u32 {
        self.input_clip_events.load(Ordering::Acquire)
    }

    pub fn output_clip_events(&self) -> u32 {
        self.output_clip_events.load(Ordering::Acquire)
    }

    /// Audio thread: record the host sample rate so the UI can map FFT bins to Hz.
    pub fn set_analyzer_sample_rate(&self, sample_rate: f32) {
        self.analyzer_sample_rate
            .store(sample_rate.max(1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn analyzer_sample_rate(&self) -> f32 {
        let sr = f32::from_bits(self.analyzer_sample_rate.load(Ordering::Relaxed));
        if sr.is_finite() && sr > 0.0 {
            sr
        } else {
            44_100.0
        }
    }

    /// Audio thread: push one input sample into the capture ring. Allocation-free
    /// and lock-free — a single relaxed store plus a wrapping counter increment.
    pub fn push_analyzer_sample(&self, sample: f32) {
        let index = self.analyzer_write.fetch_add(1, Ordering::Relaxed) & (ANALYZER_FFT_SIZE - 1);
        let value = if sample.is_finite() { sample } else { 0.0 };
        self.analyzer_ring[index].store(value.to_bits(), Ordering::Relaxed);
    }

    /// UI thread: copy the most recent `ANALYZER_FFT_SIZE` samples in chronological
    /// order (oldest first) into `out`.
    pub fn analyzer_snapshot(&self, out: &mut [f32; ANALYZER_FFT_SIZE]) {
        let write = self.analyzer_write.load(Ordering::Relaxed);
        for (k, slot) in out.iter_mut().enumerate() {
            let index = (write + k) & (ANALYZER_FFT_SIZE - 1);
            *slot = f32::from_bits(self.analyzer_ring[index].load(Ordering::Relaxed));
        }
    }

    fn publish_input_peak(&self, peak: f32) {
        publish_peak(&self.input_peak, peak);
    }

    fn publish_output_peak(&self, peak: f32) {
        publish_peak(&self.output_peak, peak);
    }
}

fn publish_peak(atomic: &AtomicU32, peak: f32) {
    let peak = sanitize_peak(peak);
    let mut current_bits = atomic.load(Ordering::Relaxed);

    loop {
        let current = peak_from_bits(current_bits);
        if peak <= current {
            return;
        }

        match atomic.compare_exchange_weak(
            current_bits,
            peak.to_bits(),
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(next_bits) => current_bits = next_bits,
        }
    }
}

fn sanitize_peak(peak: f32) -> f32 {
    if peak.is_finite() {
        peak.abs().clamp(0.0, MAX_METER_PEAK)
    } else {
        0.0
    }
}

fn peak_from_bits(bits: u32) -> f32 {
    sanitize_peak(f32::from_bits(bits))
}

#[cfg(test)]
mod tests {
    use super::Meters;

    #[test]
    fn meter_reports_max_peak_since_last_read() {
        let meters = Meters::default();

        meters.publish_block(0.2, 0.3);
        meters.publish_block(0.7, 0.1);

        assert!((meters.take_input_peak() - 0.7).abs() < 0.000_001);
        assert!((meters.take_output_peak() - 0.3).abs() < 0.000_001);
        assert_eq!(meters.take_input_peak(), 0.0);
    }

    #[test]
    fn clip_events_are_counted() {
        let meters = Meters::default();

        meters.publish_block(0.5, 1.0);
        meters.publish_block(1.2, 0.2);

        assert_eq!(meters.input_clip_events(), 1);
        assert_eq!(meters.output_clip_events(), 1);
    }
}
