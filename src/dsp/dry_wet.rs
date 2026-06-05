const HALF_PI: f32 = core::f32::consts::FRAC_PI_2;

#[derive(Debug, Clone, Copy, Default)]
pub struct DryWet;

impl DryWet {
    #[inline]
    pub fn mix(&self, dry: f32, wet: f32, amount: f32) -> f32 {
        let (dry_gain, wet_gain) = equal_power_gains(amount);
        (dry * dry_gain) + (wet * wet_gain)
    }
}

#[inline]
pub fn equal_power_gains(amount: f32) -> (f32, f32) {
    let normalized = amount.clamp(0.0, 1.0);
    let angle = normalized * HALF_PI;
    (angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::equal_power_gains;

    #[test]
    fn crossfade_endpoints_are_exact() {
        let (dry, wet) = equal_power_gains(0.0);
        assert!((dry - 1.0).abs() < f32::EPSILON);
        assert!(wet.abs() < f32::EPSILON);

        let (dry, wet) = equal_power_gains(1.0);
        assert!(dry.abs() < 0.000_001);
        assert!((wet - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn crossfade_keeps_constant_power() {
        let (dry, wet) = equal_power_gains(0.5);
        assert!(((dry * dry) + (wet * wet) - 1.0).abs() < 0.000_001);
    }
}
