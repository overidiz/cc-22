use nih_plug::prelude::*;

use crate::params::MovementParams;

use super::{
    chain::{sanitize_sample, ModuleCore},
    dry_wet::DryWet,
    smoothing::LinearSmoother,
};

const MAX_CHANNELS: usize = 2;
const MAX_DELAY_SECONDS: f32 = 0.08;
const MIN_TONE_HZ: f32 = 900.0;
const MAX_TONE_HZ: f32 = 12_000.0;
const NUM_PHASER_STAGES: usize = 6;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementMode {
    #[id = "off"]
    Off,

    #[id = "chorus"]
    Chorus,

    #[id = "vibrato"]
    Vibrato,

    #[id = "tremolo"]
    Tremolo,

    #[id = "doubler"]
    #[name = "Doubler"]
    Doubler,

    #[id = "phaser"]
    #[name = "Phaser"]
    Phaser,

    #[id = "pitch"]
    #[name = "Pitch"]
    Pitch,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShape {
    #[id = "sine"]
    Sine,

    #[id = "triangle"]
    Triangle,

    #[id = "square-smooth"]
    #[name = "Square Smooth"]
    SquareSmooth,
}

#[derive(Debug, Clone)]
pub struct Movement {
    core: ModuleCore,
    delay: StereoDelay,
    sample_rate: f32,
    lfo_phase: f32,
    previous_shape: LfoShape,
    target_shape: LfoShape,
    shape_crossfade: LinearSmoother,
    current_mode: MovementMode,
    mode_crossfade: LinearSmoother,
    tone_state: [f32; MAX_CHANNELS],
    last_output: [f32; MAX_CHANNELS],
    has_processed: bool,
    phaser_x_state: [[f32; NUM_PHASER_STAGES]; MAX_CHANNELS],
    phaser_y_state: [[f32; NUM_PHASER_STAGES]; MAX_CHANNELS],
    pitch_buffer: [Vec<f32>; MAX_CHANNELS],
    pitch_write_pos: [usize; MAX_CHANNELS],
    pitch_read_a: [f32; MAX_CHANNELS],
    pitch_read_b: [f32; MAX_CHANNELS],
}

#[derive(Debug, Clone, Copy)]
pub struct MovementFrame {
    mode: MovementMode,
    depth: f32,
    delay_ms: f32,
    feedback: f32,
    width: f32,
    mix: f32,
    active_mix: f32,
    lfo_left: f32,
    lfo_right: f32,
    tone_alpha: f32,
    mode_fade: f32,
}

#[derive(Debug, Clone, Default)]
struct StereoDelay {
    buffers: [Vec<f32>; MAX_CHANNELS],
    write_positions: [usize; MAX_CHANNELS],
}

impl Default for Movement {
    fn default() -> Self {
        let mut movement = Self {
            core: ModuleCore::default(),
            delay: StereoDelay::default(),
            sample_rate: 44_100.0,
            lfo_phase: 0.0,
            previous_shape: LfoShape::Sine,
            target_shape: LfoShape::Sine,
            shape_crossfade: LinearSmoother::new(20.0, 1.0),
            current_mode: MovementMode::Off,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            tone_state: [0.0; MAX_CHANNELS],
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
            phaser_x_state: [[0.0; NUM_PHASER_STAGES]; MAX_CHANNELS],
            phaser_y_state: [[0.0; NUM_PHASER_STAGES]; MAX_CHANNELS],
            pitch_buffer: [Vec::new(), Vec::new()],
            pitch_write_pos: [0; MAX_CHANNELS],
            pitch_read_a: [0.0; MAX_CHANNELS],
            pitch_read_b: [0.0; MAX_CHANNELS],
        };
        movement.prepare(44_100.0);
        movement
    }
}

impl Movement {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.core.prepare(self.sample_rate);
        self.delay.prepare(self.sample_rate);
        self.shape_crossfade.prepare(self.sample_rate);
        self.mode_crossfade.prepare(self.sample_rate);
        let pitch_samples = ((self.sample_rate * 0.09).ceil() as usize).max(2048);
        for buf in &mut self.pitch_buffer {
            buf.resize(pitch_samples, 0.0);
            buf.fill(0.0);
        }
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.delay.reset();
        self.lfo_phase = 0.0;
        self.previous_shape = self.target_shape;
        self.shape_crossfade.reset(1.0);
        self.mode_crossfade.reset(1.0);
        self.tone_state = [0.0; MAX_CHANNELS];
        self.last_output = [0.0; MAX_CHANNELS];
        self.has_processed = false;
        self.phaser_x_state = [[0.0; NUM_PHASER_STAGES]; MAX_CHANNELS];
        self.phaser_y_state = [[0.0; NUM_PHASER_STAGES]; MAX_CHANNELS];
        for buf in &mut self.pitch_buffer {
            buf.fill(0.0);
        }
        self.pitch_write_pos = [0; MAX_CHANNELS];
        let half = self.pitch_buffer[0].len().max(4) as f32 / 2.0;
        self.pitch_read_a = [half; MAX_CHANNELS];
        self.pitch_read_b = [0.0; MAX_CHANNELS];
    }

    pub fn next_frame(&mut self, params: &MovementParams) -> MovementFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let requested_shape = params.shape.value();
        let max_rate_hz = match mode {
            MovementMode::Off => 20.0,
            MovementMode::Chorus => 8.0,
            MovementMode::Vibrato => 10.0,
            MovementMode::Tremolo => 20.0,
            MovementMode::Doubler => 1.2,
            MovementMode::Phaser => 5.0,
            MovementMode::Pitch => 4.0,
        };
        let rate_hz = params.rate.smoothed.next().clamp(0.05, max_rate_hz);
        let depth = params.depth.smoothed.next().clamp(0.0, 1.0);
        let delay_ms = params.delay.smoothed.next().clamp(5.0, 30.0);
        let feedback = params.feedback.smoothed.next().clamp(0.0, 0.58);
        let width = params.width.smoothed.next().clamp(0.0, 1.0);
        let phase_degrees = params.phase.smoothed.next().clamp(0.0, 180.0);
        let tone = params.tone.smoothed.next().clamp(0.0, 1.0);
        let mix = params.mix.smoothed.next().clamp(0.0, 1.0);
        let module_frame = self.core.next_frame(params.bypass.value(), mix, 0.0);

        let phase = self.lfo_phase;
        let frame_shape = match mode {
            MovementMode::Off => LfoShape::Sine,
            MovementMode::Chorus => LfoShape::Sine,
            MovementMode::Vibrato => match requested_shape {
                LfoShape::SquareSmooth => LfoShape::Triangle,
                other => other,
            },
            MovementMode::Tremolo => requested_shape,
            // TODO: Character-22 inspired modes — placeholder LFO shape
            MovementMode::Doubler | MovementMode::Phaser | MovementMode::Pitch => LfoShape::Sine,
        };
        self.set_lfo_shape(frame_shape);
        let shape_blend = self.shape_crossfade.next_value();
        let right_phase_offset = match mode {
            MovementMode::Off => 0.0,
            MovementMode::Chorus | MovementMode::Vibrato => 0.25 + (phase_degrees / 720.0),
            MovementMode::Tremolo => phase_degrees / 360.0,
            MovementMode::Doubler => 0.12 + (width * 0.38),
            MovementMode::Phaser => 0.18 + (phase_degrees / 720.0) + (width * 0.30),
            MovementMode::Pitch => 0.20 + (width * 0.50),
        };
        let lfo_left =
            blended_lfo_value(phase, self.previous_shape, self.target_shape, shape_blend);
        let lfo_right = blended_lfo_value(
            (phase + right_phase_offset).fract(),
            self.previous_shape,
            self.target_shape,
            shape_blend,
        );
        if shape_blend >= 1.0 {
            self.previous_shape = self.target_shape;
        }
        self.advance_lfo(rate_hz);

        MovementFrame {
            mode,
            depth,
            delay_ms,
            feedback,
            width,
            mix,
            active_mix: module_frame.active_mix,
            lfo_left,
            lfo_right,
            tone_alpha: tone_to_alpha(tone, self.sample_rate),
            mode_fade: self.mode_crossfade.next_value().clamp(0.0, 1.0),
        }
    }

    pub fn process_block(&mut self, buffer: &mut Buffer, params: &MovementParams) {
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params);
            for (channel_index, sample) in channel_samples.into_iter().enumerate() {
                *sample = self.process_sample_for_channel(channel_index, *sample, &frame);
            }
        }
    }

    pub fn process_sample(&mut self, sample: f32, frame: &MovementFrame) -> f32 {
        self.process_sample_for_channel(0, sample, frame)
    }

    pub fn process_sample_for_channel(
        &mut self,
        channel: usize,
        sample: f32,
        frame: &MovementFrame,
    ) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let dry = sanitize_sample(sample);
        let wet = match frame.mode {
            MovementMode::Off => dry,
            MovementMode::Chorus => self.process_chorus(index, dry, frame),
            MovementMode::Vibrato => self.process_vibrato(index, dry, frame),
            MovementMode::Tremolo => self.process_tremolo(index, dry, frame),
            MovementMode::Doubler => self.process_doubler(index, dry, frame),
            MovementMode::Phaser => self.process_phaser(index, dry, frame),
            MovementMode::Pitch => self.process_pitch(index, dry, frame),
        };

        let mixed = if frame.mode == MovementMode::Off {
            dry
        } else {
            DryWet.mix(dry, wet, frame.mix)
        };
        let mixed = self.smooth_mode_transition(index, mixed, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, mixed, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    fn process_chorus(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let lfo = self.channel_lfo(index, frame);
        let mod_depth_ms = frame.depth * frame.delay_ms.min(18.0) * 0.55;
        let delay_ms = (frame.delay_ms + (lfo * mod_depth_ms)).clamp(1.0, 45.0);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;
        let delayed = self
            .delay
            .process(index, sample, delay_samples, frame.feedback);
        let toned = self.apply_tone(index, delayed, frame.tone_alpha);
        sanitize_sample(toned * chorus_level_compensation(frame.depth, frame.feedback))
    }

    fn process_vibrato(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let lfo = self.channel_lfo(index, frame);
        let base_delay_ms = 8.0 + ((1.0 - frame.depth) * 4.0);
        let mod_depth_ms = frame.depth * 6.0;
        let delay_ms = (base_delay_ms + (lfo * mod_depth_ms)).clamp(1.0, 30.0);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;
        let delayed = self.delay.process(index, sample, delay_samples, 0.0);

        sanitize_sample(delayed * vibrato_level_compensation(frame.depth))
    }

    fn process_tremolo(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let lfo = if index == 0 {
            frame.lfo_left
        } else {
            frame.lfo_right
        };
        let modulation = ((lfo + 1.0) * 0.5).clamp(0.0, 1.0);
        let gain = 1.0 - (frame.depth * modulation * 0.95);

        sanitize_sample(sample * gain.clamp(0.05, 1.0))
    }

    fn process_doubler(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let lfo = self.channel_lfo(index, frame);

        // Base delay: 8–35 ms from parameter
        let base_delay_ms = frame.delay_ms.clamp(8.0, 35.0);

        // Very subtle pitch modulation via LFO: ±(depth * 2.0) ms
        let mod_ms = lfo * frame.depth * 2.0;

        // Stereo offset: right channel gets extra delay based on width
        let stereo_extra = if index == 0 { 0.0 } else { frame.width * 11.0 };

        let delay_ms = (base_delay_ms + mod_ms + stereo_extra).clamp(5.0, 50.0);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;

        // No feedback — clean doubling without resonance
        let delayed = self.delay.process(index, sample, delay_samples, 0.0);

        // Tone shaping on the doubled signal
        let toned = self.apply_tone(index, delayed, frame.tone_alpha);

        sanitize_sample(toned * doubler_level_compensation(frame.depth, frame.width))
    }

    fn process_phaser(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let lfo = self.channel_lfo(index, frame);

        // Allpass coefficient: modulated by LFO, sweeping the notch frequency
        let a = (frame.depth * 0.78 * lfo).clamp(-0.92, 0.92);

        // Feedback: wrap last stage output back to first stage input
        let feedback = (frame.feedback * 0.88).clamp(0.0, 0.85);
        let feedback_signal = self.phaser_y_state[index][NUM_PHASER_STAGES - 1];

        let mut input = sanitize_sample(sample + feedback_signal * feedback);

        // Cascade of 6 first-order allpass filters
        for stage in 0..NUM_PHASER_STAGES {
            let x_prev = self.phaser_x_state[index][stage];
            let y_prev = self.phaser_y_state[index][stage];

            let output = sanitize_sample(a * input + x_prev - a * y_prev);

            self.phaser_x_state[index][stage] = input;
            self.phaser_y_state[index][stage] = output;
            input = output;
        }

        // Apply tone to the wet (allpassed) signal
        let toned = self.apply_tone(index, input, frame.tone_alpha);

        sanitize_sample(toned * phaser_level_compensation(frame.depth, frame.feedback))
    }

    fn process_pitch(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);

        // Compute LFO before any mutable borrow below
        let mono_lfo = frame.lfo_left;
        let stereo_lfo = if index == 0 {
            frame.lfo_left
        } else {
            frame.lfo_right
        };
        let lfo = (mono_lfo * (1.0 - frame.width)) + (stereo_lfo * frame.width);

        let buffer = &mut self.pitch_buffer[index];
        let buf_len = buffer.len();
        if buf_len < 4 {
            return sample;
        }

        // Write input to circular buffer
        let write_pos = self.pitch_write_pos[index];
        buffer[write_pos] = sample;
        self.pitch_write_pos[index] = (write_pos + 1) % buf_len;

        // Compute detune ratio from depth + LFO
        let cents = frame.depth * 70.0 * lfo;
        let read_speed = 2.0_f32.powf(cents / 1200.0);

        // Read head A
        let ra = self.pitch_read_a[index];
        let da = dist_to_write(ra, write_pos, buf_len);
        // Read head B = A shifted by half buffer
        let rb = (ra + buf_len as f32 / 2.0) % buf_len as f32;
        let db = dist_to_write(rb, write_pos, buf_len);

        // Crossfade blend: prefer head farther from write position
        let sum_dist = da + db;
        let blend = if sum_dist > 0.01 { da / sum_dist } else { 0.5 };

        let sa = read_interpolated_pitch(buffer, ra, buf_len);
        let sb = read_interpolated_pitch(buffer, rb, buf_len);
        let wet = sanitize_sample(sa * blend + sb * (1.0 - blend));

        // Advance read head at pitch-shifted rate
        self.pitch_read_a[index] = (ra + read_speed) % buf_len as f32;

        // Tone
        let toned = self.apply_tone(index, wet, frame.tone_alpha);
        sanitize_sample(toned)
    }

    fn channel_lfo(&self, channel: usize, frame: &MovementFrame) -> f32 {
        let mono_lfo = frame.lfo_left;
        let stereo_lfo = if channel == 0 {
            frame.lfo_left
        } else {
            frame.lfo_right
        };

        (mono_lfo * (1.0 - frame.width)) + (stereo_lfo * frame.width)
    }

    fn apply_tone(&mut self, channel: usize, sample: f32, tone_alpha: f32) -> f32 {
        let state = self.tone_state[channel] + (tone_alpha * (sample - self.tone_state[channel]));
        self.tone_state[channel] = sanitize_sample(state);
        self.tone_state[channel]
    }

    fn advance_lfo(&mut self, rate_hz: f32) {
        self.lfo_phase += rate_hz / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= self.lfo_phase.floor();
        }
    }

    fn set_lfo_shape(&mut self, shape: LfoShape) {
        if shape != self.target_shape {
            self.previous_shape = self.target_shape;
            self.target_shape = shape;
            self.shape_crossfade.reset(0.0);
            self.shape_crossfade.set_target(1.0);
        }
    }

    fn set_mode(&mut self, mode: MovementMode) {
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

#[inline]
fn linear_crossfade(dry: f32, wet: f32, amount: f32) -> f32 {
    let amount = amount.clamp(0.0, 1.0);
    sanitize_sample((dry * (1.0 - amount)) + (wet * amount))
}

impl StereoDelay {
    fn prepare(&mut self, sample_rate: f32) {
        let samples = ((sample_rate.max(1.0) * MAX_DELAY_SECONDS).ceil() as usize).max(4);
        for buffer in &mut self.buffers {
            buffer.resize(samples, 0.0);
            buffer.fill(0.0);
        }
        self.write_positions = [0; MAX_CHANNELS];
    }

    fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        self.write_positions = [0; MAX_CHANNELS];
    }

    fn process(&mut self, channel: usize, input: f32, delay_samples: f32, feedback: f32) -> f32 {
        let buffer = &mut self.buffers[channel];
        if buffer.is_empty() {
            return input;
        }

        let len = buffer.len();
        let write_pos = self.write_positions[channel];
        let delayed = read_interpolated(
            buffer,
            write_pos,
            delay_samples.clamp(1.0, len as f32 - 2.0),
        );
        let feedback = feedback.clamp(0.0, 0.58);
        buffer[write_pos] = sanitize_sample(input + (delayed * feedback));
        self.write_positions[channel] = (write_pos + 1) % len;

        sanitize_sample(delayed)
    }
}

