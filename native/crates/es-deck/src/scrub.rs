//! Vinyl-style scrubbing: a read head that follows the pointer through decoded audio.
//!
//! The head does not jump to each pointer position. It moves at the pointer's speed, measured
//! from the input events' own timestamps, plus a correction toward where the pointer is, so a
//! drag sounds like a record under a hand: pitch follows speed, and slow movement is dark and
//! quiet because a low-pass filter tracks the speed.

use crate::block::BLOCK_FRAMES;
use crate::tempo::catmull_rom;

/// Seconds of decoded audio kept either side of the head.
pub const WINDOW_REACH: f64 = 2.0;

/// Smoothing of the head's rate toward its target, per block.
const SMOOTH: f64 = 0.15;
/// The same once the pointer has stopped: a hand stopping a record stops it quickly.
const STOP_SMOOTH: f64 = 0.5;
/// Smoothing of the pointer's measured speed, per input event.
const SPEED_SMOOTH: f64 = 0.30;
/// Smoothing of the time between input events, per event.
const INTERVAL_SMOOTH: f64 = 0.2;
/// Blocks ahead the head aims, covering the time its audio waits in the ring.
const LOOKAHEAD: f64 = 2.0;
/// Fastest the head moves, in multiples of normal speed.
const MAX_RATE: f64 = 8.0;
/// Furthest the head may trail the pointer before it is pulled along, in frames.
const MAX_LAG: f64 = 4096.0;
/// Below this rate the head is at rest and silent.
const REST_RATE: f64 = 0.01;
/// Blocks at rest before the head stops for good.
const SETTLE_BLOCKS: u32 = 4;
/// Missed input intervals after which the pointer counts as stopped.
const STOPPED_INTERVALS: f64 = 2.0;
/// Share of the distance to the pointer the head closes per block.
const CORRECT: f64 = 0.10;
/// Assumed time between input events until some are measured, in ms.
const FIRST_INTERVAL_MS: f64 = 16.0;

/// Cutoff at rest and at full speed, in Hz.
const CUTOFF_MIN: f64 = 400.0;
const CUTOFF_MAX: f64 = 19_000.0;
/// Highest cutoff as a share of the sample rate, where the filter stays well-behaved.
const CUTOFF_CEILING: f64 = 0.4;
/// Time for filter and gain changes to glide over, in seconds.
const GLIDE_SECONDS: f64 = 0.005;
/// Corner of the DC blocker, in Hz: a head barely moving makes near-DC, not sound.
const DC_HZ: f64 = 10.0;

/// Decoded stereo audio around the head, at the device rate.
#[derive(Default)]
pub struct PcmWindow {
    start: u64,
    /// Interleaved stereo.
    data: Vec<f32>,
}

impl PcmWindow {
    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn end(&self) -> u64 {
        self.start + self.frames() as u64
    }

