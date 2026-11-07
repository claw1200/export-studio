//! A one-band isolator: the low end of a deck turned down on its own, down to nothing, the way a
//! DJ mixer's isolator-mode LOW knob does. It is a level on a band, not a filter sweep: the
//! crossover never moves, only how much of the band below it comes through.
//!
//! The signal splits at [`CROSSOVER_HZ`] through a 4th-order Linkwitz-Riley crossover, two
//! cascaded Butterworth sections (Q = 1/√2) each side. The two halves are in phase and sum flat,
//! so with the low at full level the output has the input's magnitude at every frequency, and
//! with it at zero the band below the crossover falls away at 24 dB/octave.
//!
//! Splitting and summing still shifts phase around the crossover, so a deck that is not using the
//! isolator bypasses it and hears its input exactly. Going in or out of the path crossfades over a
//! few milliseconds rather than switching, so neither change clicks.

use crate::smooth::Smoothed;

/// Where the low band ends. Pioneer does not publish its isolator crossovers: the 70 Hz in DJM
/// spec sheets is the EQ mode's band centre. Measurements and DJs' reports put the isolator's LOW
/// knob acting up to about 200–300 Hz, and Denon's X1800/X1850 isolator crosses at 220 Hz. At
/// 200 Hz a Linkwitz-Riley 4th-order low cut is already 1.6 dB down at 300 Hz and 24 dB down by 100
/// Hz, which matches how a DJM kill sounds: the kick and sub go, the bass guitar's body thins.
pub const CROSSOVER_HZ: f64 = 200.0;

/// Q of each Butterworth section; two in series make a Linkwitz-Riley 4th order.
const BUTTERWORTH_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Time constant, in milliseconds, for the low level and for going in and out of the path.
const GLIDE_MS: f32 = 10.0;

/// One second-order section, transposed direct form II, in f64: at 200 Hz and audio sample rates
/// the poles sit close to the unit circle, where f32 state drifts and hisses.
#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// RBJ cookbook low- or high-pass at `hz`.
    fn new(hz: f64, q: f64, sample_rate: f64, high: bool) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * hz / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high {
            ((1.0 + cos) / 2.0, -(1.0 + cos))
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos)
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn step(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// One channel's crossover: two sections low-pass, two high-pass.
#[derive(Clone, Copy)]
struct Crossover {
    low: [Biquad; 2],
    high: [Biquad; 2],
}

impl Crossover {
    fn new(sample_rate: f64) -> Self {
        let low = Biquad::new(CROSSOVER_HZ, BUTTERWORTH_Q, sample_rate, false);
        let high = Biquad::new(CROSSOVER_HZ, BUTTERWORTH_Q, sample_rate, true);
        Self {
            low: [low; 2],
            high: [high; 2],
        }
    }

    /// The input split into the band below the crossover and the rest.
    fn split(&mut self, x: f64) -> (f64, f64) {
        let [low_a, low_b] = &mut self.low;
        let [high_a, high_b] = &mut self.high;
        let low = low_b.step(low_a.step(x));
        let high = high_b.step(high_a.step(x));
        (low, high)
    }
}

/// A deck's isolator. `target` is the low band's level, linear, 0 to 1, or `None` to leave the
/// deck's signal untouched.
pub struct Isolator {
    channels: [Crossover; 2],
    low: Smoothed,
    /// How far into the path: 0 bypassed, 1 the isolator's output alone.
    engaged: Smoothed,
}

impl Isolator {
    pub fn new(sample_rate: u32) -> Self {
        let crossover = Crossover::new(f64::from(sample_rate.max(1)));
        Self {
            channels: [crossover; 2],
            low: Smoothed::new(1.0, sample_rate, GLIDE_MS),
            engaged: Smoothed::new(0.0, sample_rate, GLIDE_MS),
        }
    }

    /// Processes interleaved stereo in place.
    pub fn process(&mut self, frames: &mut [f32], target: Option<f32>) {
        let (low_target, engaged_target) = match target {
            Some(level) => (level, 1.0),
            None => (1.0, 0.0),
        };
        for frame in frames.chunks_exact_mut(2) {
            // The crossover runs even while bypassed so its state is warm when it comes back in.
            let engaged = f64::from(self.engaged.step(engaged_target));
            let low = f64::from(self.low.step(low_target));
            for (sample, crossover) in frame.iter_mut().zip(&mut self.channels) {
                let dry = f64::from(*sample);
                let (lows, highs) = crossover.split(dry);
                if engaged > 0.0 {
                    let wet = highs + lows * low;
                    *sample = (dry + (wet - dry) * engaged) as f32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Peak level of a sine at `hz` after the isolator settles, relative to the input's.
    fn level_at(hz: f64, low: Option<f32>) -> f64 {
        let mut iso = Isolator::new(RATE);
        let frames = RATE as usize;
        let mut buf: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let s = (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE)).sin() * 0.5;
                [s as f32, s as f32]
            })
            .collect();
        iso.process(&mut buf, low);
        let tail = &buf[buf.len() / 2..];
        f64::from(tail.iter().fold(0.0_f32, |m, s| m.max(s.abs()))) / 0.5
    }

    fn db(gain: f64) -> f64 {
        20.0 * gain.log10()
    }

    #[test]
    fn bypassed_is_exactly_the_input() {
        let mut iso = Isolator::new(RATE);
        let input: Vec<f32> = (0..4096).map(|i| ((i % 300) as f32 / 300.0) - 0.5).collect();
        let mut out = input.clone();
        iso.process(&mut out, None);
        assert_eq!(out, input);
    }

    #[test]
    fn full_low_is_flat() {
        for hz in [40.0, 100.0, 200.0, 400.0, 2000.0] {
            let level = db(level_at(hz, Some(1.0)));
            assert!(level.abs() < 0.1, "{hz} Hz is {level:.2} dB with the low up");
        }
    }

    #[test]
    fn killed_low_removes_the_bass_and_keeps_the_rest() {
        assert!(db(level_at(50.0, Some(0.0))) < -45.0, "50 Hz should be gone");
        assert!(db(level_at(100.0, Some(0.0))) < -20.0, "100 Hz should be well down");
        assert!(
            (db(level_at(200.0, Some(0.0))) + 6.0).abs() < 0.5,
            "the crossover is 6 dB down"
        );
        assert!(db(level_at(1000.0, Some(0.0))).abs() < 0.1, "1 kHz is untouched");
    }

    #[test]
    fn going_into_the_path_does_not_click() {
        let mut iso = Isolator::new(RATE);
        let mut buf: Vec<f32> = (0..RATE as usize)
            .flat_map(|i| {
                let s = (2.0 * std::f64::consts::PI * 60.0 * i as f64 / f64::from(RATE)).sin();
                [s as f32 * 0.8, s as f32 * 0.8]
            })
            .collect();
        let (bypassed, rest) = buf.split_at_mut(RATE as usize);
        iso.process(bypassed, None);
        iso.process(rest, Some(1.0));
        let largest = buf
            .chunks_exact(2)
            .zip(buf.chunks_exact(2).skip(1))
            .map(|(a, b)| (b[0] - a[0]).abs())
            .fold(0.0_f32, f32::max);
        // A 60 Hz sine at 0.8 moves by at most 0.0063 a frame at 48 kHz.
        assert!(largest < 0.01, "largest step {largest}");
    }
}
