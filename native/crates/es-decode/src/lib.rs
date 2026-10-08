//! Streaming decode to interleaved stereo `f32`, resampled to a fixed output rate.
//!
//! One decoder serves playback and waveform analysis, so the audio you hear and the
//! waveform you see are always on the same timeline.

use std::fs::File;
use std::path::Path;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_MP3, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("could not open the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported or unreadable audio: {0}")]
    Format(String),
}

impl From<SymphoniaError> for DecodeError {
    fn from(e: SymphoniaError) -> Self {
        match e {
            SymphoniaError::IoError(io) => DecodeError::Io(io),
            other => DecodeError::Format(other.to_string()),
        }
    }
}

mod mp3;

pub type Result<T> = std::result::Result<T, DecodeError>;

/// Frames the resampler consumes per pass.
const RESAMPLE_CHUNK: usize = 1024;

/// A file being read from start to end, or from wherever it was last sought to.
pub struct Streamer {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    source_rate: u32,
    output_rate: u32,
    /// Decoded, not yet resampled: interleaved stereo at the source rate.
    source: Vec<f32>,
    /// Source frames at the front of `source` still to be dropped, after a seek landed early.
    skip: u64,
    /// Source frames of silence before the file's audio, to start where rekordbox does.
    lead: u64,
    /// Lead-in frames still to be played.
    lead_left: u64,
    resampler: Option<Box<StereoResampler>>,
    /// Ready to hand out: interleaved stereo at the output rate, read from `ready_at`.
    ready: Vec<f32>,
    ready_at: usize,
    /// Output-rate frame of the next frame `fill` returns.
    position: u64,
    total: u64,
    demuxed_all: bool,
    finished: bool,
    sample_buffer: Option<SampleBuffer<f32>>,
}

