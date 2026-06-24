use nih_plug::prelude::*;

use crate::params::MovementParams;

use super::{
    chain::{sanitize_sample, soft_clip_sample, ModuleCore},
    dry_wet::DryWet,
    smoothing::LinearSmoother,
    transport::{beats_for_division, hz_for_division, ms_for_division, TransportFrame},
    util::safe_feedback,
};

const MAX_CHANNELS: usize = 2;
const MAX_DELAY_SECONDS: f32 = 0.08;
const MIN_TONE_HZ: f32 = 900.0;
const MAX_TONE_HZ: f32 = 12_000.0;
const NUM_PHASER_STAGES: usize = 6;

/// The five Movement modes, in product order. Variants are identified by their
/// stable `#[id]`, so dropping the legacy modes (off/chorus) and reordering the
/// rest keeps state compatibility for every id that remains.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementMode {
    #[id = "doubler"]
    #[name = "Doubler"]
    Doubler,

    #[id = "vibrato"]
    Vibrato,

    #[id = "phaser"]
    #[name = "Phaser"]
    Phaser,

    #[id = "tremolo"]
    Tremolo,

    #[id = "pitch"]
    #[name = "Pitch"]
    Pitch,
}

impl MovementMode {
    /// The five product modes, in the exact order shown in the UI.
    pub const PRODUCT_MODES: [MovementMode; 5] = [
        MovementMode::Doubler,
        MovementMode::Vibrato,
        MovementMode::Phaser,
        MovementMode::Tremolo,
        MovementMode::Pitch,
    ];
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
    last_ppq: Option<f64>,
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
            last_ppq: None,
            previous_shape: LfoShape::Sine,
            target_shape: LfoShape::Sine,
            shape_crossfade: LinearSmoother::new(20.0, 1.0),
            current_mode: MovementMode::Doubler,
            mode_crossfade: LinearSmoother::new(25.0, 1.0),
            tone_state: [0.0; MAX_CHANNELS],
            last_output: [0.0; MAX_CHANNELS],
            has_processed: false,
            phaser_x_state: [[0.0; NUM_PHASER_STAGES]; MAX_CHANNELS],
            phaser_y_state: [[0.0; NUM_PHASER_STAGES]; MAX_CHANNELS],
            pitch_buffer: [Vec::new(), Vec::new()],
            pitch_write_pos: [0; MAX_CHANNELS],
            pitch_read_a: [0.0; MAX_CHANNELS],
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
        self.last_ppq = None;
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
    }

    pub fn next_frame(
        &mut self,
        params: &MovementParams,
        transport: &TransportFrame,
    ) -> MovementFrame {
        let mode = params.mode.value();
        self.set_mode(mode);
        let requested_shape = params.shape.value();
        let max_rate_hz = match mode {
            MovementMode::Vibrato => 10.0,
            MovementMode::Tremolo => 20.0,
            MovementMode::Doubler => 1.2,
            MovementMode::Phaser => 5.0,
            MovementMode::Pitch => 4.0,
        };
        // Always advance the smoothers so toggling sync never jumps the manual
        // value; tempo sync just overrides the result for this block.
        let manual_rate = params.rate.smoothed.next().clamp(0.05, max_rate_hz);
        let manual_delay = params.delay.smoothed.next().clamp(5.0, 30.0);
        let synced = params.sync_enabled.value();
        let division = params.sync_division.value();
        let rate_hz = if synced {
            // LFO modes (Vibrato/Phaser/Tremolo/Pitch) lock their rate to tempo.
            hz_for_division(transport.bpm, division).clamp(0.05, max_rate_hz)
        } else {
            manual_rate
        };
        let depth = params.depth.smoothed.next().clamp(0.0, 1.0);
        let delay_ms = if synced {
            // Doubler is delay-based; keep it inside the safe short-delay range.
            ms_for_division(transport.bpm, division).clamp(5.0, 30.0)
        } else {
            manual_delay
        };
        let feedback = safe_feedback(params.feedback.smoothed.next(), 0.58);
        let width = params.width.smoothed.next().clamp(0.0, 1.0);
        let phase_degrees = params.phase.smoothed.next().clamp(0.0, 180.0);
        let tone = params.tone.smoothed.next().clamp(0.0, 1.0);
        let mix = params.mix.smoothed.next().clamp(0.0, 1.0);
        let module_frame = self.core.next_frame(params.bypass.value(), mix, 0.0);

        // Phase lock: when the host is playing with a known position, pull the LFO
        // toward the musical grid — but with a *smooth, clamped* correction rather
        // than a hard snap, so re-locking never produces an audible jump/click. The
        // shortest wrapped error is nudged by a small fraction, bounded per update,
        // so it converges over a few buffers. If the host isn't playing or gives no
        // position, we just free-run.
        if synced && params.sync_phase_lock.value() && transport.playing {
            if let Some(ppq) = transport.ppq_position {
                if self.last_ppq != Some(ppq) {
                    let division_beats = beats_for_division(division).max(0.001) as f64;
                    let target = (ppq / division_beats).rem_euclid(1.0) as f32;
                    // Shortest signed distance on the phase circle, in [-0.5, 0.5].
                    let mut error = target - self.lfo_phase;
                    error -= error.round();
                    let step = (error * 0.25).clamp(-0.03, 0.03);
                    self.lfo_phase = (self.lfo_phase + step).rem_euclid(1.0);
                    self.last_ppq = Some(ppq);
                }
            }
        } else {
            self.last_ppq = None;
        }

        let phase = self.lfo_phase;
        let frame_shape = match mode {
            MovementMode::Vibrato => match requested_shape {
                LfoShape::SquareSmooth => LfoShape::Triangle,
                other => other,
            },
            MovementMode::Tremolo => requested_shape,
            MovementMode::Doubler | MovementMode::Phaser | MovementMode::Pitch => LfoShape::Sine,
        };
        self.set_lfo_shape(frame_shape);
        let shape_blend = self.shape_crossfade.next_value();
        let right_phase_offset = match mode {
            MovementMode::Vibrato => 0.25 + (phase_degrees / 720.0),
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
        let transport = TransportFrame::default();
        for channel_samples in buffer.iter_samples() {
            let frame = self.next_frame(params, &transport);
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
            MovementMode::Vibrato => self.process_vibrato(index, dry, frame),
            MovementMode::Tremolo => self.process_tremolo(index, dry, frame),
            MovementMode::Doubler => self.process_doubler(index, dry, frame),
            MovementMode::Phaser => self.process_phaser(index, dry, frame),
            MovementMode::Pitch => self.process_pitch(index, dry, frame),
        };

        let mixed = DryWet.mix(dry, wet, frame.mix);
        let mixed = self.smooth_mode_transition(index, mixed, frame.mode_fade);
        let output = sanitize_sample(self.core.bypass_mix(dry, mixed, frame.active_mix));
        self.last_output[index] = output;
        self.has_processed = true;
        output
    }

    fn process_vibrato(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let lfo = self.channel_lfo(index, frame);

        // A modulated delay line is a pitch shifter: the LFO sweeping the read
        // position is the vibrato. Depth is held to ±5 ms around an 8–12 ms base
        // so even at full depth the detune stays musical, never absurd.
        let base_delay_ms = 8.0 + ((1.0 - frame.depth) * 4.0);
        let mod_depth_ms = frame.depth * 5.0;
        let delay_ms = (base_delay_ms + (lfo * mod_depth_ms)).clamp(1.0, 30.0);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;

        // Cubic-interpolated read keeps the sweep smooth and click-free.
        let delayed = self.delay.process_cubic(index, sample, delay_samples);

        sanitize_sample(delayed * vibrato_level_compensation(frame.depth))
    }

    fn process_tremolo(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);

        // Width blends from a shared (mono) LFO to the phase-offset (stereo)
        // LFO, so width sets *how much* stereo tremolo there is while the Phase
        // param sets the L/R offset. At width 0 both channels pulse together.
        let mono_lfo = frame.lfo_left;
        let stereo_lfo = if index == 0 {
            frame.lfo_left
        } else {
            frame.lfo_right
        };
        let lfo = mono_lfo * (1.0 - frame.width) + stereo_lfo * frame.width;

        // Amplitude-modulation law: a smooth depth dip that never fully kills the
        // signal (floor 0.05), so deep settings read as musical tremolo rather
        // than a hard gate.
        let modulation = ((lfo + 1.0) * 0.5).clamp(0.0, 1.0);
        let gain = (1.0 - frame.depth * modulation * 0.95).clamp(0.05, 1.0);

        sanitize_sample(sample * gain)
    }

    fn process_doubler(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);

        // Width-scaled motion. When centred both voices share the same LFO (so
        // the mono sum stays coherent); as width opens, a fraction of the
        // opposite channel's LFO is blended in, making the modulation irregular
        // and decorrelated rather than a pure single-rate vibrato.
        let base_lfo = self.channel_lfo(index, frame);
        let cross_lfo = if index == 0 {
            frame.lfo_right
        } else {
            frame.lfo_left
        };
        let lfo = base_lfo + (cross_lfo - base_lfo) * (frame.width * 0.25);

        // Base delay: 8–35 ms — the "second take" time.
        let base_delay_ms = frame.delay_ms.clamp(8.0, 35.0);

        // Subtle, slow micro-detune (±depth·1.6 ms) so it reads as a second
        // performance, not an obvious vibrato.
        let mod_ms = lfo * frame.depth * 1.6;

        // Right voice spreads behind the left with width, decorrelating the two
        // delays so the stereo image widens and the mono sum lands on two comb
        // frequencies instead of one deep notch.
        let stereo_extra = if index == 0 { 0.0 } else { frame.width * 11.0 };

        let delay_ms = (base_delay_ms + mod_ms + stereo_extra).clamp(5.0, 50.0);
        let delay_samples = delay_ms * 0.001 * self.sample_rate;

        // No feedback — clean doubling without resonance.
        let delayed = self.delay.process(index, sample, delay_samples, 0.0);

        // Tone shaping on the doubled signal.
        let toned = self.apply_tone(index, delayed, frame.tone_alpha);

        sanitize_sample(toned * doubler_level_compensation(frame.depth, frame.width))
    }

    fn process_phaser(&mut self, channel: usize, sample: f32, frame: &MovementFrame) -> f32 {
        let index = channel.min(MAX_CHANNELS - 1);
        let lfo = self.channel_lfo(index, frame);

        // Allpass coefficient: modulated by LFO, sweeping the notch frequency
        let a = (frame.depth * 0.78 * lfo).clamp(-0.92, 0.92);

        // Feedback: wrap the last stage output back to the first stage input.
        // The returned signal is soft-clipped so high feedback resonates richly
        // without spiking into aggressive peaks (analog-style self-limiting).
        let feedback = safe_feedback(frame.feedback * 0.85, 0.82);
        let feedback_signal = self.phaser_y_state[index][NUM_PHASER_STAGES - 1];

        let mut input = sanitize_sample(sample + soft_clip_sample(feedback_signal * feedback));

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

        // Detune ratio from depth + LFO. The amount is held to a musical
        // ±70 cents (well under a semitone) so it reads as a microshift / pitch
        // motion and the rotating read heads never have to race the write
        // pointer fast enough to glitch.
        let cents = frame.depth * 70.0 * lfo;
        let read_speed = 2.0_f32.powf(cents / 1200.0);

        // Two read heads, half a buffer apart, both read with modulo indexing so
        // they can never fall outside the buffer.
        let ra = self.pitch_read_a[index];
        let da = dist_to_write(ra, write_pos, buf_len);
        let rb = (ra + buf_len as f32 / 2.0) % buf_len as f32;
        let db = dist_to_write(rb, write_pos, buf_len);

        // Raised-sine windows that vanish at the write pointer (distance → 0),
        // so each head is silent exactly where it would otherwise read the
        // write-pointer discontinuity — no click at the wrap. Because the two
        // heads are half a buffer apart, the windows are sin and cos of the same
        // angle, so their squares sum to one: an equal-power crossfade that never
        // dips and never combs.
        let wa = (core::f32::consts::PI * da / buf_len as f32).sin();
        let wb = (core::f32::consts::PI * db / buf_len as f32).sin();

        let sa = read_interpolated_pitch(buffer, ra, buf_len);
        let sb = read_interpolated_pitch(buffer, rb, buf_len);
        let wet = sanitize_sample(sa * wa + sb * wb);

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
        let feedback = safe_feedback(feedback, 0.58);
        buffer[write_pos] = sanitize_sample(input + (delayed * feedback));
        self.write_positions[channel] = (write_pos + 1) % len;

        sanitize_sample(delayed)
    }

    /// Feedback-free read with cubic interpolation, for clean modulated delay.
    fn process_cubic(&mut self, channel: usize, input: f32, delay_samples: f32) -> f32 {
        let buffer = &mut self.buffers[channel];
        if buffer.is_empty() {
            return input;
        }

        let len = buffer.len();
        let write_pos = self.write_positions[channel];
        let delayed = read_cubic(
            buffer,
            write_pos,
            delay_samples.clamp(1.0, len as f32 - 3.0),
        );
        buffer[write_pos] = sanitize_sample(input);
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

/// 4-point Catmull-Rom (cubic) interpolated read. Higher quality than linear for
/// a continuously modulated delay (Vibrato): less high-frequency loss and fewer
/// gritty artifacts when the read position sweeps.
#[inline]
fn read_cubic(buffer: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
    let len = buffer.len();
    let mut read_pos = write_pos as f32 - delay_samples;
    while read_pos < 0.0 {
        read_pos += len as f32;
    }

    let i1 = read_pos.floor() as usize % len;
    let frac = read_pos - read_pos.floor();
    let i0 = (i1 + len - 1) % len;
    let i2 = (i1 + 1) % len;
    let i3 = (i1 + 2) % len;

    let y0 = buffer[i0];
    let y1 = buffer[i1];
    let y2 = buffer[i2];
    let y3 = buffer[i3];

    let a = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
    let b = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c = -0.5 * y0 + 0.5 * y2;
    sanitize_sample(((a * frac + b) * frac + c) * frac + y1)
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
    use crate::dsp::transport::{NoteDivision, TransportFrame};
    use crate::params::MovementParams;
    use nih_plug::prelude::{BoolParam, EnumParam};

    #[test]
    fn movement_sync_locks_doubler_delay_to_tempo() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let mut params = MovementParams::default();
        params.mode = EnumParam::new("m", MovementMode::Doubler);
        params.sync_enabled = BoolParam::new("s", true);
        params.sync_division = EnumParam::new("d", NoteDivision::Sixteenth);
        params.reset_smoothers();

        let transport = TransportFrame {
            bpm: 120.0,
            ..TransportFrame::default()
        };
        // 1/16 @ 120 BPM = 125 ms, clamped to the Doubler's safe 30 ms ceiling.
        let synced = movement.next_frame(&params, &transport);
        assert!(
            (synced.delay_ms - 30.0).abs() < 0.01,
            "synced doubler delay {}",
            synced.delay_ms
        );

        // Sync off returns to the manual short-delay range, finite, no panic.
        params.sync_enabled = BoolParam::new("s", false);
        params.reset_smoothers();
        let manual = movement.next_frame(&params, &transport);
        assert!((5.0..=30.0).contains(&manual.delay_ms) && manual.delay_ms.is_finite());
    }

    #[test]
    fn movement_phase_lock_aligns_lfo_to_transport() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let mut params = MovementParams::default();
        params.mode = EnumParam::new("m", MovementMode::Vibrato);
        params.sync_enabled = BoolParam::new("s", true);
        params.sync_phase_lock = BoolParam::new("l", true);
        params.sync_division = EnumParam::new("d", NoteDivision::Quarter);
        params.reset_smoothers();

        // Smooth lock: a single buffer must NOT hard-snap to the grid phase — the
        // correction is clamped, so after one update it's still far from target.
        let first = TransportFrame {
            bpm: 120.0,
            playing: true,
            ppq_position: Some(2.25),
            ..TransportFrame::default()
        };
        let _ = movement.next_frame(&params, &first);
        assert!(
            (movement.lfo_phase - 0.25).abs() > 0.05,
            "phase lock must not hard-snap in one step (phase {})",
            movement.lfo_phase
        );

        // Over several buffers (1/4 @ 120 → integer beats keep target phase 0.25)
        // it converges smoothly toward the grid.
        for k in 1..60 {
            let playing = TransportFrame {
                bpm: 120.0,
                playing: true,
                ppq_position: Some(2.25 + k as f64),
                ..TransportFrame::default()
            };
            let _ = movement.next_frame(&params, &playing);
        }
        assert!(
            (movement.lfo_phase - 0.25).abs() < 0.05,
            "phase-locked LFO should converge to the grid, phase {}",
            movement.lfo_phase
        );

        // When stopped, it must NOT snap (free-runs from where it was).
        let before = movement.lfo_phase;
        let stopped = TransportFrame {
            bpm: 120.0,
            playing: false,
            ppq_position: Some(9.0),
            ..TransportFrame::default()
        };
        let _ = movement.next_frame(&params, &stopped);
        assert!(
            (movement.lfo_phase - before).abs() < 0.02,
            "phase should free-run when stopped"
        );
    }

    #[test]
    fn movement_sync_tremolo_stays_finite_across_divisions() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let transport = TransportFrame {
            bpm: 128.0,
            ..TransportFrame::default()
        };
        for division in [
            NoteDivision::Quarter,
            NoteDivision::Eighth,
            NoteDivision::SixteenthTriplet,
            NoteDivision::DottedEighth,
        ] {
            let mut params = MovementParams::default();
            params.mode = EnumParam::new("m", MovementMode::Tremolo);
            params.sync_enabled = BoolParam::new("s", true);
            params.sync_division = EnumParam::new("d", division);
            params.reset_smoothers();
            for _ in 0..4_000 {
                let frame = movement.next_frame(&params, &transport);
                let out = movement.process_sample_for_channel(0, 0.5, &frame);
                assert!(
                    out.is_finite() && out.abs() <= 2.0,
                    "tremolo sync {division:?} produced {out}"
                );
            }
        }
    }

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

    fn vibrato_frame(depth: f32, width: f32, lfo_left: f32, lfo_right: f32) -> MovementFrame {
        MovementFrame {
            mode: MovementMode::Vibrato,
            depth,
            delay_ms: 16.0,
            feedback: 0.0,
            width,
            mix: 1.0,
            active_mix: 1.0,
            lfo_left,
            lfo_right,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn vibrato_high_depth_sine_has_no_click() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let mut sig_phase = 0.0_f32;
        let mut lfo_phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..48_000 {
            lfo_phase = (lfo_phase + 5.0 / 48_000.0).fract();
            let lfo = (lfo_phase * tau).sin();
            let frame = vibrato_frame(1.0, 0.0, lfo, lfo);
            sig_phase += 440.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.4;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "vibrato: NaN/inf");
            peak = peak.max(output.abs());
            if index > 4096 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(peak < 1.0, "vibrato peak should be controlled, {peak}");
        assert!(
            max_step < 0.15,
            "vibrato should not click, max step {max_step}"
        );
    }

    #[test]
    fn vibrato_depth_automation_has_no_zipper() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let total = 48_000usize;
        let mut sig_phase = 0.0_f32;
        let mut lfo_phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..total {
            let depth = index as f32 / (total - 1) as f32;
            lfo_phase = (lfo_phase + 6.0 / 48_000.0).fract();
            let lfo = (lfo_phase * tau).sin();
            let frame = vibrato_frame(depth, 0.0, lfo, lfo);
            sig_phase += 330.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.35;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 4096 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            max_step < 0.2,
            "vibrato depth automation should not zipper, max step {max_step}"
        );
    }

    #[test]
    fn vibrato_stereo_phase_does_not_break_mono() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let mut sig_phase = 0.0_f32;
        let mut lfo_phase = 0.0_f32;
        let mut in_sq = 0.0_f64;
        let mut mono_sq = 0.0_f64;
        let mut count = 0u32;
        for index in 0..8192 {
            lfo_phase = (lfo_phase + 5.0 / 48_000.0).fract();
            let left_lfo = (lfo_phase * tau).sin();
            let right_lfo = ((lfo_phase + 0.25).fract() * tau).sin(); // 90° apart
            let frame = vibrato_frame(0.6, 1.0, left_lfo, right_lfo);
            sig_phase += 440.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.3;
            let left = movement.process_sample_for_channel(0, input, &frame);
            let right = movement.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());
            if index > 4096 {
                let mono = (left + right) * 0.5;
                in_sq += (input as f64) * (input as f64);
                mono_sq += (mono as f64) * (mono as f64);
                count += 1;
            }
        }
        let in_rms = (in_sq / count as f64).sqrt() as f32;
        let mono_rms = (mono_sq / count as f64).sqrt() as f32;
        assert!(
            mono_rms > in_rms * 0.4,
            "vibrato stereo phase should not collapse mono (in {in_rms}, mono {mono_rms})"
        );
    }

    #[test]
    fn tremolo_modulates_level_without_pitch_delay() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        // Full width so the stereo (phase-offset) LFO drives L and R apart.
        let frame = MovementFrame {
            mode: MovementMode::Tremolo,
            depth: 1.0,
            delay_ms: 16.0,
            feedback: 0.0,
            width: 1.0,
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

    fn tremolo_frame(depth: f32, width: f32, lfo_left: f32, lfo_right: f32) -> MovementFrame {
        MovementFrame {
            mode: MovementMode::Tremolo,
            depth,
            delay_ms: 16.0,
            feedback: 0.0,
            width,
            mix: 1.0,
            active_mix: 1.0,
            lfo_left,
            lfo_right,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn tremolo_square_max_depth_has_no_click() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let mut lfo_phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..48_000 {
            lfo_phase = (lfo_phase + 6.0 / 48_000.0).fract();
            let lfo = square_smooth_lfo(lfo_phase);
            let frame = tremolo_frame(1.0, 0.0, lfo, lfo);
            // Steady input exposes the gain edges directly.
            let output = movement.process_sample_for_channel(0, 0.5, &frame);
            assert!(output.is_finite());
            if index > 0 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            max_step < 0.1,
            "smoothed square tremolo should not click, max step {max_step}"
        );
    }

    #[test]
    fn tremolo_rate_automation_has_no_zipper() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let total = 48_000usize;
        let mut lfo_phase = 0.0_f32;
        let mut sig_phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..total {
            let t = index as f32 / (total - 1) as f32;
            let rate = 2.0 + t * 16.0; // 2 -> 18 Hz
            lfo_phase = (lfo_phase + rate / 48_000.0).fract();
            let lfo = (lfo_phase * tau).sin();
            let frame = tremolo_frame(0.7, 0.0, lfo, lfo);
            sig_phase += 440.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.4;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 0 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            max_step < 0.15,
            "tremolo rate automation should not zipper, max step {max_step}"
        );
    }

    #[test]
    fn tremolo_mix_zero_dry_mix_one_tremolo() {
        let mut dry_movement = Movement::default();
        dry_movement.prepare(48_000.0);
        let mut wet_movement = Movement::default();
        wet_movement.prepare(48_000.0);

        // Deep tremolo at its dip (lfo = 1.0 -> gain 0.05).
        let mut dry_frame = tremolo_frame(0.8, 0.0, 1.0, 1.0);
        dry_frame.mix = 0.0;
        let wet_frame = tremolo_frame(0.8, 0.0, 1.0, 1.0);

        let mut max_dry_diff = 0.0_f32;
        let mut wet_diff = 0.0_f32;
        for _ in 0..1024 {
            let input = 0.4;
            let dry = dry_movement.process_sample_for_channel(0, input, &dry_frame);
            let wet = wet_movement.process_sample_for_channel(0, input, &wet_frame);
            max_dry_diff = max_dry_diff.max((dry - input).abs());
            wet_diff = wet_diff.max((wet - input).abs());
        }
        assert!(
            max_dry_diff < 1e-4,
            "tremolo mix 0 should be dry, {max_dry_diff}"
        );
        assert!(
            wet_diff > 0.1,
            "tremolo mix 1 should modulate level, {wet_diff}"
        );
    }

    #[test]
    fn tremolo_stereo_width_mono_sum_is_acceptable() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let mut lfo_phase = 0.0_f32;
        let mut sig_phase = 0.0_f32;
        let mut in_sq = 0.0_f64;
        let mut mono_sq = 0.0_f64;
        let mut count = 0u32;
        for _ in 0..8192 {
            lfo_phase = (lfo_phase + 5.0 / 48_000.0).fract();
            let left_lfo = (lfo_phase * tau).sin();
            let right_lfo = ((lfo_phase + 0.5).fract() * tau).sin(); // anti-phase
            let frame = tremolo_frame(0.9, 1.0, left_lfo, right_lfo);
            sig_phase += 440.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.3;
            let left = movement.process_sample_for_channel(0, input, &frame);
            let right = movement.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());
            let mono = (left + right) * 0.5;
            in_sq += (input as f64) * (input as f64);
            mono_sq += (mono as f64) * (mono as f64);
            count += 1;
        }
        let in_rms = (in_sq / count as f64).sqrt() as f32;
        let mono_rms = (mono_sq / count as f64).sqrt() as f32;
        // Amplitude modulation can't phase-cancel, so the mono sum stays healthy.
        assert!(
            mono_rms > in_rms * 0.3,
            "tremolo stereo width should keep mono sum acceptable (in {in_rms}, mono {mono_rms})"
        );
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
    fn doubler_differs_from_vibrato() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let vibrato_frame = MovementFrame {
            mode: MovementMode::Vibrato,
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
        // Warm up: doubler and vibrato use different delay/modulation paths
        for _ in 0..1200 {
            phase += 600.0 / 48_000.0;
            let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
            movement.process_sample_for_channel(0, sig, &vibrato_frame);
            movement.process_sample_for_channel(0, sig, &doubler_frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
        let vibrato_out = movement.process_sample_for_channel(0, sig, &vibrato_frame);
        let doubler_out = movement.process_sample_for_channel(0, sig, &doubler_frame);
        assert!(vibrato_out.is_finite() && doubler_out.is_finite());
        assert!(
            (vibrato_out - doubler_out).abs() > 0.000_01,
            "Doubler and Vibrato should differ, vb={vibrato_out}, db={doubler_out}"
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

    #[test]
    fn doubler_mono_sum_does_not_collapse() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = doubler_frame(0.3, 18.0, 1.0, 0.5); // wide, 50% blend

        let mut phase = 0.0_f32;
        let mut in_sq = 0.0_f64;
        let mut mono_sq = 0.0_f64;
        let mut count = 0u32;
        for index in 0..8192 {
            phase += 440.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let left = movement.process_sample_for_channel(0, input, &frame);
            let right = movement.process_sample_for_channel(1, input, &frame);
            if index > 2048 {
                let mono = (left + right) * 0.5;
                in_sq += (input as f64) * (input as f64);
                mono_sq += (mono as f64) * (mono as f64);
                count += 1;
            }
        }
        let in_rms = (in_sq / count as f64).sqrt() as f32;
        let mono_rms = (mono_sq / count as f64).sqrt() as f32;
        assert!(
            mono_rms > in_rms * 0.45,
            "doubler mono sum should not collapse (in {in_rms}, mono {mono_rms})"
        );
    }

    #[test]
    fn doubler_sine_has_no_absurd_peaks() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = doubler_frame(1.0, 30.0, 1.0, 1.0);

        let mut phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..8192 {
            phase += 220.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.4;
            let left = movement.process_sample_for_channel(0, input, &frame);
            let right = movement.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());
            peak = peak.max(left.abs()).max(right.abs());
        }
        assert!(peak < 1.0, "doubler sine peak should be controlled, {peak}");
    }

    #[test]
    fn doubler_mix_zero_dry_mix_one_wet() {
        let mut dry_movement = Movement::default();
        dry_movement.prepare(48_000.0);
        let mut wet_movement = Movement::default();
        wet_movement.prepare(48_000.0);
        let dry_frame = doubler_frame(0.5, 18.0, 1.0, 0.0);
        let wet_frame = doubler_frame(0.5, 18.0, 1.0, 1.0);

        let mut phase = 0.0_f32;
        let mut max_dry_diff = 0.0_f32;
        let mut wet_diff = 0.0_f32;
        for _ in 0..2048 {
            phase += 440.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.3;
            let dry = dry_movement.process_sample_for_channel(0, input, &dry_frame);
            let wet = wet_movement.process_sample_for_channel(0, input, &wet_frame);
            max_dry_diff = max_dry_diff.max((dry - input).abs());
            wet_diff = wet_diff.max((wet - input).abs());
        }
        assert!(
            max_dry_diff < 1e-4,
            "doubler mix 0 should be dry, {max_dry_diff}"
        );
        assert!(
            wet_diff > 0.001,
            "doubler mix 1 should be processed, {wet_diff}"
        );
    }

    #[test]
    fn doubler_depth_delay_sweep_has_no_click() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let total = 48_000usize;
        let mut phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..total {
            let t = index as f32 / (total - 1) as f32;
            let frame = doubler_frame(t, 8.0 + t * 27.0, 0.6, 1.0);
            phase += 330.0 / 48_000.0;
            let input = (phase * core::f32::consts::TAU).sin() * 0.35;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "doubler sweep: NaN/inf");
            peak = peak.max(output.abs());
            if index > 0 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(peak < 1.0, "doubler sweep peak {peak}");
        assert!(
            max_step < 0.2,
            "doubler sweep should not click, max step {max_step}"
        );
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
    fn phaser_differs_from_vibrato() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let vibrato_frame = MovementFrame {
            mode: MovementMode::Vibrato,
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
            movement.process_sample_for_channel(0, sig, &vibrato_frame);
            movement.process_sample_for_channel(0, sig, &phaser_frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
        let vibrato_out = movement.process_sample_for_channel(0, sig, &vibrato_frame);
        let phaser_out = movement.process_sample_for_channel(0, sig, &phaser_frame);
        assert!(vibrato_out.is_finite() && phaser_out.is_finite());
        assert!(
            (vibrato_out - phaser_out).abs() > 0.000_01,
            "Phaser and Vibrato should differ"
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

    fn phaser_frame_lfo(
        depth: f32,
        feedback: f32,
        width: f32,
        mix: f32,
        lfo_left: f32,
        lfo_right: f32,
    ) -> MovementFrame {
        MovementFrame {
            mode: MovementMode::Phaser,
            depth,
            delay_ms: 16.0,
            feedback,
            width,
            mix,
            active_mix: 1.0,
            lfo_left,
            lfo_right,
            tone_alpha: 1.0,
            mode_fade: 1.0,
        }
    }

    #[test]
    fn phaser_sine_sweep_has_no_nan() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let mut lfo_phase = 0.0_f32;
        let mut sig_phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            lfo_phase = (lfo_phase + 1.0 / 48_000.0).fract();
            let lfo = (lfo_phase * tau).sin();
            let frame = phaser_frame_lfo(0.8, 0.5, 1.0, 1.0, lfo, lfo);
            sig_phase += 330.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.35;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "phaser sweep: NaN/inf");
            assert!(output.abs() <= 8.0);
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 2.0,
            "phaser sine sweep peak should be controlled, {peak}"
        );
    }

    #[test]
    fn phaser_rate_depth_sweep_has_no_click() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let total = 48_000usize;
        let mut lfo_phase = 0.0_f32;
        let mut sig_phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        let mut peak = 0.0_f32;
        for index in 0..total {
            let t = index as f32 / (total - 1) as f32;
            lfo_phase = (lfo_phase + (0.5 + t * 4.0) / 48_000.0).fract();
            let lfo = (lfo_phase * tau).sin();
            let frame = phaser_frame_lfo(0.3 + t * 0.6, 0.4, 0.5, 1.0, lfo, lfo);
            sig_phase += 220.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.35;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
            if index > 0 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(peak < 2.0, "phaser sweep peak {peak}");
        assert!(
            max_step < 0.2,
            "phaser sweep should not click, max step {max_step}"
        );
    }

    #[test]
    fn phaser_mono_compatibility_is_acceptable() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let mut lfo_phase = 0.0_f32;
        let mut sig_phase = 0.0_f32;
        let mut in_sq = 0.0_f64;
        let mut mono_sq = 0.0_f64;
        let mut count = 0u32;
        for index in 0..8192 {
            lfo_phase = (lfo_phase + 1.0 / 48_000.0).fract();
            let left_lfo = (lfo_phase * tau).sin();
            let right_lfo = ((lfo_phase + 0.2).fract() * tau).sin();
            let frame = phaser_frame_lfo(0.7, 0.4, 1.0, 0.5, left_lfo, right_lfo);
            sig_phase += 440.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.3;
            let left = movement.process_sample_for_channel(0, input, &frame);
            let right = movement.process_sample_for_channel(1, input, &frame);
            assert!(left.is_finite() && right.is_finite());
            if index > 2048 {
                let mono = (left + right) * 0.5;
                in_sq += (input as f64) * (input as f64);
                mono_sq += (mono as f64) * (mono as f64);
                count += 1;
            }
        }
        let in_rms = (in_sq / count as f64).sqrt() as f32;
        let mono_rms = (mono_sq / count as f64).sqrt() as f32;
        assert!(
            mono_rms > in_rms * 0.4,
            "phaser mono compatibility should be acceptable (in {in_rms}, mono {mono_rms})"
        );
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
    fn pitch_differs_from_vibrato() {
        let mut movement_p = Movement::default();
        movement_p.prepare(48_000.0);
        let mut movement_v = Movement::default();
        movement_v.prepare(48_000.0);

        let vibrato_frame = MovementFrame {
            mode: MovementMode::Vibrato,
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
            movement_v.process_sample_for_channel(0, sig, &vibrato_frame);
            movement_p.process_sample_for_channel(0, sig, &pitch_frame);
        }
        let sig = (phase * core::f32::consts::TAU).sin() * 0.3;
        let vb_out = movement_v.process_sample_for_channel(0, sig, &vibrato_frame);
        let pt_out = movement_p.process_sample_for_channel(0, sig, &pitch_frame);
        assert!(vb_out.is_finite() && pt_out.is_finite());

        let diff = (vb_out - pt_out).abs();
        // Either they differ, or at least both are non-zero (alive)
        assert!(
            diff > 0.000_01 || (vb_out.abs() > 0.01 && pt_out.abs() > 0.01),
            "Pitch output should be active, vb={vb_out}, pt={pt_out}"
        );
    }

    fn pitch_frame_lfo(depth: f32, width: f32, lfo_left: f32, lfo_right: f32) -> MovementFrame {
        let mut frame = pitch_frame(depth, width, 1.0);
        frame.lfo_left = lfo_left;
        frame.lfo_right = lfo_right;
        frame
    }

    #[test]
    fn pitch_max_sine_440_has_no_nan() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let mut sig_phase = 0.0_f32;
        let mut lfo_phase = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            lfo_phase = (lfo_phase + 4.0 / 48_000.0).fract();
            let lfo = (lfo_phase * tau).sin();
            let frame = pitch_frame_lfo(1.0, 0.0, lfo, lfo);
            sig_phase += 440.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.4;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite(), "pitch max: NaN/inf");
            peak = peak.max(output.abs());
        }
        assert!(
            peak < 1.5,
            "pitch max sine peak should be controlled, {peak}"
        );
    }

    #[test]
    fn pitch_impulse_has_no_dangerous_peak() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        let frame = pitch_frame_lfo(0.8, 0.0, 0.5, 0.5);

        let mut peak = 0.0_f32;
        for index in 0..8192 {
            let input = if index == 0 { 0.9 } else { 0.0 };
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
        }
        assert!(peak < 1.5, "pitch impulse peak should be safe, got {peak}");
    }

    #[test]
    fn pitch_read_head_wrap_has_no_strong_click() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);
        // Constant max detune makes read head A drift and lap the write pointer
        // repeatedly, exercising the crossfade at every wrap.
        let frame = pitch_frame_lfo(1.0, 0.0, 1.0, 1.0);

        let tau = core::f32::consts::TAU;
        let mut sig_phase = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..(48_000 * 2) {
            sig_phase += 330.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.35;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            if index > 4096 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(
            max_step < 0.2,
            "pitch read-head wrap should not click strongly, max step {max_step}"
        );
    }

    #[test]
    fn pitch_automation_stays_finite_and_bounded() {
        let mut movement = Movement::default();
        movement.prepare(48_000.0);

        let tau = core::f32::consts::TAU;
        let total = 48_000usize;
        let mut sig_phase = 0.0_f32;
        let mut lfo_phase = 0.0_f32;
        let mut peak = 0.0_f32;
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for index in 0..total {
            let depth = index as f32 / (total - 1) as f32;
            lfo_phase = (lfo_phase + 3.0 / 48_000.0).fract();
            let lfo = (lfo_phase * tau).sin();
            let frame = pitch_frame_lfo(depth, 0.0, lfo, lfo);
            sig_phase += 220.0 / 48_000.0;
            let input = (sig_phase * tau).sin() * 0.35;
            let output = movement.process_sample_for_channel(0, input, &frame);
            assert!(output.is_finite());
            peak = peak.max(output.abs());
            if index > 4096 {
                max_step = max_step.max((output - previous).abs());
            }
            previous = output;
        }
        assert!(peak < 1.5, "pitch automation peak {peak}");
        assert!(
            max_step < 0.2,
            "pitch automation should not click, max step {max_step}"
        );
    }
}
