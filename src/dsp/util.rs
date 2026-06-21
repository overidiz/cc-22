//! Shared DSP quality helpers — the internal standard every CC-22 mode follows.
//!
//! These are deliberately tiny, branch-light, allocation-free building blocks so
//! that the safety rules (clamped feedback, bounded Q, safe frequencies, gain
//! compensation, equal-power crossfades, DC blocking) are expressed once and
//! reused identically across Character, Movement, Diffusion, and Texture.
//!
//! Each helper is a bit-exact replacement for the inline math it replaced, so
//! adopting them does not change the sonic identity of any mode.

use super::chain::sanitize_sample;

/// Clamp a feedback coefficient to a stable range (Rule 5: every feedback path
/// must be clamped so the loop can never reach or exceed unity gain).
#[inline]
pub fn safe_feedback(feedback: f32, max: f32) -> f32 {
    feedback.clamp(0.0, max)
}

/// Clamp a resonance / Q value (Rule 7: resonant filters must have bounded Q so
/// they cannot self-oscillate uncontrollably).
#[inline]
pub fn safe_q(q: f32, min: f32, max: f32) -> f32 {
    q.clamp(min, max)
}

/// Clamp a frequency to a safe range, typically `[min, sample_rate * 0.45]`
/// (Rule 13: respect the sample rate; never run a filter at/over Nyquist).
#[inline]
pub fn safe_frequency(hz: f32, min: f32, max: f32) -> f32 {
    hz.clamp(min, max)
}

/// One-pole low-pass smoothing coefficient for `cutoff_hz` at `sample_rate`,
/// for use as `state += alpha * (input - state)` (Rules 10 & 13).
#[inline]
pub fn one_pole_alpha(cutoff_hz: f32, sample_rate: f32) -> f32 {
    (1.0 - (-core::f32::consts::TAU * cutoff_hz / sample_rate.max(1.0)).exp()).clamp(0.0, 1.0)
}

/// Equal-power (constant-energy) crossfade from `from` to `to` as `amount` goes
/// 0→1 (Rules 8 & 9: granular/reverse/delay transitions must not click or dip).
#[inline]
pub fn equal_power_crossfade(from: f32, to: f32, amount: f32) -> f32 {
    let amount = amount.clamp(0.0, 1.0);
    let angle = amount * core::f32::consts::FRAC_PI_2;
    sanitize_sample(from * angle.cos() + to * angle.sin())
}

/// Smoothstep easing on `[0, 1]` (Rule 9: smooth, click-free transitions).
#[inline]
pub fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Bounded tanh soft saturation with unity makeup at the drive point:
/// `tanh(sample * drive) / tanh(drive)` (Rules 6 & 20). `drive` must be > 0.
#[inline]
pub fn soft_saturate(sample: f32, drive: f32) -> f32 {
    (sample * drive).tanh() / drive.tanh()
}

/// One DC-blocker step. Updates the tracked DC `state` toward the input and
/// returns the DC-removed sample (not sanitized — callers sanitize as needed).
/// Keeps asymmetric saturation stages from drifting toward a DC offset.
#[inline]
pub fn dc_blocker_step(state: &mut f32, sample: f32, alpha: f32) -> f32 {
    *state += (1.0 - alpha) * (sample - *state);
    sample - *state
}

/// Inverse-power gain-compensation curve `1 / gain^exponent` (Rules 6 & 19:
/// strong saturation must be level-compensated for consistent perceived gain).
#[inline]
pub fn gain_compensation_curve(gain: f32, exponent: f32) -> f32 {
    1.0 / gain.powf(exponent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_feedback_never_reaches_unity() {
        assert_eq!(safe_feedback(2.0, 0.949), 0.949);
        assert_eq!(safe_feedback(-1.0, 0.949), 0.0);
        assert!(safe_feedback(0.949, 0.949) < 1.0);
    }

    #[test]
    fn safe_q_and_frequency_clamp_to_range() {
        assert_eq!(safe_q(10.0, 0.5, 3.8), 3.8);
        assert_eq!(safe_q(0.0, 0.5, 3.8), 0.5);
        assert_eq!(safe_frequency(50_000.0, 40.0, 20_000.0), 20_000.0);
        assert_eq!(safe_frequency(1.0, 40.0, 20_000.0), 40.0);
    }

    #[test]
    fn one_pole_alpha_is_in_unit_range_and_monotonic() {
        let low = one_pole_alpha(200.0, 48_000.0);
        let high = one_pole_alpha(8_000.0, 48_000.0);
        assert!((0.0..=1.0).contains(&low));
        assert!((0.0..=1.0).contains(&high));
        assert!(high > low, "higher cutoff should have a larger coefficient");
        // Degenerate sample rate must not divide by zero.
        assert!(one_pole_alpha(1_000.0, 0.0).is_finite());
    }

    #[test]
    fn equal_power_crossfade_preserves_endpoints_and_energy() {
        assert!((equal_power_crossfade(1.0, 0.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((equal_power_crossfade(1.0, 0.0, 1.0) - 0.0).abs() < 1e-6);
        // Equal sources at the midpoint keep energy (cos+sin at 45° = sqrt(2)).
        let mid = equal_power_crossfade(1.0, 1.0, 0.5);
        assert!((mid - core::f32::consts::SQRT_2).abs() < 1e-5);
    }

    #[test]
    fn smoothstep_hits_endpoints_and_clamps() {
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(-5.0), 0.0);
        assert_eq!(smoothstep(5.0), 1.0);
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn soft_saturate_is_bounded_and_odd() {
        let drive = 2.0_f32;
        let ceiling = 1.0 / drive.tanh();
        for x in [-100.0, -1.0, 0.0, 1.0, 100.0] {
            let y = soft_saturate(x, drive);
            assert!(y.is_finite());
            assert!(y.abs() <= ceiling + 1e-4);
        }
        assert!((soft_saturate(0.5, drive) + soft_saturate(-0.5, drive)).abs() < 1e-6);
    }

    #[test]
    fn dc_blocker_removes_a_constant_offset() {
        let mut state = 0.0;
        let mut out = 0.0;
        for _ in 0..20_000 {
            out = dc_blocker_step(&mut state, 0.5, 0.999);
        }
        assert!(
            out.abs() < 0.01,
            "DC offset should be driven toward zero: {out}"
        );
    }

    #[test]
    fn gain_compensation_curve_reduces_for_high_gain() {
        assert!((gain_compensation_curve(1.0, 0.58) - 1.0).abs() < 1e-6);
        assert!(gain_compensation_curve(4.0, 0.58) < 1.0);
        assert!(gain_compensation_curve(4.0, 0.58) > 0.0);
    }
}
