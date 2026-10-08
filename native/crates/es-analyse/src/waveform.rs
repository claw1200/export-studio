//! Loudness per waveform entry, overall and in three bands, for the engine to pack into the
//! rekordbox waveform tags.

use std::f64::consts::{FRAC_1_SQRT_2, PI};

use rayon::prelude::*;

/// Entries per second, as rekordbox's detailed waveforms use.
pub const RATE: f64 = 150.0;
const LOW_HZ: f64 = 200.0;
const HIGH_HZ: f64 = 2_000.0;
/// Samples of filter history each part replays before its first entry.
const WARMUP_SECS: f64 = 0.05;
/// Lanes per entry: peak and RMS of the full signal, then of the low, mid and high bands.
pub const LANES: usize = 8;

#[derive(Clone, Copy)]
enum Pass {
    Low,
    High,
}

/// A second-order Butterworth section (Q = 1/√2).
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    fn new(pass: Pass, sample_rate: f64, freq: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos = w0.cos();
        let alpha = w0.sin() / (2.0 * FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let (b0, b1) = match pass {
            Pass::Low => ((1.0 - cos) / 2.0, 1.0 - cos),
            Pass::High => ((1.0 + cos) / 2.0, -(1.0 + cos)),
        };
        Self {
            b: [b0 / a0, b1 / a0, b0 / a0],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

fn quantise(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `LANES` bytes per entry, `RATE` entries per second; each lane is 0 to 1 scaled to 0 to 255,
/// with RMS scaled by √2 so a sine's RMS matches its peak.
pub fn features(mono: &[f32], sample_rate: f64) -> Vec<u8> {
    let step = sample_rate / RATE;
    let count = (mono.len() as f64 / step).ceil() as usize;
    let warmup = (sample_rate * WARMUP_SECS).ceil() as usize;
    let parts = rayon::current_num_threads().max(1);
    let per_part = count.div_ceil(parts).max(1);
    let mut out = vec![0u8; count * LANES];
    out.par_chunks_mut(per_part * LANES)
        .enumerate()
        .for_each(|(part, chunk)| {
            let first = part * per_part;
            let mut low = Biquad::new(Pass::Low, sample_rate, LOW_HZ);
            let mut mid_hp = Biquad::new(Pass::High, sample_rate, LOW_HZ);
            let mut mid_lp = Biquad::new(Pass::Low, sample_rate, HIGH_HZ);
            let mut high = Biquad::new(Pass::High, sample_rate, HIGH_HZ);
            let start = (first as f64 * step).floor() as usize;
            for &x in &mono[start.saturating_sub(warmup)..start.min(mono.len())] {
                let x = f64::from(x);
                low.run(x);
                mid_lp.run(mid_hp.run(x));
                high.run(x);
            }
            for (j, entry) in chunk.chunks_mut(LANES).enumerate() {
                let k = first + j;
                let a = ((k as f64 * step).floor() as usize).min(mono.len());
                let b = (((k + 1) as f64 * step).floor() as usize).min(mono.len());
                let mut peak = [0.0f64; 4];
                let mut energy = [0.0f64; 4];
                for &x in &mono[a..b] {
                    let x = f64::from(x);
                    let v = [x, low.run(x), mid_lp.run(mid_hp.run(x)), high.run(x)];
                    for i in 0..4 {
                        peak[i] = peak[i].max(v[i].abs());
                        energy[i] += v[i] * v[i];
                    }
                }
                let n = (b - a).max(1) as f64;
                for i in 0..4 {
                    entry[i * 2] = quantise(peak[i]);
                    entry[i * 2 + 1] = quantise((energy[i] / n).sqrt() * std::f64::consts::SQRT_2);
                }
            }
        });
    out
}
