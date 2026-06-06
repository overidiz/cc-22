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
        };
        self.set_lfo_shape(frame_shape);
        let shape_blend = self.shape_crossfade.next_value();
        let right_phase_offset = match mode {
            MovementMode::Off => 0.0,
            MovementMode::Chorus | MovementMode::Vibrato => 0.25 + (phase_degrees / 720.0),
            MovementMode::Tremolo => phase_degrees / 360.0,
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

#[cfg(test)]
mod tests {
    use super::{
        lfo_value, read_interpolated, sine_lfo, square_smooth_lfo, LfoShape, Movement,
        MovementFrame, MovementMode, StereoDelay,
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
}
