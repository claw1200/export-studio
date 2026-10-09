#![allow(dead_code)]

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use es_deck::{DeckEvent, Engine, EventSink, NullSink, Sink};

pub const RATE: u32 = 48_000;

/// A 16-bit stereo WAV whose left channel is `f(frame)` and right channel its negation.
pub fn wav(name: &str, frames: u32, f: impl Fn(u32) -> i16) -> PathBuf {
    let path = std::env::temp_dir().join(format!("es-deck-{}-{name}.wav", std::process::id()));
    let mut file = File::create(&path).unwrap();
    let data_len = frames * 4;
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&RATE.to_le_bytes()).unwrap();
    file.write_all(&(RATE * 4).to_le_bytes()).unwrap();
    file.write_all(&4u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_len.to_le_bytes()).unwrap();
    let mut bytes = Vec::with_capacity(data_len as usize);
    for i in 0..frames {
        let v = f(i);
        bytes.extend_from_slice(&v.to_le_bytes());
        bytes.extend_from_slice(&v.saturating_neg().to_le_bytes());
    }
    file.write_all(&bytes).unwrap();
    path
}

/// A frame counter: frame `i` reads `i % 30000`.
pub fn ramp(name: &str, frames: u32) -> PathBuf {
    wav(name, frames, |i| (i % 30_000) as i16)
}

/// A constant level near full scale: the worst case for a click.
pub fn dc(name: &str, frames: u32) -> PathBuf {
    wav(name, frames, |_| 26_000)
}

pub fn value(sample: f32) -> i64 {
    (f64::from(sample) * 32768.0).round() as i64
}

pub struct Rig {
    pub engine: Engine,
    pub sink: Arc<NullSink>,
    pub events: Arc<Mutex<Vec<DeckEvent>>>,
}

impl Rig {
    pub fn new() -> Self {
        let events: Arc<Mutex<Vec<DeckEvent>>> = Arc::default();
        let log = Arc::clone(&events);
        let sink_events: EventSink = Arc::new(move |e| log.lock().unwrap().push(e));
        let slot: Arc<Mutex<Option<Arc<NullSink>>>> = Arc::default();
        let keep = Arc::clone(&slot);
        let engine = Engine::with_sink(1, &sink_events, move |render| {
            let sink = Arc::new(NullSink::new(render, RATE));
            *keep.lock().unwrap() = Some(Arc::clone(&sink));
            Ok(sink as Arc<dyn Sink>)
        })
        .unwrap();
        let sink = slot.lock().unwrap().take().unwrap();
        Self {
            engine,
            sink,
            events,
        }
    }

    pub fn load(&self, path: &Path) {
        self.engine.load(0, path, 1);
        wait_for(|| self.engine.snapshot(0).unwrap().loaded);
        // Let the ring fill so playback starts without an underrun.
        self.settle_ring();
    }

    /// Gives the decode thread time to catch up with whatever it was just asked.
    pub fn settle_ring(&self) {
        std::thread::sleep(Duration::from_millis(60));
    }

    /// Pulls `frames` frames in device-sized callbacks, pausing between them like a real device.
    pub fn pull(&self, frames: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * 2);
        let mut left = frames;
        while left > 0 {
            let n = left.min(256);
            out.extend(self.sink.pull(n));
            left -= n;
            std::thread::sleep(Duration::from_millis(2));
        }
        out
    }

    pub fn position(&self) -> u64 {
        self.engine.snapshot(0).unwrap().position_frames
    }
}

pub fn wait_for(mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The largest jump between consecutive left-channel samples.
pub fn largest_step(out: &[f32]) -> f32 {
    out.chunks_exact(2)
        .map(|f| f[0])
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
}

/// Index of the first frame with any sound in it.
pub fn first_audible(out: &[f32]) -> usize {
    out.chunks_exact(2)
        .position(|f| f[0] != 0.0)
        .expect("no sound at all")
}
