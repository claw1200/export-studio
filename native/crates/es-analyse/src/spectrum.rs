//! Short-time magnitude spectra, shared by the onset, downbeat and key stages.

use std::f32::consts::PI;
use std::sync::Arc;

use rayon::prelude::*;
use realfft::{RealFftPlanner, RealToComplex};

pub struct Stft {
    pub frame: usize,
    pub hop: usize,
    pub sample_rate: f64,
    /// `frames * bins` magnitudes, frame-major.
    pub mags: Vec<f32>,
    pub bins: usize,
    pub frames: usize,
}

impl Stft {
    pub fn frame(&self, t: usize) -> &[f32] {
        &self.mags[t * self.bins..(t + 1) * self.bins]
    }

    pub fn bin_hz(&self) -> f64 {
        self.sample_rate / self.frame as f64
    }

    /// Seconds at the centre of frame `t`.
    pub fn time(&self, t: usize) -> f64 {
        (t * self.hop + self.frame / 2) as f64 / self.sample_rate
    }
}

pub fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos())
        .collect()
}

/// Magnitude spectra of `samples` in frames of `frame` every `hop` samples, Hann windowed.
/// Frames run past the end with zeros, so the last samples are covered.
pub fn stft(samples: &[f32], sample_rate: f64, frame: usize, hop: usize) -> Stft {
    let frames = if samples.is_empty() {
        0
    } else {
        samples.len().div_ceil(hop)
    };
    let bins = frame / 2 + 1;
    let window = hann(frame);
    // A full-scale sine peaks at 1, whatever the frame size.
    let scale = 2.0 / window.iter().sum::<f32>();
    let fft: Arc<dyn RealToComplex<f32>> = RealFftPlanner::<f32>::new().plan_fft_forward(frame);
    let mut mags = vec![0.0f32; frames * bins];
    mags.par_chunks_mut(bins * 64)
        .enumerate()
        .for_each(|(chunk, out)| {
            let mut input = fft.make_input_vec();
            let mut spectrum = fft.make_output_vec();
            let mut scratch = fft.make_scratch_vec();
            for (j, row) in out.chunks_mut(bins).enumerate() {
                let t = chunk * 64 + j;
                let start = t * hop;
                for (i, slot) in input.iter_mut().enumerate() {
                    *slot = samples.get(start + i).copied().unwrap_or(0.0) * window[i];
                }
                if fft
                    .process_with_scratch(&mut input, &mut spectrum, &mut scratch)
                    .is_err()
                {
                    continue;
                }
                for (m, c) in row.iter_mut().zip(&spectrum) {
                    *m = c.norm() * scale;
                }
            }
        });
    Stft {
        frame,
        hop,
        sample_rate,
        mags,
        bins,
        frames,
    }
}
