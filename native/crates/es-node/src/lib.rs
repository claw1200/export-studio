//! Node bindings for the deck engine and waveform analysis, loaded into Electron's main process.
//!
//! Commands are synchronous and never block: they only send messages and set atomics. Positions
//! are read by polling `snapshot`; load results and errors arrive through the event callback.

use std::path::Path;
use std::sync::Arc;

use es_deck::{DeckEvent, DeckSnapshot, Engine, EventSink};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{Env, JsFunction, Task};
use napi_derive::napi;

#[napi(object)]
pub struct DeckEventJs {
    /// `loaded` or `error`.
    pub kind: String,
    pub deck: u32,
    pub load_id: f64,
    pub total_frames: f64,
    pub sample_rate: u32,
    pub message: Option<String>,
}

#[napi(object)]
pub struct DeckSnapshotJs {
    pub position_frames: f64,
    pub total_frames: f64,
    pub generation: u32,
    pub sample_rate: u32,
    pub playing: bool,
    pub scrubbing: bool,
    pub loaded: bool,
    pub load_id: f64,
    pub loop_in_frames: f64,
    pub loop_out_frames: f64,
    pub looping: bool,
    pub tempo: f64,
    pub master_tempo: bool,
}

impl From<DeckSnapshot> for DeckSnapshotJs {
    fn from(s: DeckSnapshot) -> Self {
        Self {
            position_frames: s.position_frames as f64,
            total_frames: s.total_frames as f64,
            generation: s.generation,
            sample_rate: s.sample_rate,
            playing: s.playing,
            scrubbing: s.scrubbing,
            loaded: s.loaded,
            load_id: s.load_id as f64,
            loop_in_frames: s.loop_in_frames as f64,
            loop_out_frames: s.loop_out_frames as f64,
            looping: s.looping,
            tempo: s.tempo,
            master_tempo: s.master_tempo,
        }
    }
}

fn event_js(event: DeckEvent) -> DeckEventJs {
    match event {
        DeckEvent::Loaded {
            deck,
            load_id,
            total_frames,
            sample_rate,
        } => DeckEventJs {
            kind: "loaded".into(),
            deck: deck as u32,
            load_id: load_id as f64,
            total_frames: total_frames as f64,
            sample_rate,
            message: None,
        },
        DeckEvent::Error {
            deck,
            load_id,
            message,
        } => DeckEventJs {
            kind: "error".into(),
            deck: deck as u32,
            load_id: load_id as f64,
            total_frames: 0.0,
            sample_rate: 0,
            message: Some(message),
        },
    }
}

fn deck_index(deck: u32) -> usize {
    deck as usize
}

#[napi]
pub struct AudioEngine {
    engine: Engine,
}

#[napi]
impl AudioEngine {
    /// Opens the default output device (paused) and starts `decks` decode threads.
    #[napi(constructor)]
    pub fn new(
        env: Env,
        decks: u32,
        #[napi(ts_arg_type = "(event: DeckEventJs) => void")] on_event: JsFunction,
    ) -> Result<Self> {
        let mut tsfn: ThreadsafeFunction<DeckEventJs, ErrorStrategy::Fatal> = on_event
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<DeckEventJs>| {
                Ok(vec![ctx.value])
            })?;
        // Events must not keep the process alive on their own.
        tsfn.unref(&env)?;
        let events: EventSink = Arc::new(move |event| {
            tsfn.call(event_js(event), ThreadsafeFunctionCallMode::NonBlocking);
        });
        let engine = Engine::new(decks.max(1) as usize, &events)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
        Ok(Self { engine })
    }

    #[napi(getter)]
    pub fn sample_rate(&self) -> u32 {
        self.engine.sample_rate()
    }

    #[napi]
    pub fn load(&self, deck: u32, path: String, request: f64) {
        self.engine
            .load(deck_index(deck), Path::new(&path), request.max(1.0) as u64);
    }

    #[napi]
    pub fn unload(&self, deck: u32) {
        self.engine.unload(deck_index(deck));
    }

    #[napi]
    pub fn play(&self, deck: u32) {
        self.engine.play(deck_index(deck));
    }

    #[napi]
    pub fn pause(&self, deck: u32) {
        self.engine.pause(deck_index(deck));
    }

    #[napi]
    pub fn seek(&self, deck: u32, ms: f64) {
        self.engine.seek(deck_index(deck), ms);
    }

    #[napi]
    pub fn stop_at(&self, deck: u32, ms: f64) {
        self.engine.stop_at(deck_index(deck), ms);
    }

    #[napi]
    pub fn scrub_begin(&self, deck: u32) {
        self.engine.scrub_begin(deck_index(deck));
    }

    /// `at` is the pointer event's timestamp in ms, from which the scrub's speed is measured.
    #[napi]
    pub fn scrub_to(&self, deck: u32, ms: f64, at: f64) {
        self.engine.scrub_to(deck_index(deck), ms, at);
    }

    #[napi]
    pub fn scrub_end(&self, deck: u32) {
        self.engine.scrub_end(deck_index(deck));
    }

    #[napi]
    pub fn set_loop(&self, deck: u32, in_ms: f64, out_ms: f64, enter: bool) {
        self.engine.set_loop(deck_index(deck), in_ms, out_ms, enter);
    }

    #[napi]
    pub fn set_looping(&self, deck: u32, on: bool) {
        self.engine.set_looping(deck_index(deck), on);
    }

    #[napi]
    pub fn clear_loop(&self, deck: u32) {
        self.engine.clear_loop(deck_index(deck));
    }

    /// Playback speed as a multiple of the file's.
    #[napi]
    pub fn set_tempo(&self, deck: u32, tempo: f64) {
        self.engine.set_tempo(deck_index(deck), tempo);
    }

    #[napi]
    pub fn set_master_tempo(&self, deck: u32, on: bool) {
        self.engine.set_master_tempo(deck_index(deck), on);
    }

    #[napi]
    pub fn set_volume(&self, gain: f64) {
        self.engine.set_volume(gain as f32);
    }

    #[napi]
    pub fn snapshot(&self, deck: u32) -> Option<DeckSnapshotJs> {
        self.engine.snapshot(deck_index(deck)).map(Into::into)
    }

    #[napi]
    pub fn any_sounding(&self) -> bool {
        self.engine.any_sounding()
    }

    /// Lets the device go idle once nothing is sounding.
    #[napi]
    pub fn settle(&self) {
        self.engine.settle();
    }
}

