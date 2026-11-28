//! A DJ's way of placing cues, learnt from tracks they have cued and replayed on new ones.
//!
//! A style is a list of rules, each a number of bars from the start of some section: "on every
//! drop", "16 bars before the first drop", "on the second breakdown", "on every drop and every 16
//! bars through it". Rules are tied to sections rather than to bar counts from the top, because no
//! two tracks share a layout.
//!
//! Learning is greedy. Every rule that could be written is tried on every cued track; the rule that
//! most raises the F1 score of the kept rules' cues against the DJ's (balancing cues found against
//! cues put where the DJ had none) is kept, and the search repeats until no rule raises it. Memory cues learn a set of rules. Hot cues learn one rule per pad, by the same
//! search held to rules that give a single cue.

use std::collections::HashSet;

use crate::structure::{BeatIndex, Section, SectionKind};

pub type BarOffset = i32;
pub type Bars = u16;
pub type HotCueSlot = u8;

const BEATS_PER_BAR: i64 = 4;
const OFFSETS: [BarOffset; 9] = [-32, -16, -8, -4, 0, 4, 8, 16, 32];
const REPEATS: [Bars; 3] = [8, 16, 32];
const LATEST_OCCURRENCE: u8 = 4;
/// Cues nearer than this to one already placed are left out.
const MIN_GAP_BEATS: usize = 4;

/// Which of the sections of a kind a rule counts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Which {
    /// 1-based.
    Nth(u8),
    Last,
    Every,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CueRule {
    /// `None` counts sections of every kind.
    pub kind: Option<SectionKind>,
    pub which: Which,
    pub bars_from_start: BarOffset,
    /// Cue again this many bars on, and on, until the section ends.
    pub then_every: Option<Bars>,
}

impl CueRule {
    /// The beats this rule cues on a track with these sections and this many beats.
    pub fn place(&self, sections: &[Section], beat_count: usize) -> Vec<BeatIndex> {
        let matching: Vec<&Section> =
            sections.iter().filter(|section| self.kind.is_none_or(|kind| section.kind == kind)).collect();
        let anchors: Vec<&Section> = match self.which {
            Which::Every => matching,
            Which::Last => matching.last().copied().into_iter().collect(),
            Which::Nth(n) => matching.get(usize::from(n).saturating_sub(1)).copied().into_iter().collect(),
        };
        anchors
            .iter()
            .flat_map(|section| {
                let first = i64::try_from(section.first_beat).unwrap_or(i64::MAX);
                let end = first.saturating_add(i64::try_from(section.beats).unwrap_or(0));
                let step = self.then_every.map(|bars| i64::from(bars) * BEATS_PER_BAR);
                let start = first + i64::from(self.bars_from_start) * BEATS_PER_BAR;
                std::iter::successors(Some(start), move |&beat| step.map(|step| beat + step).filter(|&next| next < end))
            })
            .filter_map(|beat| usize::try_from(beat).ok().filter(|&beat| beat < beat_count))
            .collect()
    }

    fn gives_one_cue(&self) -> bool {
        self.which != Which::Every && self.then_every.is_none()
    }
}

