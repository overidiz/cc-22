#[derive(Debug, Clone)]
pub struct LinearSmoother {
    sample_rate: f32,
    time_ms: f32,
    current: f32,
    target: f32,
    step: f32,
    samples_remaining: u32,
}

impl LinearSmoother {
    pub fn new(time_ms: f32, initial_value: f32) -> Self {
        Self {
            sample_rate: 44_100.0,
            time_ms,
            current: initial_value,
            target: initial_value,
            step: 0.0,
            samples_remaining: 0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    pub fn reset(&mut self, value: f32) {
        self.current = value;
        self.target = value;
        self.step = 0.0;
        self.samples_remaining = 0;
    }

    pub fn set_target(&mut self, target: f32) {
        if (target - self.target).abs() <= f32::EPSILON {
            return;
        }

        self.target = target;
        let samples = ((self.time_ms * 0.001) * self.sample_rate).round() as u32;
        self.samples_remaining = samples.max(1);
        self.step = (self.target - self.current) / self.samples_remaining as f32;
    }

    pub fn next_value(&mut self) -> f32 {
        if self.samples_remaining == 0 {
            return self.target;
        }

        self.current += self.step;
        self.samples_remaining -= 1;

        if self.samples_remaining == 0 {
            self.current = self.target;
        }

        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::LinearSmoother;

    #[test]
    fn reaches_target_after_configured_time() {
        let mut smoother = LinearSmoother::new(10.0, 0.0);
        smoother.prepare(1_000.0);
        smoother.set_target(1.0);

        let mut value = 0.0;
        for _ in 0..10 {
            value = smoother.next_value();
        }

        assert!((value - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reset_jumps_without_ramp() {
        let mut smoother = LinearSmoother::new(100.0, 0.0);
        smoother.prepare(48_000.0);
        smoother.set_target(1.0);
        smoother.reset(0.25);

        assert_eq!(smoother.next_value(), 0.25);
    }
}
