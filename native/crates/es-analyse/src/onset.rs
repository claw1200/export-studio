//! Onset strength: how sharply the spectrum rises from one frame to the next.

use crate::spectrum::Stft;

/// Frames either side the local mean is taken over, before it is subtracted.
const MEAN_REACH: usize = 16;
/// Gain inside the log, so quiet partials still count but loud ones don't swamp the rest.
const LOG_GAIN: f32 = 100.0;

/// An evenly sampled curve over the track.
#[derive(Debug, Clone)]
pub struct Envelope {
    pub values: Vec<f32>,
    /// Values per second.
    pub rate: f64,
    /// Seconds at value 0.
    pub start: f64,
}

impl Envelope {
    pub fn time(&self, index: f64) -> f64 {
        self.start + index / self.rate
    }

    pub fn index(&self, seconds: f64) -> f64 {
        (seconds - self.start) * self.rate
    }

    /// Linear interpolation at a fractional index, zero outside the curve.
    pub fn at(&self, index: f64) -> f32 {
        if index < 0.0 {
            return 0.0;
        }
        let i = index.floor() as usize;
        if i + 1 >= self.values.len() {
            return self.values.get(i).copied().unwrap_or(0.0);
        }
        let f = (index - i as f64) as f32;
        self.values[i] * (1.0 - f) + self.values[i + 1] * f
    }

    pub fn at_time(&self, seconds: f64) -> f32 {
        self.at(self.index(seconds))
    }

    pub fn duration(&self) -> f64 {
        self.values.len() as f64 / self.rate
    }
}

/// Half-wave rectified spectral flux of log magnitudes, between `lo_hz` and `hi_hz`, with the
/// local mean taken away and the peak scaled to 1.
pub fn flux(stft: &Stft, lo_hz: f64, hi_hz: f64) -> Envelope {
    let bin_hz = stft.bin_hz();
    let lo = ((lo_hz / bin_hz).floor() as usize).min(stft.bins);
    let hi = ((hi_hz / bin_hz).ceil() as usize).clamp(lo, stft.bins);
    let mut prev = vec![0.0f32; hi - lo];
    let mut raw = Vec::with_capacity(stft.frames);
    for t in 0..stft.frames {
        let frame = &stft.frame(t)[lo..hi];
        let mut sum = 0.0f32;
        for (p, &m) in prev.iter_mut().zip(frame) {
            let v = (1.0 + LOG_GAIN * m).ln();
            let rise = v - *p;
            if rise > 0.0 && t > 0 {
                sum += rise;
            }
            *p = v;
        }
        raw.push(sum);
    }
    let values = normalise(&subtract_local_mean(&raw, MEAN_REACH));
    Envelope {
        values,
        rate: stft.sample_rate / stft.hop as f64,
        start: (stft.frame / 2) as f64 / stft.sample_rate,
    }
}

pub fn subtract_local_mean(raw: &[f32], reach: usize) -> Vec<f32> {
    let n = raw.len();
    let mut prefix = vec![0.0f64; n + 1];
    for (i, &v) in raw.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(v);
    }
    (0..n)
        .map(|i| {
            let a = i.saturating_sub(reach);
            let b = (i + reach + 1).min(n);
            let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
            (f64::from(raw[i]) - mean).max(0.0) as f32
        })
        .collect()
}

pub fn normalise(values: &[f32]) -> Vec<f32> {
    let peak = values.iter().copied().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return values.to_vec();
    }
    values.iter().map(|v| v / peak).collect()
}

/// Smooths with a small triangular kernel, so a grid point a frame off still finds its onset.
pub fn smooth(env: &Envelope, reach: usize) -> Envelope {
    let n = env.values.len();
    let weights: Vec<f32> = (0..=reach * 2)
        .map(|i| (reach + 1) as f32 - (i as f32 - reach as f32).abs())
        .collect();
    let total: f32 = weights.iter().sum();
    let values = (0..n)
        .map(|i| {
            let mut sum = 0.0;
            for (j, w) in weights.iter().enumerate() {
                let k = i as isize + j as isize - reach as isize;
                if k >= 0 && (k as usize) < n {
                    sum += env.values[k as usize] * w;
                }
            }
            sum / total
        })
        .collect();
    Envelope {
        values,
        rate: env.rate,
        start: env.start,
    }
}
