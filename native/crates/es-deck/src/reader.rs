//! One deck's side of the audio callback.
//!
//! Realtime: no allocation, no locks, nothing that can block. Where audio is missing it writes
//! silence, and every edge goes through the fade ramp.

use std::sync::Arc;

use rtrb::Consumer;

use crate::block::Block;
use crate::clock::DeckClock;
use crate::fade::{Ramp, FADE_FRAMES};

pub(crate) struct DeckReader {
    consumer: Consumer<Block>,
    clock: Arc<DeckClock>,
    current: Option<Block>,
    offset: usize,
    ramp: Ramp,
    /// The last frame played, so running dry fades it out instead of cutting.
    last: (f32, f32),
    /// The generation being heard. It trails the clock's across a seek: the old audio is faded
    /// to silence first, and only then is the new generation taken up.
    generation: u32,
}

impl DeckReader {
    pub fn new(consumer: Consumer<Block>, clock: Arc<DeckClock>) -> Self {
        Self {
            consumer,
            clock,
            current: None,
            offset: 0,
            ramp: Ramp::silent(),
            last: (0.0, 0.0),
            generation: 0,
        }
    }

    /// Adds this deck's audio to `out` (interleaved stereo).
    pub fn mix_into(&mut self, out: &mut [f32]) {
        let target = self.clock.generation();
        let sounding = self.clock.sounding();
        if !sounding && self.ramp.is_silent() {
            self.adopt(target);
            return;
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
            let Some((left, right, at)) = self.next_frame(!changing) else {
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

        if done < frames
            && self.current.is_none()
            && self.consumer.is_empty()
            && self.clock.end_of_stream()
        {
            self.clock.set_playing(false);
        }
    }

    /// Takes `generation` as the one being heard. Only called with the ramp at zero.
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

    /// The next frame and the playhead after it. Blocks from an older generation are dropped.
    fn next_frame(&mut self, pop: bool) -> Option<(f32, f32, u64)> {
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

    /// Whether what is left is the last of the file and short enough that the fade must start.
    fn ending(&self) -> bool {
        if !self.clock.end_of_stream() || !self.consumer.is_empty() {
            return false;
        }
        let left = self
            .current
            .as_ref()
            .map_or(0, |b| usize::from(b.frames).saturating_sub(self.offset));
        left <= usize::from(FADE_FRAMES)
    }
}
