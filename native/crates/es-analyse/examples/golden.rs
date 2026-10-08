//! Scores the analyser against rekordbox's own analysis of the same tracks.
//!
//!     uv run python -m exportstudio_engine.golden /tmp/golden.json     (in engine/)
//!     cargo run --release -p es-analyse --example golden -- /tmp/golden.json [options]
//!
//! Options: --limit N, --only TITLE, --dynamic, --low-precision, --no-round, --range MIN-MAX, --verbose,
//! --features DIR (writes each track's waveform features as DIR/{id}.bin for calibration).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use es_analyse::{analyse_file, key, Analysis, Options};
use rayon::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct Track {
    id: String,
    title: String,
    path: String,
    bpm: f64,
    key: Option<String>,
    beats: Grid,
}

#[derive(Deserialize)]
struct Grid {
    times: Vec<f64>,
    beats: Vec<u8>,
}

/// Camelot code for a key, as rekordbox shows it in "alphanumeric" mode.
fn camelot(k: key::Key) -> String {
    let major_root = if k.minor { (k.tonic + 3) % 12 } else { k.tonic };
    let n = (7 * major_root + 7) % 12 + 1;
    format!("{n}{}", if k.minor { "A" } else { "B" })
}

fn parse_camelot(s: &str) -> Option<(u32, char)> {
    let letter = s.chars().last()?;
    let n = s[..s.len() - 1].parse().ok()?;
    matches!(letter, 'A' | 'B').then_some((n, letter))
}

#[derive(Default, Debug)]
struct Score {
    tracks: usize,
    failed: usize,
    no_grid: usize,
    bpm_exact: usize,
    bpm_close: usize,
    bpm_double: usize,
    bpm_half: usize,
    bpm_other: usize,
    phase_ok: usize,
    phase_half: usize,
    phase_other: usize,
    offsets_ms: Vec<f64>,
    downbeat_ok: usize,
    downbeat_checked: usize,
    downbeat_shifts: [usize; 4],
    key_exact: usize,
    key_near: usize,
    key_checked: usize,
}

enum Outcome {
    Failed(String),
    Done(Box<Analysis>),
}