/// One track's sections and the cues the DJ put on it, of the kind being learnt.
pub struct CuedTrack {
    pub sections: Vec<Section>,
    pub beat_count: usize,
    pub cues: HashSet<BeatIndex>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Learning {
    /// The share of training tracks a rule must find a cue on to be kept, so that a coincidence on
    /// one or two tracks isn't learnt as a habit.
    pub least_track_share: f64,
}

impl Default for Learning {
    fn default() -> Self {
        Self { least_track_share: 0.1 }
    }
}

/// A rule kept by learning, with how often it found the DJ's cues.
#[derive(Debug, Clone, PartialEq)]
pub struct LearntRule {
    pub rule: CueRule,
    pub cues_found: usize,
    pub cues_misplaced: usize,
}

fn every_rule() -> Vec<CueRule> {
    let kinds = std::iter::once(None).chain(SectionKind::ALL.into_iter().map(Some));
    let whiches: Vec<Which> = (1..=LATEST_OCCURRENCE).map(Which::Nth).chain([Which::Last, Which::Every]).collect();
    let kinds: Vec<Option<SectionKind>> = kinds.collect();
    let mut rules = Vec::new();
    for &kind in &kinds {
        for &which in &whiches {
            for &bars_from_start in &OFFSETS {
                rules.push(CueRule { kind, which, bars_from_start, then_every: None });
            }
            for &bars in &REPEATS {
                rules.push(CueRule { kind, which, bars_from_start: 0, then_every: Some(bars) });
            }
        }
    }
    rules
}

/// The rules that find the most of the DJ's cues, most valuable first, at most `limit` of them.
fn learn_rules(
    tracks: &[CuedTrack],
    learning: &Learning,
    limit: usize,
    allowed: impl Fn(&CueRule) -> bool,
) -> Vec<LearntRule> {
    let candidates: Vec<(CueRule, Vec<Vec<BeatIndex>>)> = every_rule()
        .into_iter()
        .filter(|rule| allowed(rule))
        .map(|rule| (rule, tracks.iter().map(|track| rule.place(&track.sections, track.beat_count)).collect()))
        .collect();
    let cue_count: usize = tracks.iter().map(|track| track.cues.len()).sum();
    let least_tracks = (learning.least_track_share * tracks.len() as f64).ceil() as usize;
    let mut placed: Vec<HashSet<BeatIndex>> = vec![HashSet::new(); tracks.len()];
    let (mut found_so_far, mut placed_so_far) = (0, 0);
    let mut kept = Vec::new();
    while kept.len() < limit {
        let added_by = |placements: &Vec<Vec<BeatIndex>>| {
            let (mut found, mut misplaced, mut tracks_found_on) = (0, 0, 0);
            for ((beats, track), already) in placements.iter().zip(tracks).zip(&placed) {
                let fresh: HashSet<&BeatIndex> = beats.iter().filter(|beat| !already.contains(beat)).collect();
                let found_here = fresh.iter().filter(|beat| track.cues.contains(beat)).count();
                found += found_here;
                misplaced += fresh.len() - found_here;
                tracks_found_on += usize::from(found_here > 0);
            }
            (found, misplaced, tracks_found_on)
        };
        let f1_with = |found: usize, misplaced: usize| {
            2.0 * (found_so_far + found) as f64 / (placed_so_far + found + misplaced + cue_count).max(1) as f64
        };
        let best = candidates
            .iter()
            .map(|(rule, placements)| (rule, placements, added_by(placements)))
            .filter(|(_, _, (_, _, tracks_found_on))| *tracks_found_on >= least_tracks.max(1))
            .max_by(|a, b| f1_with(a.2 .0, a.2 .1).total_cmp(&f1_with(b.2 .0, b.2 .1)));
        let Some((rule, placements, (found, misplaced, _))) = best else { break };
        if f1_with(found, misplaced) <= f1_with(0, 0) {
            break;
        }
        for (already, beats) in placed.iter_mut().zip(placements) {
            already.extend(beats);
        }
        found_so_far += found;
        placed_so_far += found + misplaced;
        kept.push(LearntRule { rule: *rule, cues_found: found, cues_misplaced: misplaced });
    }
    kept
}

/// Where the rules put cues on one track, in the rules' order of value, skipping any too near a
/// cue already placed, at most `limit` of them.
fn place_all(rules: &[CueRule], sections: &[Section], beat_count: usize, limit: usize) -> Vec<BeatIndex> {
    let mut placed: Vec<BeatIndex> = Vec::new();
    for beat in rules.iter().flat_map(|rule| rule.place(sections, beat_count)) {
        if placed.len() < limit && placed.iter().all(|&other| other.abs_diff(beat) >= MIN_GAP_BEATS) {
            placed.push(beat);
        }
    }
    placed.sort_unstable();
    placed
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCueStyle {
    pub rules: Vec<LearntRule>,
}

impl MemoryCueStyle {
    pub fn learn(tracks: &[CuedTrack], learning: &Learning) -> Self {
        Self { rules: learn_rules(tracks, learning, usize::MAX, |_| true) }
    }

    pub fn place(&self, sections: &[Section], beat_count: usize, limit: usize) -> Vec<BeatIndex> {
        let rules: Vec<CueRule> = self.rules.iter().map(|learnt| learnt.rule).collect();
        place_all(&rules, sections, beat_count, limit)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HotCueStyle {
    pub pads: Vec<(HotCueSlot, LearntRule)>,
}

impl HotCueStyle {
    /// `tracks_by_pad` holds, for each pad, every training track with only that pad's cue.
    pub fn learn(tracks_by_pad: &[(HotCueSlot, Vec<CuedTrack>)], learning: &Learning) -> Self {
        let pads = tracks_by_pad
            .iter()
            .filter_map(|(slot, tracks)| {
                let learnt = learn_rules(tracks, learning, 1, CueRule::gives_one_cue).into_iter().next()?;
                Some((*slot, learnt))
            })
            .collect();
        Self { pads }
    }

    /// A beat for each pad whose rule finds one, leaving a pad empty when an earlier pad holds a
    /// cue too near it.
    pub fn place(&self, sections: &[Section], beat_count: usize) -> Vec<(HotCueSlot, BeatIndex)> {
        let mut placed: Vec<(HotCueSlot, BeatIndex)> = Vec::new();
        for (slot, learnt) in &self.pads {
            let Some(beat) = learnt.rule.place(sections, beat_count).first().copied() else { continue };
            if placed.iter().all(|&(_, other)| other.abs_diff(beat) >= MIN_GAP_BEATS) {
                placed.push((*slot, beat));
            }
        }
        placed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(first_beat: BeatIndex, kind: SectionKind) -> Section {
        Section { first_beat, beats: 64, kind, relative_energy: 0.5 }
    }

    /// Intro, build, drop, breakdown, build, drop, outro, with the drops `drop_at` and `second_drop_at`.
    fn track(drop_at: BeatIndex, second_drop_at: BeatIndex) -> Vec<Section> {
        vec![
            section(0, SectionKind::Intro),
            section(drop_at - 32, SectionKind::Build),
            section(drop_at, SectionKind::Drop),
            section(drop_at + 128, SectionKind::Breakdown),
            section(second_drop_at - 32, SectionKind::Build),
            section(second_drop_at, SectionKind::Drop),
            section(second_drop_at + 128, SectionKind::Outro),
        ]
    }

    /// A DJ who cues the top, every drop, and 16 bars before each drop.
    fn cued(sections: Vec<Section>) -> CuedTrack {
        let drops: Vec<BeatIndex> =
            sections.iter().filter(|s| s.kind == SectionKind::Drop).map(|s| s.first_beat).collect();
        let mut cues: HashSet<BeatIndex> = drops.iter().flat_map(|&drop| [drop, drop - 64]).collect();
        cues.insert(0);
        CuedTrack { sections, beat_count: 1000, cues }
    }

    #[test]
    fn rules_place_cues_relative_to_their_sections() {
        let sections = track(128, 448);
        let rule = |kind, which, bars_from_start| CueRule { kind, which, bars_from_start, then_every: None };
        assert_eq!(rule(Some(SectionKind::Drop), Which::Every, 0).place(&sections, 1000), [128, 448]);
        assert_eq!(rule(Some(SectionKind::Drop), Which::Nth(2), -16).place(&sections, 1000), [384]);
        assert_eq!(rule(Some(SectionKind::Build), Which::Last, 0).place(&sections, 1000), [416]);
        assert_eq!(rule(None, Which::Nth(1), -4).place(&sections, 1000), Vec::<BeatIndex>::new());
        assert_eq!(rule(Some(SectionKind::Outro), Which::Every, 32).place(&sections, 600), Vec::<BeatIndex>::new());
        let through_drops = CueRule { then_every: Some(8), ..rule(Some(SectionKind::Drop), Which::Every, 0) };
        assert_eq!(through_drops.place(&sections, 1000), [128, 160, 448, 480]);
    }

    #[test]
    fn a_learnt_memory_style_finds_the_djs_cues_on_a_new_track() {
        let training: Vec<CuedTrack> =
            [(128, 448), (192, 512), (64, 384), (256, 640)].into_iter().map(|(a, b)| cued(track(a, b))).collect();
        let style = MemoryCueStyle::learn(&training, &Learning::default());

        let new_track = track(160, 544);
        assert_eq!(style.place(&new_track, 1000, 10), [0, 96, 160, 480, 544]);
    }

    #[test]
    fn hot_cue_pads_each_learn_one_place() {
        let pad_tracks = |slot: HotCueSlot, pick: fn(&[Section]) -> BeatIndex| {
            let tracks = [(128, 448), (192, 512), (64, 384)]
                .into_iter()
                .map(|(a, b)| {
                    let sections = track(a, b);
                    let cues = HashSet::from([pick(&sections)]);
                    CuedTrack { sections, beat_count: 1000, cues }
                })
                .collect();
            (slot, tracks)
        };
        let style = HotCueStyle::learn(
            &[pad_tracks(1, |_| 0), pad_tracks(2, |s| s[2].first_beat), pad_tracks(3, |s| s[5].first_beat - 32)],
            &Learning::default(),
        );
        assert_eq!(style.place(&track(160, 544), 1000), [(1, 0), (2, 160), (3, 512)]);
    }

    #[test]
    fn cues_too_near_one_already_placed_are_left_out() {
        let sections = vec![section(0, SectionKind::Intro), section(2, SectionKind::Drop)];
        let every_start = CueRule { kind: None, which: Which::Every, bars_from_start: 0, then_every: None };
        assert_eq!(place_all(&[every_start], &sections, 100, 10), [0]);
    }
}