impl Streamer {
    /// Opens `path` for streaming at `output_rate`, or at the file's own rate (no resampling)
    /// when `output_rate` is 0.
    pub fn open(path: &Path, output_rate: u32) -> Result<Self> {
        let file = File::open(path)?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe().format(
            &hint,
            stream,
            &FormatOptions {
                enable_gapless: false,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| DecodeError::Format("no audio track".into()))?;
        let params = track.codec_params.clone();
        let source_rate = params
            .sample_rate
            .filter(|r| *r > 0)
            .ok_or_else(|| DecodeError::Format("unknown sample rate".into()))?;
        let decoder = symphonia::default::get_codecs().make(&params, &DecoderOptions::default())?;
        let output_rate = if output_rate == 0 {
            source_rate
        } else {
            output_rate
        };
        let lead = if params.codec == CODEC_TYPE_MP3 {
            mp3::lead_in(path)
        } else {
            0
        };
        let total = params
            .n_frames
            .map(|n| scale(n + lead, source_rate, output_rate))
            .unwrap_or(0);
        let resampler = if source_rate == output_rate {
            None
        } else {
            Some(Box::new(StereoResampler::new(source_rate, output_rate)?))
        };
        Ok(Self {
            track_id: track.id,
            time_base: params.time_base,
            format,
            decoder,
            source_rate,
            output_rate,
            source: Vec::new(),
            skip: 0,
            lead,
            lead_left: lead,
            resampler,
            ready: Vec::new(),
            ready_at: 0,
            position: 0,
            total,
            demuxed_all: false,
            finished: false,
            sample_buffer: None,
        })
    }

    pub fn source_rate(&self) -> u32 {
        self.source_rate
    }

    pub fn output_rate(&self) -> u32 {
        self.output_rate
    }

    /// The track's length in output-rate frames, or 0 when the container does not say.
    pub fn total_frames(&self) -> u64 {
        self.total
    }

    /// Output-rate frame of the next frame [`fill`](Self::fill) returns.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Everything has been handed out.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Fills `out` (interleaved stereo) and returns the frames written. Zero means the end.
    pub fn fill(&mut self, out: &mut [f32]) -> Result<usize> {
        let wanted = out.len() / 2;
        let mut written = 0;
        while written < wanted {
            let available = (self.ready.len() - self.ready_at) / 2;
            if available == 0 {
                if !self.refill()? {
                    self.finished = true;
                    break;
                }
                continue;
            }
            let take = available.min(wanted - written);
            let from = self.ready_at;
            out[written * 2..(written + take) * 2]
                .copy_from_slice(&self.ready[from..from + take * 2]);
            self.ready_at += take * 2;
            written += take;
        }
        self.position += written as u64;
        if self.ready_at == self.ready.len() {
            self.ready.clear();
            self.ready_at = 0;
        }
        Ok(written)
    }

    /// Moves to `frame` (output rate) exactly, and returns where it landed.
    pub fn seek(&mut self, frame: u64) -> Result<u64> {
        let frame = if self.total > 0 {
            frame.min(self.total)
        } else {
            frame
        };
        let source_frame = scale(frame, self.output_rate, self.source_rate);
        self.source.clear();
        self.ready.clear();
        self.ready_at = 0;
        self.demuxed_all = false;
        self.finished = false;
        self.skip = 0;
        if let Some(resampler) = self.resampler.as_mut() {
            resampler.reset();
        }
        self.lead_left = self.lead.saturating_sub(source_frame);
        let source_frame = source_frame.saturating_sub(self.lead);
        let seconds = source_frame as f64 / f64::from(self.source_rate);
        let landed = self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::from(seconds),
                track_id: Some(self.track_id),
            },
        );
        match landed {
            Ok(seeked) => {
                self.decoder.reset();
                let actual = self.ts_to_frames(seeked.actual_ts);
                self.skip = source_frame.saturating_sub(actual);
            }
            Err(SymphoniaError::SeekError(_)) if seconds <= 0.0 => {
                self.decoder.reset();
            }
            Err(SymphoniaError::SeekError(_)) => {
                // Past the end: the file has nothing more to give.
                self.demuxed_all = true;
                self.finished = true;
            }
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.demuxed_all = true;
                self.finished = true;
            }
            Err(e) => return Err(e.into()),
        }
        self.position = frame;
        Ok(frame)
    }

    fn ts_to_frames(&self, ts: u64) -> u64 {
        match self.time_base {
            Some(tb) if tb.denom > 0 => {
                (u128::from(ts) * u128::from(tb.numer) * u128::from(self.source_rate)
                    / u128::from(tb.denom)) as u64
            }
            _ => ts,
        }
    }

    /// Produces more output-rate audio. False at the end of the file.
    fn refill(&mut self) -> Result<bool> {
        loop {
            match self.resampler.as_mut() {
                None => {
                    if !self.source.is_empty() {
                        std::mem::swap(&mut self.ready, &mut self.source);
                        self.source.clear();
                        self.ready_at = 0;
                        return Ok(true);
                    }
                    if self.demuxed_all {
                        return Ok(false);
                    }
                }
                Some(resampler) => {
                    if resampler.process(&mut self.source, &mut self.ready, self.demuxed_all) {
                        self.ready_at = 0;
                        if !self.ready.is_empty() {
                            return Ok(true);
                        }
                        continue;
                    }
                    if self.demuxed_all {
                        return Ok(false);
                    }
                }
            }
            self.decode_packet()?;
        }
    }

    /// Decodes one packet into `source`. Marks the end of the file when there are none left.
    fn decode_packet(&mut self) -> Result<()> {
        if self.lead_left > 0 {
            let silence = usize::try_from(self.lead_left).unwrap_or(usize::MAX) * 2;
            self.source.resize(self.source.len() + silence, 0.0);
            self.lead_left = 0;
            return Ok(());
        }
        let packet = match self.format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.demuxed_all = true;
                return Ok(());
            }
            Err(SymphoniaError::ResetRequired) => {
                self.demuxed_all = true;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != self.track_id {
            return Ok(());
        }
        let decoded = match self.decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A damaged packet is skipped, as every player does.
            Err(SymphoniaError::DecodeError(_)) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let frames = decoded.frames();
        if frames == 0 {
            return Ok(());
        }
        let needed = frames * spec.channels.count();
        let buffer = match self.sample_buffer.as_mut() {
            Some(buffer) if buffer.capacity() >= needed => buffer,
            _ => {
                self.sample_buffer = Some(SampleBuffer::new(frames as u64, spec));
                self.sample_buffer.as_mut().expect("just set")
            }
        };
        buffer.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        let samples = buffer.samples();
        let skip = (self.skip as usize).min(frames);
        self.skip -= skip as u64;
        self.source.reserve((frames - skip) * 2);
        for frame in samples.chunks_exact(channels).skip(skip) {
            let left = frame[0];
            let right = if channels > 1 { frame[1] } else { left };
            self.source.push(left);
            self.source.push(right);
        }
        Ok(())
    }
}

