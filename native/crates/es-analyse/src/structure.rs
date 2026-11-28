//! Where a track's sections start, and what each one is: intro, build, drop, breakdown or outro.
//!
//! Every beat gets a profile of the bar that starts on it: its timbre, harmony, rhythm and loudness.
//! A beat is a likely section start when the bars counted on from it sound unlike the bars counted
//! back from it, while each side sounds like itself: Foote's checkerboard novelty over the bars'
//! self-similarity. Counting bars from every beat, rather than from the grid's downbeats, keeps a
//! downbeat that is off by a beat from moving every section with it.
//!
//! The starts are then chosen together, by dynamic programming: each section's length is counted
//! from the start before it, and lengths of 8, 16 and 32 bars cost nothing while others cost a
//! little. A section of an odd length is taken when the music clearly changes there, and every
//! section after it follows on from it rather than from a grid fixed for the whole track.

use crate::downbeat::{self, BANDS};
use crate::{onset, spectrum, Beat, KICK_HZ, ONSET_FRAME, ONSET_HOP};

pub type BeatIndex = usize;
pub type Beats = usize;
pub type Bars = usize;

const HAT_HZ: (f64, f64) = (6_000.0, 16_000.0);
const BEATS_PER_BAR: Beats = 4;
const MIN_KERNEL_SIDE: Bars = 2;
const PHASE_PEAK_REACH: Beats = 2;
/// Changes in the first bars are left out of the phase vote: the comparison there is cut short.
const PHASE_VOTE_FROM: BeatIndex = 2 * BEATS_PER_BAR;
/// Standard deviations of bar-to-bar novelty a change needs to vote, and the most its vote can weigh.
const PHASE_VOTE_FLOOR: f64 = 1.0;
const PHASE_VOTE_CAP: f64 = 3.0;
const LONGEST_SECTION: Bars = 64;
const FLAT_SPREAD: f64 = 1e-9;
const QUIET_PERCENTILE: f64 = 0.1;
const LOUD_PERCENTILE: f64 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    Intro,
    Build,
    Drop,
    Breakdown,
    Outro,
}

impl SectionKind {
    pub const ALL: [Self; 5] = [Self::Intro, Self::Build, Self::Drop, Self::Breakdown, Self::Outro];

    pub fn name(self) -> &'static str {
        match self {
            Self::Intro => "intro",
            Self::Build => "build",
            Self::Drop => "drop",
            Self::Breakdown => "breakdown",
            Self::Outro => "outro",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub first_beat: BeatIndex,
    pub beats: Beats,
    pub kind: SectionKind,
    /// 0 at the track's quiet end, 1 at its loud end.
    pub relative_energy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    pub timbre_weight: f64,
    pub harmony_weight: f64,
    pub rhythm_weight: f64,
    pub loudness_weight: f64,
    pub kernel_half_widths: [Bars; 2],
    /// Standard deviations of novelty a beat must stand above the track's typical beat to start a section.
    pub boundary_threshold: f64,
    /// Novelty given up for a section of 4, 12, 24, 48 or 64 bars rather than 8, 16 or 32.
    pub uncommon_length_cost: f64,
    /// Novelty given up for a section of any other whole number of bars.
    pub odd_length_cost: f64,
    /// How far from the quietest section towards the most energetic a section must reach to be a drop.
    pub drop_energy: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            timbre_weight: 1.0,
            harmony_weight: 2.0,
            rhythm_weight: 1.0,
            loudness_weight: 0.5,
            kernel_half_widths: [4, 8],
            boundary_threshold: 0.25,
            uncommon_length_cost: 0.5,
            odd_length_cost: 3.0,
            drop_energy: 0.6,
        }
    }
}

/// What the bar starting on each beat of the grid sounds like, measured once so it can be
/// segmented many ways.
pub struct BeatFeatures {
    on_downbeat: Vec<bool>,
    timbre: Vec<[f64; BANDS]>,
    harmony: Vec<[f64; 12]>,
    kick: Vec<f64>,
    hats: Vec<f64>,
    loudness: Vec<f64>,
}