    pub fn frames(&self) -> usize {
        self.data.len() / 2
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Replaces the window with `data` starting at `start`.
    pub fn reset(&mut self, start: u64, data: Vec<f32>) {
        self.start = start;
        self.data = data;
    }

    pub fn append(&mut self, data: &[f32]) {
        self.data.extend_from_slice(data);
    }

    /// Adds `data` before the window, which then starts at `start`.
    pub fn prepend(&mut self, start: u64, data: &[f32]) {
        self.data.splice(0..0, data.iter().copied());
        self.start = start;
    }

    /// Drops audio further than `keep` frames from `head`, so the window does not grow.
    pub fn trim(&mut self, head: f64, keep: u64) {
        let head = head.max(0.0) as u64;
        let end = head.saturating_add(keep);
        if self.end() > end {
            self.data
                .truncate((end.saturating_sub(self.start) as usize) * 2);
        }
        let first = head.saturating_sub(keep);
        if first > self.start {
            let drop = ((first - self.start) as usize).min(self.frames());
            self.data.drain(..drop * 2);
            self.start += drop as u64;
        }
    }

    fn frame(&self, index: i64) -> (f32, f32) {
        if index < self.start as i64 {
            return (0.0, 0.0);
        }
        let i = (index - self.start as i64) as usize;
        if i >= self.frames() {
            return (0.0, 0.0);
        }
        (self.data[i * 2], self.data[i * 2 + 1])
    }

    /// The audio at fractional frame `at`, by Catmull-Rom interpolation. Silence outside.
    pub fn sample(&self, at: f64) -> (f32, f32) {
        let base = at.floor();
        let t = (at - base) as f32;
        let i = base as i64;
        let p = [
            self.frame(i - 1),
            self.frame(i),
            self.frame(i + 1),
            self.frame(i + 2),
        ];
        (
            catmull_rom([p[0].0, p[1].0, p[2].0, p[3].0], t),
            catmull_rom([p[0].1, p[1].1, p[2].1, p[3].1], t),
        )
    }
}

/// A two-pole Butterworth low-pass (trapezoidal state-variable form) whose cutoff follows the
/// head's speed, then a DC blocker.
pub struct ScrubFilter {
    rate: f64,
    cutoff: f64,
    glide: f64,
    state: [[f64; 2]; 2],
    dc_pole: f64,
    dc: [[f64; 2]; 2],
}

impl ScrubFilter {
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate.max(1));
        Self {
            rate,
            cutoff: CUTOFF_MIN,
            glide: 1.0 - (-1.0 / (GLIDE_SECONDS * rate)).exp(),
            state: [[0.0; 2]; 2],
            dc_pole: 1.0 - 2.0 * std::f64::consts::PI * DC_HZ / rate,
            dc: [[0.0; 2]; 2],
        }
    }

    /// Cutoff for a head moving at `speed`: dark when slow, open from normal speed up.
    pub fn cutoff_for(&self, speed: f64) -> f64 {
        let x = speed.abs().min(1.0);
        let eased = 1.0 - (1.0 - x) * (1.0 - x);
        (CUTOFF_MIN + (CUTOFF_MAX - CUTOFF_MIN) * eased).min(CUTOFF_CEILING * self.rate)
    }

    pub fn process(&mut self, speed: f64, input: (f32, f32)) -> (f32, f32) {
        self.cutoff += (self.cutoff_for(speed) - self.cutoff) * self.glide;
        let g = (std::f64::consts::PI * self.cutoff / self.rate).tan();
        let k = std::f64::consts::SQRT_2;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let mut out = [0.0f32; 2];
        for (ch, x) in [f64::from(input.0), f64::from(input.1)]
            .into_iter()
            .enumerate()
        {
            let [ic1, ic2] = self.state[ch];
            let v3 = x - ic2;
            let v1 = a1 * ic1 + a2 * v3;
            let v2 = ic2 + a2 * ic1 + a3 * v3;
            self.state[ch] = [2.0 * v1 - ic1, 2.0 * v2 - ic2];
            let [x1, y1] = self.dc[ch];
            let y = v2 - x1 + self.dc_pole * y1;
            self.dc[ch] = [v2, y];
            out[ch] = y as f32;
        }
        (out[0], out[1])
    }
}

/// The read head and what it knows about the pointer.
pub struct Scrubber {
    sample_rate: f64,
    /// Frame the head is on.
    head: f64,
    /// Frames the head moves per output frame.
    rate: f64,
    /// Where the pointer is.
    target: f64,
    /// The pointer's speed in multiples of normal speed, from its event timestamps.
    speed: f64,
    /// The last input event, by its own timestamp and its position.
    last_event: Option<(f64, f64)>,
    /// When the last event arrived here, in engine ms.
    received_at: f64,
    interval_ms: f64,
    rest_blocks: u32,
    gain: f64,
    glide: f64,
    filter: ScrubFilter,
}

impl Scrubber {
    pub fn new(head: f64, sample_rate: u32, now_ms: f64) -> Self {
        let rate = f64::from(sample_rate.max(1));
        Self {
            sample_rate: rate,
            head,
            rate: 0.0,
            target: head,
            speed: 0.0,
            last_event: None,
            received_at: now_ms,
            interval_ms: FIRST_INTERVAL_MS,
            rest_blocks: SETTLE_BLOCKS,
            gain: 0.0,
            glide: 1.0 - (-1.0 / (GLIDE_SECONDS * rate)).exp(),
            filter: ScrubFilter::new(sample_rate),
        }
    }

    pub fn head(&self) -> f64 {
        self.head
    }

    pub fn target(&self) -> f64 {
        self.target
    }

