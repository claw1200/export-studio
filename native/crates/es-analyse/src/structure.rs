//! Where a track's sections start, and what each one is: intro, build, drop, breakdown or outro.
//!
//! Every bar gets a profile of its timbre, harmony, rhythm and loudness. A section starts where the
//! bars after a line sound unlike the bars before it while each side sounds like itself: Foote's
//! checkerboard novelty over the bars' self-similarity. Dance music changes on phrase lines, so a
//! track's sections only start on one 4-bar grid, at the phase where its novelty lines up best.
//! Each section is then named from its energy and where it falls among the drops.

use crate::downbeat::{self, BANDS};
use crate::{onset, spectrum, Beat, KICK_HZ, ONSET_FRAME, ONSET_HOP};

pub type BarIndex = usize;
pub type BeatIndex = usize;

const HAT_HZ: (f64, f64) = (6_000.0, 16_000.0);
const BEATS_PER_BAR: usize = 4;
const BARS_PER_PHRASE: usize = 4;
const MIN_KERNEL_SIDE: usize = 2;
/// How much of the track's energy range a build's second half must climb above its first.
const MIN_BUILD_RISE: f64 = 0.1;
const FLAT_SPREAD: f64 = 1e-9;
const QUIET_PERCENTILE: f64 = 0.1;
const LOUD_PERCENTILE: f64 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub first_bar: BarIndex,
    pub bars: usize,
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
    pub kernel_half_widths: [usize; 2],
    /// Standard deviations above the track's typical phrase line that a line's novelty must reach.
    pub boundary_threshold: f64,
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
            boundary_threshold: 0.0,
            drop_energy: 0.6,
        }
    }
}

/// What each bar of the grid sounds like, measured once so it can be segmented many ways.
pub struct BarFeatures {
    first_beats: Vec<BeatIndex>,
    timbre: Vec<[f64; BANDS]>,
    harmony: Vec<[f64; 12]>,
    kick: Vec<f64>,
    hats: Vec<f64>,
    loudness: Vec<f64>,
}

impl BarFeatures {
    pub fn measure(mono: &[f32], sample_rate: f64, beats: &[Beat]) -> Self {
        let times: Vec<f64> = beats.iter().map(|beat| f64::from(beat.time_ms) / 1000.0).collect();
        let stft = spectrum::stft(mono, sample_rate, ONSET_FRAME, ONSET_HOP);
        let kick = onset::smooth(&onset::flux(&stft, KICK_HZ.0, KICK_HZ.1), 1);
        let hats = onset::smooth(&onset::flux(&stft, HAT_HZ.0, HAT_HZ.1), 1);
        let per_beat = downbeat::profiles(&stft, &times, &|t| f64::from(kick.at_time(t)));
        let first_beats: Vec<BeatIndex> =
            beats.iter().enumerate().filter(|(_, beat)| beat.number == 1).map(|(index, _)| index).collect();

        let mut bars = Self {
            first_beats: first_beats.clone(),
            timbre: Vec::new(),
            harmony: Vec::new(),
            kick: Vec::new(),
            hats: Vec::new(),
            loudness: Vec::new(),
        };
        for (bar, &first) in first_beats.iter().enumerate() {
            let end = first_beats.get(bar + 1).copied().unwrap_or((first + BEATS_PER_BAR).min(beats.len()));
            let span = first..end;
            bars.timbre.push(mean_rows(&per_beat.bands[span.clone()]));
            bars.harmony.push(unit(mean_rows(&per_beat.chroma[span.clone()])));
            bars.kick.push(mean(&per_beat.kick[span.clone()]));
            bars.hats.push(mean(&times[span.clone()].iter().map(|&t| f64::from(hats.at_time(t))).collect::<Vec<_>>()));
            bars.loudness.push(mean(&per_beat.bands[span].iter().map(loudness).collect::<Vec<_>>()));
        }
        bars
    }

    pub fn bars(&self) -> usize {
        self.first_beats.len()
    }
}

pub fn segment(features: &BarFeatures, tuning: &Tuning) -> Vec<Section> {
    if features.bars() == 0 {
        return Vec::new();
    }
    let starts = section_starts(features, tuning);
    name_sections(features, &starts, tuning)
}