impl BeatFeatures {
    pub fn measure(mono: &[f32], sample_rate: f64, beats: &[Beat]) -> Self {
        let times: Vec<f64> = beats.iter().map(|beat| f64::from(beat.time_ms) / 1000.0).collect();
        let stft = spectrum::stft(mono, sample_rate, ONSET_FRAME, ONSET_HOP);
        let kick = onset::smooth(&onset::flux(&stft, KICK_HZ.0, KICK_HZ.1), 1);
        let hats = onset::smooth(&onset::flux(&stft, HAT_HZ.0, HAT_HZ.1), 1);
        let per_beat = downbeat::profiles(&stft, &times, &|t| f64::from(kick.at_time(t)));
        let hats_per_beat: Vec<f64> = times.iter().map(|&t| f64::from(hats.at_time(t))).collect();
        let loudness_per_beat: Vec<f64> = per_beat.bands.iter().map(loudness).collect();
        let bar_from = |first: BeatIndex| first..(first + BEATS_PER_BAR).min(beats.len());
        Self {
            on_downbeat: beats.iter().map(|beat| beat.number == 1).collect(),
            timbre: (0..beats.len()).map(|beat| mean_rows(&per_beat.bands[bar_from(beat)])).collect(),
            harmony: (0..beats.len()).map(|beat| unit(mean_rows(&per_beat.chroma[bar_from(beat)]))).collect(),
            kick: (0..beats.len()).map(|beat| mean(&per_beat.kick[bar_from(beat)])).collect(),
            hats: (0..beats.len()).map(|beat| mean(&hats_per_beat[bar_from(beat)])).collect(),
            loudness: (0..beats.len()).map(|beat| mean(&loudness_per_beat[bar_from(beat)])).collect(),
        }
    }

    pub fn on_downbeat(&self) -> &[bool] {
        &self.on_downbeat
    }

    /// The same beats with their bars counted from `phase`, the beat of the bar, 0 to 3.
    #[must_use]
    pub fn counting_bars_from(self, phase: Beats) -> Self {
        let on_downbeat = (0..self.beats()).map(|beat| beat % BEATS_PER_BAR == phase).collect();
        Self { on_downbeat, ..self }
    }

    pub fn beats(&self) -> Beats {
        self.on_downbeat.len()
    }
}

pub fn segment(features: &BeatFeatures, tuning: &Tuning) -> Vec<Section> {
    if features.beats() == 0 {
        return Vec::new();
    }
    let starts = section_starts(features, tuning);
    name_sections(features, &starts, tuning)
}

fn novelty(distances: &Distances, tuning: &Tuning) -> Vec<f64> {
    let summed = tuning
        .kernel_half_widths
        .iter()
        .map(|&half_width| standardise(&distances.novelty(half_width)))
        .fold(vec![0.0; distances.beats], |sum, part| sum.iter().zip(&part).map(|(a, b)| a + b).collect());
    standardise(&summed)
}

fn length_cost(bars: Bars, tuning: &Tuning) -> f64 {
    match bars {
        8 | 16 | 32 => 0.0,
        4 | 12 | 24 | 48 | 64 => tuning.uncommon_length_cost,
        _ => tuning.odd_length_cost,
    }
}

fn grid_phase(on_downbeat: &[bool]) -> Beats {
    on_downbeat.iter().position(|&downbeat| downbeat).unwrap_or(0) % BEATS_PER_BAR
}

/// How strongly the music changes on each beat of the bar, 0 to 3 counted from the grid's first
/// beat. Each bar is compared only with the one before, since wider comparisons barely notice a
/// shift of one beat. Every clear change then votes for the beat of the bar it peaks on, its vote
/// capped so one outsized change can't outvote the rest.
pub fn bar_phase_votes(features: &BeatFeatures) -> [f64; BEATS_PER_BAR] {
    let tuning = Tuning::default();
    bar_phase_votes_between(&Distances::between_beats(features, &tuning, 2 * BEATS_PER_BAR))
}

