//! One deck's side of the audio callback.
//!
//! Realtime: no allocation, no locks, nothing that can block. Where audio is missing it writes
//! silence, and every edge goes through the fade ramp.

use std::sync::Arc;

use rtrb::Consumer;

use crate::block::{Block, BLOCK_FRAMES};
use crate::clock::DeckClock;
use crate::fade::{Ramp, FADE_FRAMES};
use crate::tempo::{Mode, Stretcher, Varispeed};

/// The deck's decoded audio as single frames, in file order.
pub(crate) struct Feed {
    consumer: Consumer<Block>,
    current: Option<Block>,
    offset: usize,
    /// Blocks from before this generation are dropped.
    generation: u32,
}

impl Feed {
    /// The next frame and the playhead after it. With `pop` false only the block in hand is read.
    pub fn next_frame(&mut self, pop: bool) -> Option<(f32, f32, u64)> {
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

    /// Frames queued, near enough: whole blocks are counted as full.
    pub fn available(&self) -> usize {
        self.left_in_block() + self.consumer.slots() * BLOCK_FRAMES
    }

    fn left_in_block(&self) -> usize {
        self.current
            .as_ref()
            .map_or(0, |b| usize::from(b.frames).saturating_sub(self.offset))
    }

    fn drained(&self) -> bool {
        self.current.is_none() && self.consumer.is_empty()
    }

    fn adopt(&mut self, generation: u32) {
        self.generation = generation;
        if self
            .current
            .as_ref()
            .is_some_and(|b| b.generation < generation)
        {
            self.current = None;
            self.offset = 0;
        }
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
            feed: Feed {
                consumer,
                current: None,
                offset: 0,
                generation: 0,
            },
            clock,
            ramp: Ramp::silent(),
            last: (0.0, 0.0),
            last_at: 0,
            generation: 0,
            mode: Mode::Direct,
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
            let changing = self.generation != target;
            let open = sounding && !changing && !self.ending();
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
            if !sounding && self.ramp.is_silent() {
                break;
            }
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
        if self.clock.scrubbing() || self.clock.tempo() == 1.0 {
            Mode::Direct
        } else if self.clock.master_tempo() && self.stretcher.is_some() {
            Mode::Stretch
        } else {
            Mode::Varispeed
        }
    }

    fn next_frame(&mut self, pop: bool, rate: f64) -> Option<(f32, f32, u64)> {
        match (self.mode, self.stretcher.as_mut()) {
            (Mode::Varispeed, _) => self.varispeed.next(&mut self.feed, pop, rate),
            (Mode::Stretch, Some(stretcher)) => {
                stretcher.next(&mut self.feed, pop, rate, self.clock.end_of_stream())
            }
            _ => self.feed.next_frame(pop),
        }
    }

    /// Takes `generation` as the one being heard, and picks how to play it. Only called with
    /// the ramp at zero.
    fn adopt(&mut self, generation: u32) {
        if generation == self.generation && self.feed.generation == generation {
            return;
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
        if !self.clock.end_of_stream() || !self.feed.consumer.is_empty() {
            return false;
        }
        self.feed.left_in_block() <= usize::from(FADE_FRAMES)
    }
}
