//! Scores section starts against where a DJ put their cues, using a dump from `es-index`'s
//! `cue_survey` example. Cues sit on section changes, so a good segmenter's starts carry them.
//! Each track is measured once; tunings are then tried one setting at a time, keeping the best.
//! `cargo run --release -p es-analyse --example structure_eval -- survey.json`

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use es_analyse::structure::{segment, BarFeatures, BarIndex, Section, SectionKind, Tuning};
use es_analyse::{decode_mono, Beat, SAMPLE_RATE};
use rayon::prelude::*;
use serde_json::Value;

const ON_BEAT_TOLERANCE_MS: i64 = 15;

struct Track {
    title: String,
    features: BarFeatures,
    cue_bars: HashSet<BarIndex>,
    rekordbox_phrase_bars: Option<HashSet<BarIndex>>,
    rekordbox_change_bars: Option<HashSet<BarIndex>>,
    rekordbox_bar_kinds: Option<Vec<Option<SectionKind>>>,
}

#[derive(Default, Clone, Copy)]
struct Score {
    cues: usize,
    starts: usize,
    hits: usize,
    tracks: usize,
}

impl Score {
    fn add(&mut self, cues: &HashSet<BarIndex>, starts: &HashSet<BarIndex>) {
        self.cues += cues.len();
        self.starts += starts.len();
        self.hits += cues.intersection(starts).count();
        self.tracks += 1;
    }

    fn recall(&self) -> f64 {
        self.hits as f64 / self.cues.max(1) as f64
    }

    fn precision(&self) -> f64 {
        self.hits as f64 / self.starts.max(1) as f64
    }

    fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    fn line(&self, name: &str) -> String {
        format!(
            "{name:<34} recall {:>5.1}%  precision {:>5.1}%  F1 {:.3}  starts/track {:>5.1}  cues/track {:.1}",
            100.0 * self.recall(),
            100.0 * self.precision(),
            self.f1(),
            self.starts as f64 / self.tracks.max(1) as f64,
            self.cues as f64 / self.tracks.max(1) as f64
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("pass the survey JSON from cue_survey")?;
    let survey: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let tracks: Vec<Track> = survey.par_iter().filter_map(measure).collect();
    println!("measured {} of {} tracks\n", tracks.len(), survey.len());

    report_baselines(&tracks);
    let best = report_labels(&tracks, &tune(&tracks));
    println!("\nbest {best:?}");
    report_kinds(&tracks, &best);
    Ok(())
}

fn measure(entry: &Value) -> Option<Track> {
    let grid = entry.get("grid")?;
    let times: Vec<u32> = numbers(&grid["times"]).into_iter().map(|t| t as u32).collect();
    let beat_numbers: Vec<u8> = numbers(&grid["beats"]).into_iter().map(|b| b as u8).collect();
    let beats: Vec<Beat> =
        times.iter().zip(&beat_numbers).map(|(&time_ms, &number)| Beat { number, tempo_x100: 0, time_ms }).collect();
    let bar_of_beat: BTreeMap<usize, BarIndex> = beats
        .iter()
        .enumerate()
        .filter(|(_, b)| b.number == 1)
        .enumerate()
        .map(|(bar, (beat, _))| (beat, bar))
        .collect();
    let bar_at_ms = |ms: i64| {
        let beat = times.partition_point(|&t| i64::from(t) < ms - ON_BEAT_TOLERANCE_MS);
        let on_beat = times.get(beat).is_some_and(|&t| (i64::from(t) - ms).abs() <= ON_BEAT_TOLERANCE_MS);
        on_beat.then(|| bar_of_beat.get(&beat).copied()).flatten()
    };
    let cue_bars: HashSet<BarIndex> =
        entry["cues"].as_array()?.iter().filter_map(|cue| cue["startMs"].as_i64()).filter_map(bar_at_ms).collect();
    if cue_bars.is_empty() {
        return None;
    }

    let phrases = entry["structure"]["phrases"].as_array();
    let phrase_bar = |phrase: &Value| {
        let first_beat = usize::try_from(phrase["firstBeat"].as_u64()?).ok()?.checked_sub(1)?;
        bar_of_beat.get(&first_beat).copied()
    };
    let rekordbox_phrase_bars = phrases.map(|all| all.iter().filter_map(phrase_bar).collect());
    let rekordbox_change_bars = phrases.map(|all| {
        let kind = |phrase: &Value| phrase["label"].as_str().unwrap_or("").split(' ').next().unwrap_or("").to_owned();
        all.iter()
            .enumerate()
            .filter(|(index, phrase)| *index == 0 || kind(&all[index - 1]) != kind(phrase))
            .filter_map(|(_, phrase)| phrase_bar(phrase))
            .collect()
    });

    let downbeats: Vec<usize> = bar_of_beat.keys().copied().collect();
    let bar_containing = |beat: usize| downbeats.partition_point(|&first| first <= beat).saturating_sub(1);
    let rekordbox_bar_kinds = phrases.map(|all| {
        let mut kinds = vec![None; downbeats.len()];
        for (index, phrase) in all.iter().enumerate() {
            let first_beat = |p: &Value| p["firstBeat"].as_u64().map_or(0, |b| b.saturating_sub(1) as usize);
            let start = bar_containing(first_beat(phrase));
            let end = all.get(index + 1).map_or(downbeats.len(), |next| bar_containing(first_beat(next)));
            let kind = rekordbox_kind(phrase["label"].as_str().unwrap_or(""));
            kinds[start.min(downbeats.len())..end.min(downbeats.len())].fill(kind);
        }
        kinds
    });

    let audio = decode_mono(Path::new(entry["path"].as_str()?)).ok()?;
    Some(Track {
        title: format!("{} - {}", entry["artist"].as_str().unwrap_or(""), entry["title"].as_str().unwrap_or("")),
        features: BarFeatures::measure(&audio, f64::from(SAMPLE_RATE), &beats),
        cue_bars,
        rekordbox_phrase_bars,
        rekordbox_change_bars,
        rekordbox_bar_kinds,
    })
}

fn rekordbox_kind(label: &str) -> Option<SectionKind> {
    match label.split(' ').next()? {
        "INTRO" => Some(SectionKind::Intro),
        "UP" => Some(SectionKind::Build),
        "DOWN" => Some(SectionKind::Breakdown),
        "CHORUS" => Some(SectionKind::Drop),
        "OUTRO" => Some(SectionKind::Outro),
        _ => None,
    }
}

/// Share of bars whose section kind matches rekordbox's, and the counts behind it, ours by theirs.
fn label_agreement(tracks: &[Track], tuning: &Tuning) -> (f64, [[usize; 5]; 5]) {
    let mut counts = [[0usize; 5]; 5];
    for track in tracks {
        let Some(theirs) = &track.rekordbox_bar_kinds else { continue };
        for section in segment(&track.features, tuning) {
            let ours = SectionKind::ALL.iter().position(|&k| k == section.kind).unwrap_or(0);
            for bar in section.first_bar..section.first_bar + section.bars {
                if let Some(Some(kind)) = theirs.get(bar) {
                    counts[ours][SectionKind::ALL.iter().position(|k| k == kind).unwrap_or(0)] += 1;
                }
            }
        }
    }
    let total: usize = counts.iter().flatten().sum();
    let agreed: usize = (0..5).map(|i| counts[i][i]).sum();
    (agreed as f64 / total.max(1) as f64, counts)
}

fn report_labels(tracks: &[Track], best: &Tuning) -> Tuning {
    println!("\nbar labels against rekordbox's:");
    let mut chosen = *best;
    let mut chosen_agreement = 0.0;
    for drop_energy in [0.5, 0.6, 0.7, 0.75, 0.8, 0.85, 0.9] {
        let tuning = Tuning { drop_energy, ..*best };
        let (agreement, _) = label_agreement(tracks, &tuning);
        println!("  drop energy {drop_energy}: {:.1}% of bars agree", 100.0 * agreement);
        if agreement > chosen_agreement {
            (chosen, chosen_agreement) = (tuning, agreement);
        }
    }
    let (_, counts) = label_agreement(tracks, &chosen);
    println!("  ours \\ theirs  {}", SectionKind::ALL.iter().map(|&k| format!("{:>9}", k.name())).collect::<String>());
    for (ours, row) in SectionKind::ALL.iter().zip(counts) {
        println!("  {:<14} {}", ours.name(), row.iter().map(|n| format!("{n:>9}")).collect::<String>());
    }
    chosen
}

fn numbers(value: &Value) -> Vec<u64> {
    value.as_array().map(|all| all.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

fn starts(sections: &[Section]) -> HashSet<BarIndex> {
    sections.iter().map(|section| section.first_bar).collect()
}

fn score(tracks: &[Track], tuning: &Tuning) -> Score {
    let mut total = Score::default();
    for track in tracks {
        total.add(&track.cue_bars, &starts(&segment(&track.features, tuning)));
    }
    total
}

fn report_baselines(tracks: &[Track]) {
    for every in [8, 16, 32] {
        let mut total = Score::default();
        for track in tracks {
            total.add(&track.cue_bars, &(0..track.features.bars()).step_by(every).collect());
        }
        println!("{}", total.line(&format!("every {every} bars from the first")));
    }
    let with_phrases: Vec<&Track> = tracks.iter().filter(|t| t.rekordbox_phrase_bars.is_some()).collect();
    let (mut phrases, mut changes, mut ours) = (Score::default(), Score::default(), Score::default());
    for track in &with_phrases {
        phrases.add(&track.cue_bars, track.rekordbox_phrase_bars.as_ref().unwrap_or(&HashSet::new()));
        changes.add(&track.cue_bars, track.rekordbox_change_bars.as_ref().unwrap_or(&HashSet::new()));
        ours.add(&track.cue_bars, &starts(&segment(&track.features, &Tuning::default())));
    }
    println!("-- the {} tracks rekordbox phrased --", with_phrases.len());
    println!("{}", phrases.line("rekordbox phrase starts"));
    println!("{}", changes.line("rekordbox phrase kind changes"));
    println!("{}", ours.line("ours, default tuning"));
}

type Setting = Vec<(String, Tuning)>;

fn settings(base: &Tuning) -> Vec<Setting> {
    let vary = |name: &'static str, values: &[f64], set: fn(&mut Tuning, f64)| -> Setting {
        values
            .iter()
            .map(|&value| {
                let mut tuning = *base;
                set(&mut tuning, value);
                (format!("{name} {value}"), tuning)
            })
            .collect()
    };
    let kernels = [[2, 4], [4, 8], [4, 16], [8, 16], [2, 8]]
        .iter()
        .map(|&widths| (format!("kernels {widths:?}"), Tuning { kernel_half_widths: widths, ..*base }))
        .collect();
    vec![
        vary("threshold", &[-0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0], |t, v| t.boundary_threshold = v),
        kernels,
        vary("timbre", &[0.0, 0.5, 1.0, 2.0], |t, v| t.timbre_weight = v),
        vary("harmony", &[0.0, 0.25, 0.5, 1.0, 2.0], |t, v| t.harmony_weight = v),
        vary("rhythm", &[0.0, 0.5, 1.0, 2.0], |t, v| t.rhythm_weight = v),
        vary("loudness", &[0.0, 0.5, 1.0, 2.0, 4.0], |t, v| t.loudness_weight = v),
    ]
}

fn tune(tracks: &[Track]) -> Tuning {
    let mut best = Tuning::default();
    let mut best_score = score(tracks, &best);
    println!("\n{}", best_score.line("start"));
    for round in 1..=2 {
        println!("-- round {round} --");
        for options in settings(&best) {
            let scored: Vec<(String, Tuning, Score)> =
                options.into_par_iter().map(|(label, tuning)| (label, tuning, score(tracks, &tuning))).collect();
            for (label, tuning, candidate) in scored {
                if candidate.f1() > best_score.f1() + 1e-9 {
                    best = tuning;
                    best_score = candidate;
                    println!("{}", candidate.line(&label));
                }
            }
        }
    }
    println!("{}", best_score.line("best"));
    best
}

/// What our sections are called where the DJ cued, as "Drop#2" style anchors.
fn report_kinds(tracks: &[Track], tuning: &Tuning) {
    let mut at_cues: BTreeMap<String, usize> = BTreeMap::new();
    let mut everywhere: BTreeMap<String, usize> = BTreeMap::new();
    for track in tracks {
        let sections = segment(&track.features, tuning);
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for section in &sections {
            let kind = section.kind.name();
            let occurrence = seen.entry(kind.to_owned()).and_modify(|n| *n += 1).or_insert(1);
            let anchor = format!("{kind}#{occurrence}");
            *everywhere.entry(anchor.clone()).or_default() += 1;
            if track.cue_bars.contains(&section.first_bar) {
                *at_cues.entry(anchor).or_default() += 1;
            }
        }
    }
    let mut rows: Vec<_> = everywhere.into_iter().collect();
    rows.sort_by_key(|(_, found)| std::cmp::Reverse(*found));
    println!("\n{:<14} {:>6} {:>7}", "section start", "found", "cued");
    for (anchor, found) in rows.into_iter().take(24) {
        let cued = at_cues.get(&anchor).copied().unwrap_or(0);
        println!("{anchor:<14} {found:>6} {:>6.0}%", 100.0 * cued as f64 / found as f64);
    }
    if let Some(track) = tracks.first() {
        println!("\nexample: {}", track.title);
        for section in segment(&track.features, tuning) {
            let cued = if track.cue_bars.contains(&section.first_bar) { "cued" } else { "" };
            println!(
                "  bar {:>4} {:>3} bars {:<9} energy {:.2} {cued}",
                section.first_bar,
                section.bars,
                section.kind.name(),
                section.relative_energy
            );
        }
        let mut missed: Vec<_> =
            track.cue_bars.difference(&starts(&segment(&track.features, tuning))).copied().collect();
        missed.sort_unstable();
        println!("  cues on no section start: bars {missed:?}");
    }
}
