//! Beat sync: one deck follows another's tempo, and a deck started while its partner plays lands
//! on the partner's beat.
//!
//! It lives in the engine because only here are both playheads known at the same instant. The
//! callback keeps the follower's tempo, and publishes every deck's position together on a
//! [`Timeline`], so a move can be worked out from one moment and the time since paid back.

use std::sync::atomic::{fence, AtomicU64, Ordering};
use std::sync::Arc;

use crate::clock::DeckClock;
use crate::tempo::{MAX_TEMPO, MIN_TEMPO};
use crate::DeckId;

/// A deck's beats, in device-rate frames of the file.
#[derive(Debug, Clone)]
pub(crate) struct Grid {
    times: Vec<f64>,
    /// Frames per beat from each beat on, as the grid states it: what the edges extend with.
    steps: Vec<f64>,
}

impl Grid {
    /// `bpms` is the tempo from each beat on. `None` when there is no beat to go by.
    pub fn new(times_ms: &[f64], bpms: &[f64], sample_rate: u32) -> Option<Self> {
        let per_ms = f64::from(sample_rate) / 1000.0;
        let times: Vec<f64> = times_ms.iter().map(|ms| ms * per_ms).collect();
        let sorted = times.windows(2).all(|w| w[1] >= w[0]);
        if times.is_empty() || !sorted || times.iter().any(|t| !t.is_finite()) {
            return None;
        }
        let half_second = f64::from(sample_rate) / 2.0;
        let steps = (0..times.len())
            .map(|i| match bpms.get(i) {
                Some(&bpm) if bpm > 0.0 && bpm.is_finite() => 60.0 * f64::from(sample_rate) / bpm,
                _ => half_second,
            })
            .collect();
        Some(Self { times, steps })
    }

    fn last_at_or_before(&self, frame: f64) -> usize {
        self.times
            .partition_point(|&t| t <= frame)
            .saturating_sub(1)
    }

    /// Fractional beats along the grid at `frame`.
    pub fn beat_at(&self, frame: f64) -> f64 {
        let last = self.times.len() - 1;
        if frame < self.times[0] {
            return (frame - self.times[0]) / self.steps[0];
        }
        if frame >= self.times[last] {
            return last as f64 + (frame - self.times[last]) / self.steps[last];
        }
        let i = self.last_at_or_before(frame);
        let span = self.times[i + 1] - self.times[i];
        i as f64
            + if span > 0.0 {
                (frame - self.times[i]) / span
            } else {
                0.0
            }
    }

    /// The frame `beat` beats along the grid: the inverse of [`beat_at`](Self::beat_at).
    pub fn frame_at(&self, beat: f64) -> f64 {
        let last = self.times.len() - 1;
        if beat < 0.0 {
            return self.times[0] + beat * self.steps[0];
        }
        if beat >= last as f64 {
            return self.times[last] + (beat - last as f64) * self.steps[last];
        }
        let i = beat.floor() as usize;
        self.times[i] + (beat - i as f64) * (self.times[i + 1] - self.times[i])
    }

    /// Beats per minute of the music at `frame`, at the file's own speed.
    pub fn bpm_at(&self, frame: f64, sample_rate: u32) -> f64 {
        60.0 * f64::from(sample_rate) / self.steps[self.last_at_or_before(frame)]
    }
}

/// The follower's speed to keep time with a master playing at `master_bpm`, and how many of its
/// beats go by for each of the master's. Half and double time count as in time, so it takes
/// whichever of the three is nearest its own speed.
pub(crate) fn match_tempo(master_bpm: f64, own_bpm: f64) -> Option<(f64, f64)> {
    // A NaN tempo gets through here, but no rate made from it is in range.
    if master_bpm <= 0.0 || own_bpm <= 0.0 {
        return None;
    }
    [1.0, 2.0, 0.5]
        .into_iter()
        .map(|ratio| (master_bpm * ratio / own_bpm, ratio))
        .filter(|(rate, _)| (MIN_TEMPO..=MAX_TEMPO).contains(rate))
        .min_by(|a, b| a.0.ln().abs().total_cmp(&b.0.ln().abs()))
}

/// How far `own_beat` is off `reference_beat`, in its own beats, from -½ to ½: positive when ahead.
pub(crate) fn beat_offset(reference_beat: f64, own_beat: f64, ratio: f64) -> f64 {
    let offset = own_beat.rem_euclid(1.0) - (reference_beat * ratio).rem_euclid(1.0);
    if offset > 0.5 {
        offset - 1.0
    } else if offset <= -0.5 {
        offset + 1.0
    } else {
        offset
    }
}

