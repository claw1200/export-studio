//! Scores section starts against where a DJ put their cues, using a dump from `es-index`'s
//! `cue_survey` example. Cues sit on section changes, so a good segmenter's starts carry them.
//! Each track is measured once; tunings are then tried one setting at a time, keeping the best.
//! `cargo run --release -p es-analyse --example structure_eval -- survey.json [--our-grid]`
//!
//! `--our-grid` finds sections on the grid our own analysis detects instead of the one the survey
//! holds, as a track has after it is analysed here. Cues are then matched to its nearest beats.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use es_analyse::structure::{
    bar_phase, bar_phase_votes, segment, BeatFeatures, BeatIndex, Section, SectionKind, Tuning,
};
use es_analyse::{analyse_samples, decode_mono, Options, SAMPLE_RATE};
use rayon::prelude::*;
use serde_json::Value;

#[path = "survey/mod.rs"]
mod survey;
use survey::{nearest_beat, share, survey_grid, Score, BEATS_PER_BAR, SURVEY_ON_BEAT_MS};

const OUR_GRID_ON_BEAT_SHARE: f64 = 0.25;

struct Track {
    title: String,
    features: BeatFeatures,
    first_downbeat: BeatIndex,
    cue_beats: HashSet<BeatIndex>,
    rekordbox_phrase_beats: Option<HashSet<BeatIndex>>,
    rekordbox_change_beats: Option<HashSet<BeatIndex>>,
    rekordbox_beat_kinds: Option<Vec<Option<SectionKind>>>,
    /// With `--our-grid`: the beat of the survey grid's bar our downbeats mostly land on, 0 when they agree.
    downbeat_offset: Option<usize>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("pass the survey JSON from cue_survey")?;
    let our_grid = args.any(|arg| arg == "--our-grid");
    let survey = survey::load(&path)?;
    let tracks: Vec<Track> = survey.par_iter().filter_map(|entry| measure(entry, our_grid)).collect();
    println!("measured {} of {} tracks{}\n", tracks.len(), survey.len(), if our_grid { " on our grid" } else { "" });

    let offsets: Vec<usize> = tracks.iter().filter_map(|track| track.downbeat_offset).collect();
    if !offsets.is_empty() {
        let count = |offset| offsets.iter().filter(|&&o| o == offset).count();
        println!(
            "our downbeats against the survey grid's: on its beat 1 {:.1}%, beat 2 {:.1}%, beat 3 {:.1}%, beat 4 {:.1}% of tracks\n",
            share(count(0), offsets.len()),
            share(count(1), offsets.len()),
            share(count(2), offsets.len()),
            share(count(3), offsets.len())
        );
        report_phase_correction(&tracks);
    }
    report_baselines(&tracks);
    let best = tune(&tracks);
    report_labels(&tracks, &best);
    println!("\nbest {best:?}");
    report_anchors(&tracks, &best);
    Ok(())
}

fn measure(entry: &Value, our_grid: bool) -> Option<Track> {
    let audio = decode_mono(Path::new(entry["path"].as_str()?)).ok()?;
    let rate = f64::from(SAMPLE_RATE);
    let beats = if our_grid {
        analyse_samples(&audio, rate, &Options { key: false, sections: false, ..Options::default() }).beats
    } else {
        survey_grid(entry)
    };
    if beats.len() < 2 {
        return None;
    }
    let beat_ms = f64::from(beats[1].time_ms - beats[0].time_ms);
    let downbeat_offset = our_grid.then(|| {
        let theirs = survey_grid(entry);
        let mut offsets = [0usize; BEATS_PER_BAR];
        for beat in beats.iter().filter(|beat| beat.number == 1) {
            if let Some(index) = nearest_beat(&theirs, f64::from(beat.time_ms), beat_ms * OUR_GRID_ON_BEAT_SHARE) {
                offsets[(usize::from(theirs[index].number) + BEATS_PER_BAR - 1) % BEATS_PER_BAR] += 1;
            }
        }
        (0..BEATS_PER_BAR).max_by_key(|&offset| offsets[offset]).unwrap_or(0)
    });
    let tolerance = if our_grid { beat_ms * OUR_GRID_ON_BEAT_SHARE } else { SURVEY_ON_BEAT_MS };
    let cue_beats: HashSet<BeatIndex> = entry["cues"]
        .as_array()?
        .iter()
        .filter_map(|cue| cue["startMs"].as_f64())
        .filter_map(|ms| nearest_beat(&beats, ms, tolerance))
        .collect();
    if cue_beats.is_empty() {
        return None;
    }

    let phrases = if our_grid { None } else { entry["structure"]["phrases"].as_array() };
    let phrase_beat = |phrase: &Value| usize::try_from(phrase["firstBeat"].as_u64()?).ok()?.checked_sub(1);
    let kind_word = |phrase: &Value| phrase["label"].as_str().unwrap_or("").split(' ').next().unwrap_or("").to_owned();
    let rekordbox_phrase_beats = phrases.map(|all| all.iter().filter_map(phrase_beat).collect());
    let rekordbox_change_beats = phrases.map(|all| {
        all.iter()
            .enumerate()
            .filter(|(index, phrase)| *index == 0 || kind_word(&all[index - 1]) != kind_word(phrase))
            .filter_map(|(_, phrase)| phrase_beat(phrase))
            .collect()
    });
    let rekordbox_beat_kinds = phrases.map(|all| {
        let mut kinds = vec![None; beats.len()];
        for (index, phrase) in all.iter().enumerate() {
            let start = phrase_beat(phrase).unwrap_or(0).min(beats.len());
            let end = all.get(index + 1).and_then(phrase_beat).unwrap_or(beats.len()).min(beats.len());
            kinds[start..end.max(start)].fill(rekordbox_kind(&kind_word(phrase)));
        }
        kinds
    });

    Some(Track {
        title: format!("{} - {}", entry["artist"].as_str().unwrap_or(""), entry["title"].as_str().unwrap_or("")),
        features: BeatFeatures::measure(&audio, rate, &beats),
        first_downbeat: beats.iter().position(|beat| beat.number == 1).unwrap_or(0),
        cue_beats,
        rekordbox_phrase_beats,
        rekordbox_change_beats,
        rekordbox_beat_kinds,
        downbeat_offset,
    })
}

