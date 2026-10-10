//! The high-detail waveform: a peak envelope of the decoded audio, plus low, mid and high band
//! peaks for the 3-band view. The drawing stays in the renderer; this is only the analysis.
//!
//! The numbers follow the renderer's original TypeScript scan step for step, rounding included,
//! so waveforms look the same as before. The golden tests hold it to that.

use std::f64::consts::{FRAC_1_SQRT_2, PI};
use std::path::Path;

use es_decode::{DecodeError, Streamer};
use rayon::prelude::*;

pub const ENVELOPE_RATE: f64 = 16_000.0;
pub const MAX_ENVELOPE_SAMPLES: f64 = 6_000_000.0;
pub const BAND_RATE: f64 = 8_000.0;
pub const MAX_BAND_FRAMES: f64 = 4_000_000.0;
const LOW_HZ: f64 = 200.0;
const HIGH_HZ: f64 = 2_000.0;
/// Seconds of filter history a part replays, so its band peaks match a single full pass.
const BAND_WARMUP_SEC: f64 = 0.08;

/// JavaScript's `Math.round`: halves go up, so -2.5 rounds to -2.
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WaveRates {
    pub band_step: f64,
    pub band_frames: usize,
    pub env_step: f64,
    pub env_frames: usize,
}

/// How many input samples each band and envelope entry covers, for a mono signal of `length`.
pub fn wave_rates(length: usize, sample_rate: f64) -> WaveRates {
    scaled_rates(length, sample_rate, 100)
}

/// `quality` is 1–100. 100 is the full scan; lower values thin the entries so a preview is cheap.
fn scaled_rates(length: usize, sample_rate: f64, quality: u32) -> WaveRates {
    let scale = quality.clamp(1, 100) as f64 / 100.0;
    let duration = length as f64 / sample_rate;
    let band_rate = (BAND_RATE * scale)
        .min(sample_rate)
        .min(MAX_BAND_FRAMES / duration);
    let env_rate = (ENVELOPE_RATE * scale).min(MAX_ENVELOPE_SAMPLES / duration);
    let band_step = sample_rate / band_rate;
    let env_step = sample_rate / env_rate;
    WaveRates {
        band_step,
        band_frames: (length as f64 / band_step).floor() as usize,
        env_step,
        env_frames: (length as f64 / env_step).floor() as usize,
    }
}

/// Entries per second of audio, as the renderer maps them back to time.
pub fn reported_rate(frames: usize, length: usize, sample_rate: f64) -> f64 {
    if length > 0 {
        frames as f64 * sample_rate / length as f64
    } else {
        0.0
    }
}

/// One part of the scan: the samples it reads (`origin..to`) and the entries it writes.
#[derive(Debug, Clone, Copy)]
pub struct ScanSpec {
    pub origin: usize,
    pub to: usize,
    pub mono_length: usize,
    pub sample_rate: f64,
    pub band_step: f64,
    pub band_k0: usize,
    pub band_frames: usize,
    pub band_filter_from: usize,
    pub env_step: f64,
    pub env_k0: usize,
    pub env_frames: usize,
}

/// Splits the scan into up to `parts` independent parts, each starting its band filters early
/// enough to settle before its first entry.
pub fn plan_wave(
    length: usize,
    sample_rate: f64,
    parts: usize,
    want_bands: bool,
    want_env: bool,
) -> Vec<ScanSpec> {
    plan_with(
        length,
        sample_rate,
        parts,
        want_bands,
        want_env,
        wave_rates(length, sample_rate),
    )
}

fn plan_with(
    length: usize,
    sample_rate: f64,
    parts: usize,
    want_bands: bool,
    want_env: bool,
    rates: WaveRates,
) -> Vec<ScanSpec> {
    let band_frames = if want_bands { rates.band_frames } else { 0 };
    let env_frames = if want_env { rates.env_frames } else { 0 };
    let slices = parts.min(band_frames.max(env_frames).max(1)).max(1);
    let span = (length as f64 / slices as f64).ceil() as usize;
    let warmup = (sample_rate * BAND_WARMUP_SEC).ceil() as usize;
    let mut specs = Vec::with_capacity(slices);
    for part in 0..slices {
        let sample0 = part * span;
        let sample1 = length.min(sample0 + span);
        let band = frames_in(rates.band_step, band_frames, sample0, sample1);
        let env = frames_in(rates.env_step, env_frames, sample0, sample1);
        if band.1 == 0 && env.1 == 0 {
            continue;
        }
        let band_start = if band.1 > 0 {
            (band.0 as f64 * rates.band_step).floor() as usize
        } else {
            sample0
        };
        let band_filter_from = if band.1 > 0 {
            band_start.saturating_sub(if band.0 == 0 { 0 } else { warmup })
        } else {
            band_start
        };
        let band_end = if band.1 > 0 {
            frame_end(band.0 + band.1 - 1, rates.band_step, length)
        } else {
            sample0
        };
        let env_start = if env.1 > 0 {
            (env.0 as f64 * rates.env_step).floor() as usize
        } else {
            band_filter_from
        };
        let env_end = if env.1 > 0 {
            frame_end(env.0 + env.1 - 1, rates.env_step, length)
        } else {
            band_end
        };
        specs.push(ScanSpec {
            origin: band_filter_from.min(env_start),
            to: band_end.max(env_end),
            mono_length: length,
            sample_rate,
            band_step: rates.band_step,
            band_k0: band.0,
            band_frames: band.1,
            band_filter_from,
            env_step: rates.env_step,
            env_k0: env.0,
            env_frames: env.1,
        });
    }
    specs
}

