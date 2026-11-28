//! How well a cue style learnt from some of a DJ's tracks places their memory cues on the rest,
//! using a dump from `es-index`'s `cue_survey` example. Tracks are split into folds; each fold's
//! cues are placed by a style learnt from all the others, and compared with where the DJ put them.
//! `cargo run --release -p es-analyse --example cue_style_eval -- survey.json`

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use es_analyse::cue_style::{CueRule, CuedTrack, HotCueSlot, HotCueStyle, Learning, MemoryCueStyle, Which};
use es_analyse::structure::{segment, BeatFeatures, BeatIndex, Section, Tuning};
use es_analyse::{decode_mono, SAMPLE_RATE};
use rayon::prelude::*;
use serde_json::Value;

#[path = "survey/mod.rs"]
mod survey;
use survey::{nearest_beat, survey_grid, Score, BEATS_PER_BAR, SURVEY_ON_BEAT_MS};

const FOLDS: usize = 10;
const MEMORY_CUE_LIMIT: usize = 10;

struct Track {
    sections: Vec<Section>,
    beat_count: usize,
    first_downbeat: BeatIndex,
    memory_cues: HashSet<BeatIndex>,
    hot_cues: Vec<(HotCueSlot, BeatIndex)>,
}

impl Track {
    fn cued(&self, cues: HashSet<BeatIndex>) -> CuedTrack {
        CuedTrack { sections: self.sections.clone(), beat_count: self.beat_count, cues }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("pass the survey JSON from cue_survey")?;
    let survey = survey::load(&path)?;
    let tracks: Vec<Track> = survey.par_iter().filter_map(measure).collect();
    let memory: Vec<&Track> = tracks.iter().filter(|track| !track.memory_cues.is_empty()).collect();
    println!("measured {} of {} tracks, {} with memory cues\n", tracks.len(), survey.len(), memory.len());

    report_baselines(&memory);
    for least_track_share in [0.02, 0.05, 0.1, 0.2] {
        let score = cross_validate(&memory, &Learning { least_track_share });
        println!("{}", score.line(&format!("learnt, rules on {}%+ of tracks", least_track_share * 100.0)));
    }
    report_style(&memory);
    report_missed(&memory);
    report_hot_cues(&tracks);
    Ok(())
}

fn measure(entry: &Value) -> Option<Track> {
    let beats = survey_grid(entry);
    if beats.len() < 2 {
        return None;
    }
    let audio = decode_mono(Path::new(entry["path"].as_str()?)).ok()?;
    let sections = segment(&BeatFeatures::measure(&audio, f64::from(SAMPLE_RATE), &beats), &Tuning::default());
    let mut memory_cues = HashSet::new();
    let mut hot_cues = Vec::new();
    for cue in entry["cues"].as_array()? {
        let Some(beat) = cue["startMs"].as_f64().and_then(|ms| nearest_beat(&beats, ms, SURVEY_ON_BEAT_MS)) else {
            continue;
        };
        match cue["hotSlot"].as_str().and_then(|letter| letter.bytes().next()) {
            Some(letter) => hot_cues.push((letter - b'A' + 1, beat)),
            None => {
                memory_cues.insert(beat);
            }
        }
    }
    Some(Track {
        sections,
        beat_count: beats.len(),
        first_downbeat: beats.iter().position(|beat| beat.number == 1).unwrap_or(0),
        memory_cues,
        hot_cues,
    })
}

fn report_baselines(tracks: &[&Track]) {
    let (mut every_start, mut every_16_bars) = (Score::default(), Score::default());
    for track in tracks {
        every_start.add(&track.memory_cues, &track.sections.iter().map(|s| s.first_beat).collect());
        let grid = (track.first_downbeat..track.beat_count).step_by(16 * BEATS_PER_BAR);
        every_16_bars.add(&track.memory_cues, &grid.collect());
    }
    println!("{}", every_start.line("a cue on every section start"));
    println!("{}", every_16_bars.line("a cue every 16 bars"));
}

fn cross_validate(tracks: &[&Track], learning: &Learning) -> Score {
    let scores: Vec<Score> = (0..FOLDS)
        .into_par_iter()
        .map(|fold| {
            let training: Vec<CuedTrack> = tracks
                .iter()
                .enumerate()
                .filter(|(index, _)| index % FOLDS != fold)
                .map(|(_, track)| track.cued(track.memory_cues.clone()))
                .collect();
            let style = MemoryCueStyle::learn(&training, learning);
            let mut score = Score::default();
            for track in tracks.iter().enumerate().filter(|(index, _)| index % FOLDS == fold).map(|(_, track)| track) {
                let placed = style.place(&track.sections, track.beat_count, MEMORY_CUE_LIMIT);
                score.add(&track.memory_cues, &placed.into_iter().collect());
            }
            score
        })
        .collect();
    scores.into_iter().fold(Score::default(), |total, fold| total.merged(&fold))
}

fn describe(rule: &CueRule) -> String {
    let which = match rule.which {
        Which::Nth(n) => format!("#{n}"),
        Which::Last => "last".to_owned(),
        Which::Every => "every".to_owned(),
    };
    let kind = rule.kind.map_or("section", |kind| kind.name());
    let offset = match rule.bars_from_start {
        0 => "start".to_owned(),
        bars => format!("{bars:+} bars"),
    };
    let repeat = rule.then_every.map(|bars| format!(", then every {bars} bars")).unwrap_or_default();
    format!("{which} {kind}, {offset}{repeat}")
}

fn report_style(tracks: &[&Track]) {
    let training: Vec<CuedTrack> = tracks.iter().map(|track| track.cued(track.memory_cues.clone())).collect();
    let style = MemoryCueStyle::learn(&training, &Learning::default());
    println!("\nmemory cue style learnt from all {} tracks:", tracks.len());
    for learnt in &style.rules {
        println!(
            "  {:<32} finds {:>4} cues, misplaces {:>4}",
            describe(&learnt.rule),
            learnt.cues_found,
            learnt.cues_misplaced
        );
    }
}

/// Where the cues a style learnt from every track still misses sit, as bars into the section that
/// holds them and how long that section is.
fn report_missed(tracks: &[&Track]) {
    let training: Vec<CuedTrack> = tracks.iter().map(|track| track.cued(track.memory_cues.clone())).collect();
    let style = MemoryCueStyle::learn(&training, &Learning::default());
    let mut into: BTreeMap<String, usize> = BTreeMap::new();
    let mut missed = 0;
    for track in tracks {
        let placed: HashSet<BeatIndex> =
            style.place(&track.sections, track.beat_count, MEMORY_CUE_LIMIT).into_iter().collect();
        for &cue in track.memory_cues.difference(&placed) {
            missed += 1;
            let Some(section) = track.sections.iter().rev().find(|section| section.first_beat <= cue) else { continue };
            let bars = |beats: usize| beats as f64 / BEATS_PER_BAR as f64;
            let key = format!(
                "{:>5} bars into a {:>4}-bar {}",
                bars(cue - section.first_beat),
                bars(section.beats),
                section.kind.name()
            );
            *into.entry(key).or_default() += 1;
        }
    }
    let mut rows: Vec<_> = into.into_iter().collect();
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    println!("\n{missed} cues missed; the commonest places:");
    for (place, count) in rows.into_iter().take(20) {
        println!("  {count:>4}  {place}");
    }
}

fn report_hot_cues(tracks: &[Track]) {
    let mut by_pad: BTreeMap<HotCueSlot, Vec<CuedTrack>> = BTreeMap::new();
    for track in tracks {
        for &(slot, beat) in &track.hot_cues {
            by_pad.entry(slot).or_default().push(track.cued(HashSet::from([beat])));
        }
    }
    if by_pad.is_empty() {
        return;
    }
    let by_pad: Vec<(HotCueSlot, Vec<CuedTrack>)> = by_pad.into_iter().collect();
    let style = HotCueStyle::learn(&by_pad, &Learning::default());
    println!("\nhot cue style ({} hot cues in the survey):", by_pad.iter().map(|(_, t)| t.len()).sum::<usize>());
    for (slot, learnt) in &style.pads {
        println!("  pad {}: {}", char::from(b'A' + slot - 1), describe(&learnt.rule));
    }
}
