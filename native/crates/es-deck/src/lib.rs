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
mod isolator;
mod reader;
mod scrub;
mod sink;
mod smooth;
mod sync;
mod tempo;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

pub use block::{BLOCK_FRAMES, RING_BLOCKS, RING_CAPACITY};
pub use clock::DeckSnapshot;
pub use fade::FADE_FRAMES;
pub use sink::{CpalSink, NullSink, Render, Sink};
pub use tempo::{MAX_TEMPO, MIN_TEMPO};

use clock::{DeckClock, KeepTime};
use reader::DeckReader;
use smooth::Smoothed;
use sync::{BeatSync, Grid, Timeline};
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

/// Longest `play_together` waits for the decks' audio to be queued before starting anyway.
const READY_WAIT: Duration = Duration::from_millis(250);

/// Furthest the key can be moved either way, in semitones.
pub const MAX_KEY_SHIFT: i32 = 12;

/// Frames of a loop end closer than this to the playhead mean the ring already holds audio past
/// it, so setting the loop has to resync rather than wait for the decoder to reach the out point.
const RING_REACH: u64 = (RING_BLOCKS * BLOCK_FRAMES) as u64;

pub struct Engine {
    decks: Vec<deck::DeckHandle>,
    sink: Arc<dyn Sink>,
    sample_rate: u32,
    master: Arc<AtomicU32>,
    /// Each deck's own level, linear, applied before the master: a channel fader or crossfader.
    gains: Vec<Arc<AtomicU32>>,
    /// Each deck's isolator low level, linear, 0 to 1, as f32 bits. Negative is out of the path.
    lows: Vec<Arc<AtomicU32>>,
    /// The callback only ever tries the lock, so holding it never holds up audio.
    sync: Arc<Mutex<BeatSync>>,
    timeline: Arc<Timeline>,
}

