//! One deck: its decode thread, and the handle the engine steers it with.
//!
//! All blocking work (opening, demuxing, decoding, resampling) happens on this thread. The
//! callback only reads blocks that were finished here.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use es_decode::Streamer;
use rtrb::Producer;

use crate::block::{Block, BLOCK_FRAMES, RING_BLOCKS};
use crate::clock::DeckClock;
use crate::scrub::{PcmWindow, Scrubber, WINDOW_REACH};
use crate::{DeckEvent, DeckId, EventSink};

pub(crate) enum Command {
    Load {
        request: u64,
        path: PathBuf,
    },
    /// Transport changes are already in the clock; this only wakes the thread.
    Wake,
    Seek(u64),
    /// The clock is already scrubbing under a new generation; start the read head where it is.
    ScrubBegin,
    /// The pointer is at `frame` (fractional, device rate); `at_ms` is the input event's time.
    ScrubTo {
        frame: f64,
        at_ms: f64,
    },
    Unload,
    Quit,
}

/// How long the thread waits between top-ups while audio is running and the ring is full.
const TOP_UP_WAIT: Duration = Duration::from_micros(500);

/// Blocks queued during a scrub: about 32 ms, so the sound follows the hand closely.
const SCRUB_RING_BLOCKS: usize = 3;

/// Decoded audio added to the scrub window at a time, in seconds.
const WINDOW_STEP: f64 = 0.5;

pub(crate) struct DeckHandle {
    commands: Sender<Command>,
    clock: Arc<DeckClock>,
}

impl DeckHandle {
    pub fn clock(&self) -> &Arc<DeckClock> {
        &self.clock
    }

    pub fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            eprintln!("[es-deck] a deck's decode thread has stopped");
        }
    }
}

impl Drop for DeckHandle {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
    }
}

pub(crate) fn spawn(
    deck: DeckId,
    clock: Arc<DeckClock>,
    producer: Producer<Block>,
    device_rate: u32,
    events: EventSink,
) -> std::io::Result<DeckHandle> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_clock = Arc::clone(&clock);
    std::thread::Builder::new()
        .name(format!("es-deck-{deck}"))
        .spawn(move || {
            let mut worker = Worker {
                deck,
                clock: worker_clock,
                producer,
                device_rate,
                events,
                streamer: None,
                generation: 0,
                scrub: None,
                started: Instant::now(),
            };
            worker.run(&rx);
        })?;
    Ok(DeckHandle {
        commands: tx,
        clock,
    })
}

struct Worker {
    deck: DeckId,
    clock: Arc<DeckClock>,
    producer: Producer<Block>,
    device_rate: u32,
    events: EventSink,
    streamer: Option<Streamer>,
    generation: u32,
    scrub: Option<Scrub>,
    started: Instant,
}

struct Scrub {
    head: Scrubber,
    window: PcmWindow,
}

