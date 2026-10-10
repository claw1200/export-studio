//! Where the mixed audio goes: the system output device, or a test harness.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::{EngineError, Result};

/// The render callback: fills interleaved stereo at the sink's rate. Realtime.
pub type Render = Box<dyn FnMut(&mut [f32]) + Send>;

pub trait Sink: Send + Sync {
    fn sample_rate(&self) -> u32;
    /// Asks for the device to run. Cheap when it already is.
    fn start(&self);
    /// Lets the device go idle. The sink keeps running briefly first so a fade-out is heard.
    fn stop(&self);
}

/// Frames rendered per pass when converting to the device's layout.
const CHUNK_FRAMES: usize = 1024;

/// How long the device keeps running after the last sound, so stop fades reach the speakers.
const IDLE_GRACE: Duration = Duration::from_millis(150);

enum SinkCommand {
    Start,
    Stop,
    Quit,
}

/// The system's default output, driven from a thread that owns the stream (cpal streams cannot
/// move between threads).
pub struct CpalSink {
    sample_rate: u32,
    commands: Mutex<Sender<SinkCommand>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl CpalSink {
    pub fn open(render: Render) -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32>>();
        let (tx, rx) = mpsc::channel::<SinkCommand>();
        let thread = thread::Builder::new()
            .name("es-audio-device".into())
            .spawn(move || {
                let stream = match build_stream(render) {
                    Ok((stream, rate)) => {
                        let _ = ready_tx.send(Ok(rate));
                        stream
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let mut running = false;
                let mut stop_pending = false;
                loop {
                    let command = if stop_pending {
                        match rx.recv_timeout(IDLE_GRACE) {
                            Ok(c) => Some(c),
                            Err(RecvTimeoutError::Timeout) => None,
                            Err(RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match rx.recv() {
                            Ok(c) => Some(c),
                            Err(_) => break,
                        }
                    };
                    match command {
                        Some(SinkCommand::Start) => {
                            stop_pending = false;
                            if !running {
                                running = stream.play().is_ok();
                            }
                        }
                        Some(SinkCommand::Stop) => stop_pending = running,
                        Some(SinkCommand::Quit) => break,
                        None => {
                            stop_pending = false;
                            if running {
                                let _ = stream.pause();
                                running = false;
                            }
                        }
                    }
                }
            })?;
        let sample_rate = ready_rx
            .recv()
            .map_err(|_| EngineError::Device("the audio thread did not start".into()))??;
        Ok(Self {
            sample_rate,
            commands: Mutex::new(tx),
            thread: Some(thread),
        })
    }

    fn send(&self, command: SinkCommand) {
        let tx = self.commands.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = tx.send(command);
    }
}

impl Drop for CpalSink {
    fn drop(&mut self) {
        self.send(SinkCommand::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Sink for CpalSink {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn start(&self) {
        self.send(SinkCommand::Start);
    }

    fn stop(&self) {
        self.send(SinkCommand::Stop);
    }
}

fn build_stream(render: Render) -> Result<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or(EngineError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0;
    let stream = match format {
        SampleFormat::F32 => output::<f32>(&device, &config, render),
        SampleFormat::I16 => output::<i16>(&device, &config, render),
        SampleFormat::U16 => output::<u16>(&device, &config, render),
        SampleFormat::I32 => output::<i32>(&device, &config, render),
        other => Err(EngineError::Device(format!(
            "unsupported sample format {other}"
        ))),
    }?;
    // Built paused: nothing plays until a deck asks for it.
    let _ = stream.pause();
    Ok((stream, rate))
}

fn output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut render: Render,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let mut stereo = vec![0.0_f32; CHUNK_FRAMES * 2];
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                for chunk in data.chunks_mut(CHUNK_FRAMES * channels) {
                    let frames = chunk.len() / channels;
                    let buffer = &mut stereo[..frames * 2];
                    buffer.fill(0.0);
                    render(buffer);
                    for (frame, pair) in
                        chunk.chunks_exact_mut(channels).zip(buffer.chunks_exact(2))
                    {
                        if channels == 1 {
                            frame[0] = T::from_sample((pair[0] + pair[1]) * 0.5);
                            continue;
                        }
                        frame[0] = T::from_sample(pair[0]);
                        frame[1] = T::from_sample(pair[1]);
                        for rest in &mut frame[2..] {
                            *rest = T::from_sample(0.0);
                        }
                    }
                }
            },
            |e| eprintln!("[es-deck] audio device error: {e}"),
            None,
        )
        .map_err(|e| EngineError::Device(e.to_string()))
}

/// A sink with no device: tests pull audio from it by hand.
pub struct NullSink {
    sample_rate: u32,
    render: Mutex<Render>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl NullSink {
    pub fn new(render: Render, sample_rate: u32) -> Self {
        Self {
            sample_rate,
            render: Mutex::new(render),
            running: Arc::default(),
        }
    }

    /// Renders `frames` frames of interleaved stereo, as the device callback would.
    pub fn pull(&self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        self.pull_into(&mut out);
        out
    }

    pub fn pull_into(&self, out: &mut [f32]) {
        out.fill(0.0);
        let mut render = self.render.lock().unwrap_or_else(PoisonError::into_inner);
        (render)(out);
    }

    pub fn running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Sink for NullSink {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn start(&self) {
        self.running
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
