//! Musical key, from a harmonic pitch class profile matched against key templates.

use crate::spectrum::Stft;

pub const NAMES: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

/// Key templates tuned for electronic music (Faraldo's EDMA profiles), tonic first.
const MAJOR: [f64; 12] = [1.00, 0.29, 0.50, 0.40, 0.60, 0.56, 0.32, 0.80, 0.31, 0.45, 0.42, 0.39];
const MINOR: [f64; 12] = [1.00, 0.31, 0.44, 0.58, 0.33, 0.49, 0.29, 0.78, 0.43, 0.29, 0.53, 0.32];

const FRAME: usize = 8192;
const HOP: usize = 4096;
const BINS_PER_SEMITONE: usize = 3;
const BINS: usize = 12 * BINS_PER_SEMITONE;
/// Bass lines often name the key in dance music, so the range reaches well down.
const MIN_HZ: f64 = 50.0;
const MAX_HZ: f64 = 2_000.0;
const HARMONICS: usize = 4;
const HARMONIC_DECAY: f64 = 0.6;
/// Each peak spreads over this many bins, cos² weighted.
const SPREAD_BINS: f64 = 4.0 / 3.0 * BINS_PER_SEMITONE as f64;
/// Peaks quieter than this share of the frame's loudest are ignored.
const PEAK_FLOOR: f32 = 0.01;
const PEAKS_PER_FRAME: usize = 60;
const A4: f64 = 440.0;
/// Before matching, chroma is scaled so its peak is this, then log compressed, so one loud pitch
/// class doesn't decide the key alone.
const COMPRESSION: f64 = 10.0;
/// Added to minor keys' scores; most dance music is minor, and rekordbox agrees more often with it.
const MINOR_BIAS: f64 = 0.12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// 0 is C.
    pub tonic: usize,
    pub minor: bool,
}

impl Key {
    pub fn name(&self) -> String {
        format!("{}{}", NAMES[self.tonic], if self.minor { "m" } else { "" })
    }
}

/// Spectral peaks of a frame, as (Hz, magnitude), interpolated between bins.
fn peaks(frame: &[f32], bin_hz: f64) -> Vec<(f64, f64)> {
    let lo = ((MIN_HZ / bin_hz).floor() as usize).max(1);
    let hi = ((MAX_HZ / bin_hz).ceil() as usize).min(frame.len() - 2);
    let loudest = frame[lo..=hi].iter().copied().fold(0.0f32, f32::max);
    if loudest <= 0.0 {
        return Vec::new();
    }
    let floor = loudest * PEAK_FLOOR;
    let mut out = Vec::new();
    for b in lo..=hi {
        let m = frame[b];
        if m > floor && m > frame[b - 1] && m >= frame[b + 1] {
            let (a, c) = (f64::from(frame[b - 1]), f64::from(frame[b + 1]));
            let denom = a - 2.0 * f64::from(m) + c;
            let shift = if denom.abs() > 1e-12 { 0.5 * (a - c) / denom } else { 0.0 };
            out.push(((b as f64 + shift) * bin_hz, f64::from(m)));
        }
    }
    out.sort_by(|x, y| y.1.total_cmp(&x.1));
    out.truncate(PEAKS_PER_FRAME);
    out
}

/// How far the track's tuning sits from A = 440 Hz, in semitones (-0.5 to 0.5).
fn tuning(all_peaks: &[Vec<(f64, f64)>]) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for (hz, m) in all_peaks.iter().flatten() {
        let semis = 12.0 * (hz / A4).log2();
        let angle = 2.0 * std::f64::consts::PI * (semis - semis.round());
        s += m * angle.sin();
        c += m * angle.cos();
    }
    s.atan2(c) / (2.0 * std::f64::consts::PI)
}

/// The track's average HPCP, folded to 12 semitones starting at C.
pub fn chroma(stft: &Stft) -> [f64; 12] {
    let bin_hz = stft.bin_hz();
    let all: Vec<Vec<(f64, f64)>> = (0..stft.frames).map(|t| peaks(stft.frame(t), bin_hz)).collect();
    let offset = tuning(&all);
    let c_ref = A4 * 2f64.powf((offset - 9.0) / 12.0);
    let mut total = [0.0f64; BINS];
    for frame_peaks in &all {
        let mut hpcp = [0.0f64; BINS];
        for &(hz, m) in frame_peaks {
            for h in 1..=HARMONICS {
                let f0 = hz / h as f64;
                if f0 < MIN_HZ / 2.0 {
                    break;
                }
                let weight = HARMONIC_DECAY.powi(h as i32 - 1) * m;
                let pos = (BINS as f64 * (f0 / c_ref).log2()).rem_euclid(BINS as f64);
                let reach = (SPREAD_BINS / 2.0).ceil() as i64;
                let centre = pos.round() as i64;
                for b in centre - reach..=centre + reach {
                    let d = (b as f64 - pos).abs();
                    if d >= SPREAD_BINS / 2.0 {
                        continue;
                    }
                    let w = (std::f64::consts::PI * d / SPREAD_BINS).cos().powi(2);
                    hpcp[b.rem_euclid(BINS as i64) as usize] += weight * w;
                }
            }
        }
        let peak = hpcp.iter().copied().fold(0.0, f64::max);
        if peak > 0.0 {
            for (t, v) in total.iter_mut().zip(hpcp) {
                *t += v / peak;
            }
        }
    }
    let mut out = [0.0; 12];
    for (b, v) in total.iter().enumerate() {
        // Bin 0 is centred on C, so the bin either side of a semitone's centre belongs to it.
        let semitone = ((b + 1) / BINS_PER_SEMITONE) % 12;
        out[semitone] += v;
    }
    out
}

fn correlation(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let ma = a.iter().sum::<f64>() / 12.0;
    let mb = b.iter().sum::<f64>() / 12.0;
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        num += (a[i] - ma) * (b[i] - mb);
        da += (a[i] - ma) * (a[i] - ma);
        db += (b[i] - mb) * (b[i] - mb);
    }
    if da <= 0.0 || db <= 0.0 {
        0.0
    } else {
        num / (da * db).sqrt()
    }
}

pub fn from_chroma(chroma: &[f64; 12]) -> Option<Key> {
    let peak = chroma.iter().copied().fold(0.0, f64::max);
    if peak <= 0.0 {
        return None;
    }
    let chroma = chroma.map(|v| (1.0 + COMPRESSION * v / peak).ln());
    let mut best = (f64::MIN, Key { tonic: 0, minor: false });
    for tonic in 0..12 {
        for (minor, profile) in [(false, &MAJOR), (true, &MINOR)] {
            let mut rotated = [0.0; 12];
            for i in 0..12 {
                rotated[(i + tonic) % 12] = profile[i];
            }
            let r = correlation(&chroma, &rotated) + if minor { MINOR_BIAS } else { 0.0 };
            if r > best.0 {
                best = (r, Key { tonic, minor });
            }
        }
    }
    Some(best.1)
}

pub fn detect(stft: &Stft) -> Option<Key> {
    from_chroma(&chroma(stft))
}

/// The track's chroma, from mono samples.
pub fn chroma_of(mono: &[f32], sample_rate: f64) -> [f64; 12] {
    chroma(&crate::spectrum::stft(mono, sample_rate, FRAME, HOP))
}

pub fn analyse(mono: &[f32], sample_rate: f64) -> Option<Key> {
    from_chroma(&chroma_of(mono, sample_rate))
}