    #[cfg(test)]
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// The pointer moved to `frame`; `at_ms` is the event's own timestamp, `now_ms` engine time.
    pub fn point(&mut self, frame: f64, at_ms: f64, now_ms: f64) {
        if let Some((last_at, last_frame)) = self.last_event {
            let dt = at_ms - last_at;
            if dt > 0.0 {
                let measured = (frame - last_frame) / (dt / 1000.0 * self.sample_rate);
                // A gap much longer than usual means the pointer rested in between.
                let measured = if dt > STOPPED_INTERVALS * self.interval_ms * 4.0 {
                    0.0
                } else {
                    measured
                };
                self.speed += (measured.clamp(-MAX_RATE, MAX_RATE) - self.speed) * SPEED_SMOOTH;
                self.interval_ms += (dt.min(100.0) - self.interval_ms) * INTERVAL_SMOOTH;
            }
        }
        self.last_event = Some((at_ms, frame));
        self.target = frame;
        self.received_at = now_ms;
        self.rest_blocks = 0;
    }

    fn pointer_stopped(&self, now_ms: f64) -> bool {
        now_ms - self.received_at > STOPPED_INTERVALS * self.interval_ms
    }

    /// Renders `out` (interleaved stereo) from `window`, moving the head.
    pub fn render(&mut self, window: &PcmWindow, out: &mut [f32], now_ms: f64) {
        let frames = out.len() / 2;
        let block = BLOCK_FRAMES as f64;
        let stopped = self.pointer_stopped(now_ms);
        if stopped {
            self.speed = 0.0;
        }

        let aim = self.target + self.speed * LOOKAHEAD * block;
        let error = aim - self.head;
        let behind = self.target - self.head;
        if behind.abs() > MAX_LAG + self.speed.abs() * LOOKAHEAD * block {
            self.head = self.target - behind.signum() * MAX_LAG;
        }
        // The head runs ahead of the pointer to cover the ring's latency, so it is past the
        // pointer when the pointer stops. It stops too, rather than backing up audibly.
        let wanted = if stopped {
            0.0
        } else {
            (self.speed + CORRECT * error / block).clamp(-MAX_RATE, MAX_RATE)
        };
        let smooth = if stopped { STOP_SMOOTH } else { SMOOTH };
        let mut next = self.rate + (wanted - self.rate) * smooth;
        let resting = self.speed == 0.0 && next.abs() < REST_RATE;
        if resting {
            self.rest_blocks = self.rest_blocks.saturating_add(1);
            if self.rest_blocks >= SETTLE_BLOCKS {
                next = 0.0;
            }
        } else {
            self.rest_blocks = 0;
        }

        let start = self.rate;
        for i in 0..frames {
            let rate = start + (next - start) * (i + 1) as f64 / frames as f64;
            self.head += rate;
            let audible = if rate.abs() >= REST_RATE { 1.0 } else { 0.0 };
            self.gain += (audible - self.gain) * self.glide;
            let (l, r) = self.filter.process(rate, window.sample(self.head));
            let gain = self.gain as f32;
            out[i * 2] = l * gain;
            out[i * 2 + 1] = r * gain;
        }
        self.rate = next;
        // At rest the head is silent, so it can move onto the pointer unheard.
        if self.settled() {
            self.head = self.target;
        }
    }

    /// Done moving: at rest and silent.
    pub fn settled(&self) -> bool {
        self.rest_blocks >= SETTLE_BLOCKS && self.gain < 1e-4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const BLOCK_MS: f64 = BLOCK_FRAMES as f64 * 1000.0 / RATE as f64;

    /// A 1 kHz tone across 20 seconds.
    fn window() -> PcmWindow {
        let frames = RATE as usize * 20;
        let mut data = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let v = (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / RATE as f32).sin() * 0.5;
            data.push(v);
            data.push(v);
        }
        let mut w = PcmWindow::default();
        w.reset(0, data);
        w
    }

    /// Drags at `speed` for `ms`, with events every 8 ms as a pointer sends them, then lets go
    /// of the pointer (no more events) for `rest_ms`. Returns the head per block and the audio.
    fn drag(from: f64, speed: f64, ms: f64, rest_ms: f64) -> (Vec<f64>, Vec<f32>, Scrubber) {
        let w = window();
        let mut s = Scrubber::new(from, RATE, 0.0);
        let mut heads = Vec::new();
        let mut audio = Vec::new();
        let mut block = vec![0.0; BLOCK_FRAMES * 2];
        let mut now = 0.0;
        let mut next_event = 0.0;
        while now < ms + rest_ms {
            while next_event <= now && next_event <= ms {
                s.point(
                    from + speed * next_event / 1000.0 * RATE as f64,
                    next_event,
                    now,
                );
                next_event += 8.0;
            }
            s.render(&w, &mut block, now);
            heads.push(s.head());
            audio.extend_from_slice(&block);
            now += BLOCK_MS;
        }
        (heads, audio, s)
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|v| v * v).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    #[test]
    fn forward_and_backward_drags_mirror_each_other() {
        let from = 10.0 * RATE as f64;
        let (ahead, fwd, _) = drag(from, 1.0, 400.0, 0.0);
        let (back, rev, _) = drag(from, -1.0, 400.0, 0.0);
        for (a, b) in ahead.iter().zip(&back) {
            assert!(
                ((a - from) + (b - from)).abs() < 1e-6,
                "{a} and {b} are not mirrored"
            );
        }
        let (f, r) = (rms(&fwd), rms(&rev));
        assert!(
            (f - r).abs() / f < 0.05,
            "forward {f} and backward {r} differ in level"
        );
    }