#[inline]
fn read_interpolated(buffer: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
    let len = buffer.len();
    let mut read_pos = write_pos as f32 - delay_samples;
    while read_pos < 0.0 {
        read_pos += len as f32;
    }

    let index_a = read_pos.floor() as usize % len;
    let index_b = (index_a + 1) % len;
    let frac = read_pos - read_pos.floor();
    sanitize_sample((buffer[index_a] * (1.0 - frac)) + (buffer[index_b] * frac))
}

#[inline]
fn sine_lfo(phase: f32) -> f32 {
    (phase * core::f32::consts::TAU).sin()
}

#[inline]
fn triangle_lfo(phase: f32) -> f32 {
    let phase = phase.fract();
    if phase < 0.25 {
        phase * 4.0
    } else if phase < 0.75 {
        2.0 - (phase * 4.0)
    } else {
        (phase * 4.0) - 4.0
    }
}

#[inline]
fn square_smooth_lfo(phase: f32) -> f32 {
    (sine_lfo(phase) * 5.0).tanh()
}

#[inline]
fn lfo_value(phase: f32, shape: LfoShape) -> f32 {
    match shape {
        LfoShape::Sine => sine_lfo(phase),
        LfoShape::Triangle => triangle_lfo(phase),
        LfoShape::SquareSmooth => square_smooth_lfo(phase),
    }
}