fn bar_phase_votes_between(distances: &Distances) -> [f64; BEATS_PER_BAR] {
    let novelty = standardise(&distances.novelty(1));
    let peaks_on = |beat: BeatIndex| {
        let around = beat.saturating_sub(PHASE_PEAK_REACH)..(beat + PHASE_PEAK_REACH + 1).min(novelty.len());
        around.into_iter().all(|other| novelty[other] <= novelty[beat])
    };
    let mut votes = [0.0; BEATS_PER_BAR];
    for beat in (PHASE_VOTE_FROM..novelty.len()).filter(|&beat| novelty[beat] > PHASE_VOTE_FLOOR && peaks_on(beat)) {
        votes[beat % BEATS_PER_BAR] += novelty[beat].min(PHASE_VOTE_CAP);
    }
    votes
}

/// The beat of the bar, 0 to 3, that bars start on: the grid's downbeat, unless the music changes
/// on another beat of the bar `trust` times as strongly across the whole track.
pub fn bar_phase(on_downbeat: &[bool], votes: &[f64; BEATS_PER_BAR], trust: f64) -> Beats {
    let grid_phase = grid_phase(on_downbeat);
    let strongest = (0..BEATS_PER_BAR).max_by(|&a, &b| votes[a].total_cmp(&votes[b])).unwrap_or(grid_phase);
    if votes[strongest] > votes[grid_phase] * trust {
        strongest
    } else {
        grid_phase
    }
}

/// The section starts that earn the most novelty, less what their lengths cost. The track's first
/// beat always starts a section, and the others fall on the grid's downbeats; each section's length
/// is counted from the start before it, so after an odd-length section the rest follow on from it.
fn section_starts(features: &BeatFeatures, tuning: &Tuning) -> Vec<BeatIndex> {
    let widest = tuning.kernel_half_widths.iter().copied().max().unwrap_or(1);
    let distances = Distances::between_beats(features, tuning, 2 * widest * BEATS_PER_BAR);
    let novelty = novelty(&distances, tuning);
    let lines: Vec<BeatIndex> =
        (grid_phase(&features.on_downbeat)..features.beats()).step_by(BEATS_PER_BAR).filter(|&b| b > 0).collect();
    let is_peak = |index: usize| {
        let around = index.saturating_sub(1)..(index + 2).min(lines.len());
        around.into_iter().all(|other| novelty[lines[other]] <= novelty[lines[index]])
    };

    let mut best = vec![f64::NEG_INFINITY; lines.len()];
    let mut previous: Vec<Option<usize>> = vec![None; lines.len()];
    for index in (0..lines.len()).filter(|&index| is_peak(index)) {
        let gain = novelty[lines[index]] - tuning.boundary_threshold;
        best[index] = gain;
        for bars in 1..=LONGEST_SECTION.min(index) {
            let before = index - bars;
            let total = best[before] + gain - length_cost(bars, tuning);
            if total > best[index] {
                best[index] = total;
                previous[index] = Some(before);
            }
        }
    }

    let last = (0..lines.len()).filter(|&index| best[index] > 0.0).max_by(|&a, &b| best[a].total_cmp(&best[b]));
    let mut chosen: Vec<BeatIndex> =
        std::iter::successors(last, |&index| previous[index]).map(|index| lines[index]).collect();
    chosen.push(0);
    chosen.reverse();
    chosen
}

/// Distances between the profiles of beats no further apart than `reach`.
struct Distances {
    beats: Beats,
    reach: Beats,
    values: Vec<f64>,
}

impl Distances {
    fn between_beats(features: &BeatFeatures, tuning: &Tuning, reach: Beats) -> Self {
        let beats = features.beats();
        let mut columns: Vec<(Vec<f64>, f64)> = Vec::new();
        let mut add_group = |group: Vec<Vec<f64>>, weight: f64| {
            let share = weight / (group.len() as f64).sqrt();
            columns.extend(group.into_iter().map(|column| (standardise(&column), share)));
        };
        add_group(
            (0..BANDS).map(|band| features.timbre.iter().map(|beat| beat[band]).collect()).collect(),
            tuning.timbre_weight,
        );
        add_group(
            (0..12).map(|pitch| features.harmony.iter().map(|beat| beat[pitch]).collect()).collect(),
            tuning.harmony_weight,
        );
        add_group(vec![features.kick.clone(), features.hats.clone()], tuning.rhythm_weight);
        add_group(vec![features.loudness.clone()], tuning.loudness_weight);

        let mut values = vec![0.0; beats * reach];
        for a in 0..beats {
            for offset in 1..=reach.min(beats - 1 - a) {
                let b = a + offset;
                let squared: f64 =
                    columns.iter().map(|(column, share)| (share * (column[a] - column[b])).powi(2)).sum();
                values[a * reach + offset - 1] = squared.sqrt();
            }
        }
        Self { beats, reach, values }
    }