impl Worker {
    fn run(&mut self, commands: &Receiver<Command>) {
        loop {
            // At most a ring's worth before looking at the channel again, so a seek is never
            // stuck behind a long run of decoding.
            let mut produced = false;
            for _ in 0..RING_BLOCKS {
                if !self.produce() {
                    break;
                }
                produced = true;
            }
            let command = if produced || self.clock.sounding() {
                match commands.recv_timeout(TOP_UP_WAIT) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                // Paused with a full ring: only a command can change anything.
                match commands.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            };
            if let Some(command) = command {
                if !self.handle(command) {
                    return;
                }
            }
        }
    }

    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Load { request, path } => self.load(request, &path),
            Command::Seek(frame) => self.seek(frame),
            Command::ScrubBegin => self.scrub_begin(),
            Command::ScrubTo { frame, at_ms } => {
                let now = self.now_ms();
                if let Some(scrub) = self.scrub.as_mut() {
                    scrub.head.point(frame, at_ms, now);
                }
            }
            Command::Unload => self.unload(),
            Command::Wake => {}
            Command::Quit => return false,
        }
        true
    }

    fn error(&self, load_id: u64, message: String) {
        (self.events)(DeckEvent::Error {
            deck: self.deck,
            load_id,
            message,
        });
    }

    fn load(&mut self, request: u64, path: &std::path::Path) {
        // Selections queued behind a slow open are skipped; only the newest matters.
        if self.clock.requested_load() != request {
            return;
        }
        self.streamer = None;
        self.scrub = None;
        self.clock.set_loop(None);
        self.clock.set_end_of_stream(false);
        let opened = Streamer::open(path, self.device_rate);
        if self.clock.requested_load() != request {
            return;
        }
        match opened {
            Ok(streamer) => {
                self.generation = self.clock.bump_generation();
                self.clock.set_position(0);
                self.clock.set_total(streamer.total_frames());
                self.streamer = Some(streamer);
                if !self.clock.install_load(request) {
                    self.streamer = None;
                    return;
                }
                (self.events)(DeckEvent::Loaded {
                    deck: self.deck,
                    load_id: request,
                    total_frames: self.clock.total(),
                    sample_rate: self.device_rate,
                });
            }
            Err(e) => {
                if !self.clock.owns_load(request) {
                    return;
                }
                self.clock.clear_load();
                self.clock.set_total(0);
                self.clock.set_position(0);
                self.error(request, e.to_string());
            }
        }
    }

    fn unload(&mut self) {
        self.streamer = None;
        self.scrub = None;
        self.generation = self.clock.bump_generation();
        self.clock.set_loop(None);
        self.clock.set_playing(false);
        self.clock.set_scrubbing(false);
        self.clock.clear_load();
        self.clock.set_position(0);
        self.clock.set_total(0);
        self.clock.set_end_of_stream(false);
    }

    fn seek(&mut self, frame: u64) {
        self.scrub = None;
        let Some(streamer) = self.streamer.as_mut() else {
            return;
        };
        match streamer.seek(frame) {
            Ok(landed) => {
                self.clock.set_end_of_stream(false);
                self.clock.set_position(landed);
                // The control side bumped the generation when it moved the position, so the
                // callback stopped trusting the ring at that instant. Adopt it, don't bump again.
                self.generation = self.clock.generation();
            }
            Err(e) => self.error(self.clock.load_id(), e.to_string()),
        }
    }

    /// Back to the loop's in point without a generation change, so the callback plays straight
    /// from the last block before the out point into the first block after the in point.
    fn loop_jump(&mut self, frame: u64) {
        let Some(streamer) = self.streamer.as_mut() else {
            return;
        };
        match streamer.seek(frame) {
            Ok(_) => self.clock.set_end_of_stream(false),
            Err(e) => self.error(self.clock.load_id(), e.to_string()),
        }
    }

    fn now_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    fn scrub_begin(&mut self) {
        if self.streamer.is_none() || !self.clock.scrubbing() {
            return;
        }
        self.generation = self.clock.generation();
        self.clock.set_end_of_stream(false);
        let head = self.clock.position() as f64;
        self.scrub = Some(Scrub {
            head: Scrubber::new(head, self.device_rate, self.now_ms()),
            window: PcmWindow::default(),
        });
    }

    /// Keeps decoded audio around the scrub head: refilled after a jump, extended a step at a
    /// time as the head nears either edge, and trimmed behind it.
    fn feed_window(&mut self) {
        let (Some(scrub), Some(streamer)) = (self.scrub.as_mut(), self.streamer.as_mut()) else {
            return;
        };
        let rate = f64::from(self.device_rate);
        let reach = (WINDOW_REACH * rate) as u64;
        let step = (WINDOW_STEP * rate) as u64;
        let total = self.clock.total();
        let at = scrub.head.head().max(0.0) as u64;
        let target = scrub.head.target().max(0.0) as u64;
        let window = &mut scrub.window;
        let mut decode = |from: u64, frames: u64| -> Option<Vec<f32>> {
            if streamer.position() != from {
                streamer.seek(from).ok()?;
            }
            let mut data = vec![0.0f32; frames as usize * 2];
            let got = streamer.fill(&mut data).ok()?;
            data.truncate(got * 2);
            Some(data)
        };
        let near = |frame: u64| frame + step >= window.start() && frame < window.end() + step;
        if window.is_empty() || !near(at) {
            let from = at.saturating_sub(reach / 2);
            if let Some(data) = decode(from, reach) {
                window.reset(from, data);
            }
            return;
        }
        let ahead = at.max(target);
        let behind = at.min(target);
        if ahead + reach / 2 > window.end() && (total == 0 || window.end() < total) {
            if let Some(data) = decode(window.end(), step) {
                window.append(&data);
            }
        } else if behind < window.start() + reach / 2 && window.start() > 0 {
            let from = window.start().saturating_sub(step);
            if let Some(data) = decode(from, window.start() - from) {
                window.prepend(from, &data);
            }
        }
        window.trim(scrub.head.head(), reach + step);
    }

    /// Renders one block of scrub audio into the shallow ring. False when it is full enough.
    fn produce_scrub(&mut self) -> bool {
        if !self.clock.scrubbing() {
            // Let go: the control side has already sent where to land.
            return false;
        }
        if self.producer.slots() + SCRUB_RING_BLOCKS <= RING_BLOCKS {
            return false;
        }
        self.feed_window();
        let now = self.now_ms();
        let Some(scrub) = self.scrub.as_mut() else {
            return false;
        };
        let mut block = Block::empty(self.generation, scrub.head.head().max(0.0) as u64);
        scrub.head.render(&scrub.window, &mut block.samples, now);
        block.frames = BLOCK_FRAMES as u16;
        self.producer.push(block).is_ok()
    }

    /// Decodes one block into the ring. False when there is nothing to add.
    fn produce(&mut self) -> bool {
        if self.generation != self.clock.generation() {
            return false;
        }
        if self.scrub.is_some() {
            return self.produce_scrub();
        }
        if self.producer.is_full() {
            return false;
        }
        let active = self.clock.active_loop();
        if let Some((from, to)) = active {
            if self.streamer.as_ref().is_some_and(|s| s.position() >= to) {
                self.loop_jump(from);
            }
        }
        let generation = self.generation;
        let Some(streamer) = self.streamer.as_mut() else {
            return false;
        };
        if streamer.finished() {
            self.clock.set_end_of_stream(true);
            return false;
        }
        let start = streamer.position();
        // Only up to the out point, so the jump lands on the frame.
        let room = active.map_or(BLOCK_FRAMES, |(_, to)| {
            usize::try_from(to.saturating_sub(start))
                .unwrap_or(BLOCK_FRAMES)
                .clamp(1, BLOCK_FRAMES)
        });
        let mut block = Block::empty(generation, start);
        let frames = match streamer.fill(&mut block.samples[..room * 2]) {
            Ok(frames) => frames,
            Err(e) => {
                // A decode that fails mid-track stops the deck rather than playing on garbage.
                self.clock.set_end_of_stream(true);
                self.error(self.clock.load_id(), e.to_string());
                return false;
            }
        };
        if frames == 0 {
            if streamer.finished() {
                self.clock.set_end_of_stream(true);
            }
            return false;
        }
        block.frames = frames as u16;
        let pushed = self.producer.push(block).is_ok();
        if let Some((from, to)) = active {
            if self.streamer.as_ref().is_some_and(|s| s.position() >= to) {
                self.loop_jump(from);
            }
        }
        pushed
    }
}