/// The first entry starting at or after `sample0`, and how many start before `sample1`.
fn frames_in(step: f64, frame_count: usize, sample0: usize, sample1: usize) -> (usize, usize) {
    if frame_count == 0 || sample1 <= sample0 {
        return (0, 0);
    }
    let mut k0 = frame_count.min((sample0 as f64 / step).ceil().max(0.0) as usize);
    while k0 < frame_count && ((k0 as f64 * step).floor() as usize) < sample0 {
        k0 += 1;
    }
    let mut k1 = k0;
    while k1 < frame_count && ((k1 as f64 * step).floor() as usize) < sample1 {
        k1 += 1;
    }
    (k0, k1 - k0)
}

fn frame_end(frame: usize, step: f64, length: usize) -> usize {
    let start = (frame as f64 * step).floor() as usize;
    length.min((start + 1).max(((frame + 1) as f64 * step).floor() as usize))
}

#[derive(Clone, Copy)]
enum Pass {
    Low,
    High,
}

/// A second-order Butterworth section (Q = 1/√2), direct form I.
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(pass: Pass, sample_rate: f64, freq: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos = w0.cos();
        let alpha = w0.sin() / (2.0 * FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let (b0, b1) = match pass {
            Pass::Low => ((1.0 - cos) / 2.0, 1.0 - cos),
            Pass::High => ((1.0 + cos) / 2.0, -(1.0 + cos)),
        };
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

fn quant_band(peak: f64) -> u8 {
    if peak <= 0.0 {
        0
    } else {
        js_round(peak * 255.0).min(255.0) as u8
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanOut {
    /// Low, mid, high per entry.
    pub bands: Vec<u8>,
    pub band_max: u8,
    pub env: Vec<i16>,
    pub env_peak: i32,
}

/// Scans one part. `samples` are the mono samples from `spec.origin` to `spec.to`.
pub fn scan_part(spec: &ScanSpec, samples: &[f32]) -> ScanOut {
    let at = |index: usize| f64::from(samples[index - spec.origin]);
    let mut out = ScanOut {
        bands: vec![0; spec.band_frames * 3],
        ..ScanOut::default()
    };
    if spec.band_frames > 0 {
        let mut low = Biquad::new(Pass::Low, spec.sample_rate, LOW_HZ);
        let mut mid_high = Biquad::new(Pass::High, spec.sample_rate, LOW_HZ);
        let mut mid_low = Biquad::new(Pass::Low, spec.sample_rate, HIGH_HZ);
        let mut high = Biquad::new(Pass::High, spec.sample_rate, HIGH_HZ);
        let mut index = spec.band_filter_from;
        let record_from = (spec.band_k0 as f64 * spec.band_step).floor() as usize;
        while index < record_from {
            let v = at(index);
            low.run(v);
            let mid = mid_high.run(v);
            mid_low.run(mid);
            high.run(v);
            index += 1;
        }
        for k in 0..spec.band_frames {
            let end = frame_end(spec.band_k0 + k, spec.band_step, spec.mono_length);
            let (mut low_peak, mut mid_peak, mut high_peak) = (0.0f64, 0.0f64, 0.0f64);
            while index < end {
                let v = at(index);
                low_peak = low_peak.max(low.run(v).abs());
                mid_peak = mid_peak.max(mid_low.run(mid_high.run(v)).abs());
                high_peak = high_peak.max(high.run(v).abs());
                index += 1;
            }
            let quantised = [
                quant_band(low_peak),
                quant_band(mid_peak),
                quant_band(high_peak),
            ];
            out.bands[k * 3..k * 3 + 3].copy_from_slice(&quantised);
            out.band_max = out
                .band_max
                .max(quantised[0])
                .max(quantised[1])
                .max(quantised[2]);
        }
    }

    out.env = Vec::with_capacity(spec.env_frames);
    for k in 0..spec.env_frames {
        let frame = spec.env_k0 + k;
        let start = (frame as f64 * spec.env_step).floor() as usize;
        let end = frame_end(frame, spec.env_step, spec.mono_length);
        // The sample furthest from zero, keeping its sign; the first wins a tie.
        let mut s = 0.0f64;
        for j in start..end {
            let v = at(j);
            if v.abs() > s.abs() {
                s = v;
            }
        }
        let q = js_round(s * 32767.0).clamp(-32767.0, 32767.0) as i16;
        out.env.push(q);
        out.env_peak = out.env_peak.max(i32::from(q).abs());
    }
    out
}

/// Stretches band peaks so the loudest is 255, unless they are already full or nearly silent.
pub fn normalize_bands(data: &mut [u8], max: u8) {
    if !(max > 8 && max < 255) {
        return;
    }
    let scale = 255.0 / f64::from(max);
    for v in data {
        *v = js_round(f64::from(*v) * scale).min(255.0) as u8;
    }
}

/// The finished analysis, in the shape the renderer draws from.
#[derive(Debug, Clone)]
pub struct Wave {
    /// Envelope entries per second.
    pub rate: f64,
    pub data: Vec<i16>,
    pub peak: i32,
    pub bands: Option<Vec<u8>>,
    pub band_rate: f64,
}

/// Analyses mono samples in up to `parts` parts at once. None when the audio is too short to draw.
pub fn build(mono: &[f32], sample_rate: f64, parts: usize, want_bands: bool) -> Option<Wave> {
    build_scaled(mono, sample_rate, parts, want_bands, 100)
}

fn build_scaled(
    mono: &[f32],
    sample_rate: f64,
    parts: usize,
    want_bands: bool,
    quality: u32,
) -> Option<Wave> {
    let rates = scaled_rates(mono.len(), sample_rate, quality);
    if rates.env_frames < 2 {
        return None;
    }
    let want_bands = want_bands && rates.band_frames >= 2;
    let specs = plan_with(
        mono.len(),
        sample_rate,
        parts.max(1),
        want_bands,
        true,
        rates,
    );
    let scanned: Vec<ScanOut> = specs
        .par_iter()
        .map(|spec| scan_part(spec, &mono[spec.origin..spec.to]))
        .collect();

    let data: Vec<i16> = scanned
        .iter()
        .flat_map(|part| part.env.iter().copied())
        .collect();
    let peak = scanned
        .iter()
        .map(|part| part.env_peak)
        .max()
        .unwrap_or(0)
        .max(1);
    let bands = want_bands.then(|| {
        let mut bands: Vec<u8> = scanned
            .iter()
            .flat_map(|part| part.bands.iter().copied())
            .collect();
        let max = scanned.iter().map(|part| part.band_max).max().unwrap_or(0);
        normalize_bands(&mut bands, max);
        bands
    });
    Some(Wave {
        rate: reported_rate(rates.env_frames, mono.len(), sample_rate),
        data,
        peak,
        band_rate: if want_bands {
            reported_rate(rates.band_frames, mono.len(), sample_rate)
        } else {
            0.0
        },
        bands,
    })
}

/// Left and right averaged, as the renderer mixed them. Mono sources come out unchanged.
pub fn mix_mono(interleaved: &[f32]) -> Vec<f32> {
    interleaved
        .chunks_exact(2)
        .map(|frame| ((f64::from(frame[0]) + f64::from(frame[1])) * 0.5) as f32)
        .collect()
}

/// Decodes `path` at its own sample rate, through the same decoder the deck plays, and analyses it.
/// `quality` is a percent of the full scan. 0 and 100 are the full scan.
pub fn analyse(path: &Path, want_bands: bool, quality: u32) -> Result<Option<Wave>, DecodeError> {
    let mut streamer = Streamer::open(path, 0)?;
    let rate = f64::from(streamer.output_rate());
    let mut mono = Vec::with_capacity(usize::try_from(streamer.total_frames()).unwrap_or(0));
    let mut buffer = vec![0.0f32; 8192];
    loop {
        let frames = streamer.fill(&mut buffer)?;
        if frames == 0 {
            break;
        }
        mono.extend(mix_mono(&buffer[..frames * 2]));
    }
    let quality = if quality == 0 {
        100
    } else {
        quality.clamp(1, 100)
    };
    Ok(build_scaled(
        &mono,
        rate,
        rayon::current_num_threads(),
        want_bands,
        quality,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_quality_matches_the_fixed_rates() {
        let full = wave_rates(44_100 * 60, 44_100.0);
        let scaled = scaled_rates(44_100 * 60, 44_100.0, 100);
        assert_eq!(full.env_frames, scaled.env_frames);
        assert_eq!(full.band_frames, scaled.band_frames);
        assert_eq!(full.env_step, scaled.env_step);
        assert_eq!(full.band_step, scaled.band_step);
    }

    #[test]
    fn two_percent_is_about_a_fiftieth() {
        let full = wave_rates(44_100 * 60, 44_100.0);
        let fast = scaled_rates(44_100 * 60, 44_100.0, 2);
        let ratio = full.env_frames as f64 / fast.env_frames as f64;
        assert!((40.0..60.0).contains(&ratio), "{ratio}");
        let bands = full.band_frames as f64 / fast.band_frames as f64;
        assert!((40.0..60.0).contains(&bands), "{bands}");
    }
}