    fn at(&self, a: BeatIndex, b: BeatIndex) -> f64 {
        let (low, high) = if a < b { (a, b) } else { (b, a) };
        if low == high {
            return 0.0;
        }
        self.values[low * self.reach + (high - low) - 1]
    }

    fn mean_across(&self, left: &[BeatIndex], right: &[BeatIndex]) -> f64 {
        let pairs = (left.len() * right.len()) as f64;
        left.iter().flat_map(|&a| right.iter().map(move |&b| self.at(a, b))).sum::<f64>() / pairs
    }

    fn mean_within(&self, bars: &[BeatIndex]) -> f64 {
        if bars.len() < 2 {
            return 0.0;
        }
        let pairs = (bars.len() * (bars.len() - 1) / 2) as f64;
        let total: f64 =
            bars.iter().enumerate().flat_map(|(i, &a)| bars[i + 1..].iter().map(move |&b| self.at(a, b))).sum();
        total / pairs
    }

    /// For each beat, how unlike the bars counted back from it the bars counted on from it sound,
    /// less how unlike each side sounds within itself.
    fn novelty(&self, half_width: Bars) -> Vec<f64> {
        (0..self.beats)
            .map(|line| {
                let before: Vec<BeatIndex> =
                    (1..=half_width).filter_map(|bar| line.checked_sub(bar * BEATS_PER_BAR)).collect();
                let after: Vec<BeatIndex> = (0..half_width)
                    .map(|bar| line + bar * BEATS_PER_BAR)
                    .filter(|&first| first + BEATS_PER_BAR <= self.beats)
                    .collect();
                if before.len() < half_width.min(MIN_KERNEL_SIDE) || after.len() < half_width.min(MIN_KERNEL_SIDE) {
                    return 0.0;
                }
                let within = (self.mean_within(&before) + self.mean_within(&after)) / 2.0;
                self.mean_across(&before, &after) - within
            })
            .collect()
    }
}

fn name_sections(features: &BeatFeatures, starts: &[BeatIndex], tuning: &Tuning) -> Vec<Section> {
    let energy = beat_energies(features);
    let spans: Vec<(BeatIndex, BeatIndex)> =
        starts.iter().zip(starts.iter().skip(1).copied().chain([features.beats()])).map(|(&a, b)| (a, b)).collect();
    let energies: Vec<f64> = spans.iter().map(|&(a, b)| mean(&energy[a..b])).collect();
    let last = spans.len() - 1;
    let opens_the_track = |index: usize| last > 0 && index == 0;
    let mut is_drop: Vec<bool> = energies
        .iter()
        .enumerate()
        .map(|(index, &energy)| !opens_the_track(index) && energy >= tuning.drop_energy)
        .collect();
    if !is_drop.contains(&true) {
        let loudest_inside =
            (0..=last).filter(|&index| !opens_the_track(index)).max_by(|&a, &b| energies[a].total_cmp(&energies[b]));
        if let Some(index) = loudest_inside {
            is_drop[index] = true;
        }
    }
    let first_drop = is_drop.iter().position(|&drop| drop).unwrap_or(last);
    let last_drop = is_drop.iter().rposition(|&drop| drop).unwrap_or(last);

    spans
        .iter()
        .enumerate()
        .map(|(index, &(first_beat, end_beat))| {
            let leads_into_drop = is_drop.get(index + 1).copied().unwrap_or(false);
            let kind = if opens_the_track(index) {
                SectionKind::Intro
            } else if is_drop[index] {
                SectionKind::Drop
            } else if index > last_drop {
                SectionKind::Outro
            } else if leads_into_drop {
                SectionKind::Build
            } else if index < first_drop {
                SectionKind::Intro
            } else {
                SectionKind::Breakdown
            };
            Section { first_beat, beats: end_beat - first_beat, kind, relative_energy: energies[index] }
        })
        .collect()
}