/// The sample rate a file is stored at, without decoding it.
pub fn probe_rate(path: &Path) -> Result<u32> {
    Streamer::open(path, 0).map(|s| s.source_rate())
}

fn scale(frames: u64, from: u32, to: u32) -> u64 {
    if from == to || from == 0 {
        return frames;
    }
    ((u128::from(frames) * u128::from(to) + u128::from(from) / 2) / u128::from(from)) as u64
}

/// Sinc resampling of interleaved stereo. Rubato starts its read position half a filter back,
/// so output frame 0 already lines up with input frame 0.
struct StereoResampler {
    inner: SincFixedIn<f32>,
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
    ratio: f64,
    /// Input frames fed since the reset, and output frames handed out, for trimming the tail.
    fed: u64,
    produced: u64,
}

impl StereoResampler {
    fn new(from: u32, to: u32) -> Result<Self> {
        let ratio = f64::from(to) / f64::from(from);
        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            oversampling_factor: 128,
            interpolation: SincInterpolationType::Cubic,
            window: WindowFunction::BlackmanHarris2,
        };
        let inner = SincFixedIn::<f32>::new(ratio, 1.0, params, RESAMPLE_CHUNK, 2)
            .map_err(|e| DecodeError::Format(e.to_string()))?;
        let max_out = inner.output_frames_max();
        Ok(Self {
            inner,
            input: [vec![0.0; RESAMPLE_CHUNK], vec![0.0; RESAMPLE_CHUNK]],
            output: [vec![0.0; max_out], vec![0.0; max_out]],
            ratio,
            fed: 0,
            produced: 0,
        })
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.fed = 0;
        self.produced = 0;
    }

    /// Resamples one chunk from the front of `source` into `ready`. At the end of the input
    /// (`draining`), pads with silence until everything owed has come out. False when there is
    /// not enough input for a chunk, or nothing left to drain.
    fn process(&mut self, source: &mut Vec<f32>, ready: &mut Vec<f32>, draining: bool) -> bool {
        let need = self.inner.input_frames_next();
        let have = source.len() / 2;
        let owed = (self.fed as f64 * self.ratio).round() as u64;
        if have < need && !(draining && (have > 0 || self.produced < owed)) {
            return false;
        }
        let take = have.min(need);
        for (i, frame) in source[..take * 2].chunks_exact(2).enumerate() {
            self.input[0][i] = frame[0];
            self.input[1][i] = frame[1];
        }
        for channel in &mut self.input {
            channel[take..need].fill(0.0);
        }
        source.drain(..take * 2);
        self.fed += take as u64;
        let Ok((_, out_frames)) = self.inner.process_into_buffer(
            &[&self.input[0][..need], &self.input[1][..need]],
            &mut self.output,
            None,
        ) else {
            return false;
        };
        ready.clear();
        let owed = (self.fed as f64 * self.ratio).round() as u64;
        let mut count = out_frames;
        if draining && take < need {
            count = count.min(owed.saturating_sub(self.produced) as usize);
        }
        ready.reserve(count * 2);
        for i in 0..count {
            ready.push(self.output[0][i]);
            ready.push(self.output[1][i]);
        }
        self.produced += count as u64;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A 16-bit stereo WAV of `frames` frames whose left channel counts up, so any frame can be
    /// recognised by its value.
    pub(crate) fn ramp_wav(path: &Path, rate: u32, frames: u32) {
        let mut file = File::create(path).unwrap();
        let data_len = frames * 4;
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&rate.to_le_bytes()).unwrap();
        file.write_all(&(rate * 4).to_le_bytes()).unwrap();
        file.write_all(&4u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_len.to_le_bytes()).unwrap();
        for i in 0..frames {
            let v = (i % 30_000) as i16;
            file.write_all(&v.to_le_bytes()).unwrap();
            file.write_all(&(-v).to_le_bytes()).unwrap();
        }
    }

    fn value(sample: f32) -> u32 {
        (sample * 32768.0).round() as u32
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("es-decode-{}-{name}.wav", std::process::id()))
    }

    #[test]
    fn reads_every_frame_in_order() {
        let path = temp("order");
        ramp_wav(&path, 44_100, 20_000);
        let mut s = Streamer::open(&path, 44_100).unwrap();
        assert_eq!(s.total_frames(), 20_000);
        let mut out = vec![0.0; 2 * 1000];
        let mut seen = 0u32;
        loop {
            let n = s.fill(&mut out).unwrap();
            if n == 0 {
                break;
            }
            for f in 0..n {
                assert_eq!(value(out[f * 2]), seen % 30_000);
                seen += 1;
            }
        }
        assert_eq!(seen, 20_000);
        assert!(s.finished());
    }

    #[test]
    fn a_seek_lands_on_the_exact_frame() {
        let path = temp("seek");
        ramp_wav(&path, 44_100, 100_000);
        let mut s = Streamer::open(&path, 44_100).unwrap();
        for target in [12_345u64, 99_000, 0, 54_321] {
            assert_eq!(s.seek(target).unwrap(), target);
            let mut out = vec![0.0; 2 * 4];
            assert_eq!(s.fill(&mut out).unwrap(), 4);
            assert_eq!(value(out[0]), (target % 30_000) as u32, "seek to {target}");
            assert_eq!(s.position(), target + 4);
        }
    }

    #[test]
    fn a_seek_to_the_end_or_past_it_is_the_end_not_an_error() {
        let path = temp("seekend");
        ramp_wav(&path, 44_100, 10_000);
        let mut s = Streamer::open(&path, 44_100).unwrap();
        for target in [10_000u64, 50_000] {
            assert_eq!(s.seek(target).unwrap(), 10_000);
            let mut out = vec![0.0; 2 * 4];
            assert_eq!(s.fill(&mut out).unwrap(), 0);
            assert!(s.finished());
        }
        assert_eq!(s.seek(0).unwrap(), 0, "and can come back");
    }

    #[test]
    fn resampling_keeps_length_and_alignment() {
        let path = temp("resample");
        ramp_wav(&path, 44_100, 44_100);
        let mut s = Streamer::open(&path, 48_000).unwrap();
        assert_eq!(s.total_frames(), 48_000);
        let mut out = vec![0.0; 2 * 4096];
        let mut all = Vec::new();
        loop {
            let n = s.fill(&mut out).unwrap();
            if n == 0 {
                break;
            }
            all.extend_from_slice(&out[..n * 2]);
        }
        let frames = all.len() / 2;
        assert!((frames as i64 - 48_000).abs() <= 2, "got {frames}");
        // The ramp rises one step per input frame; at output frame k it should read k * 44.1/48.
        let k = 24_000usize;
        let expected = k as f64 * 44_100.0 / 48_000.0;
        let got = f64::from(all[k * 2]) * 32768.0;
        assert!(
            (got - expected).abs() < 2.0,
            "frame {k}: {got} vs {expected}"
        );
    }
}