    #[test]
    fn a_drag_at_normal_speed_keeps_up_with_the_pointer() {
        let from = 5.0 * RATE as f64;
        let (heads, audio, s) = drag(from, 1.0, 600.0, 0.0);
        let travelled = heads.last().unwrap() - from;
        let pointer = 0.6 * RATE as f64;
        assert!(
            (travelled - pointer).abs() < 0.08 * pointer,
            "head moved {travelled}, pointer {pointer}"
        );
        assert!((s.rate() - 1.0).abs() < 0.1, "rate settled at {}", s.rate());
        assert!(
            rms(&audio[audio.len() / 2..]) > 0.2,
            "full-speed scrub is audible"
        );
    }

    #[test]
    fn stopping_the_pointer_brings_the_head_to_rest_on_it_in_silence() {
        let from = 5.0 * RATE as f64;
        let (_, audio, s) = drag(from, 1.5, 300.0, 400.0);
        assert!(s.settled(), "head still moving at rate {}", s.rate());
        assert!(
            (s.head() - s.target()).abs() < 1.0,
            "rests at {} not {}",
            s.head(),
            s.target()
        );
        let tail = &audio[audio.len() - BLOCK_FRAMES * 2..];
        assert!(tail.iter().all(|v| v.abs() < 1e-3), "silent at rest");
    }

    #[test]
    fn a_slow_drag_moves_steadily_forward() {
        let from = 5.0 * RATE as f64;
        let (heads, audio, s) = drag(from, 0.1, 1000.0, 0.0);
        for pair in heads.windows(2).skip(4) {
            assert!(
                pair[1] >= pair[0],
                "the head went backwards on a slow forward drag"
            );
        }
        assert!(
            (s.rate() - 0.1).abs() < 0.03,
            "rate {} for a 0.1x drag",
            s.rate()
        );
        // Slow is dark and quiet: a 1 kHz tone at 0.1x is a 100 Hz tone, mostly passed, but
        // nothing in it may blow up.
        assert!(audio.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn the_filter_is_stable_and_dark_at_low_speed() {
        let mut f = ScrubFilter::new(RATE);
        let mut peak = 0.0f32;
        for i in 0..RATE as usize {
            let x = if i % 2 == 0 { 0.5 } else { -0.5 };
            let (l, _) = f.process(0.01, (x, x));
            assert!(l.is_finite());
            if i > RATE as usize / 2 {
                peak = peak.max(l.abs());
            }
        }
        assert!(
            peak < 0.01,
            "Nyquist tone leaks {peak} through the slow filter"
        );
        assert!(f.cutoff_for(10.0) <= CUTOFF_CEILING * RATE as f64);
        assert!((f.cutoff_for(0.0) - CUTOFF_MIN).abs() < 1e-9);
    }

    #[test]
    fn a_pointer_far_ahead_pulls_the_head_along() {
        let w = window();
        let mut s = Scrubber::new(0.0, RATE, 0.0);
        s.point(5.0 * RATE as f64, 0.0, 0.0);
        let mut block = vec![0.0; BLOCK_FRAMES * 2];
        s.render(&w, &mut block, 0.0);
        assert!(s.target() - s.head() <= MAX_LAG + 1.0);
    }

    #[test]
    fn the_window_trims_around_the_head() {
        let mut w = window();
        w.trim(10.0 * RATE as f64, RATE as u64);
        assert_eq!(w.start(), 9 * RATE as u64);
        assert_eq!(w.end(), 11 * RATE as u64);
        w.prepend(w.start() - 4, &[0.25; 8]);
        assert_eq!(w.sample((9 * RATE as u64 - 3) as f64).0, 0.25);
    }
}