/// Loudness stands for energy: a drop's sustained bass masks its kick's onsets, and a riser's noise
/// fills the spectrum as fully as a drop does. The range is taken between percentiles, so a silent
/// tail or one clipped beat doesn't squash every other beat together.
fn beat_energies(features: &BeatFeatures) -> Vec<f64> {
    between_percentiles(&features.loudness)
}

fn between_percentiles(values: &[f64]) -> Vec<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |share: f64| sorted[((sorted.len() - 1) as f64 * share).round() as usize];
    let (low, high) = (at(QUIET_PERCENTILE), at(LOUD_PERCENTILE));
    if high - low < FLAT_SPREAD {
        return vec![1.0; values.len()];
    }
    values.iter().map(|value| ((value - low) / (high - low)).clamp(0.0, 1.0)).collect()
}

fn loudness(bands: &[f64; BANDS]) -> f64 {
    bands.iter().map(|log_energy| 10f64.powf(*log_energy)).sum::<f64>().log10()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn mean_rows<const N: usize>(rows: &[[f64; N]]) -> [f64; N] {
    let mut sum = [0.0; N];
    for row in rows {
        for (total, value) in sum.iter_mut().zip(row) {
            *total += value;
        }
    }
    sum.map(|total| total / rows.len().max(1) as f64)
}

fn unit<const N: usize>(values: [f64; N]) -> [f64; N] {
    let norm = values.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm > 0.0 {
        values.map(|v| v / norm)
    } else {
        values
    }
}

fn standardise(values: &[f64]) -> Vec<f64> {
    let average = mean(values);
    let spread = (values.iter().map(|v| (v - average).powi(2)).sum::<f64>() / values.len().max(1) as f64).sqrt();
    if spread < FLAT_SPREAD {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| (v - average) / spread).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    const RATE: f64 = 44_100.0;
    const BPM: f64 = 128.0;

    #[derive(Clone, Copy)]
    struct Part {
        beats: usize,
        kick: bool,
        bass: bool,
        hats: bool,
        pad: bool,
        riser: bool,
    }

    const SILENT: Part = Part { beats: 0, kick: false, bass: false, hats: false, pad: false, riser: false };

    fn bars(count: usize, part: Part) -> Part {
        Part { beats: count * BEATS_PER_BAR, ..part }
    }

    /// Audio for `parts` in turn, and a grid whose bars start `downbeat_offset` beats in.
    fn render(parts: &[Part], downbeat_offset: usize) -> (Vec<f32>, Vec<Beat>) {
        let beat_seconds = 60.0 / BPM;
        let mut audio = Vec::new();
        let mut noise = 7u32;
        for part in parts {
            let samples = (part.beats as f64 * beat_seconds * RATE) as usize;
            for i in 0..samples {
                let t = i as f64 / RATE;
                let progress = i as f64 / samples as f64;
                let since_beat = t % beat_seconds;
                let off_beat = (t + beat_seconds / 2.0) % beat_seconds;
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let white = f64::from(noise >> 9) / f64::from(1u32 << 23) - 1.0;
                let mut v = 0.0;
                if part.kick {
                    v += 0.8 * (-since_beat * 18.0).exp() * (TAU * 55.0 * since_beat).sin();
                }
                if part.bass {
                    v += 0.3 * (TAU * 41.2 * t).sin();
                }
                if part.hats {
                    v += 0.15 * (-off_beat * 60.0).exp() * white;
                }
                if part.pad {
                    v += 0.06 * [261.63, 311.13, 392.0].iter().map(|hz| (TAU * hz * t).sin()).sum::<f64>();
                }
                if part.riser {
                    v += 0.3 * progress * white;
                }
                audio.push(v as f32);
            }
        }
        let total: usize = parts.iter().map(|part| part.beats).sum();
        let beats = (0..total)
            .map(|index| Beat {
                number: ((index + BEATS_PER_BAR - downbeat_offset % BEATS_PER_BAR) % BEATS_PER_BAR) as u8 + 1,
                tempo_x100: 12_800,
                time_ms: (index as f64 * beat_seconds * 1000.0).round() as u32,
            })
            .collect();
        (audio, beats)
    }

    fn found(parts: &[Part], downbeat_offset: usize) -> Vec<(BeatIndex, SectionKind)> {
        let (audio, beats) = render(parts, downbeat_offset);
        let sections = segment(&BeatFeatures::measure(&audio, RATE, &beats), &Tuning::default());
        sections.iter().map(|section| (section.first_beat, section.kind)).collect()
    }

    const INTRO: Part = Part { kick: true, hats: true, ..SILENT };
    const DROP: Part = Part { kick: true, bass: true, hats: true, pad: true, ..SILENT };
    const BREAKDOWN: Part = Part { pad: true, ..SILENT };
    const BUILD: Part = Part { pad: true, riser: true, ..SILENT };
    const OUTRO: Part = Part { kick: true, ..SILENT };

    #[test]
    fn finds_and_names_the_sections_of_a_dance_track() {
        let parts =
            [bars(16, INTRO), bars(16, DROP), bars(16, BREAKDOWN), bars(8, BUILD), bars(16, DROP), bars(16, OUTRO)];
        assert_eq!(
            found(&parts, 0),
            [
                (0, SectionKind::Intro),
                (64, SectionKind::Drop),
                (128, SectionKind::Breakdown),
                (192, SectionKind::Build),
                (224, SectionKind::Drop),
                (288, SectionKind::Outro),
            ]
        );
    }

    #[test]
    fn bars_move_off_the_grid_downbeat_only_for_clearly_stronger_changes() {
        let grid_on_beat_one = [false, true, false, false, false, true];
        assert_eq!(bar_phase(&grid_on_beat_one, &[2.0, 3.0, 0.0, 0.0], 3.0), 1);
        assert_eq!(bar_phase(&grid_on_beat_one, &[10.0, 3.0, 0.0, 0.0], 3.0), 0);
        assert_eq!(bar_phase(&grid_on_beat_one, &[0.0, 0.0, 0.0, 0.0], 3.0), 1);
    }

    #[test]
    fn a_short_sweep_before_the_intro_does_not_shift_the_sections_after_it() {
        let sweep = Part { beats: 2, riser: true, ..SILENT };
        let parts = [sweep, bars(16, INTRO), bars(16, DROP), bars(16, BREAKDOWN), bars(16, DROP), bars(16, OUTRO)];
        let starts: Vec<BeatIndex> = found(&parts, 2).iter().map(|&(beat, _)| beat).collect();
        assert_eq!(starts, [0, 66, 130, 194, 258]);
    }

    #[test]
    fn sections_after_an_odd_length_one_count_on_from_it() {
        let parts = [bars(16, INTRO), bars(7, BREAKDOWN), bars(16, DROP), bars(16, BREAKDOWN), bars(16, OUTRO)];
        let starts: Vec<BeatIndex> = found(&parts, 0).iter().map(|&(beat, _)| beat).collect();
        assert_eq!(starts, [0, 64, 92, 156, 220]);
    }

    #[test]
    fn a_last_drop_that_runs_to_the_end_keeps_the_sections_before_it_out_of_the_outro() {
        let parts = [bars(16, INTRO), bars(16, DROP), bars(16, BREAKDOWN), bars(8, BUILD), bars(32, DROP)];
        let kinds: Vec<SectionKind> = found(&parts, 0).iter().map(|&(_, kind)| kind).collect();
        assert_eq!(
            kinds,
            [SectionKind::Intro, SectionKind::Drop, SectionKind::Breakdown, SectionKind::Build, SectionKind::Drop]
        );
    }

    #[test]
    fn a_grid_without_beats_has_no_sections() {
        assert!(segment(&BeatFeatures::measure(&[0.0; 4410], RATE, &[]), &Tuning::default()).is_empty());
    }
}
