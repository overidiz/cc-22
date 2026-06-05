#[derive(Debug, Clone, Copy, Default)]
pub struct GainStage;

impl GainStage {
    #[inline]
    pub fn db_to_gain(&self, db: f32) -> f32 {
        db_to_gain(db)
    }

    #[inline]
    pub fn apply(&self, sample: f32, gain: f32) -> f32 {
        sample * gain
    }
}

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::db_to_gain;

    #[test]
    fn converts_db_to_linear_gain() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 0.000_001);
        assert!((db_to_gain(6.0) - 1.995_262).abs() < 0.000_01);
        assert!((db_to_gain(-6.0) - 0.501_187).abs() < 0.000_01);
    }
}