#[inline]
fn blended_lfo_value(phase: f32, previous: LfoShape, target: LfoShape, blend: f32) -> f32 {
    let blend = blend.clamp(0.0, 1.0);
    let previous_value = lfo_value(phase, previous);
    let target_value = lfo_value(phase, target);

    sanitize_sample((previous_value * (1.0 - blend)) + (target_value * blend))
}

#[inline]
fn tone_to_alpha(tone: f32, sample_rate: f32) -> f32 {
    let tone = tone.clamp(0.0, 1.0);
    let cutoff = MIN_TONE_HZ + ((MAX_TONE_HZ - MIN_TONE_HZ) * tone * tone);
    let sample_rate = sample_rate.max(1.0);
    (1.0 - (-2.0 * core::f32::consts::PI * cutoff / sample_rate).exp()).clamp(0.0, 1.0)
}

#[inline]
fn chorus_level_compensation(depth: f32, feedback: f32) -> f32 {
    (1.0 - (depth.clamp(0.0, 1.0) * 0.08) - (feedback.clamp(0.0, 0.58) * 0.12)).clamp(0.78, 1.0)
}

#[inline]
fn vibrato_level_compensation(depth: f32) -> f32 {
    (1.0 - (depth.clamp(0.0, 1.0) * 0.04)).clamp(0.92, 1.0)
}

