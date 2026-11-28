//! Track analysis for import: beatgrid, tempo, key, and the loudness the waveform tags are built
//! from. Our own algorithm, written from published methods; it is not rekordbox's.

mod attack;
pub mod cue_style;
pub mod downbeat;
pub mod key;
pub mod onset;
pub mod spectrum;
pub mod structure;
pub mod tempo;
pub mod waveform;

use std::path::Path;

use es_decode::{DecodeError, Streamer};

pub use tempo::TempoOptions;

/// Everything is analysed at this rate, so frame sizes mean the same for every file.
pub const SAMPLE_RATE: u32 = 44_100;
/// Longer audio is analysed only up to here.
pub const MAX_SECONDS: f64 = 30.0 * 60.0;
const ONSET_FRAME: usize = 1024;
const ONSET_HOP: usize = 256;
const KICK_HZ: (f64, f64) = (30.0, 150.0);
const ONSET_HZ: (f64, f64) = (30.0, 16_000.0);
/// How many times more of the phrase changes another beat of the bar must carry before a detected
/// grid's downbeat moves to it.
const DOWNBEAT_RECHECK_TRUST: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct Options {
    pub tempo: TempoOptions,
    pub grid: bool,
    pub key: bool,
    pub sections: bool,
    /// Sections are found on this grid, and no new grid is detected.
    pub kept_grid: Option<Vec<Beat>>,
}