#[napi(object)]
pub struct WaveJs {
    /// Envelope entries per second.
    pub rate: f64,
    pub data: Int16Array,
    pub peak: i32,
    pub bands: Option<Uint8Array>,
    pub band_rate: Option<f64>,
}

pub struct Analyse {
    path: String,
    bands: bool,
}

impl Task for Analyse {
    type Output = Option<es_wave::Wave>;
    type JsValue = Option<WaveJs>;

    fn compute(&mut self) -> Result<Self::Output> {
        es_wave::analyse(Path::new(&self.path), self.bands)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.map(|wave| WaveJs {
            rate: wave.rate,
            data: Int16Array::new(wave.data),
            peak: wave.peak,
            band_rate: wave.bands.is_some().then_some(wave.band_rate),
            bands: wave.bands.map(Uint8Array::new),
        }))
    }
}

/// Decodes and analyses a file off the main thread. Null when it is too short to draw.
#[napi(ts_return_type = "Promise<WaveJs | null>")]
pub fn analyse(path: String, bands: bool) -> AsyncTask<Analyse> {
    AsyncTask::new(Analyse { path, bands })
}

#[napi(object)]
pub struct TrackAnalysisOptionsJs {
    pub grid: bool,
    pub key: bool,
    pub dynamic: bool,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub high_precision: bool,
    pub round_bpm: bool,
}

#[napi(object)]
pub struct KeyJs {
    /// 0 is C.
    pub tonic: u32,
    pub minor: bool,
}

#[napi(object)]
pub struct TrackAnalysisJs {
    pub duration: f64,
    /// 0 when no grid was found or asked for.
    pub bpm: f64,
    /// Per beat: its number in the bar (1 to 4), tempo × 100, and time in ms.
    pub beat_numbers: Uint8Array,
    pub beat_tempos: Uint16Array,
    pub beat_times: Uint32Array,
    pub key: Option<KeyJs>,
    /// Eight bytes per 1/150 s: peak and RMS of the whole signal, then the low, mid and high bands.
    pub waveform: Uint8Array,
}

pub struct AnalyseTrack {
    path: String,
    options: es_analyse::Options,
}

impl Task for AnalyseTrack {
    type Output = es_analyse::Analysis;
    type JsValue = TrackAnalysisJs;

    fn compute(&mut self) -> Result<Self::Output> {
        es_analyse::analyse_file(Path::new(&self.path), &self.options)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
    }

    fn resolve(&mut self, _env: Env, a: Self::Output) -> Result<Self::JsValue> {
        Ok(TrackAnalysisJs {
            duration: a.duration,
            bpm: a.bpm,
            beat_numbers: Uint8Array::new(a.beats.iter().map(|b| b.number).collect()),
            beat_tempos: Uint16Array::new(a.beats.iter().map(|b| b.tempo_x100).collect()),
            beat_times: Uint32Array::new(a.beats.iter().map(|b| b.time_ms).collect()),
            key: a.key.map(|k| KeyJs {
                tonic: k.tonic as u32,
                minor: k.minor,
            }),
            waveform: Uint8Array::new(a.waveform),
        })
    }
}

/// Beatgrid, tempo, key and waveform loudness for a file, worked out off the main thread.
#[napi(ts_return_type = "Promise<TrackAnalysisJs>")]
pub fn analyse_track(path: String, options: TrackAnalysisOptionsJs) -> AsyncTask<AnalyseTrack> {
    AsyncTask::new(AnalyseTrack {
        path,
        options: es_analyse::Options {
            grid: options.grid,
            key: options.key,
            tempo: es_analyse::TempoOptions {
                min_bpm: options.min_bpm,
                max_bpm: options.max_bpm.max(options.min_bpm + 1.0),
                dynamic: options.dynamic,
                high_precision: options.high_precision,
                round_bpm: options.round_bpm,
            },
        },
    })
}