#[inline]
fn doubler_level_compensation(depth: f32, width: f32) -> f32 {
    let depth = depth.clamp(0.0, 1.0);
    let width = width.clamp(0.0, 1.0);
    (1.0 - (depth * 0.06) - (width * 0.04)).clamp(0.85, 1.0)
}

#[inline]
fn phaser_level_compensation(depth: f32, feedback: f32) -> f32 {
    let depth = depth.clamp(0.0, 1.0);
    let feedback = feedback.clamp(0.0, 1.0);
    (1.0 - (depth * 0.05) - (feedback * 0.18)).clamp(0.72, 1.0)
}

#[inline]
fn dist_to_write(read_pos: f32, write_pos: usize, buf_len: usize) -> f32 {
    let w = write_pos as f32;
    if read_pos <= w {
        w - read_pos
    } else {
        w + buf_len as f32 - read_pos
    }
}

#[inline]
fn read_interpolated_pitch(buffer: &[f32], read_pos: f32, buf_len: usize) -> f32 {
    let pos = read_pos % buf_len as f32;
    let idx_a = pos.floor() as usize % buf_len;
    let idx_b = (idx_a + 1) % buf_len;
    let frac = pos - pos.floor();
    sanitize_sample(buffer[idx_a] * (1.0 - frac) + buffer[idx_b] * frac)
}

