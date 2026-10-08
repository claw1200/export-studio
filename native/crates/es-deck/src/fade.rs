//! The short gain ramp every start and stop goes through.
//!
//! Audio rarely sits at zero, so starting or stopping it on an arbitrary sample is a step, and a
//! step is a click. Play, pause, seek, cue, loop entry, the end of a file and a decoder underrun
//! all go through this ramp, so sound only ever starts or stops where the gain is exactly zero.

/// Frames a start or stop takes: about 2 ms at 44.1 kHz.
pub const FADE_FRAMES: u16 = 88;

/// A linear gain counted in whole frames, so it lands on exactly 0 and exactly 1.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ramp {
    at: u16,
}

impl Ramp {
    pub const fn silent() -> Self {
        Self { at: 0 }
    }

    pub fn gain(self) -> f32 {
        f32::from(self.at) / f32::from(FADE_FRAMES)
    }

    pub fn is_silent(self) -> bool {
        self.at == 0
    }

    /// Moves one frame towards open or closed and returns that frame's gain.
    pub fn step(&mut self, open: bool) -> f32 {
        self.at = if open {
            (self.at + 1).min(FADE_FRAMES)
        } else {
            self.at.saturating_sub(1)
        };
        self.gain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_exactly_one_and_exactly_zero() {
        let mut ramp = Ramp::silent();
        for _ in 0..FADE_FRAMES {
            ramp.step(true);
        }
        assert_eq!(ramp.gain(), 1.0);
        assert_eq!(ramp.gain(), 1.0);
        for _ in 0..FADE_FRAMES {
            ramp.step(false);
        }
        assert!(ramp.is_silent());
        assert_eq!(ramp.gain(), 0.0);
    }

    #[test]
    fn never_steps_by_more_than_one_increment() {
        let mut ramp = Ramp::silent();
        let mut last = 0.0;
        for i in 0..400 {
            let g = ramp.step(i % 150 < 100);
            assert!((g - last).abs() <= 1.0 / f32::from(FADE_FRAMES) + 1e-6);
            last = g;
        }
    }
}