fn section_starts(features: &BarFeatures, tuning: &Tuning) -> Vec<BarIndex> {
    let distances = Distances::between_bars(features, tuning);
    let novelty = tuning
        .kernel_half_widths
        .iter()
        .map(|&half_width| standardise(&distances.novelty(half_width)))
        .fold(vec![0.0; features.bars()], |sum, part| sum.iter().zip(&part).map(|(a, b)| a + b).collect());
    let lines: Vec<BarIndex> = (phrase_phase(&novelty)..features.bars()).step_by(BARS_PER_PHRASE).collect();
    let line_scores = standardise(&lines.iter().map(|&bar| novelty[bar]).collect::<Vec<_>>());

    let mut starts = vec![0];
    for (index, &bar) in lines.iter().enumerate() {
        let score = line_scores[index];
        let beats_previous = index == 0 || score >= line_scores[index - 1];
        let beats_next = line_scores.get(index + 1).is_none_or(|&next| score > next);
        if bar > 0 && score > tuning.boundary_threshold && beats_previous && beats_next {
            starts.push(bar);
        }
    }
    starts
}

/// The bar, 0 to 3, whose 4-bar grid carries the most novelty.
fn phrase_phase(novelty: &[f64]) -> BarIndex {
    let line_total = |phase: BarIndex| novelty.iter().skip(phase).step_by(BARS_PER_PHRASE).sum::<f64>();
    (0..BARS_PER_PHRASE).max_by(|&a, &b| line_total(a).total_cmp(&line_total(b))).unwrap_or(0)
}

struct Distances {
    bars: usize,
    values: Vec<f64>,
}

impl Distances {
    fn between_bars(features: &BarFeatures, tuning: &Tuning) -> Self {
        let bars = features.bars();
        let mut columns: Vec<(Vec<f64>, f64)> = Vec::new();
        let mut add_group = |group: Vec<Vec<f64>>, weight: f64| {
            let share = weight / (group.len() as f64).sqrt();
            columns.extend(group.into_iter().map(|column| (standardise(&column), share)));
        };
        add_group(
            (0..BANDS).map(|band| features.timbre.iter().map(|bar| bar[band]).collect()).collect(),
            tuning.timbre_weight,
        );
        add_group(
            (0..12).map(|pitch| features.harmony.iter().map(|bar| bar[pitch]).collect()).collect(),
            tuning.harmony_weight,
        );
        add_group(vec![features.kick.clone(), features.hats.clone()], tuning.rhythm_weight);
        add_group(vec![features.loudness.clone()], tuning.loudness_weight);

        let mut values = vec![0.0; bars * bars];
        for a in 0..bars {
            for b in (a + 1)..bars {
                let squared: f64 =
                    columns.iter().map(|(column, share)| (share * (column[a] - column[b])).powi(2)).sum();
                values[a * bars + b] = squared.sqrt();
                values[b * bars + a] = squared.sqrt();
            }
        }
        Self { bars, values }
    }

    fn at(&self, a: BarIndex, b: BarIndex) -> f64 {
        self.values[a * self.bars + b]
    }

    fn mean_across(&self, left: std::ops::Range<BarIndex>, right: std::ops::Range<BarIndex>) -> f64 {
        let pairs = (left.len() * right.len()) as f64;
        left.flat_map(|a| right.clone().map(move |b| (a, b))).map(|(a, b)| self.at(a, b)).sum::<f64>() / pairs
    }

    fn mean_within(&self, span: std::ops::Range<BarIndex>) -> f64 {
        let pairs = (span.len() * (span.len() - 1) / 2) as f64;
        let total: f64 =
            span.clone().flat_map(|a| ((a + 1)..span.end).map(move |b| (a, b))).map(|(a, b)| self.at(a, b)).sum();
        total / pairs
    }

    /// For each bar, how unlike the bars before it the bars from it on sound, less how unlike
    /// each side sounds within itself.
    fn novelty(&self, half_width: usize) -> Vec<f64> {
        (0..self.bars)
            .map(|line| {
                let before = line.saturating_sub(half_width)..line;
                let after = line..(line + half_width).min(self.bars);
                if before.len() < MIN_KERNEL_SIDE || after.len() < MIN_KERNEL_SIDE {
                    return 0.0;
                }
                let within = (self.mean_within(before.clone()) + self.mean_within(after.clone())) / 2.0;
                self.mean_across(before, after) - within
            })
            .collect()
    }
}