fn nearest(sorted: &[f64], t: f64) -> usize {
    match sorted.binary_search_by(|x| x.total_cmp(&t)) {
        Ok(i) => i,
        Err(0) => 0,
        Err(i) if i >= sorted.len() => sorted.len() - 1,
        Err(i) => {
            if t - sorted[i - 1] < sorted[i] - t {
                i - 1
            } else {
                i
            }
        }
    }
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut file = None;
    let mut options = Options::default();
    let mut limit = usize::MAX;
    let mut verbose = false;
    let mut features: Option<PathBuf> = None;
    let mut only: Option<String> = None;
    let mut chroma: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                i += 1;
                limit = args[i].parse().expect("--limit N");
            }
            "--dynamic" => options.tempo.dynamic = true,
            "--low-precision" => options.tempo.high_precision = false,
            "--no-round" => options.tempo.round_bpm = false,
            "--verbose" => verbose = true,
            "--only" => {
                i += 1;
                only = Some(args[i].to_lowercase());
            }
            "--range" => {
                i += 1;
                let (lo, hi) = args[i].split_once('-').expect("--range MIN-MAX");
                options.tempo.min_bpm = lo.parse().unwrap();
                options.tempo.max_bpm = hi.parse().unwrap();
            }
            "--chroma" => {
                i += 1;
                chroma = Some(PathBuf::from(&args[i]));
            }
            "--features" => {
                i += 1;
                features = Some(PathBuf::from(&args[i]));
            }
            other => file = Some(other.to_string()),
        }
        i += 1;
    }
    let file = file.expect("usage: golden <golden.json> [options]");
    let golden: Golden = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let tracks: Vec<&Track> = golden
        .tracks
        .iter()
        .filter(|t| only.as_ref().is_none_or(|o| t.title.to_lowercase().contains(o)))
        .take(limit)
        .collect();
    if let Some(dir) = &features {
        std::fs::create_dir_all(dir).unwrap();
    }

    if let Some(out) = chroma {
        let lines: Vec<String> = tracks
            .par_iter()
            .filter_map(|t| {
                let key = t.key.as_ref()?;
                let mono = es_analyse::decode_mono(Path::new(&t.path)).ok()?;
                let c = key::chroma_of(&mono, f64::from(es_analyse::SAMPLE_RATE));
                let values: Vec<String> = c.iter().map(|v| format!("{v:.5}")).collect();
                Some(format!("{key},{}", values.join(",")))
            })
            .collect();
        std::fs::write(out, lines.join("\n")).unwrap();
        return;
    }

    let done = AtomicUsize::new(0);
    let started = std::time::Instant::now();
    let results: Vec<Outcome> = tracks
        .par_iter()
        .map(|t| {
            let out = match analyse_file(Path::new(&t.path), &options) {
                Ok(a) => {
                    if let Some(dir) = &features {
                        std::fs::write(dir.join(format!("{}.bin", t.id)), &a.waveform).unwrap();
                    }
                    Outcome::Done(Box::new(a))
                }
                Err(e) => Outcome::Failed(e.to_string()),
            };
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(50) {
                eprintln!("{n}/{} ({:.0}s)", tracks.len(), started.elapsed().as_secs_f64());
            }
            out
        })
        .collect();

    let mut s = Score::default();
    for (t, outcome) in tracks.iter().zip(&results) {
        s.tracks += 1;
        let a = match outcome {
            Outcome::Failed(e) => {
                s.failed += 1;
                if verbose {
                    println!("FAIL  {}: {e}", t.title);
                }
                continue;
            }
            Outcome::Done(a) => a,
        };

        if let Some(rb_key) = &t.key {
            if let Some(k) = a.key {
                s.key_checked += 1;
                let ours = camelot(k);
                if &ours == rb_key || &k.name() == rb_key {
                    s.key_exact += 1;
                } else if let (Some((n1, l1)), Some((n2, l2))) = (parse_camelot(&ours), parse_camelot(rb_key)) {
                    let step = (n1 as i32 - n2 as i32).rem_euclid(12);
                    if (l1 == l2 && (step == 1 || step == 11)) || (l1 != l2 && n1 == n2) {
                        s.key_near += 1;
                    }
                }
            }
        }

        if a.beats.is_empty() {
            s.no_grid += 1;
            continue;
        }
        let ratio = a.bpm / t.bpm;
        let label = if (a.bpm - t.bpm).abs() <= 0.011 {
            s.bpm_exact += 1;
            "exact"
        } else if (ratio - 1.0).abs() < 0.005 {
            s.bpm_close += 1;
            "close"
        } else if (ratio - 2.0).abs() < 0.02 {
            s.bpm_double += 1;
            "double"
        } else if (ratio - 0.5).abs() < 0.01 {
            s.bpm_half += 1;
            "half"
        } else {
            s.bpm_other += 1;
            "other"
        };
        let mut line = format!("{label:6} {:7.2} vs {:7.2}", a.bpm, t.bpm);

        if matches!(label, "exact" | "close") && !t.beats.times.is_empty() {
            let rb: Vec<f64> = t.beats.times.iter().map(|ms| ms / 1000.0).collect();
            let period = 60.0 / t.bpm;
            let mut diffs: Vec<f64> = a
                .beats
                .iter()
                .map(|b| {
                    let time = f64::from(b.time_ms) / 1000.0;
                    time - rb[nearest(&rb, time)]
                })
                .collect();
            let offset = median(&mut diffs);
            line += &format!("  offset {:+6.1}ms", offset * 1000.0);
            if offset.abs() < 0.02 {
                s.phase_ok += 1;
                s.offsets_ms.push(offset * 1000.0);
                let mut shifts = [0usize; 4];
                for b in &a.beats {
                    let time = f64::from(b.time_ms) / 1000.0;
                    let j = nearest(&rb, time);
                    if let Some(&theirs) = t.beats.beats.get(j) {
                        if (time - rb[j]).abs() < 0.03 && (1..=4).contains(&theirs) {
                            shifts[(usize::from(theirs) + 4 - usize::from(b.number)) % 4] += 1;
                        }
                    }
                }
                s.downbeat_checked += 1;
                let shift = (0..4).max_by_key(|&k| shifts[k]).unwrap_or(0);
                s.downbeat_shifts[shift] += 1;
                if shift == 0 {
                    s.downbeat_ok += 1;
                } else {
                    line += &format!("  downbeat +{shift}");
                }
            } else if (offset.abs() - period / 2.0).abs() < 0.03 {
                s.phase_half += 1;
                line += "  half-beat";
            } else {
                s.phase_other += 1;
                line += "  phase";
            }
        }
        if verbose {
            println!("{line}  {}", t.title);
        }
    }

    let pct = |n: usize, d: usize| if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 };
    let analysed = s.tracks - s.failed;
    let gridded = analysed - s.no_grid;
    println!("\n{} tracks in {:.0}s, {} failed to decode, {} without a grid", s.tracks, started.elapsed().as_secs_f64(), s.failed, s.no_grid);
    println!(
        "BPM: exact {:.1}%  close {:.1}%  double {:.1}%  half {:.1}%  other {:.1}%",
        pct(s.bpm_exact, gridded),
        pct(s.bpm_close, gridded),
        pct(s.bpm_double, gridded),
        pct(s.bpm_half, gridded),
        pct(s.bpm_other, gridded)
    );
    let tempo_ok = s.bpm_exact + s.bpm_close;
    let mut offs = s.offsets_ms.clone();
    let med = median(&mut offs);
    let mut abs: Vec<f64> = s.offsets_ms.iter().map(|o| o.abs()).collect();
    let within5 = abs.iter().filter(|&&o| o <= 5.0).count();
    println!(
        "Grid (of right-tempo tracks): on the beat {:.1}%  half a beat off {:.1}%  other {:.1}%",
        pct(s.phase_ok, tempo_ok),
        pct(s.phase_half, tempo_ok),
        pct(s.phase_other, tempo_ok)
    );
    println!(
        "  offset vs rekordbox: median {:+.1}ms, median |offset| {:.1}ms, within 5ms {:.1}%",
        med,
        median(&mut abs),
        pct(within5, s.offsets_ms.len())
    );
    println!(
        "Downbeat (of on-beat grids): {:.1}%  off by 1/2/3 beats: {:?}",
        pct(s.downbeat_ok, s.downbeat_checked),
        &s.downbeat_shifts[1..]
    );
    println!(
        "Key: exact {:.1}%  neighbour {:.1}%  of {}",
        pct(s.key_exact, s.key_checked),
        pct(s.key_near, s.key_checked),
        s.key_checked
    );
}
