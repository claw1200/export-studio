//! Playing a deck faster or slower than the file, in the audio callback so a tempo change is
//! heard at once rather than after the ring's worth of queued audio.
//!
//! Without master tempo the audio is resampled, so the pitch moves with the speed as a record's
//! does. With it the audio is time-stretched and keeps its key. Both read frames from the deck's
//! [`Feed`](crate::reader::Feed) at the tempo and hand back one output frame at a time, with the
//! file position of what is being heard.

use signalsmith_stretch::Stretch;

use crate::reader::Feed;

/// Slowest and fastest a deck plays, as a multiple of the file's speed.
pub const MIN_TEMPO: f64 = 0.5;
pub const MAX_TEMPO: f64 = 2.0;

/// The value at `t` (0 to 1) between `p[1]` and `p[2]`, by Catmull-Rom interpolation.
pub(crate) fn catmull_rom(p: [f32; 4], t: f32) -> f32 {
    p[1] + 0.5
        * t
        * (p[2] - p[0] + t * (2.0 * p[0] - 5.0 * p[1] + 4.0 * p[2] - p[3] + t * (3.0 * (p[1] - p[2]) + p[3] - p[0])))
}

/// How the deck turns file frames into output frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// One for one: normal speed, and every scrub.
    Direct,
    /// Resampled: pitch follows speed.
    Varispeed,
    /// Time-stretched: the key stays put.
    Stretch,
}

/// A fractional read head over the last four file frames.
pub(crate) struct Varispeed {
    frames: [(f32, f32); 4],
    /// The playhead after each of those frames.
    at: [u64; 4],
    /// How far past `frames[1]` the head is, in frames. At 1 or more, frames are pulled first.
    phase: f64,
}

impl Default for Varispeed {
    fn default() -> Self {
        Self {
            frames: [(0.0, 0.0); 4],
            at: [0; 4],
            phase: 3.0,
        }
    }
}

impl Varispeed {
    /// Starts on the feed's next frame, after `last` (silence on a fresh start) at `at`.
    pub fn start(&mut self, last: (f32, f32), at: u64) {
        self.frames = [last; 4];
        self.at = [at; 4];
        // Three pulls put the next frame at `frames[1]`, with `last` before it.
        self.phase = 3.0;
    }

    pub fn next(&mut self, feed: &mut Feed, pop: bool, rate: f64) -> Option<(f32, f32, u64)> {
        // Back at normal speed the head lands on whole frames, so the audio is the file's own.
        if rate == 1.0 && self.phase.fract() != 0.0 {
            self.phase = self.phase.round();
        }
        while self.phase >= 1.0 {
            let (left, right, at) = feed.next_frame(pop)?;
            self.frames.copy_within(1.., 0);
            self.at.copy_within(1.., 0);
            self.frames[3] = (left, right);
            self.at[3] = at;
            self.phase -= 1.0;
        }
        let t = self.phase as f32;
        let f = &self.frames;
        let left = catmull_rom([f[0].0, f[1].0, f[2].0, f[3].0], t);
        let right = catmull_rom([f[0].1, f[1].1, f[2].1, f[3].1], t);
        self.phase += rate.max(0.0);
        Some((left, right, self.at[1]))
    }
}

/// Output frames stretched per call into the stretcher.
const CHUNK: usize = 128;
/// File positions remembered for the frames fed in: more than the stretcher's latency at the
/// fastest tempo, so the frame being heard can always be looked up.
const POSITIONS: usize = 1 << 15;

/// Signalsmith Stretch, with buffers sized up front so the callback never allocates.
pub struct Stretcher {
    stretch: Stretch,
    input: Vec<f32>,
    output: Vec<f32>,
    /// Throwaway output while priming.
    scratch: Vec<f32>,
    out_len: usize,
    out_read: usize,
    positions: Vec<u64>,
    /// Frames fed in since the last prime, and how many there were before the current chunk.
    fed: u64,
    chunk_start: u64,
    chunk_in: usize,
    /// Fractional input frames owed to the next chunk.
    carry: f64,
    input_latency: usize,
    output_latency: usize,
    primed: bool,
}

impl Stretcher {
    /// Builds and warms a stereo stretcher for `sample_rate`. Not for the audio callback.
    pub fn new(sample_rate: u32) -> Self {
        let stretch = Stretch::preset_default(2, sample_rate);
        let input_latency = stretch.input_latency();
        let output_latency = stretch.output_latency();
        let most_input = input_latency
            .max((output_latency as f64 * MAX_TEMPO).ceil() as usize)
            .max((CHUNK as f64 * MAX_TEMPO).ceil() as usize + 1);
        let mut stretcher = Self {
            stretch,
            input: vec![0.0; most_input * 2],
            output: vec![0.0; CHUNK * 2],
            scratch: vec![0.0; output_latency.max(1) * 2],
            out_len: 0,
            out_read: 0,
            positions: vec![0; POSITIONS],
            fed: 0,
            chunk_start: 0,
            chunk_in: 0,
            carry: 0.0,
            input_latency,
            output_latency,
            primed: false,
        };
        stretcher.warm();
        stretcher
    }