fn name_sections(features: &BarFeatures, starts: &[BarIndex], tuning: &Tuning) -> Vec<Section> {
    let bar_energy = bar_energies(features);
    let spans: Vec<(BarIndex, BarIndex)> =
        starts.iter().zip(starts.iter().skip(1).copied().chain([features.bars()])).map(|(&a, b)| (a, b)).collect();
    let energies: Vec<f64> = spans.iter().map(|&(a, b)| mean(&bar_energy[a..b])).collect();
    let last = spans.len() - 1;
    let frames_the_track = |index: usize| last > 0 && (index == 0 || index == last);
    let mut is_drop: Vec<bool> = energies
        .iter()
        .enumerate()
        .map(|(index, &energy)| !frames_the_track(index) && energy >= tuning.drop_energy)
        .collect();
    if !is_drop.contains(&true) {
        let loudest_inside =
            (0..=last).filter(|&index| !frames_the_track(index)).max_by(|&a, &b| energies[a].total_cmp(&energies[b]));
        if let Some(index) = loudest_inside {
            is_drop[index] = true;
        }
    }
    let first_drop = is_drop.iter().position(|&drop| drop).unwrap_or(last);
    let last_drop = is_drop.iter().rposition(|&drop| drop).unwrap_or(last);

    spans
        .iter()
        .enumerate()
        .map(|(index, &(first_bar, end_bar))| {
            let leads_into_drop = is_drop.get(index + 1).copied().unwrap_or(false);
            let kind = if last > 0 && index == 0 {
                SectionKind::Intro
            } else if last > 0 && index == last {
                SectionKind::Outro
            } else if is_drop[index] {
                SectionKind::Drop
            } else if index > last_drop {
                SectionKind::Outro
            } else if leads_into_drop && rises(&bar_energy[first_bar..end_bar]) {
                SectionKind::Build
            } else if index < first_drop {
                SectionKind::Intro
            } else {
                SectionKind::Breakdown
            };
            Section {
                first_beat: features.first_beats[first_bar],
                first_bar,
                bars: end_bar - first_bar,
                kind,
                relative_energy: energies[index],
            }
        })
        .collect()
}

/// Loudness stands for energy: a drop's sustained bass masks its kick's onsets, and a riser's noise
/// fills the spectrum as fully as a drop does. The range is taken between percentiles, so a silent
/// tail or one clipped bar doesn't squash every other bar together.
fn bar_energies(features: &BarFeatures) -> Vec<f64> {
    let mut sorted = features.loudness.clone();
    sorted.sort_by(f64::total_cmp);
    let at = |share: f64| sorted[((sorted.len() - 1) as f64 * share).round() as usize];
    let (quiet, loud) = (at(QUIET_PERCENTILE), at(LOUD_PERCENTILE));
    if loud - quiet < FLAT_SPREAD {
        return vec![1.0; sorted.len()];
    }
    features.loudness.iter().map(|level| ((level - quiet) / (loud - quiet)).clamp(0.0, 1.0)).collect()
}

fn rises(energies: &[f64]) -> bool {
    let half = energies.len() / 2;
    half > 0 && mean(&energies[half..]) - mean(&energies[..half]) > MIN_BUILD_RISE
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
        bars: usize,
        kick: bool,
        bass: bool,
        hats: bool,
        pad: bool,
        riser: bool,
    }

    const SILENT: Part = Part { bars: 0, kick: false, bass: false, hats: false, pad: false, riser: false };

    fn render(parts: &[Part]) -> (Vec<f32>, Vec<Beat>) {
        let beat_seconds = 60.0 / BPM;
        let mut audio = Vec::new();
        let mut beats = Vec::new();
        let mut noise = 7u32;
        let mut beat_count = 0usize;
        for part in parts {
            let samples = (part.bars as f64 * 4.0 * beat_seconds * RATE) as usize;
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
            for _ in 0..part.bars * 4 {
                let time_ms = (beat_count as f64 * beat_seconds * 1000.0).round() as u32;
                beats.push(Beat { number: (beat_count % 4) as u8 + 1, tempo_x100: 12_800, time_ms });
                beat_count += 1;
            }
        }
        (audio, beats)
    }

    #[test]
    fn finds_and_names_the_sections_of_a_dance_track() {
        let intro = Part { bars: 16, kick: true, hats: true, ..SILENT };
        let drop = Part { bars: 16, kick: true, bass: true, hats: true, pad: true, ..SILENT };
        let breakdown = Part { bars: 16, pad: true, ..SILENT };
        let build = Part { bars: 8, pad: true, riser: true, ..SILENT };
        let outro = Part { bars: 16, kick: true, ..SILENT };
        let (audio, beats) = render(&[intro, drop, breakdown, build, drop, outro]);

        let sections = segment(&BarFeatures::measure(&audio, RATE, &beats), &Tuning::default());

        let found: Vec<(BarIndex, SectionKind)> = sections.iter().map(|s| (s.first_bar, s.kind)).collect();
        assert_eq!(
            found,
            [
                (0, SectionKind::Intro),
                (16, SectionKind::Drop),
                (32, SectionKind::Breakdown),
                (48, SectionKind::Build),
                (56, SectionKind::Drop),
                (72, SectionKind::Outro),
            ]
        );
        assert_eq!(sections[1].first_beat, 64);
    }

    #[test]
    fn a_grid_without_bars_has_no_sections() {
        assert!(segment(&BarFeatures::measure(&[0.0; 4410], RATE, &[]), &Tuning::default()).is_empty());
    }
}