#[derive(Debug, Clone, Copy)]
struct Link {
    master: DeckId,
    follower: DeckId,
    /// Follower beats per master beat, fixed once chosen so the follower does not flip between
    /// half and double time as the master's tempo moves.
    ratio: Option<f64>,
}

#[derive(Debug)]
pub(crate) struct BeatSync {
    sample_rate: u32,
    grids: Vec<Option<Grid>>,
    link: Option<Link>,
}

impl BeatSync {
    pub fn new(decks: usize, sample_rate: u32) -> Self {
        Self {
            sample_rate,
            grids: vec![None; decks],
            link: None,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn set_grid(&mut self, deck: DeckId, grid: Option<Grid>) {
        if let Some(slot) = self.grids.get_mut(deck) {
            *slot = grid;
        }
    }

    pub fn set_link(&mut self, link: Option<(DeckId, DeckId)>) {
        let decks = self.grids.len();
        self.link = link
            .filter(|&(master, follower)| master != follower && master < decks && follower < decks)
            .map(|(master, follower)| Link {
                master,
                follower,
                ratio: None,
            });
    }

    pub fn link(&self) -> Option<(DeckId, DeckId)> {
        self.link.map(|link| (link.master, link.follower))
    }

    /// The deck synced with `deck`, if it is one of a linked pair.
    pub fn partner(&self, deck: DeckId) -> Option<DeckId> {
        let link = self.link?;
        if deck == link.follower {
            Some(link.master)
        } else if deck == link.master {
            Some(link.follower)
        } else {
            None
        }
    }

    /// Sets the follower's tempo from the master's. Realtime: called from the callback.
    pub fn follow_tempo<C: AsRef<DeckClock>>(&mut self, clocks: &[C]) {
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let (Some(Some(master_grid)), Some(Some(own_grid))) =
            (self.grids.get(link.master), self.grids.get(link.follower))
        else {
            return;
        };
        let (Some(master), Some(follower)) = (clocks.get(link.master), clocks.get(link.follower))
        else {
            return;
        };
        let (master, follower) = (master.as_ref(), follower.as_ref());
        if !master.loaded() || !follower.loaded() {
            return;
        }
        let master_bpm =
            master_grid.bpm_at(master.position() as f64, self.sample_rate) * master.tempo();
        let own_bpm = own_grid.bpm_at(follower.position() as f64, self.sample_rate);
        let kept = link
            .ratio
            .map(|ratio| (master_bpm * ratio / own_bpm, ratio))
            .filter(|(rate, _)| (MIN_TEMPO..=MAX_TEMPO).contains(rate));
        let Some((rate, ratio)) = kept.or_else(|| match_tempo(master_bpm, own_bpm)) else {
            return;
        };
        link.ratio = Some(ratio);
        if (follower.tempo() - rate).abs() > 1e-9 {
            follower.set_tempo(rate);
        }
    }

    /// Where `deck`, at `own_at`, should be so its beats land on its partner's at `partner_at`. It
    /// moves by under half a beat, or under one at the start of the file. Call [`follow_tempo`](Self::follow_tempo) first, which picks
    /// the half or double time this goes by.
    pub fn phase_target(&self, deck: DeckId, own_at: f64, partner_at: f64) -> Option<f64> {
        let link = self.link?;
        let partner = self.partner(deck)?;
        let ratio = link.ratio?;
        let own_ratio = if deck == link.follower {
            ratio
        } else {
            1.0 / ratio
        };
        let own_grid = self.grids.get(deck)?.as_ref()?;
        let partner_grid = self.grids.get(partner)?.as_ref()?;
        let own_beat = own_grid.beat_at(own_at);
        let offset = beat_offset(partner_grid.beat_at(partner_at), own_beat, own_ratio);
        let mut target = own_beat - offset;
        // Behind the start of the file there is nothing to play: a deck waiting there, as on a
        // cue at the first beat, takes the next beat on instead.
        let first_beat = own_grid.beat_at(0.0);
        if target < first_beat {
            target += (first_beat - target).ceil();
        }
        Some(own_grid.frame_at(target))
    }
}

/// Every deck's playhead as of the end of one callback, with the frames output up to then.
pub(crate) struct Moment {
    pub frames: u64,
    pub positions: Vec<u64>,
}

/// What the callback last published, read whole. Each deck's own clock is written as that deck is
/// mixed, so reading two clocks can catch one before the callback and one after.
pub(crate) struct Timeline {
    /// Odd while a publish is under way.
    seq: AtomicU64,
    frames: AtomicU64,
    positions: Vec<AtomicU64>,
}

impl Timeline {
    pub fn new(decks: usize) -> Self {
        Self {
            seq: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            positions: (0..decks).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    /// Realtime: called from the callback, the only writer.
    pub fn publish(&self, frames: u64, clocks: &[Arc<DeckClock>]) {
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq + 1, Ordering::Relaxed);
        fence(Ordering::Release);
        self.frames.store(frames, Ordering::Relaxed);
        for (slot, clock) in self.positions.iter().zip(clocks) {
            slot.store(clock.position(), Ordering::Relaxed);
        }
        self.seq.store(seq + 2, Ordering::Release);
    }

    pub fn read(&self) -> Moment {
        loop {
            let before = self.seq.load(Ordering::Acquire);
            if before % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let frames = self.frames.load(Ordering::Relaxed);
            let positions = self
                .positions
                .iter()
                .map(|p| p.load(Ordering::Relaxed))
                .collect();
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == before {
                return Moment { frames, positions };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn steady(bpm: f64, beats: usize, first_ms: f64) -> Grid {
        let step = 60_000.0 / bpm;
        let times: Vec<f64> = (0..beats).map(|i| first_ms + i as f64 * step).collect();
        Grid::new(&times, &vec![bpm; beats], RATE).unwrap()
    }

    #[test]
    fn beats_and_frames_are_inverses_inside_and_past_the_grid() {
        let grid = steady(120.0, 8, 100.0);
        for frame in [0.0, 4_800.0, 30_000.0, 500_000.0] {
            assert!((grid.frame_at(grid.beat_at(frame)) - frame).abs() < 1e-6);
        }
        assert_eq!(grid.beat_at(4_800.0), 0.0, "the first beat is at 100 ms");
        assert_eq!(grid.beat_at(4_800.0 + 24_000.0), 1.0);
    }

    #[test]
    fn a_slower_track_is_sped_up_to_the_master() {
        assert_eq!(match_tempo(128.0, 124.0), Some((128.0 / 124.0, 1.0)));
        assert_eq!(match_tempo(126.0, 126.0), Some((1.0, 1.0)));
    }

    #[test]
    fn half_or_double_time_is_taken_when_it_is_nearer_the_followers_own_speed() {
        assert_eq!(match_tempo(174.0, 87.0), Some((1.0, 0.5)));
        assert_eq!(match_tempo(70.0, 140.0), Some((1.0, 2.0)));
        // Following 150 at 100 BPM: +50% for the same tempo, −25% for half. The smaller wins.
        assert_eq!(match_tempo(150.0, 100.0), Some((0.75, 0.5)));
    }

    #[test]
    fn there_is_nothing_to_match_without_a_tempo_on_both_decks() {
        assert_eq!(match_tempo(0.0, 120.0), None);
        assert_eq!(match_tempo(120.0, 0.0), None);
        assert_eq!(match_tempo(f64::NAN, 120.0), None);
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} is not {b}");
    }

    #[test]
    fn the_offset_is_positive_ahead_of_the_beat_and_negative_behind() {
        assert_eq!(beat_offset(16.0, 64.0, 1.0), 0.0);
        close(beat_offset(10.0, 20.25, 1.0), 0.25);
        close(beat_offset(10.5, 20.25, 1.0), -0.25);
    }

    #[test]
    fn the_offset_takes_the_shorter_way_round_the_beat() {
        close(beat_offset(4.0, 8.9, 1.0), -0.1);
        close(beat_offset(4.9, 8.0, 1.0), 0.1);
        close(beat_offset(-0.25, 0.75, 1.0), 0.0);
    }

    #[test]
    fn the_offset_counts_the_partner_in_the_decks_own_beats() {
        // At double time, partner beat 3.25 is 6.5 of this deck's, so 7.5 is on it.
        close(beat_offset(3.25, 7.5, 2.0), 0.0);
        close(beat_offset(3.25, 7.75, 2.0), 0.25);
    }

    #[test]
    fn the_target_lands_on_the_partners_beat_from_either_side() {
        let mut sync = BeatSync::new(2, RATE);
        sync.set_grid(0, Some(steady(120.0, 64, 0.0)));
        sync.set_grid(1, Some(steady(120.0, 64, 37.0)));
        sync.set_link(Some((0, 1)));
        sync.link.as_mut().unwrap().ratio = Some(1.0);
        let master_at = 100_000.0;
        let target = sync.phase_target(1, 90_000.0, master_at).unwrap();
        let grids = (
            sync.grids[0].as_ref().unwrap(),
            sync.grids[1].as_ref().unwrap(),
        );
        assert!(beat_offset(grids.0.beat_at(master_at), grids.1.beat_at(target), 1.0).abs() < 1e-9);
        assert!(
            (target - 90_000.0).abs() <= 12_000.0,
            "under half a beat away"
        );
        let back = sync.phase_target(0, master_at, target).unwrap();
        assert!(
            (back - master_at).abs() < 1e-6,
            "the master is already in phase"
        );
    }
}