/// How often re-picking the downbeat from the bar-to-bar vote would put it where the survey grid has it.
fn report_phase_correction(tracks: &[Track]) {
    let cases: Vec<(&Track, [f64; BEATS_PER_BAR], usize)> = tracks
        .iter()
        .filter_map(|track| {
            let offset = track.downbeat_offset?;
            let grid_phase = track.first_downbeat % BEATS_PER_BAR;
            Some((track, bar_phase_votes(&track.features), (grid_phase + BEATS_PER_BAR - offset) % BEATS_PER_BAR))
        })
        .collect();
    for trust in [1.0, 1.25, 1.5, 2.0, 3.0, 5.0, f64::INFINITY] {
        let right = cases
            .iter()
            .filter(|(track, votes, true_phase)| bar_phase(track.features.on_downbeat(), votes, trust) == *true_phase)
            .count();
        println!("  re-picked at trust {trust}: downbeat right on {:.1}% of tracks", share(right, cases.len()));
    }
    println!();
}

fn rekordbox_kind(word: &str) -> Option<SectionKind> {
    match word {
        "INTRO" => Some(SectionKind::Intro),
        "UP" => Some(SectionKind::Build),
        "DOWN" => Some(SectionKind::Breakdown),
        "CHORUS" => Some(SectionKind::Drop),
        "OUTRO" => Some(SectionKind::Outro),
        _ => None,
    }
}

fn starts(sections: &[Section]) -> HashSet<BeatIndex> {
    sections.iter().map(|section| section.first_beat).collect()
}

fn score(tracks: &[Track], tuning: &Tuning) -> Score {
    let mut total = Score::default();
    for track in tracks {
        total.add(&track.cue_beats, &starts(&segment(&track.features, tuning)));
    }
    total
}