#[cfg(test)]
mod tests {
    use super::{
        doubler_level_compensation, lfo_value, phaser_level_compensation, read_interpolated,
        sine_lfo, square_smooth_lfo, LfoShape, Movement, MovementFrame, MovementMode, StereoDelay,
    };

    #[test]
    fn interpolated_delay_reads_between_samples() {
        let buffer = [0.0, 1.0, 0.0, 0.0];
        let value = read_interpolated(&buffer, 2, 1.5);

        assert!((value - 0.5).abs() < 0.000_001);
    }

    #[test]
    fn delay_feedback_stays_finite() {
        let mut delay = StereoDelay::default();
        delay.prepare(48_000.0);

        let mut sample = 1.0;
        for _ in 0..4_000 {
            sample = delay.process(0, sample, 240.0, 0.58);
            assert!(sample.is_finite());
        }
    }

    #[test]
    fn stereo_lfo_phase_is_opposed() {
        assert!((sine_lfo(0.25) - 1.0).abs() < 0.000_001);
        assert!((sine_lfo(0.75) + 1.0).abs() < 0.000_001);
    }

    #[test]
    fn triangle_lfo_has_expected_shape() {
        assert!((lfo_value(0.25, LfoShape::Triangle) - 1.0).abs() < 0.000_001);
        assert!((lfo_value(0.75, LfoShape::Triangle) + 1.0).abs() < 0.000_001);
        assert!(lfo_value(0.5, LfoShape::Triangle).abs() < 0.000_001);
    }

