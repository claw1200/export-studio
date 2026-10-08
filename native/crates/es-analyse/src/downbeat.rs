//! Which beat of each four starts the bar.
//!
//! Music changes on bar lines: new parts come in, chords move, fills resolve. Each beat gets a
//! coarse spectral profile and a chroma, and each of the four bar phases is scored by how much the
//! music changes on its beats, both against the beat before and against the phrase before.

use crate::spectrum::Stft;

const BANDS: usize = 12;
const BAND_LO_HZ: f64 = 40.0;
const BAND_HI_HZ: f64 = 12_000.0;
/// Phrase lengths compared, in beats, and their weights.
const PHRASES: [(usize, f64); 4] = [(4, 1.0), (8, 1.0), (16, 0.75), (32, 0.5)];
/// Weight of the beat-to-beat chroma change.
const CHORD_WEIGHT: f64 = 1.0;
/// Weight of the low band's onset at the beat.
const KICK_WEIGHT: f64 = 0.5;
/// Share of the spread between bar phases' scores given to the phase the music starts on.
const START_WEIGHT: f64 = 0.6;
/// A beat this many dB below the track's median loudness or more is still silence or a fade-in.
const SOUNDING_DB: f64 = 20.0;

/// Per-beat features: log band energies and a unit chroma, averaged over the beat.
struct Profiles {
    bands: Vec<[f64; BANDS]>,
    chroma: Vec<[f64; 12]>,
    kick: Vec<f64>,
}

fn band_edges(stft: &Stft) -> [usize; BANDS + 1] {
    let mut edges = [0usize; BANDS + 1];
    let ratio = (BAND_HI_HZ / BAND_LO_HZ).powf(1.0 / BANDS as f64);
    for (i, e) in edges.iter_mut().enumerate() {
        let hz = BAND_LO_HZ * ratio.powi(i as i32);
        *e = ((hz / stft.bin_hz()).round() as usize).clamp(1, stft.bins - 1);
    }
    edges
}

fn profiles(stft: &Stft, beats: &[f64], low_onset: &dyn Fn(f64) -> f64) -> Profiles {
    let edges = band_edges(stft);
    let bin_hz = stft.bin_hz();
    let pitch: Vec<Option<usize>> = (0..stft.bins)
        .map(|b| {
            let hz = b as f64 * bin_hz;
            (60.0..=2_000.0).contains(&hz).then(|| {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                (midi.round() as i64).rem_euclid(12) as usize
            })
        })
        .collect();
    let frame_of = |t: f64| {
        ((t * stft.sample_rate - (stft.frame / 2) as f64) / stft.hop as f64)
            .round()
            .clamp(0.0, stft.frames.saturating_sub(1) as f64) as usize
    };
    let mut out = Profiles {
        bands: Vec::with_capacity(beats.len()),
        chroma: Vec::with_capacity(beats.len()),
        kick: Vec::with_capacity(beats.len()),
    };
    for (i, &t) in beats.iter().enumerate() {
        let end = beats.get(i + 1).copied().unwrap_or(t + (t - beats[i.saturating_sub(1)]).max(0.3));
        let (f0, f1) = (frame_of(t), frame_of(end).max(frame_of(t) + 1));
        let mut bands = [0.0; BANDS];
        let mut chroma = [0.0; 12];
        for f in f0..f1.min(stft.frames) {
            let frame = stft.frame(f);
            for b in 0..BANDS {
                bands[b] += frame[edges[b]..edges[b + 1].max(edges[b] + 1)]
                    .iter()
                    .map(|&m| f64::from(m * m))
                    .sum::<f64>();
            }
            for (bin, p) in pitch.iter().enumerate() {
                if let Some(pc) = p {
                    chroma[*pc] += f64::from(frame[bin]);
                }
            }
        }
        let frames = (f1 - f0).max(1) as f64;
        for b in &mut bands {
            *b = (*b / frames + 1e-10).log10();
        }
        let norm = chroma.iter().map(|c| c * c).sum::<f64>().sqrt();
        if norm > 0.0 {
            for c in &mut chroma {
                *c /= norm;
            }
        }
        out.bands.push(bands);
        out.chroma.push(chroma);
        out.kick.push(low_onset(t));
    }
    out
}

fn distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt()
}

/// Novelty at each beat: how much the music after it differs from the music before it.
fn novelty(p: &Profiles) -> Vec<f64> {
    let n = p.bands.len();
    let mut out = vec![0.0; n];
    for (len, weight) in PHRASES {
        if n < len * 2 {
            continue;
        }
        let mut raw = vec![0.0; n];
        for (b, slot) in raw.iter_mut().enumerate().take(n - len + 1).skip(len) {
            let mut d = 0.0;
            for j in 0..len {
                d += distance(&p.bands[b + j], &p.bands[b - len + j]);
            }
            *slot = d / len as f64;
        }
        let mean = raw.iter().sum::<f64>() / n as f64;
        let sd = (raw.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / n as f64).sqrt();
        if sd > 0.0 {
            for (o, r) in out.iter_mut().zip(&raw) {
                *o += weight * ((r - mean) / sd).max(0.0);
            }
        }
    }
    let chord: Vec<f64> = (0..n)
        .map(|b| if b == 0 { 0.0 } else { distance(&p.chroma[b], &p.chroma[b - 1]) })
        .collect();
    for (series, weight) in [(&chord, CHORD_WEIGHT), (&p.kick, KICK_WEIGHT)] {
        let mean = series.iter().sum::<f64>() / n.max(1) as f64;
        let sd = (series.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / n.max(1) as f64).sqrt();
        if sd > 0.0 {
            for (o, r) in out.iter_mut().zip(series.iter()) {
                *o += weight * (r - mean) / sd;
            }
        }
    }
    out
}

/// The first beat whose loudness reaches a fair share of the track's typical loudness.
fn first_sounding(p: &Profiles) -> usize {
    let loudness: Vec<f64> = p
        .bands
        .iter()
        .map(|b| b.iter().map(|db| 10f64.powf(*db)).sum::<f64>().log10())
        .collect();
    let mut sorted = loudness.clone();
    sorted.sort_by(f64::total_cmp);
    let typical = sorted[sorted.len() / 2];
    loudness.iter().position(|&l| l > typical - SOUNDING_DB / 10.0).unwrap_or(0)
}

/// The index, 0 to 3, of the first beat that starts a bar.
pub fn first_downbeat(stft: &Stft, beats: &[f64], low_onset: &dyn Fn(f64) -> f64) -> usize {
    if beats.len() < 8 {
        return 0;
    }
    let p = profiles(stft, beats, low_onset);
    let nov = novelty(&p);
    let mut scores = [0.0f64; 4];
    for (b, v) in nov.iter().enumerate() {
        scores[b % 4] += v;
    }
    // Most tracks start on a bar line, so the first beat with sound in it gets a head start.
    let spread = scores.iter().copied().fold(f64::MIN, f64::max) - scores.iter().copied().fold(f64::MAX, f64::min);
    scores[first_sounding(&p) % 4] += START_WEIGHT * spread;
    scores
        .iter()
        .enumerate()
        .fold((0, f64::MIN), |best, (i, &s)| if s > best.1 { (i, s) } else { best })
        .0
}

/// Beat numbers 1 to 4 for `count` beats, the first bar starting at `first`.
pub fn numbers(count: usize, first: usize) -> Vec<u8> {
    (0..count)
        .map(|i| ((i + 4 - first % 4) % 4 + 1) as u8)
        .collect()
}
