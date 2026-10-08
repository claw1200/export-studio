//! A deck's shared state: written by the control side and the decode thread, read by the
//! callback and the interface. Atomics only, so the callback never waits on a reader.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

#[derive(Debug, Default)]
pub struct DeckClock {
    /// The playhead in device-rate frames. The callback writes it as audio is heard; a seek
    /// writes it immediately so a paused deck shows where it landed.
    position: AtomicU64,
    /// Track length in device-rate frames, or 0 when unknown.
    total: AtomicU64,
    /// Bumped on every load, seek and scrub boundary. Blocks carry the generation they were
    /// decoded under, and the callback only plays blocks from the current one.
    generation: AtomicU32,
    sample_rate: AtomicU32,
    playing: AtomicBool,
    scrubbing: AtomicBool,
    /// Where the pointer last put a scrub, in frames: where the deck lands when it is let go.
    scrub_target: AtomicU64,
    loaded: AtomicBool,
    /// The decode thread has queued the last block of the file.
    end_of_stream: AtomicBool,
    /// The newest load asked for; an older one finishing late is discarded.
    requested_load: AtomicU64,
    /// The load that is installed, or 0 while empty or loading.
    load_id: AtomicU64,
    load_gate: Mutex<()>,
    loop_in: AtomicU64,
    loop_out: AtomicU64,
    looping: AtomicBool,
}

/// One deck at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeckSnapshot {
    pub position_frames: u64,
    pub total_frames: u64,
    pub generation: u32,
    pub sample_rate: u32,
    pub playing: bool,
    pub scrubbing: bool,
    pub loaded: bool,
    pub load_id: u64,
    pub loop_in_frames: u64,
    pub loop_out_frames: u64,
    pub looping: bool,
}

impl DeckClock {
    pub fn snapshot(&self) -> DeckSnapshot {
        let (loop_in_frames, loop_out_frames) = self.loop_range().unwrap_or((0, 0));
        DeckSnapshot {
            position_frames: self.position(),
            total_frames: self.total(),
            generation: self.generation(),
            sample_rate: self.sample_rate(),
            playing: self.playing(),
            scrubbing: self.scrubbing(),
            loaded: self.loaded(),
            load_id: self.load_id(),
            loop_in_frames,
            loop_out_frames,
            looping: self.looping(),
        }
    }

    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Acquire)
    }

    pub fn set_position(&self, frames: u64) {
        self.position.store(frames, Ordering::Release);
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn set_total(&self, frames: u64) {
        self.total.store(frames, Ordering::Relaxed);
    }

    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    /// Always before the first block of the new generation is pushed.
    pub fn bump_generation(&self) -> u32 {
        self.generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    pub fn set_sample_rate(&self, rate: u32) {
        self.sample_rate.store(rate, Ordering::Relaxed);
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    pub fn set_playing(&self, on: bool) {
        self.playing.store(on, Ordering::Release);
    }

    pub fn scrubbing(&self) -> bool {
        self.scrubbing.load(Ordering::Acquire)
    }

    pub fn set_scrubbing(&self, on: bool) {
        self.scrubbing.store(on, Ordering::Release);
    }

    pub fn scrub_target(&self) -> u64 {
        self.scrub_target.load(Ordering::Acquire)
    }

    pub fn set_scrub_target(&self, frame: u64) {
        self.scrub_target.store(frame, Ordering::Release);
    }

    /// Whether the callback should be pulling audio from this deck. A scrub sounds whether or
    /// not the transport is playing, and releasing it leaves the transport as it was.
    pub fn sounding(&self) -> bool {
        self.playing() || self.scrubbing()
    }

    pub fn loaded(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    pub fn end_of_stream(&self) -> bool {
        self.end_of_stream.load(Ordering::Acquire)
    }

    pub fn set_end_of_stream(&self, ended: bool) {
        self.end_of_stream.store(ended, Ordering::Release);
    }

    pub fn requested_load(&self) -> u64 {
        self.requested_load.load(Ordering::Acquire)
    }

    pub fn load_id(&self) -> u64 {
        self.load_id.load(Ordering::Acquire)
    }

    /// Makes `request` the only load whose result may be installed, and empties the deck.
    pub fn request_load(&self, request: u64) {
        let _gate = self
            .load_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.requested_load.store(request, Ordering::Release);
        self.load_id.store(0, Ordering::Release);
        self.loaded.store(false, Ordering::Release);
        self.playing.store(false, Ordering::Release);
    }

    /// Publishes a finished load unless a newer one was asked for meanwhile.
    pub fn install_load(&self, request: u64) -> bool {
        let _gate = self
            .load_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.requested_load() != request {
            return false;
        }
        self.load_id.store(request, Ordering::Release);
        self.loaded.store(true, Ordering::Release);
        true
    }

    /// Whether a failed load still belongs to the current request.
    pub fn owns_load(&self, request: u64) -> bool {
        let _gate = self
            .load_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.requested_load() == request
    }

    pub fn clear_load(&self) {
        self.load_id.store(0, Ordering::Release);
        self.loaded.store(false, Ordering::Release);
    }

    /// The loop as `(in, out)`, when there is one.
    pub fn loop_range(&self) -> Option<(u64, u64)> {
        let to = self.loop_out.load(Ordering::Acquire);
        let from = self.loop_in.load(Ordering::Acquire);
        (to > from).then_some((from, to))
    }

    /// Sets the loop range, or clears it (and leaves the loop) with `None`.
    pub fn set_loop(&self, range: Option<(u64, u64)>) {
        let (from, to) = range.filter(|(f, t)| t > f).unwrap_or((0, 0));
        // Out first, so a reader between the two stores never sees out before in.
        self.loop_out.store(to, Ordering::Release);
        self.loop_in.store(from, Ordering::Release);
        if to <= from {
            self.looping.store(false, Ordering::Release);
        }
    }

    pub fn looping(&self) -> bool {
        self.looping.load(Ordering::Acquire)
    }

    pub fn set_looping(&self, on: bool) {
        self.looping
            .store(on && self.loop_range().is_some(), Ordering::Release);
    }

    /// The loop the deck is inside, if any.
    pub fn active_loop(&self) -> Option<(u64, u64)> {
        if self.looping() {
            self.loop_range()
        } else {
            None
        }
    }
}
