//! Reading a `cue_survey` dump, and scoring placed beats against the DJ's cues, for the examples.

#![allow(dead_code)]

use std::collections::HashSet;

use es_analyse::structure::BeatIndex;
use es_analyse::Beat;
use serde_json::Value;

pub const SURVEY_ON_BEAT_MS: f64 = 15.0;
pub const NEAR_MISS_BEATS: usize = 8;
pub const BEATS_PER_BAR: usize = 4;

pub fn load(path: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub fn share(agreed: usize, total: usize) -> f64 {
    100.0 * agreed as f64 / total.max(1) as f64
}

pub fn numbers(value: &Value) -> Vec<u64> {
    value.as_array().map(|all| all.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

pub fn survey_grid(entry: &Value) -> Vec<Beat> {
    let times = numbers(&entry["grid"]["times"]);
    let beat_numbers = numbers(&entry["grid"]["beats"]);
    times
        .iter()
        .zip(&beat_numbers)
        .map(|(&time_ms, &number)| Beat { number: number as u8, tempo_x100: 0, time_ms: time_ms as u32 })
        .collect()
}

pub fn nearest_beat(beats: &[Beat], ms: f64, tolerance_ms: f64) -> Option<BeatIndex> {
    let after = beats.partition_point(|beat| f64::from(beat.time_ms) < ms);
    let distance = |index: usize| (f64::from(beats[index].time_ms) - ms).abs();
    [after.checked_sub(1), Some(after)]
        .into_iter()
        .flatten()
        .filter(|&index| index < beats.len())
        .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
        .filter(|&index| distance(index) <= tolerance_ms)
}

/// Placed beats against the DJ's cues: on the very beat, or a near miss within a couple of bars.
#[derive(Default, Clone, Copy)]
pub struct Score {
    cues: usize,
    placed: usize,
    hits: usize,
    near_misses: usize,
    tracks: usize,
}

impl Score {
    pub fn add(&mut self, cues: &HashSet<BeatIndex>, placed: &HashSet<BeatIndex>) {
        self.cues += cues.len();
        self.placed += placed.len();
        self.hits += cues.intersection(placed).count();
        self.near_misses += cues
            .iter()
            .filter(|&&cue| !placed.contains(&cue) && placed.iter().any(|&beat| beat.abs_diff(cue) <= NEAR_MISS_BEATS))
            .count();
        self.tracks += 1;
    }

    pub fn merged(self, other: &Score) -> Score {
        Score {
            cues: self.cues + other.cues,
            placed: self.placed + other.placed,
            hits: self.hits + other.hits,
            near_misses: self.near_misses + other.near_misses,
            tracks: self.tracks + other.tracks,
        }
    }

    pub fn recall(&self) -> f64 {
        self.hits as f64 / self.cues.max(1) as f64
    }

    pub fn precision(&self) -> f64 {
        self.hits as f64 / self.placed.max(1) as f64
    }

    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    pub fn line(&self, name: &str) -> String {
        format!(
            "{name:<34} recall {:>5.1}%  precision {:>5.1}%  F1 {:.3}  near misses {:>4.1}%  placed/track {:>5.1}",
            100.0 * self.recall(),
            100.0 * self.precision(),
            self.f1(),
            100.0 * self.near_misses as f64 / self.cues.max(1) as f64,
            self.placed as f64 / self.tracks.max(1) as f64,
        )
    }
}