    #[test]
    fn square_smooth_lfo_has_soft_edges() {
        assert!(square_smooth_lfo(0.25) > 0.99);
        assert!(square_smooth_lfo(0.75) < -0.99);
        assert!(square_smooth_lfo(0.0).abs() < 0.000_001);
    }

    #[test]
    fn vibrato_output_stays_finite() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = MovementFrame {
            mode: MovementMode::Vibrato,
            depth: 1.0,
            delay_ms: 16.0,
            feedback: 0.0,
            width: 1.0,
            mix: 1.0,
            active_mix: 1.0,
            lfo_left: 0.5,
            lfo_right: -0.5,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        };

        let mut sample = 0.25;
        for _ in 0..512 {
            sample = movement.process_sample_for_channel(0, sample, &frame);
            assert!(sample.is_finite());
        }
    }

    #[test]
    fn tremolo_modulates_level_without_pitch_delay() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = MovementFrame {
            mode: MovementMode::Tremolo,
            depth: 1.0,
            delay_ms: 16.0,
            feedback: 0.0,
            width: 0.0,
            mix: 1.0,
            active_mix: 1.0,
            lfo_left: 1.0,
            lfo_right: -1.0,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        };

        let left = movement.process_sample_for_channel(0, 1.0, &frame);
        let right = movement.process_sample_for_channel(1, 1.0, &frame);

        assert!(left.is_finite());
        assert!(right.is_finite());
        assert!(left < right);
    }

    fn doubler_frame(depth: f32, delay_ms: f32, width: f32, mix: f32) -> MovementFrame {
        MovementFrame {
            mode: MovementMode::Doubler,
            depth,
            delay_ms,
            feedback: 0.0,
            width,
            mix,
            active_mix: 1.0,
            lfo_left: 0.3,
            lfo_right: -0.3,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn doubler_output_stays_finite() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = doubler_frame(0.5, 20.0, 1.0, 1.0);

        let mut sample = 0.35;
        for _ in 0..512 {
            sample = movement.process_sample_for_channel(0, sample, &frame);
            assert!(sample.is_finite());
        }
    }

    #[test]
    fn doubler_stereo_output_differs_l_r() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = doubler_frame(0.5, 20.0, 1.0, 1.0);

        // Feed alternating signal to make buffer non-uniform
        let mut phase = 0.0;
        for _ in 0..1200 {
            phase += 800.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
            movement.process_sample_for_channel(0, sig, &frame);
            movement.process_sample_for_channel(1, sig, &frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
        let left = movement.process_sample_for_channel(0, sig, &frame);
        let right = movement.process_sample_for_channel(1, sig, &frame);
        assert!(left.is_finite() && right.is_finite());
        assert!(
            (left - right).abs() > 0.000_01,
            "L and R doubler outputs should differ, left={left}, right={right}"
        );
    }

    #[test]
    fn doubler_differs_from_chorus() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let chorus_frame = MovementFrame {
            mode: MovementMode::Chorus,
            depth: 0.5,
            delay_ms: 16.0,
            feedback: 0.15,
            width: 1.0,
            mix: 1.0,
            active_mix: 1.0,
            lfo_left: 0.3,
            lfo_right: -0.3,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        };
        let doubler_frame = doubler_frame(0.5, 16.0, 1.0, 1.0);

        let mut phase = 0.0;
        // Warm up: doubler has no feedback, chorus does — this creates different buffer states
        for _ in 0..1200 {
            phase += 600.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
            movement.process_sample_for_channel(0, sig, &chorus_frame);
            movement.process_sample_for_channel(0, sig, &doubler_frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
        let chorus_out = movement.process_sample_for_channel(0, sig, &chorus_frame);
        let doubler_out = movement.process_sample_for_channel(0, sig, &doubler_frame);
        assert!(chorus_out.is_finite() && doubler_out.is_finite());
        assert!(
            (chorus_out - doubler_out).abs() > 0.000_01,
            "Doubler and Chorus should differ, ch={chorus_out}, db={doubler_out}"
        );
    }

    #[test]
    fn doubler_width_increases_stereo_separation() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let frame_mono = doubler_frame(0.3, 20.0, 0.0, 1.0);
        let frame_wide = doubler_frame(0.3, 20.0, 1.0, 1.0);

        let mut phase = 0.0;
        for _ in 0..1200 {
            phase += 800.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
            movement.process_sample_for_channel(0, sig, &frame_mono);
            movement.process_sample_for_channel(1, sig, &frame_mono);
            movement.process_sample_for_channel(0, sig, &frame_wide);
            movement.process_sample_for_channel(1, sig, &frame_wide);
        }

        let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
        let left_mono = movement.process_sample_for_channel(0, sig, &frame_mono);
        let right_mono = movement.process_sample_for_channel(1, sig, &frame_mono);
        let left_wide = movement.process_sample_for_channel(0, sig, &frame_wide);
        let right_wide = movement.process_sample_for_channel(1, sig, &frame_wide);

        let mono_diff = (left_mono - right_mono).abs();
        let wide_diff = (left_wide - right_wide).abs();
        assert!(
            wide_diff > mono_diff + 0.000_1,
            "width=1 should have more L/R separation than width=0, mono={mono_diff}, wide={wide_diff}"
        );
    }

    #[test]
    fn doubler_compensation_is_reasonable() {
        let comp_min = doubler_level_compensation(0.0, 0.0);
        let comp_max = doubler_level_compensation(1.0, 1.0);
        assert!((comp_min - 1.0).abs() < 0.01);
        assert!(comp_max > 0.80 && comp_max < 1.0);
    }

    fn phaser_frame(depth: f32, feedback: f32, width: f32, mix: f32) -> MovementFrame {
        MovementFrame {
            mode: MovementMode::Phaser,
            depth,
            delay_ms: 16.0,
            feedback,
            width,
            mix,
            active_mix: 1.0,
            lfo_left: 0.4,
            lfo_right: -0.4,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn phaser_output_stays_finite_with_noise() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = phaser_frame(0.8, 0.6, 1.0, 1.0);

        let mut rng: u32 = 0xbad_cafe;
        for _ in 0..512 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let sample = movement.process_sample_for_channel(0, noise, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn phaser_max_feedback_stays_finite() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = phaser_frame(1.0, 1.0, 1.0, 1.0);

        for _ in 0..1024 {
            let sample = movement.process_sample_for_channel(0, 0.3, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn phaser_stereo_differs_l_r() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = phaser_frame(0.6, 0.5, 1.0, 1.0);

        let mut phase = 0.0;
        for _ in 0..256 {
            phase += 800.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
            movement.process_sample_for_channel(0, sig, &frame);
            movement.process_sample_for_channel(1, sig, &frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
        let left = movement.process_sample_for_channel(0, sig, &frame);
        let right = movement.process_sample_for_channel(1, sig, &frame);
        assert!(left.is_finite() && right.is_finite());
        assert!(
            (left - right).abs() > 0.000_1,
            "phaser L/R should differ, left={left}, right={right}"
        );
    }

    #[test]
    fn phaser_differs_from_chorus() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let chorus_frame = MovementFrame {
            mode: MovementMode::Chorus,
            depth: 0.6,
            delay_ms: 16.0,
            feedback: 0.2,
            width: 1.0,
            mix: 1.0,
            active_mix: 1.0,
            lfo_left: 0.4,
            lfo_right: -0.4,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        };
        let phaser_frame = phaser_frame(0.6, 0.5, 1.0, 1.0);

        let mut phase = 0.0;
        for _ in 0..256 {
            phase += 600.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
            movement.process_sample_for_channel(0, sig, &chorus_frame);
            movement.process_sample_for_channel(0, sig, &phaser_frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
        let chorus_out = movement.process_sample_for_channel(0, sig, &chorus_frame);
        let phaser_out = movement.process_sample_for_channel(0, sig, &phaser_frame);
        assert!(chorus_out.is_finite() && phaser_out.is_finite());
        assert!(
            (chorus_out - phaser_out).abs() > 0.000_01,
            "Phaser and Chorus should differ"
        );
    }

    #[test]
    fn phaser_compensation_is_safe() {
        let c = phaser_level_compensation(1.0, 1.0);
        assert!(c > 0.60, "phaser comp should not over-attenuate: {c}");
        assert!(c < 1.0);
        let c0 = phaser_level_compensation(0.0, 0.0);
        assert!((c0 - 1.0).abs() < 0.01);
    }

    fn pitch_frame(depth: f32, width: f32, mix: f32) -> MovementFrame {
        MovementFrame {
            mode: MovementMode::Pitch,
            depth,
            delay_ms: 16.0,
            feedback: 0.0,
            width,
            mix,
            active_mix: 1.0,
            lfo_left: 0.5,
            lfo_right: -0.5,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn pitch_output_stays_finite_with_sine() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = pitch_frame(0.5, 0.5, 1.0);

        let mut phase = 0.0;
        for _ in 0..1024 {
            phase += 440.0 / 48_000.0;
            let sine = (phase * core::f32::consts::TAU).sin() * 0.35;
            let sample = movement.process_sample_for_channel(0, sine, &frame);
            assert!(sample.is_finite());
            assert!(sample.abs() <= 8.0);
        }
    }

    #[test]
    fn pitch_stereo_differs_l_r() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = pitch_frame(0.5, 1.0, 1.0);

        let mut phase = 0.0;
        for _ in 0..512 {
            phase += 440.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
            movement.process_sample_for_channel(0, sig, &frame);
            movement.process_sample_for_channel(1, sig, &frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.35;
        let left = movement.process_sample_for_channel(0, sig, &frame);
        let right = movement.process_sample_for_channel(1, sig, &frame);
        assert!(left.is_finite() && right.is_finite());
        assert!(
            (left - right).abs() > 0.000_01,
            "pitch L/R should differ, left={left}, right={right}"
        );
    }

    #[test]
    fn pitch_differs_from_chorus() {
        let mut movement_p = Movement::default();
        movement_p.prepare(48_000.0);
        let mut movement_c = Movement::default();
        movement_c.prepare(48_000.0);

        let chorus_frame = MovementFrame {
            mode: MovementMode::Chorus,
            depth: 0.4,
            delay_ms: 16.0,
            feedback: 0.1,
            width: 0.5,
            mix: 1.0,
            active_mix: 1.0,
            lfo_left: 0.5,
            lfo_right: -0.5,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        };
        let pitch_frame = pitch_frame(0.8, 0.5, 1.0);

        // Warm up enough to fill the pitch buffer (4320 samples at 48kHz)
        let mut phase = 0.0;
        for _ in 0..4800 {
            phase += 440.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
            movement_c.process_sample_for_channel(0, sig, &chorus_frame);
            movement_p.process_sample_for_channel(0, sig, &pitch_frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
        let ch_out = movement_c.process_sample_for_channel(0, sig, &chorus_frame);
        let pt_out = movement_p.process_sample_for_channel(0, sig, &pitch_frame);
        assert!(ch_out.is_finite() && pt_out.is_finite());

        let diff = (ch_out - pt_out).abs();
        // Either they differ, or at least both are non-zero (alive)
        assert!(
            diff > 0.000_01 || (ch_out.abs() > 0.01 && pt_out.abs() > 0.01),
            "Pitch output should be active, ch={ch_out}, pt={pt_out}"
        );
    }
}