/// A deck's isolator out of its path: its signal goes through untouched.
const ISOLATOR_OFF: f32 = -1.0;

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
        let callback_clocks: Vec<Arc<DeckClock>> = clocks.iter().map(Arc::clone).collect();
        let sync = Arc::new(Mutex::new(BeatSync::new(decks, 0)));
        let callback_sync = Arc::clone(&sync);
        let timeline = Arc::new(Timeline::new(decks));
        let callback_timeline = Arc::clone(&timeline);
        let mut elapsed: u64 = 0;
        let mut producers = Vec::with_capacity(decks);
        let mut stretchers = Vec::with_capacity(decks);
        let mut readers = Vec::with_capacity(decks);
        for clock in &clocks {
            let (producer, consumer) = rtrb::RingBuffer::<block::Block>::new(RING_CAPACITY);
            let (stretcher_in, stretcher_out) = rtrb::RingBuffer::<Box<Stretcher>>::new(1);
            producers.push(producer);
            stretchers.push(stretcher_in);
            readers.push(DeckReader::new(consumer, Arc::clone(clock), stretcher_out));
        }

        let master = Arc::new(AtomicU32::new(1.0_f32.to_bits()));
        let level = Arc::clone(&master);
        let gains: Vec<Arc<AtomicU32>> = (0..decks)
            .map(|_| Arc::new(AtomicU32::new(1.0_f32.to_bits())))
            .collect();
        let deck_levels: Vec<Arc<AtomicU32>> = gains.iter().map(Arc::clone).collect();
        let lows: Vec<Arc<AtomicU32>> = (0..decks)
            .map(|_| Arc::new(AtomicU32::new(ISOLATOR_OFF.to_bits())))
            .collect();
        let deck_lows: Vec<Arc<AtomicU32>> = lows.iter().map(Arc::clone).collect();
        let mut isolators: Vec<Option<isolator::Isolator>> = (0..decks).map(|_| None).collect();
        let rate = Arc::new(AtomicU32::new(0));
        let callback_rate = Arc::clone(&rate);
        let mut smoothed: Option<Smoothed> = None;
        let mut deck_smoothed: Vec<Option<Smoothed>> = (0..decks).map(|_| None).collect();
        // Each deck mixes here first so its own level can be applied. Sized for a large device
        // buffer up front so the callback does not allocate.
        let mut scratch: Vec<f32> = vec![0.0; 16_384];
        let render: Render = Box::new(move |out: &mut [f32]| {
            if scratch.len() < out.len() {
                scratch.resize(out.len(), 0.0);
            }
            let sample_rate = callback_rate.load(Ordering::Relaxed);
            if let Ok(mut sync) = callback_sync.try_lock() {
                sync.follow_tempo(&callback_clocks);
            }
            for ((((reader, level), smooth), low), isolator) in readers
                .iter_mut()
                .zip(&deck_levels)
                .zip(&mut deck_smoothed)
                .zip(&deck_lows)
                .zip(&mut isolators)
            {
                let target = f32::from_bits(level.load(Ordering::Relaxed));
                let smooth = smooth.get_or_insert_with(|| Smoothed::new(target, sample_rate, 10.0));
                let deck_out = &mut scratch[..out.len()];
                deck_out.fill(0.0);
                reader.mix_into(deck_out, elapsed);
                let low = f32::from_bits(low.load(Ordering::Relaxed));
                isolator
                    .get_or_insert_with(|| isolator::Isolator::new(sample_rate))
                    .process(deck_out, (low >= 0.0).then_some(low));
                for (frame, mixed) in out.chunks_exact_mut(2).zip(deck_out.chunks_exact(2)) {
                    let gain = smooth.step(target);
                    frame[0] += mixed[0] * gain;
                    frame[1] += mixed[1] * gain;
                }
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
            elapsed += (out.len() / 2) as u64;
            callback_timeline.publish(elapsed, &callback_clocks);
        });

        let sink = open(render)?;
        let sample_rate = sink.sample_rate();
        rate.store(sample_rate, Ordering::Relaxed);
        *lock(&sync) = BeatSync::new(decks, sample_rate);
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
            gains,
            lows,
            sync,
            timeline,
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

    /// Starts the deck. One synced with a playing deck starts on that deck's beat.
    pub fn play(&self, deck: DeckId) {
        self.start(deck, true);
    }

    /// Starts the deck where it is, synced or not: what holding CUE does.
    pub fn play_unsynced(&self, deck: DeckId) {
        self.start(deck, false);
    }

    fn start(&self, deck: DeckId, synced: bool) {
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
        if synced {
            self.join_partner(deck);
        }
        clock.set_playing(true);
        handle.send(deck::Command::Wake);
        self.settle();
    }

    /// Starts several decks in one step, so they begin in the same device callback and stay
    /// as far apart as their playheads say. Decks that are not loaded are left alone.
    pub fn play_together(&self, decks: &[DeckId]) {
        let ready: Vec<(DeckId, &deck::DeckHandle)> = decks
            .iter()
            .filter_map(|&deck| self.deck(deck).map(|handle| (deck, handle)))
            .filter(|(_, handle)| handle.clock().loaded())
            .collect();
        let link = lock(&self.sync).link();
        if let Some((master, follower)) = link {
            if [master, follower]
                .iter()
                .all(|d| ready.iter().any(|(r, _)| r == d))
            {
                self.line_up_paused(follower);
            }
        }
        // Each deck may have just been moved. Wait (briefly) until all have the audio for where
        // they are now queued, so none starts a buffer late on an empty ring.
        let deadline = Instant::now() + READY_WAIT;
        while ready.iter().any(|(_, handle)| !handle.clock().buffered())
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(1));
        }
        // As with `play`, a deck that played to its end starts over.
        for &(deck, handle) in &ready {
            let clock = handle.clock();
            if clock.end_of_stream() && clock.total() > 0 && clock.position() >= clock.total() {
                self.seek_frames(deck, 0);
            }
        }
        // A deck whose audio is still not queued starts late, so it skips what the others played
        // meanwhile. A paused deck moved twice can be stuck here: its ring is full of the first
        // move's audio until it plays.
        let start = self.timeline.read().frames;
        for &(deck, handle) in &ready {
            let clock = handle.clock();
            if !clock.buffered() {
                self.move_to(deck, clock.position(), KeepTime::Since(start));
            }
        }
        // Flip every deck to playing before waking any, so no callback sees only some started.
        for (_, handle) in &ready {
            handle.clock().set_playing(true);
        }
        for (_, handle) in &ready {
            handle.send(deck::Command::Wake);
        }
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

    /// Moves the playhead by `delta_ms` from wherever it is when this arrives, not from where a
    /// caller last saw it. While playing the deck keeps time through the move: the moment spent
    /// fading out and fetching the new audio is skipped at the deck's tempo, so it lands where it
    /// would be had the jump been instant. That is what lining one deck up with another needs.
    pub fn nudge(&self, deck: DeckId, delta_ms: f64) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        if !clock.loaded() || !delta_ms.is_finite() {
            return;
        }
        let delta = (delta_ms * f64::from(self.sample_rate) / 1000.0).round() as i64;
        let frame = u64::try_from(clock.position() as i64 + delta).unwrap_or(0);
        let keep_time = if clock.playing() && !clock.scrubbing() {
            KeepTime::FromMove
        } else {
            KeepTime::No
        };
        self.move_to(deck, frame, keep_time);
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
        self.move_to(deck, frame, KeepTime::No);
    }

    /// Moves the playhead to `frame`. The time-keeping mark is set with the move, never apart from
    /// it, so the callback takes it up with this move and no other.
    fn move_to(&self, deck: DeckId, frame: u64, keep_time: KeepTime) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        let clock = handle.clock();
        let total = clock.total();
        let frame = if total > 0 { frame.min(total) } else { frame };
        // A seek ends any scrub: the deck is wherever it was sent.
        clock.set_scrubbing(false);
        clock.set_keep_time(keep_time);
        // Position and generation move here, not when the deck thread gets to the command, so
        // the callback stops stamping old audio's position over this one immediately.
        clock.set_position(frame);
        let generation = clock.bump_generation();
        handle.send(deck::Command::Seek { frame, generation });
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
    /// tempo across loads, as a pitch fader stays where it was left. With master tempo, crossing
    /// the file's own speed moves the deck onto or off the time-stretcher; the callback does that
    /// in place, so the deck keeps time through it.
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

    /// Master tempo: keep the key when the tempo changes. Away from the file's own speed this moves
    /// the deck onto or off the time-stretcher, in place in the callback, so it keeps time. At the
    /// file's own speed the frames pass through either way.
    pub fn set_master_tempo(&self, deck: DeckId, on: bool) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        handle.clock().set_master_tempo(on);
    }

    /// Moves the deck's key by `semitones`, within an octave either way, keeping its speed. Any
    /// shift plays through the time-stretcher; back at zero the deck plays as it did before. Both
    /// moves happen in place in the callback, so the deck keeps time through them.
    pub fn set_key_shift(&self, deck: DeckId, semitones: i32) {
        let Some(handle) = self.deck(deck) else {
            return;
        };
        handle
            .clock()
            .set_key_shift(semitones.clamp(-MAX_KEY_SHIFT, MAX_KEY_SHIFT));
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

    /// One deck's own level, linear, 0 to 1, glided like the master so a fader move never clicks.
    pub fn set_deck_gain(&self, deck: DeckId, gain: f32) {
        let safe = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if let Some(level) = self.gains.get(deck) {
            level.store(safe.to_bits(), Ordering::Relaxed);
        }
    }

    /// One deck's isolator low band, linear, 0 (killed) to 1 (full), or `None` to take the
    /// isolator out of its path so the deck plays untouched. Glided, so a move never clicks.
    pub fn set_deck_low(&self, deck: DeckId, gain: Option<f32>) {
        let value = match gain {
            Some(g) if g.is_finite() => g.clamp(0.0, 1.0),
            Some(_) => 1.0,
            None => ISOLATOR_OFF,
        };
        if let Some(low) = self.lows.get(deck) {
            low.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// The deck's beats, at fractional `times_ms` into the file, each with the tempo from it on.
    /// Empty clears it. Sync goes by these.
    pub fn set_beatgrid(&self, deck: DeckId, times_ms: &[f64], bpms: &[f64]) {
        let mut sync = lock(&self.sync);
        let grid = Grid::new(times_ms, bpms, sync.sample_rate());
        sync.set_grid(deck, grid);
        sync.follow_tempo(&self.clocks());
    }

    /// `follower` keeps `master`'s tempo, at half or double time when that is nearer its own, and
    /// starts on its beat. Turning it on lines the follower up with the master straight away when
    /// both are playing or both paused; handing the master over does not, so a deck already in
    /// time is not moved. `None` turns sync off; each deck keeps the tempo it has.
    pub fn set_sync(&self, link: Option<(DeckId, DeckId)>) {
        let was_on = {
            let mut sync = lock(&self.sync);
            let was_on = sync.link().is_some();
            sync.set_link(link);
            sync.follow_tempo(&self.clocks());
            was_on
        };
        let Some((master, follower)) = link.filter(|_| !was_on) else {
            return;
        };
        let (Some(m), Some(f)) = (self.deck(master), self.deck(follower)) else {
            return;
        };
        match (m.clock().playing(), f.clock().playing()) {
            (true, true) => self.join_partner(follower),
            (false, false) => self.line_up_paused(follower),
            _ => {}
        }
    }

    /// Lines a playing deck up with its synced partner, if that is playing too: what pressing play
    /// during a CUE hold does, the hold having started unsynced.
    pub fn align(&self, deck: DeckId) {
        if self.deck(deck).is_some_and(|d| d.clock().playing()) {
            self.join_partner(deck);
        }
    }

    fn clocks(&self) -> Vec<&Arc<DeckClock>> {
        self.decks.iter().map(deck::DeckHandle::clock).collect()
    }

    /// Moves `deck`, playing or about to, onto its playing partner's beat. The partner's position
    /// is taken from one callback, and the deck pays back the time since then once its new audio
    /// arrives, so it lands in phase however long the move takes.
    fn join_partner(&self, deck: DeckId) {
        let moment = self.timeline.read();
        let target = {
            let mut sync = lock(&self.sync);
            let Some(partner) = sync.partner(deck) else {
                return;
            };
            let (Some(own), Some(other)) = (self.deck(deck), self.deck(partner)) else {
                return;
            };
            let (own, other) = (own.clock(), other.clock());
            if !own.loaded() || !other.loaded() || !other.playing() || other.scrubbing() {
                return;
            }
            sync.follow_tempo(&self.clocks());
            // A paused deck may have moved since the callback last ran.
            let own_at = if own.playing() {
                moment.positions[deck]
            } else {
                own.position()
            };
            let target = sync.phase_target(deck, own_at as f64, moment.positions[partner] as f64);
            // Moving a playing deck dips its sound, which is only worth it to fix an audible gap.
            // One about to start is always moved, to skip what its partner plays meanwhile.
            let in_time = target.is_some_and(|t| (t - own_at as f64).abs() < self.aligned_frames());
            if own.playing() && in_time {
                return;
            }
            target
        };
        if let Some(target) = target {
            self.move_to(
                deck,
                target.round().max(0.0) as u64,
                KeepTime::Since(moment.frames),
            );
        }
    }

    /// Off by less than this a playing deck counts as on the beat: half a millisecond.
    fn aligned_frames(&self) -> f64 {
        f64::from(self.sample_rate) / 2000.0
    }

    /// With both decks paused, moves the follower onto the master's nearest beat.
    fn line_up_paused(&self, follower: DeckId) {
        let target = {
            let mut sync = lock(&self.sync);
            let Some(master) = sync.partner(follower) else {
                return;
            };
            let (Some(own), Some(other)) = (self.deck(follower), self.deck(master)) else {
                return;
            };
            let (own, other) = (own.clock(), other.clock());
            if !own.loaded() || !other.loaded() || own.playing() || other.playing() {
                return;
            }
            sync.follow_tempo(&self.clocks());
            sync.phase_target(follower, own.position() as f64, other.position() as f64)
        };
        if let Some(target) = target {
            self.seek_frames(follower, target.round().max(0.0) as u64);
        }
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

fn lock(sync: &Mutex<BeatSync>) -> std::sync::MutexGuard<'_, BeatSync> {
    sync.lock().unwrap_or_else(PoisonError::into_inner)
}