impl Default for Options {
    fn default() -> Self {
        Self { tempo: TempoOptions::default(), grid: true, key: true, sections: true, kept_grid: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beat {
    /// 1 to 4, 1 starting a bar.
    pub number: u8,
    pub tempo_x100: u16,
    pub time_ms: u32,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub duration: f64,
    /// The main tempo, 0 when no grid was found or asked for.
    pub bpm: f64,
    pub beats: Vec<Beat>,
    pub key: Option<key::Key>,
    /// [`waveform::LANES`] bytes per entry at [`waveform::RATE`] entries per second.
    pub waveform: Vec<u8>,
    /// On the kept grid when there is one, otherwise on `beats`.
    pub sections: Vec<structure::Section>,
}

/// Decodes `path` through the deck's decoder at [`SAMPLE_RATE`], mixed to mono.
pub fn decode_mono(path: &Path) -> Result<Vec<f32>, DecodeError> {
    let mut streamer = Streamer::open(path, SAMPLE_RATE)?;
    let cap = (MAX_SECONDS * f64::from(SAMPLE_RATE)) as usize;
    let total = usize::try_from(streamer.total_frames()).unwrap_or(0);
    let mut mono = Vec::with_capacity(total.min(cap));
    let mut buffer = vec![0.0f32; 16_384];
    while mono.len() < cap {
        let frames = streamer.fill(&mut buffer)?;
        if frames == 0 {
            break;
        }
        mono.extend(buffer[..frames * 2].chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5));
    }
    mono.truncate(cap);
    Ok(mono)
}

pub fn analyse_file(path: &Path, options: &Options) -> Result<Analysis, DecodeError> {
    let mono = decode_mono(path)?;
    Ok(analyse_samples(&mono, f64::from(SAMPLE_RATE), options))
}

pub fn analyse_samples(mono: &[f32], sample_rate: f64, options: &Options) -> Analysis {
    let duration = mono.len() as f64 / sample_rate;
    let detect_grid = options.grid && options.kept_grid.is_none();
    let (grid, key) = rayon::join(
        || detect_grid.then(|| beatgrid(mono, sample_rate, duration, &options.tempo)).flatten(),
        || options.key.then(|| key::analyse(mono, sample_rate)).flatten(),
    );
    let (bpm, mut beats) = grid.unwrap_or_default();
    let (waveform, sections) = rayon::join(
        || waveform::features(mono, sample_rate),
        || recheck_downbeats_and_find_sections(mono, sample_rate, &mut beats, options),
    );
    Analysis { duration, bpm, beats, key, waveform, sections }
}

/// Sections on the kept grid, or on `detected` after checking its downbeats against where the
/// music changes: a detected grid is renumbered when another beat of the bar carries clearly more
/// of the track's phrase changes. A kept grid is the user's and is never renumbered.
fn recheck_downbeats_and_find_sections(
    mono: &[f32],
    sample_rate: f64,
    detected: &mut [Beat],
    options: &Options,
) -> Vec<structure::Section> {
    let segment = |features: &structure::BeatFeatures| {
        if options.sections {
            structure::segment(features, &structure::Tuning::default())
        } else {
            Vec::new()
        }
    };
    if let Some(kept) = options.kept_grid.as_deref().filter(|kept| !kept.is_empty()) {
        return segment(&structure::BeatFeatures::measure(mono, sample_rate, kept));
    }
    if detected.is_empty() {
        return Vec::new();
    }
    let features = structure::BeatFeatures::measure(mono, sample_rate, detected);
    let votes = structure::bar_phase_votes(&features);
    let phase = structure::bar_phase(features.on_downbeat(), &votes, DOWNBEAT_RECHECK_TRUST);
    let numbers = downbeat::numbers(detected.len(), phase);
    for (beat, number) in detected.iter_mut().zip(numbers) {
        beat.number = number;
    }
    segment(&features.counting_bars_from(phase))
}

fn beatgrid(mono: &[f32], sample_rate: f64, duration: f64, opts: &TempoOptions) -> Option<(f64, Vec<Beat>)> {
    let (stft, attacks) = rayon::join(
        || spectrum::stft(mono, sample_rate, ONSET_FRAME, ONSET_HOP),
        || attack::Attacks::new(mono, sample_rate),
    );
    let onset = onset::smooth(&onset::flux(&stft, ONSET_HZ.0, ONSET_HZ.1), 1);
    let low = onset::smooth(&onset::flux(&stft, KICK_HZ.0, KICK_HZ.1), 1);
    let bands: Vec<_> = tempo::PHASE_BANDS.iter().map(|&(a, b)| onset::smooth(&onset::flux(&stft, a, b), 1)).collect();
    let grid = tempo::estimate(&tempo::Inputs { bands: &bands, onset: &onset, attacks: &attacks, duration }, opts)?;
    let first = downbeat::first_downbeat(&stft, &grid.beats, &|t| f64::from(low.at_time(t)));
    let numbers = downbeat::numbers(grid.beats.len(), first);
    let beats = grid
        .beats
        .iter()
        .zip(&grid.tempos)
        .zip(numbers)
        .map(|((&t, &bpm), number)| Beat {
            number,
            tempo_x100: (bpm * 100.0).round().clamp(0.0, f64::from(u16::MAX)) as u16,
            time_ms: (t * 1000.0).round() as u32,
        })
        .collect();
    Some((grid.bpm, beats))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 44_100.0;

    /// A kick (decaying 55 Hz sine with a click) on every beat, hats on the off-beats, and a held
    /// chord, starting `offset` seconds in.
    fn track(bpm: f64, seconds: f64, offset: f64, chord: &[f64]) -> Vec<f32> {
        let n = (seconds * RATE) as usize;
        let period = 60.0 / bpm;
        let mut out = vec![0.0f32; n];
        let mut seed = 1u32;
        for (i, slot) in out.iter_mut().enumerate() {
            let t = i as f64 / RATE;
            let mut v = 0.0;
            for f in chord {
                v += 0.05 * (2.0 * std::f64::consts::PI * f * t).sin();
            }
            if t >= offset {
                let since = (t - offset) % period;
                v += 0.8 * (-since * 18.0).exp() * (2.0 * std::f64::consts::PI * 55.0 * since).sin();
                let off = (t - offset + period / 2.0) % period;
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 9) as f64 / (1u32 << 23) as f64 - 1.0;
                v += 0.15 * (-off * 60.0).exp() * noise;
            }
            *slot = v as f32;
        }
        out
    }

    #[test]
    fn constant_tempo() {
        for &bpm in &[124.0, 128.0, 140.0, 174.0, 95.0] {
            let audio = track(bpm, 60.0, 0.25, &[261.63, 329.63, 392.0]);
            let a = analyse_samples(&audio, RATE, &Options::default());
            assert!((a.bpm - bpm).abs() < 0.02, "{bpm}: got {}", a.bpm);
            let first = a.beats[0].time_ms as f64 / 1000.0;
            let expected = 0.25 % (60.0 / bpm);
            assert!((first - expected).abs() < 0.012, "{bpm}: first beat {first} vs {expected}");
        }
    }

    #[test]
    fn fractional_tempo() {
        let audio = track(126.5, 60.0, 0.1, &[220.0]);
        let a = analyse_samples(&audio, RATE, &Options::default());
        assert!((a.bpm - 126.5).abs() < 0.02, "got {}", a.bpm);
    }

    #[test]
    fn key_from_chord() {
        let a_minor = track(120.0, 30.0, 0.0, &[220.0, 261.63, 329.63, 110.0]);
        let key = analyse_samples(&a_minor, RATE, &Options::default()).key.unwrap();
        assert_eq!(key.name(), "Am");
        let g_major = track(120.0, 30.0, 0.0, &[196.0, 246.94, 293.66, 98.0]);
        let key = analyse_samples(&g_major, RATE, &Options::default()).key.unwrap();
        assert_eq!(key.name(), "G");
    }

    #[test]
    fn dynamic_follows_a_tempo_change() {
        let mut audio = track(120.0, 48.0, 0.0, &[220.0]);
        audio.extend(track(128.0, 48.0, 0.0, &[220.0]));
        let options =
            Options { tempo: TempoOptions { dynamic: true, ..TempoOptions::default() }, ..Options::default() };
        let a = analyse_samples(&audio, RATE, &options);
        let tempos: Vec<u16> = a.beats.iter().map(|b| b.tempo_x100).collect();
        assert_eq!(tempos.first(), Some(&12_000));
        assert_eq!(tempos.last(), Some(&12_800));
        let change = a.beats.iter().find(|b| b.tempo_x100 == 12_800).unwrap();
        assert!((f64::from(change.time_ms) / 1000.0 - 48.0).abs() < 8.0, "changes at {}", change.time_ms);
    }

    #[test]
    fn sections_follow_a_kept_grid_instead_of_a_detected_one() {
        let audio = track(128.0, 60.0, 0.0, &[220.0]);
        let detected = analyse_samples(&audio, RATE, &Options::default());
        let kept_grid: Vec<Beat> = detected.beats[..detected.beats.len() / 2].to_vec();
        let options = Options { kept_grid: Some(kept_grid.clone()), ..Options::default() };
        let kept = analyse_samples(&audio, RATE, &options);
        assert!(kept.beats.is_empty());
        let last = kept.sections.last().expect("sections on the kept grid");
        assert_eq!(last.first_beat + last.beats, kept_grid.len());
    }

    #[test]
    fn beat_numbers() {
        assert_eq!(downbeat::numbers(6, 0), vec![1, 2, 3, 4, 1, 2]);
        assert_eq!(downbeat::numbers(6, 2), vec![3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn silence_has_no_grid() {
        let a = analyse_samples(&vec![0.0; RATE as usize * 10], RATE, &Options::default());
        assert!(a.beats.is_empty());
        assert!(a.key.is_none());
        assert_eq!(a.waveform.len() % waveform::LANES, 0);
    }
}
