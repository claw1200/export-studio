//! A level that glides to its target a frame at a time, so a volume change is never a step.

pub struct Smoothed {
    value: f32,
    coefficient: f32,
}

impl Smoothed {
    /// `time_ms` is the time constant: the level covers about two thirds of a change in that time.
    pub fn new(initial: f32, sample_rate: u32, time_ms: f32) -> Self {
        let frames = (time_ms / 1000.0 * sample_rate.max(1) as f32).max(1.0);
        Self {
            value: initial,
            coefficient: 1.0 - (-1.0 / frames).exp(),
        }
    }

    pub fn step(&mut self, target: f32) -> f32 {
        self.value += (target - self.value) * self.coefficient;
        if (target - self.value).abs() < 1e-6 {
            self.value = target;
        }
        self.value
    }
}
