//! One deck's side of the audio callback.
//!
//! Realtime: no allocation, no locks, nothing that can block. Where audio is missing it writes
//! silence, and every edge goes through the fade ramp.

use std::collections::VecDeque;
use std::sync::Arc;

use rtrb::Consumer;

use crate::block::{Block, BLOCK_FRAMES};
use crate::clock::DeckClock;
use crate::fade::{Ramp, FADE_FRAMES};
use crate::tempo::{Mode, Stretcher, Varispeed, POSITIONS};

/// The deck's decoded audio as single frames, in file order.
pub(crate) struct Feed {
    consumer: Consumer<Block>,
    current: Option<Block>,
    offset: usize,
    /// Blocks from before this generation are dropped.
    generation: u32,
    /// Frames handed back by the time-stretcher when the deck leaves it: read before anything
    /// else, so the next path starts on the frame after the last one heard. Allocated up front.
    backlog: VecDeque<(f32, f32, u64)>,
}

impl Feed {
    fn new(consumer: Consumer<Block>) -> Self {
        Self {
            consumer,
            current: None,
            offset: 0,
            generation: 0,
            backlog: VecDeque::with_capacity(POSITIONS),
        }
    }

    /// Puts a frame back in front of everything else. Frames go back newest first. Never grows
    /// the backlog past its capacity, so the callback never allocates; a frame that does not fit
    /// is dropped, which only the impossible case of more than the stretcher holds could cause.
    pub fn unread(&mut self, frame: (f32, f32, u64)) {
        if self.backlog.len() < self.backlog.capacity() {
            self.backlog.push_front(frame);
        }
    }

