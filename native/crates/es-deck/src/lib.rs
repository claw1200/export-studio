//! The playback engine: decks, one output device, one clock per deck.
//!
//! # Threads
//!
//! - **The audio callback**, owned by the device. Reads each deck's ring, sums, applies the
//!   master level and publishes each deck's position. It never allocates, locks or blocks.
//! - **One decode thread per deck**, which does all file I/O, decoding and resampling. Blocks it
//!   produces carry the generation they belong to, which is what makes a seek exact.
//! - **The control side** (the Node binding, or a test), which only sends commands and reads
//!   atomics. Nothing it does can hold up audio.

mod block;
mod clock;
mod deck;
mod fade;
mod reader;
mod scrub;
mod sink;
mod smooth;
mod tempo;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub use block::{BLOCK_FRAMES, RING_BLOCKS};
pub use clock::DeckSnapshot;
pub use fade::FADE_FRAMES;
pub use sink::{CpalSink, NullSink, Render, Sink};
pub use tempo::{MAX_TEMPO, MIN_TEMPO};

use clock::DeckClock;
use reader::DeckReader;
use smooth::Smoothed;
use tempo::Stretcher;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("there is no audio output device")]
    NoDevice,
    #[error("the audio device could not be used: {0}")]
    Device(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// Which deck, by index.
pub type DeckId = usize;

#[derive(Debug, Clone)]
pub enum DeckEvent {
    Loaded {
        deck: DeckId,
        load_id: u64,
        total_frames: u64,
        sample_rate: u32,
    },
    Error {
        deck: DeckId,
        load_id: u64,
        message: String,
    },
}

/// Receives deck events. Called from a decode thread, never from the audio callback.
pub type EventSink = Arc<dyn Fn(DeckEvent) + Send + Sync>;

/// Frames of a loop end closer than this to the playhead mean the ring already holds audio past
/// it, so setting the loop has to resync rather than wait for the decoder to reach the out point.
const RING_REACH: u64 = (RING_BLOCKS * BLOCK_FRAMES) as u64;

pub struct Engine {
    decks: Vec<deck::DeckHandle>,
    sink: Arc<dyn Sink>,
    sample_rate: u32,
    master: Arc<AtomicU32>,
}

impl Engine {
    /// `decks` decks on the system's default output.
    pub fn new(decks: usize, events: &EventSink) -> Result<Self> {
        Self::with_sink(decks, events, |render| {
            Ok(Arc::new(CpalSink::open(render)?) as Arc<dyn Sink>)
        })
    }

    /// The same engine on a sink of the caller's choosing, which is how it is tested.
    pub fn with_sink<F>(decks: usize, events: &EventSink, open: F) -> Result<Self>
    where
        F: FnOnce(Render) -> Result<Arc<dyn Sink>>,
    {
        let clocks: Vec<Arc<DeckClock>> =
            (0..decks).map(|_| Arc::new(DeckClock::default())).collect();
        let mut producers = Vec::with_capacity(decks);
        let mut stretchers = Vec::with_capacity(decks);
        let mut readers = Vec::with_capacity(decks);
        for clock in &clocks {
            let (producer, consumer) = rtrb::RingBuffer::<block::Block>::new(RING_BLOCKS);
            let (stretcher_in, stretcher_out) = rtrb::RingBuffer::<Box<Stretcher>>::new(1);
            producers.push(producer);
            stretchers.push(stretcher_in);
            readers.push(DeckReader::new(consumer, Arc::clone(clock), stretcher_out));
        }

        let master = Arc::new(AtomicU32::new(1.0_f32.to_bits()));
        let level = Arc::clone(&master);
        let rate = Arc::new(AtomicU32::new(0));
        let callback_rate = Arc::clone(&rate);
        let mut smoothed: Option<Smoothed> = None;
        let render: Render = Box::new(move |out: &mut [f32]| {
            for reader in &mut readers {
                reader.mix_into(out);
            }
            let target = f32::from_bits(level.load(Ordering::Relaxed));
            let smoothed = smoothed.get_or_insert_with(|| {
                Smoothed::new(target, callback_rate.load(Ordering::Relaxed), 10.0)
            });
            for frame in out.chunks_exact_mut(2) {
                let gain = smoothed.step(target);
                frame[0] = (frame[0] * gain).clamp(-1.0, 1.0);
                frame[1] = (frame[1] * gain).clamp(-1.0, 1.0);
            }
        });

        let sink = open(render)?;
        let sample_rate = sink.sample_rate();
        rate.store(sample_rate, Ordering::Relaxed);
        // The stretcher is sized for the device rate, which is only known now. Each callback
        // takes its deck's from here; the slot holds exactly one, so the push cannot fail.
        for mut slot in stretchers {
            let _ = slot.push(Box::new(Stretcher::new(sample_rate)));
        }

        let mut handles = Vec::with_capacity(decks);
        for (id, (clock, producer)) in clocks.into_iter().zip(producers).enumerate() {
            clock.set_sample_rate(sample_rate);
            handles.push(deck::spawn(
                id,
                clock,
                producer,
                sample_rate,
                Arc::clone(events),
            )?);
        }
        Ok(Self {
            decks: handles,
            sink,
            sample_rate,
            master,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn deck_count(&self) -> usize {
        self.decks.len()
    }

    fn deck(&self, deck: DeckId) -> Option<&deck::DeckHandle> {
        self.decks.get(deck)
    }

    fn to_frames(&self, ms: f64) -> u64 {
        (ms.max(0.0) * f64::from(self.sample_rate) / 1000.0).round() as u64
    }

    /// Points a deck at a file. Readiness arrives as [`DeckEvent::Loaded`]. Only the newest
    /// `request` may finish loading.
    pub fn load(&self, deck: DeckId, path: &Path, request: u64) {
        let request = request.max(1);
        if let Some(handle) = self.deck(deck) {
            handle.clock().request_load(request);
            handle.send(deck::Command::Load {
                request,
                path: PathBuf::from(path),
            });
        }
        self.settle();
    }

    pub fn unload(&self, deck: DeckId) {
        if let Some(handle) = self.deck(deck) {
            handle.clock().request_load(0);
            handle.send(deck::Command::Unload);
        }
        self.settle();
    }

    pub fn play(&self, deck: DeckId) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        if !clock.loaded() {
            return;
        }
        // Playing to the end and pressing play again starts over.
        if clock.end_of_stream() && clock.total() > 0 && clock.position() >= clock.total() {
            self.seek_frames(deck, 0);
        }
        clock.set_playing(true);
        handle.send(deck::Command::Wake);
        self.settle();
    }

    pub fn pause(&self, deck: DeckId) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        handle.clock().set_playing(false);
        handle.send(deck::Command::Wake);
        self.settle();
    }

    /// Moves the playhead. Fractional milliseconds, because a cue point is a place in the music
    /// and rounding it to a whole millisecond moves it.
    pub fn seek(&self, deck: DeckId, ms: f64) {
        self.seek_frames(deck, self.to_frames(ms));
    }

    /// Pauses and moves in one step, which is what releasing CUE does. Two separate commands
    /// would let the fade-out of the old audio land between them.
    pub fn stop_at(&self, deck: DeckId, ms: f64) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        handle.clock().set_playing(false);
        self.seek_frames(deck, self.to_frames(ms));
        self.settle();
    }

    /// Takes hold of the deck: from here its sound follows [`scrub_to`](Self::scrub_to) rather
    /// than playback, which carries on from wherever the scrub is let go.
    pub fn scrub_begin(&self, deck: DeckId) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        if !clock.loaded() || clock.scrubbing() {
            return;
        }
        clock.set_scrub_target(clock.position());
        clock.set_scrubbing(true);
        // Whatever playback had queued is dropped, so the scrub is heard at once.
        clock.bump_generation();
        handle.send(deck::Command::ScrubBegin);
        self.settle();
    }

    /// The pointer is at `ms`. `at_ms` is the input event's own timestamp: the head's speed is
    /// measured from these, not from when the commands happen to arrive.
    pub fn scrub_to(&self, deck: DeckId, ms: f64, at_ms: f64) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        if !clock.scrubbing() {
            return;
        }
        let frame = ms.max(0.0) * f64::from(self.sample_rate) / 1000.0;
        let total = clock.total();
        let frame = if total > 0 {
            frame.min(total as f64)
        } else {
            frame
        };
        clock.set_scrub_target(frame.round() as u64);
        handle.send(deck::Command::ScrubTo { frame, at_ms });
    }

    /// Lets go: the deck lands exactly where the pointer was, playing again if it was playing.
    pub fn scrub_end(&self, deck: DeckId) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        if !clock.scrubbing() {
            return;
        }
        self.seek_frames(deck, clock.scrub_target());
        handle.send(deck::Command::Wake);
        self.settle();
    }

    pub fn seek_frames(&self, deck: DeckId, frame: u64) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        let total = clock.total();
        let frame = if total > 0 { frame.min(total) } else { frame };
        // A seek ends any scrub: the deck is wherever it was sent.
        clock.set_scrubbing(false);
        // Position and generation move here, not when the deck thread gets to the command, so
        // the callback stops stamping old audio's position over this one immediately.
        clock.set_position(frame);
        clock.bump_generation();
        handle.send(deck::Command::Seek(frame));
    }

    /// Sets a loop between two points. With `enter`, playback goes into it and a playhead outside
    /// it goes to the in point; without, the loop is left exited and the playhead stays put.
    pub fn set_loop(&self, deck: DeckId, in_ms: f64, out_ms: f64, enter: bool) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let (from, to) = (self.to_frames(in_ms), self.to_frames(out_ms));
        if to <= from {
            return;
        }
        let clock = handle.clock();
        clock.set_loop(Some((from, to)));
        clock.set_looping(enter);
        if !enter {
            return;
        }
        let at = clock.position();
        if at >= to || at < from {
            self.seek_frames(deck, from);
        } else if to < at + RING_REACH {
            // The ring may already hold audio past the out point: requeue from here.
            self.seek_frames(deck, at);
        }
        handle.send(deck::Command::Wake);
    }

    /// RELOOP (back to the in point and inside the loop) and EXIT (out of it, range kept).
    pub fn set_looping(&self, deck: DeckId, on: bool) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        let Some((from, to)) = clock.loop_range() else {
            return;
        };
        clock.set_looping(on);
        let at = clock.position();
        if on {
            self.seek_frames(deck, from);
        } else if at < to && to < at + RING_REACH {
            // The ring may already hold the jump back to the in point: requeue from here.
            self.seek_frames(deck, at);
        }
        handle.send(deck::Command::Wake);
    }

    pub fn clear_loop(&self, deck: DeckId) {
        if let Some(handle) = self.deck(deck) {
            handle.clock().set_loop(None);
        }
    }

    /// Playback speed as a multiple of the file's, heard from the next callback. A deck keeps its
    /// tempo across loads, as a pitch fader stays where it was left.
    pub fn set_tempo(&self, deck: DeckId, tempo: f64) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let tempo = if tempo.is_finite() {
            tempo.clamp(MIN_TEMPO, MAX_TEMPO)
        } else {
            1.0
        };
        handle.clock().set_tempo(tempo);
    }

    /// Master tempo: keep the key when the tempo changes. Switching requeues from the playhead,
    /// since the stretcher has to be primed with audio the ring has already passed.
    pub fn set_master_tempo(&self, deck: DeckId, on: bool) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        if clock.master_tempo() == on {
            return;
        }
        clock.set_master_tempo(on);
        if clock.loaded() && !clock.scrubbing() {
            self.seek_frames(deck, clock.position());
            handle.send(deck::Command::Wake);
        }
    }

    /// The master level, linear, 0 to 1.
    pub fn set_volume(&self, gain: f32) {
        let safe = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.master.store(safe.to_bits(), Ordering::Relaxed);
    }

    pub fn snapshot(&self, deck: DeckId) -> Option<DeckSnapshot> {
        self.deck(deck).map(|d| d.clock().snapshot())
    }

    /// Whether anything needs the device: a deck playing or being dragged.
    pub fn any_sounding(&self) -> bool {
        self.decks.iter().any(|d| d.clock().sounding())
    }

    /// Runs the device while something needs it, and lets it go idle otherwise. Call it
    /// periodically too: a deck that plays to the end stops itself without telling anyone.
    pub fn settle(&self) {
        if self.any_sounding() {
            self.sink.start();
        } else {
            self.sink.stop();
        }
    }
}