    /// Runs some sound through every path once, so internal buffers reach their working size
    /// here rather than on the first callback.
    fn warm(&mut self) {
        for (i, s) in self.input.iter_mut().enumerate() {
            *s = ((i as f32) * 0.37).sin() * 0.25;
        }
        let latency = self.input_latency;
        self.stretch.seek(&self.input[..latency * 2], 1.0);
        for rate in [MIN_TEMPO, 1.0, MAX_TEMPO] {
            let frames = ((CHUNK as f64 * rate) as usize).min(self.input.len() / 2);
            for _ in 0..64 {
                self.stretch.process(&self.input[..frames * 2], &mut self.output[..]);
            }
        }
        self.input.fill(0.0);
        self.reset();
    }

    /// Forgets everything: the next frame primes from wherever the feed is.
    pub fn reset(&mut self) {
        self.stretch.reset();
        self.out_len = 0;
        self.out_read = 0;
        self.fed = 0;
        self.chunk_start = 0;
        self.chunk_in = 0;
        self.carry = 0.0;
        self.primed = false;
    }

    fn latency_frames(&self, rate: f64) -> usize {
        self.input_latency + (self.output_latency as f64 * rate).round() as usize
    }

    fn pull(&mut self, feed: &mut Feed, pop: bool, frames: usize) -> usize {
        let mut got = 0;
        while got < frames {
            let Some((left, right, at)) = feed.next_frame(pop) else {
                break;
            };
            self.input[got * 2] = left;
            self.input[got * 2 + 1] = right;
            self.positions[((self.fed + got as u64) as usize) % POSITIONS] = at;
            got += 1;
        }
        got
    }

    /// Fills the stretcher so its first output is the feed's next frame, with its latency
    /// already run through. Waits (false) until the ring holds enough, unless the file ends.
    fn prime(&mut self, feed: &mut Feed, rate: f64, end_of_stream: bool) -> bool {
        let lead = (self.output_latency as f64 * rate).round() as usize;
        if feed.available() < self.input_latency + lead && !end_of_stream {
            return false;
        }
        let got = self.pull(feed, true, self.input_latency);
        if got == 0 {
            return false;
        }
        self.stretch.seek(&self.input[..got * 2], rate);
        self.fed = got as u64;
        let got = self.pull(feed, true, lead);
        let out = self.output_latency.min(self.scratch.len() / 2);
        self.stretch
            .process(&self.input[..got * 2], &mut self.scratch[..out * 2]);
        self.fed += got as u64;
        self.primed = true;
        true
    }

    fn render(&mut self, feed: &mut Feed, pop: bool, rate: f64) {
        let wanted = self.carry + CHUNK as f64 * rate;
        let frames = (wanted.floor() as usize).min(self.input.len() / 2);
        let got = self.pull(feed, pop, frames);
        self.out_len = 0;
        self.out_read = 0;
        if got == 0 {
            return;
        }
        self.carry = if got == frames { wanted - frames as f64 } else { 0.0 };
        self.stretch
            .process(&self.input[..got * 2], &mut self.output[..]);
        self.chunk_start = self.fed;
        self.chunk_in = got;
        self.fed += got as u64;
        self.out_len = CHUNK;
    }

    pub fn next(
        &mut self,
        feed: &mut Feed,
        pop: bool,
        rate: f64,
        end_of_stream: bool,
    ) -> Option<(f32, f32, u64)> {
        let rate = rate.max(MIN_TEMPO);
        if !self.primed && !(pop && self.prime(feed, rate, end_of_stream)) {
            return None;
        }
        if self.out_read >= self.out_len {
            self.render(feed, pop, rate);
            if self.out_len == 0 {
                return None;
            }
        }
        let i = self.out_read;
        self.out_read += 1;
        // The input frame at the centre of what is heard now, behind the newest by the latency.
        let fed_by_now =
            self.chunk_start as f64 + (i + 1) as f64 * self.chunk_in as f64 / CHUNK as f64;
        let heard = (fed_by_now - self.latency_frames(rate) as f64)
            .clamp(0.0, self.fed.saturating_sub(1) as f64) as u64;
        let at = self.positions[(heard as usize) % POSITIONS];
        Some((self.output[i * 2], self.output[i * 2 + 1], at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catmull_rom_passes_through_its_points() {
        let p = [0.1, 0.4, -0.3, 0.8];
        assert_eq!(catmull_rom(p, 0.0), 0.4);
        assert!((catmull_rom(p, 1.0) - -0.3).abs() < 1e-6);
    }

    #[test]
    fn the_stretcher_reports_its_latency() {
        let s = Stretcher::new(48_000);
        assert!(s.input_latency > 0 && s.output_latency > 0);
        assert!(s.latency_frames(MAX_TEMPO) + CHUNK * 2 < POSITIONS);
    }
}