    /// The next frame and the playhead after it. With `pop` false only the block in hand is read.
    pub fn next_frame(&mut self, pop: bool) -> Option<(f32, f32, u64)> {
        if let Some(frame) = self.backlog.pop_front() {
            return Some(frame);
        }
        loop {
            if let Some(block) = self.current.as_ref() {
                if self.offset < usize::from(block.frames) {
                    let i = self.offset * 2;
                    self.offset += 1;
                    return Some((
                        block.samples[i],
                        block.samples[i + 1],
                        block.position + self.offset as u64,
                    ));
                }
                self.current = None;
                self.offset = 0;
            }
            if !pop {
                return None;
            }
            match self.consumer.pop() {
                Ok(block) if block.generation >= self.generation => {
                    self.current = Some(block);
                    self.offset = 0;
                }
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    /// Frames queued for this generation, near enough: whole blocks are counted as full.
    pub fn available(&mut self) -> usize {
        self.drop_stale();
        self.backlog.len() + self.left_in_block() + self.consumer.slots() * BLOCK_FRAMES
    }

    /// Discards blocks of an earlier generation at the front of the ring. Only the front can hold
    /// them: once the decode thread moves on, everything it queues is the new generation's.
    fn drop_stale(&mut self) {
        while self
            .consumer
            .peek()
            .is_ok_and(|block| block.generation < self.generation)
        {
            let _ = self.consumer.pop();
        }
    }

    fn left_in_block(&self) -> usize {
        self.current
            .as_ref()
            .map_or(0, |b| usize::from(b.frames).saturating_sub(self.offset))
    }

    fn drained(&self) -> bool {
        self.backlog.is_empty() && self.current.is_none() && self.consumer.is_empty()
    }

    fn adopt(&mut self, generation: u32) {
        if generation != self.generation {
            self.backlog.clear();
        }
        self.generation = generation;
        if self
            .current
            .as_ref()
            .is_some_and(|b| b.generation < generation)
        {
            self.current = None;
            self.offset = 0;
        }
        self.drop_stale();
    }
}

pub(crate) struct DeckReader {
    feed: Feed,
    clock: Arc<DeckClock>,
    ramp: Ramp,
    /// The last frame played and the playhead after it, so running dry fades it out instead of
    /// cutting, and resampling can take over from it.
    last: (f32, f32),
    last_at: u64,
    /// The generation being heard. It trails the clock's across a seek: the old audio is faded
    /// to silence first, and only then is the new generation taken up.
    generation: u32,
    mode: Mode,
    /// Fading out to move onto or off the time-stretcher, which happens once the fade is silent.
    switching: bool,
    /// Output frames that went by between a move and its new audio, in file frames at the
    /// deck's tempo. A move that keeps time skips this much of the new audio before playing it.
    owed: f64,
    /// The move taken up keeps time and its debt is not yet paid.
    catching_up: bool,
    varispeed: Varispeed,
    /// Built off the callback once the device rate is known, and handed over through `stretchers`.
    stretcher: Option<Box<Stretcher>>,
    stretchers: Consumer<Box<Stretcher>>,
}

impl DeckReader {
    pub fn new(
        consumer: Consumer<Block>,
        clock: Arc<DeckClock>,
        stretchers: Consumer<Box<Stretcher>>,
    ) -> Self {
        Self {
            feed: Feed::new(consumer),
            clock,
            ramp: Ramp::silent(),
            last: (0.0, 0.0),
            last_at: 0,
            generation: 0,
            mode: Mode::Direct,
            switching: false,
            owed: 0.0,
            catching_up: false,
            varispeed: Varispeed::default(),
            stretcher: None,
            stretchers,
        }
    }

    /// Adds this deck's audio to `out` (interleaved stereo).
    pub fn mix_into(&mut self, out: &mut [f32]) {
        if self.stretcher.is_none() {
            self.stretcher = self.stretchers.pop().ok();
        }
        let target = self.clock.generation();
        let sounding = self.clock.sounding();
        if !sounding && self.ramp.is_silent() {
            self.adopt(target);
            return;
        }

        // Onto or off the time-stretcher: at once if nothing is sounding, else after a fade.
        if self.generation == target && self.crosses_stretch() {
            if self.ramp.is_silent() {
                self.switch_path();
            } else {
                self.switching = true;
            }
        }

        let rate = self.clock.tempo();
        if self.mode == Mode::Direct && rate != 1.0 && self.playback_mode() == Mode::Varispeed {
            self.varispeed.start(self.last, self.last_at);
            self.mode = Mode::Varispeed;
        }

        let frames = out.len() / 2;
        let mut position = None;
        let mut done = 0;
        while done < frames {
            if self.generation != target && self.ramp.is_silent() {
                self.adopt(target);
            }
            if self.switching && self.ramp.is_silent() {
                self.switch_path();
            }
            let changing = self.generation != target;
            if self.catching_up && !changing {
                self.pay_owed();
            }
            // Time passing between a move and its new audio, for a move that keeps time.
            let owing = changing || self.catching_up;
            let open = sounding && !changing && !self.switching && !self.ending();
            // Mid-changeover only the block already in hand may be faded out; the ring holds
            // where the deck is going.
            let Some((left, right, at)) = self.next_frame(!changing, rate) else {
                if self.ramp.is_silent() {
                    if changing {
                        self.adopt(target);
                        continue;
                    }
                    break;
                }
                // Underrun or an unexpected end: hold the last frame and fade it.
                let gain = self.ramp.step(false);
                out[done * 2] += self.last.0 * gain;
                out[done * 2 + 1] += self.last.1 * gain;
                done += 1;
                if owing {
                    self.owed += rate;
                }
                continue;
            };
            self.last = (left, right);
            self.last_at = at;
            let gain = self.ramp.step(open);
            out[done * 2] += left * gain;
            out[done * 2 + 1] += right * gain;
            // Audio fading out under a seek belongs to where the deck was, so it does not move
            // the playhead the seek has already set.
            if !changing {
                position = Some(at);
            }
            done += 1;
            if owing {
                self.owed += rate;
            }
            if !sounding && self.ramp.is_silent() {
                break;
            }
        }

        // The rest of the buffer went out silent while new audio was awaited.
        if self.catching_up || self.generation != target {
            self.owed += (frames - done) as f64 * rate;
        }

        // A seek that arrived during this callback has already set the position; keep it.
        if let Some(at) = position {
            if self.clock.generation() == target {
                self.clock.set_position(at);
            }
        }

        if done < frames && self.feed.drained() && self.clock.end_of_stream() {
            self.clock.set_playing(false);
        }
    }

    /// How playback should run now. Scrubs are always heard as they are rendered. At the file's
    /// own speed the frames pass through, master tempo or not: running the stretcher there would
    /// only change the sound.
    fn playback_mode(&self) -> Mode {
        if self.clock.scrubbing() || (self.clock.tempo() == 1.0 && self.clock.key_shift() == 0) {
            Mode::Direct
        } else if self.clock.stretched() && self.stretcher.is_some() {
            Mode::Stretch
        } else {
            Mode::Varispeed
        }
    }

    fn next_frame(&mut self, pop: bool, rate: f64) -> Option<(f32, f32, u64)> {
        match (self.mode, self.stretcher.as_mut()) {
            (Mode::Varispeed, _) => self.varispeed.next(&mut self.feed, pop, rate),
            (Mode::Stretch, Some(stretcher)) => {
                let pitch = self.clock.pitch();
                stretcher.next(&mut self.feed, pop, rate, pitch, self.clock.end_of_stream())
            }
            _ => self.feed.next_frame(pop),
        }
    }

    /// Takes `generation` as the one being heard, and picks how to play it. Only called with
    /// the ramp at zero.
    /// Skips as much of the new audio as time went by while it was awaited, so a move that keeps
    /// time lands where it would have had it been instant. What has not been decoded yet is
    /// skipped as it arrives.
    fn pay_owed(&mut self) {
        while self.owed >= 1.0 {
            if self.feed.next_frame(true).is_none() {
                return;
            }
            self.owed -= 1.0;
        }
        self.owed = 0.0;
        self.catching_up = false;
    }

    /// The deck should move onto or off the time-stretcher: master tempo or the key changed.
    fn crosses_stretch(&self) -> bool {
        (self.mode == Mode::Stretch) != (self.playback_mode() == Mode::Stretch)
    }

    /// Moves onto or off the time-stretcher where the deck is, without a seek. Leaving it hands
    /// back the audio it had read but not yet played, so the next path starts on the frame after
    /// the last one heard and the deck keeps time exactly.
    fn switch_path(&mut self) {
        self.switching = false;
        if !self.crosses_stretch() {
            return;
        }
        if self.mode == Mode::Stretch {
            if let Some(stretcher) = self.stretcher.as_mut() {
                stretcher.give_back(&mut self.feed);
            }
        }
        self.mode = self.playback_mode();
        match self.mode {
            Mode::Varispeed => self.varispeed.start(self.last, self.last_at),
            Mode::Stretch => {
                if let Some(stretcher) = self.stretcher.as_mut() {
                    stretcher.reset();
                }
            }
            Mode::Direct => {}
        }
    }

    fn adopt(&mut self, generation: u32) {
        if generation == self.generation && self.feed.generation == generation {
            return;
        }
        self.switching = false;
        // Only a move that keeps time carries what went by while it was being made.
        self.catching_up = self.clock.take_keep_time();
        if !self.catching_up {
            self.owed = 0.0;
        }
        self.generation = generation;
        self.feed.adopt(generation);
        self.mode = self.playback_mode();
        match self.mode {
            Mode::Varispeed => self.varispeed.start((0.0, 0.0), 0),
            Mode::Stretch => {
                if let Some(stretcher) = self.stretcher.as_mut() {
                    stretcher.reset();
                }
            }
            Mode::Direct => {}
        }
    }

    /// Whether what is left is the last of the file and short enough that the fade must start.
    fn ending(&self) -> bool {
        if !self.clock.end_of_stream()
            || !self.feed.consumer.is_empty()
            || !self.feed.backlog.is_empty()
        {
            return false;
        }
        self.feed.left_in_block() <= usize::from(FADE_FRAMES)
    }
}
