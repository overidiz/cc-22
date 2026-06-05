use super::smoothing::LinearSmoother;

const BYPASS_FADE_MS: f32 = 20.0;

#[derive(Debug, Clone)]
pub struct BypassCrossfade {
    active_mix: LinearSmoother,
}

impl Default for BypassCrossfade {
    fn default() -> Self {
        Self {
            active_mix: LinearSmoother::new(BYPASS_FADE_MS, 1.0),
        }
    }
}

impl BypassCrossfade {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.active_mix.prepare(sample_rate);
    }

    pub fn reset(&mut self, bypassed: bool) {
        self.active_mix.reset(if bypassed { 0.0 } else { 1.0 });
    }

    pub fn set_bypassed(&mut self, bypassed: bool) {
        self.active_mix.set_target(if bypassed { 0.0 } else { 1.0 });
    }

    pub fn next_active_mix(&mut self) -> f32 {
        self.active_mix.next_value()
    }

    #[inline]
    pub fn mix(&self, dry: f32, processed: f32, active_mix: f32) -> f32 {
        let amount = active_mix.clamp(0.0, 1.0);
        (dry * (1.0 - amount)) + (processed * amount)
    }
}

#[cfg(test)]
mod tests {
    use super::BypassCrossfade;

    #[test]
    fn bypass_ramps_towards_dry_signal() {
        let mut bypass = BypassCrossfade::default();
        bypass.prepare(1_000.0);
        bypass.reset(false);
        bypass.set_bypassed(true);

        let first = bypass.next_active_mix();
        assert!(first < 1.0);

        let mut last = first;
        for _ in 0..32 {
            last = bypass.next_active_mix();
        }

        assert_eq!(last, 0.0);
    }
}