fn report_baselines(tracks: &[Track]) {
    for bars in [8, 16, 32] {
        let mut total = Score::default();
        for track in tracks {
            total.add(
                &track.cue_beats,
                &(track.first_downbeat..track.features.beats()).step_by(bars * BEATS_PER_BAR).collect(),
            );
        }
        println!("{}", total.line(&format!("every {bars} bars from the first")));
    }
    let with_phrases: Vec<&Track> = tracks.iter().filter(|t| t.rekordbox_phrase_beats.is_some()).collect();
    if with_phrases.is_empty() {
        return;
    }
    let (mut phrases, mut changes, mut ours) = (Score::default(), Score::default(), Score::default());
    for track in &with_phrases {
        phrases.add(&track.cue_beats, track.rekordbox_phrase_beats.as_ref().unwrap_or(&HashSet::new()));
        changes.add(&track.cue_beats, track.rekordbox_change_beats.as_ref().unwrap_or(&HashSet::new()));
        ours.add(&track.cue_beats, &starts(&segment(&track.features, &Tuning::default())));
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
    let kernels = [[2, 4], [4, 8], [4, 16], [2, 8], [8, 16]]
        .iter()
        .map(|&widths| (format!("kernels {widths:?}"), Tuning { kernel_half_widths: widths, ..*base }))
        .collect();
    vec![
        vary("threshold", &[0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5], |t, v| t.boundary_threshold = v),
        vary("uncommon length", &[0.0, 0.25, 0.5, 1.0], |t, v| t.uncommon_length_cost = v),
        vary("odd length", &[0.5, 1.0, 1.5, 2.0, 3.0], |t, v| t.odd_length_cost = v),
        kernels,
        vary("timbre", &[0.0, 0.5, 1.0, 2.0], |t, v| t.timbre_weight = v),
        vary("harmony", &[0.5, 1.0, 2.0, 3.0], |t, v| t.harmony_weight = v),
        vary("rhythm", &[0.0, 0.5, 1.0, 2.0], |t, v| t.rhythm_weight = v),
        vary("loudness", &[0.0, 0.5, 1.0, 2.0], |t, v| t.loudness_weight = v),
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

/// How often our sections' kinds match rekordbox's over the beats they share (ours by theirs),
/// and how a section leading into a drop is named against rekordbox's UP or DOWN over the same
/// beats: rows are our build and breakdown, columns their up and down.
struct LabelAgreement {
    beats: [[usize; 5]; 5],
    before_drop: [[usize; 2]; 2],
}

fn kind_index(kind: SectionKind) -> usize {
    SectionKind::ALL.iter().position(|&k| k == kind).unwrap_or(0)
}

fn label_agreement(tracks: &[Track], tuning: &Tuning) -> LabelAgreement {
    let mut agreement = LabelAgreement { beats: [[0; 5]; 5], before_drop: [[0; 2]; 2] };
    for track in tracks {
        let Some(theirs) = &track.rekordbox_beat_kinds else { continue };
        let sections = segment(&track.features, tuning);
        for (index, section) in sections.iter().enumerate() {
            let span = section.first_beat..section.first_beat + section.beats;
            for beat in span.clone() {
                if let Some(Some(kind)) = theirs.get(beat) {
                    agreement.beats[kind_index(section.kind)][kind_index(*kind)] += 1;
                }
            }
            let leads_into_drop = sections.get(index + 1).is_some_and(|next| next.kind == SectionKind::Drop);
            let our_side = match section.kind {
                SectionKind::Build => 0,
                SectionKind::Breakdown => 1,
                _ => continue,
            };
            let count = |kind| span.clone().filter(|&beat| theirs.get(beat) == Some(&Some(kind))).count();
            let (up, down) = (count(SectionKind::Build), count(SectionKind::Breakdown));
            if leads_into_drop && up + down > 0 {
                agreement.before_drop[our_side][usize::from(down > up)] += 1;
            }
        }
    }
    agreement
}

fn report_labels(tracks: &[Track], tuning: &Tuning) {
    if tracks.iter().all(|t| t.rekordbox_beat_kinds.is_none()) {
        return;
    }
    let LabelAgreement { beats, before_drop } = label_agreement(tracks, tuning);
    let all_beats: usize = beats.iter().flatten().sum();
    println!(
        "\nlabels against rekordbox's: {:.1}% of beats agree",
        share((0..5).map(|i| beats[i][i]).sum(), all_beats)
    );
    println!(
        "  before drops, [[build: up, down], [breakdown: up, down]] {before_drop:?}: {:.1}% agree",
        share(before_drop[0][0] + before_drop[1][1], before_drop.iter().flatten().sum())
    );
    println!("  ours \\ theirs {}", SectionKind::ALL.iter().map(|k| format!("{:>10}", k.name())).collect::<String>());
    for (ours, row) in SectionKind::ALL.iter().zip(beats) {
        println!("  {:<13} {}", ours.name(), row.iter().map(|n| format!("{n:>10}")).collect::<String>());
    }
}

/// What our sections are called where the DJ cued, as "drop#2" style anchors.
fn report_anchors(tracks: &[Track], tuning: &Tuning) {
    let mut at_cues: BTreeMap<String, usize> = BTreeMap::new();
    let mut everywhere: BTreeMap<String, usize> = BTreeMap::new();
    for track in tracks {
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for section in segment(&track.features, tuning) {
            let occurrence = seen.entry(section.kind.name()).and_modify(|n| *n += 1).or_insert(1);
            let anchor = format!("{}#{occurrence}", section.kind.name());
            *everywhere.entry(anchor.clone()).or_default() += 1;
            if track.cue_beats.contains(&section.first_beat) {
                *at_cues.entry(anchor).or_default() += 1;
            }
        }
    }
    let mut rows: Vec<_> = everywhere.into_iter().collect();
    rows.sort_by_key(|(_, found)| std::cmp::Reverse(*found));
    println!("\n{:<14} {:>6} {:>7}", "section start", "found", "cued");
    for (anchor, found) in rows.into_iter().take(16) {
        let cued = at_cues.get(&anchor).copied().unwrap_or(0);
        println!("{anchor:<14} {found:>6} {:>6.0}%", share(cued, found));
    }
    if let Some(track) = tracks.first() {
        println!("\nexample: {}", track.title);
        let sections = segment(&track.features, tuning);
        for section in &sections {
            let cued = if track.cue_beats.contains(&section.first_beat) { "cued" } else { "" };
            println!(
                "  beat {:>4} {:>5} bars {:<9} energy {:.2} {cued}",
                section.first_beat,
                section.beats as f64 / BEATS_PER_BAR as f64,
                section.kind.name(),
                section.relative_energy
            );
        }
        let mut missed: Vec<_> = track.cue_beats.difference(&starts(&sections)).copied().collect();
        missed.sort_unstable();
        println!("  cues on no section start: beats {missed:?}");
    }
}
