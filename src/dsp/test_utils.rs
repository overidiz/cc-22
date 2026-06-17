use nih_plug::prelude::Buffer;

pub const TEST_SAMPLE_RATE: f32 = 48_000.0;
pub const TEST_BLOCK_SAMPLES: usize = 1_024;
pub const MAX_EXPECTED_ABS_SAMPLE: f32 = 9.0;
pub const TEN_SECONDS_SAMPLES: usize = TEST_SAMPLE_RATE as usize * 10;

#[derive(Debug, Clone, Copy)]
pub enum TestSignal {
    Sine,
    Impulse,
    WhiteNoise,
    Silence,
}

impl TestSignal {
    pub const ALL: [Self; 4] = [Self::Sine, Self::Impulse, Self::WhiteNoise, Self::Silence];

    pub fn name(self) -> &'static str {
        match self {
            Self::Sine => "sine",
            Self::Impulse => "impulse",
            Self::WhiteNoise => "white-noise",
            Self::Silence => "silence",
        }
    }

    pub fn render(self, samples: usize, sample_rate: f32) -> [Vec<f32>; 2] {
        match self {
            Self::Sine => sine_wave(samples, sample_rate, 440.0, 0.35),
            Self::Impulse => impulse(samples, 0.9),
            Self::WhiteNoise => white_noise(samples, 0.18),
            Self::Silence => silence(samples),
        }
    }
}

pub fn with_stereo_buffer(samples: &mut [Vec<f32>; 2], process: impl FnOnce(&mut Buffer<'_>)) {
    let num_samples = samples[0].len();
    assert_eq!(samples[1].len(), num_samples);

    let (left, right) = samples.split_at_mut(1);
    let mut buffer = Buffer::default();
    unsafe {
        buffer.set_slices(num_samples, |output_slices| {
            *output_slices = vec![left[0].as_mut_slice(), right[0].as_mut_slice()];
        });
    }

    process(&mut buffer);
}

pub fn process_test_signal(
    signal: [Vec<f32>; 2],
    process: impl FnOnce(&mut Buffer<'_>),
) -> [Vec<f32>; 2] {
    let mut audio = signal;
    with_stereo_buffer(&mut audio, process);
    audio
}

pub fn render_mode_for_seconds(
    seconds: f32,
    sample_rate: f32,
    make_signal: impl FnOnce(usize, f32) -> [Vec<f32>; 2],
    process: impl FnOnce(&mut Buffer<'_>),
) -> [Vec<f32>; 2] {
    let samples = (seconds * sample_rate).round() as usize;
    process_test_signal(make_signal(samples, sample_rate), process)
}

pub fn assert_audio_sane(label: &str, channels: &[Vec<f32>; 2]) {
    assert_finite_buffer(label, channels);
    assert_peak_below(label, channels, MAX_EXPECTED_ABS_SAMPLE + 0.000_1);
}

pub fn assert_finite_buffer(label: &str, channels: &[Vec<f32>; 2]) {
    for (channel_index, channel) in channels.iter().enumerate() {
        for (sample_index, sample) in channel.iter().copied().enumerate() {
            assert!(
                sample.is_finite(),
                "{label} produced non-finite sample at channel {channel_index}, sample {sample_index}: {sample}"
            );
        }
    }
}

pub fn assert_peak_below(label: &str, channels: &[Vec<f32>; 2], limit: f32) {
    for (channel_index, channel) in channels.iter().enumerate() {
        for (sample_index, sample) in channel.iter().copied().enumerate() {
            assert!(
                sample.abs() <= limit,
                "{label} exceeded peak limit {limit} at channel {channel_index}, sample {sample_index}: {sample}"
            );
        }
    }
}

pub fn max_abs_difference(left: &[Vec<f32>; 2], right: &[Vec<f32>; 2]) -> f32 {
    left.iter()
        .zip(right.iter())
        .flat_map(|(left, right)| left.iter().zip(right.iter()))
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f32::max)
}

pub fn make_sine(
    samples: usize,
    sample_rate: f32,
    frequency_hz: f32,
    amplitude: f32,
) -> [Vec<f32>; 2] {
    sine_wave(samples, sample_rate, frequency_hz, amplitude)
}

pub fn make_stereo_impulse(samples: usize) -> [Vec<f32>; 2] {
    let mut signal = silence(samples);
    if samples > 0 {
        signal[0][0] = 0.9;
        signal[1][0] = -0.45;
    }
    signal
}

pub fn make_white_noise(samples: usize, amplitude: f32) -> [Vec<f32>; 2] {
    white_noise(samples, amplitude)
}

pub fn make_stereo_different(samples: usize, sample_rate: f32) -> [Vec<f32>; 2] {
    let mut signal = sine_wave(samples, sample_rate, 440.0, 0.25);
    for (index, right) in signal[1].iter_mut().enumerate() {
        let phase = (index as f32 * 660.0 / sample_rate) * core::f32::consts::TAU;
        *right = phase.sin() * 0.17;
    }
    signal
}

pub fn peak(channels: &[Vec<f32>; 2]) -> f32 {
    channels
        .iter()
        .flat_map(|channel| channel.iter())
        .map(|sample| sample.abs())
        .fold(0.0, f32::max)
}

fn sine_wave(samples: usize, sample_rate: f32, frequency_hz: f32, amplitude: f32) -> [Vec<f32>; 2] {
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];

    for index in 0..samples {
        let phase = (index as f32 * frequency_hz / sample_rate) * core::f32::consts::TAU;
        let sample = phase.sin() * amplitude;
        left[index] = sample;
        right[index] = sample;
    }

    [left, right]
}

fn impulse(samples: usize, amplitude: f32) -> [Vec<f32>; 2] {
    let mut signal = silence(samples);
    if samples > 0 {
        signal[0][0] = amplitude;
        signal[1][0] = amplitude;
    }
    signal
}

fn white_noise(samples: usize, amplitude: f32) -> [Vec<f32>; 2] {
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];
    let mut rng = XorShift32::new(0x1234_5678);

    for index in 0..samples {
        left[index] = rng.next_bipolar() * amplitude;
        right[index] = rng.next_bipolar() * amplitude;
    }

    [left, right]
}

fn silence(samples: usize) -> [Vec<f32>; 2] {
    [vec![0.0; samples], vec![0.0; samples]]
}

#[derive(Debug, Clone, Copy)]
struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    fn next_bipolar(&mut self) -> f32 {
        let mut state = self.state;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.state = state;

        (state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}
