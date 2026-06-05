use nih_plug::prelude::Buffer;

pub const TEST_SAMPLE_RATE: f32 = 48_000.0;
pub const TEST_BLOCK_SAMPLES: usize = 1_024;
pub const MAX_EXPECTED_ABS_SAMPLE: f32 = 8.0;

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

pub fn assert_audio_sane(label: &str, channels: &[Vec<f32>; 2]) {
    for (channel_index, channel) in channels.iter().enumerate() {
        for (sample_index, sample) in channel.iter().copied().enumerate() {
            assert!(
                sample.is_finite(),
                "{label} produced non-finite sample at channel {channel_index}, sample {sample_index}: {sample}"
            );
            assert!(
                sample.abs() <= MAX_EXPECTED_ABS_SAMPLE + 0.000_1,
                "{label} exceeded safe gain at channel {channel_index}, sample {sample_index}: {sample}"
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
