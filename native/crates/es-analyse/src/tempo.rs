//! Tempo and the beatgrid.
//!
//! The tempo comes from the autocorrelation of the onset envelope, its octave from a tempo prior
//! and how strong the off-beats are. A comb over the envelope then finds the exact period and
//! phase, and each beat is moved onto the transient nearest it before a robust straight-line fit
//! gives the grid. In dynamic mode the same is done for each stretch of the track that settles on
//! its own tempo.

use realfft::RealFftPlanner;

use crate::attack::Attacks;
use crate::onset::Envelope;

#[derive(Debug, Clone, Copy)]
pub struct TempoOptions {
    pub min_bpm: f64,
    pub max_bpm: f64,
    /// Follow tempo changes with several grid segments, rather than one constant grid.
    pub dynamic: bool,
    /// Put beats on the transient's start rather than the onset envelope's peak.
    pub high_precision: bool,
    /// Snap to a whole BPM when within [`WHOLE_BPM_TOLERANCE`] of one.
    pub round_bpm: bool,
}

impl Default for TempoOptions {
    fn default() -> Self {
        Self {
            min_bpm: 70.0,
            max_bpm: 180.0,
            dynamic: false,
            high_precision: true,
            round_bpm: true,
        }
    }
}

pub const WHOLE_BPM_TOLERANCE: f64 = 0.05;
/// Most music sits near here; the prior leans the octave choice towards it.
const PRIOR_CENTRE_BPM: f64 = 128.0;
/// Width of the log-normal tempo prior, in natural-log units.
const PRIOR_WIDTH: f64 = 0.5;
/// Power of the prior in the coarse tempo search.
const TIE_PRIOR: f64 = 0.05;
/// The off-beats must reach this share of the beats, scaled by the prior, to double the tempo.
const DOUBLE_SHARE: f64 = 0.3;
/// Lags of the autocorrelation summed for a tempo, with weight 1/k.
const ACF_MULTIPLES: usize = 4;
const BPM_STEP: f64 = 0.05;
/// Comb search: period spread around the estimate, and the steps across it.
const FINE_SPREAD: f64 = 0.004;
const PERIOD_STEPS: usize = 40;
const PHASE_STEPS_PER_FRAME: f64 = 4.0;
/// How far a transient may be from its predicted beat, as a share of the period.
const ATTACK_REACH: f64 = 0.06;
/// Where rekordbox puts the beat relative to the onset envelope's peak, measured on its grids.
const BEAT_AFTER_PEAK: f64 = 0.002;
/// Hits further than this share of a period from the first fit are dropped before refitting.
const OUTLIER_SHARE: f64 = 0.04;
/// Bands whose onsets vote on whether a grid sits on the beat or the off-beat, in Hz.
pub const PHASE_BANDS: [(f64, f64); 5] = [(60.0, 120.0), (120.0, 250.0), (250.0, 1_000.0), (1_000.0, 4_000.0), (4_000.0, 16_000.0)];
/// Logistic weights for those votes, fitted to rekordbox's grids on a few hundred tracks: kick
/// bodies and claps mark the beat, hats the off-beat.
const OFFBEAT_WEIGHTS: [f64; 5] = [0.635, -1.409, -3.101, -2.652, 2.917];
const OFFBEAT_ATTACK_WEIGHT: f64 = -1.518;
const OFFBEAT_BIAS: f64 = -0.547;
const VOTE_FLOOR: f64 = 1e-4;
/// Dynamic mode: window and hop for local tempo, and how far a local tempo may stray.
const WINDOW_SECS: f64 = 16.0;
const WINDOW_HOP_SECS: f64 = 4.0;
const LOCAL_SPREAD: f64 = 0.08;
/// Local tempos within this share are the same tempo.
const SAME_TEMPO: f64 = 0.01;
/// A tempo needs this many windows in a row to get its own segment.
const MIN_RUN: usize = 3;

/// A stretch of constant tempo: beats fall at `phase + k * period` from `from` until the next.
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    pub from: f64,
    pub period: f64,
    pub phase: f64,
}

impl Segment {
    pub fn bpm(&self) -> f64 {
        60.0 / self.period
    }
}

#[derive(Debug, Clone)]
pub struct Grid {
    /// The tempo of the longest segment.
    pub bpm: f64,
    pub segments: Vec<Segment>,
    /// Beat times in seconds, and each beat's tempo.
    pub beats: Vec<f64>,
    pub tempos: Vec<f64>,
}

pub struct Inputs<'a> {
    /// Onset envelopes for [`PHASE_BANDS`], for which of two half-beat phases is the beat.
    pub bands: &'a [Envelope],
    /// Full-band onset envelope.
    pub onset: &'a Envelope,
    pub attacks: &'a Attacks,
    pub duration: f64,
}

fn prior(bpm: f64) -> f64 {
    let x = (bpm / PRIOR_CENTRE_BPM).ln() / PRIOR_WIDTH;
    (-0.5 * x * x).exp()
}

/// Normalised autocorrelation of the mean-removed envelope, by FFT.
fn autocorrelation(values: &[f32], max_lag: usize) -> Vec<f32> {
    let n = values.len();
    if n < 2 {
        return vec![0.0; max_lag + 1];
    }
    let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    let size = (2 * n).next_power_of_two();
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(size);
    let inv = planner.plan_fft_inverse(size);
    let mut input = fwd.make_input_vec();
    for (slot, &v) in input.iter_mut().zip(values) {
        *slot = v - mean as f32;
    }
    let mut spectrum = fwd.make_output_vec();
    if fwd.process(&mut input, &mut spectrum).is_err() {
        return vec![0.0; max_lag + 1];
    }
    for c in &mut spectrum {
        *c = realfft::num_complex::Complex::new(c.norm_sqr(), 0.0);
    }
    let mut out = inv.make_output_vec();
    if inv.process(&mut spectrum, &mut out).is_err() {
        return vec![0.0; max_lag + 1];
    }
    let zero = out[0].max(1e-12);
    (0..=max_lag.min(n - 1))
        .map(|lag| out[lag] / zero * n as f32 / (n - lag) as f32)
        .collect()
}

fn interp(values: &[f32], x: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let i = x.floor() as usize;
    if i + 1 >= values.len() {
        return values.last().map(|&v| f64::from(v)).unwrap_or(0.0);
    }
    let f = x - i as f64;
    f64::from(values[i]) * (1.0 - f) + f64::from(values[i + 1]) * f
}

/// The tempo whose beat period, and its multiples, line up best with the envelope. Its octave
/// is settled later, so the prior here only breaks near ties: a strong one favours 2/3 of a fast
/// tempo.
fn coarse_bpm(env: &Envelope, opts: &TempoOptions) -> Option<f64> {
    let max_lag = (60.0 * env.rate / (opts.min_bpm / 2.0) * ACF_MULTIPLES as f64).ceil() as usize;
    let acf = autocorrelation(&env.values, max_lag);
    let score = |bpm: f64| {
        let lag = 60.0 * env.rate / bpm;
        let comb: f64 = (1..=ACF_MULTIPLES)
            .map(|k| interp(&acf, lag * k as f64) / k as f64)
            .sum();
        comb.max(0.0) * prior(bpm).powf(TIE_PRIOR)
    };
    let lo = opts.min_bpm / 2.0;
    let mut best = (0.0, lo);
    let mut bpm = lo;
    while bpm <= opts.max_bpm * 2.0 {
        let s = score(bpm);
        if s > best.0 {
            best = (s, bpm);
        }
        bpm += BPM_STEP;
    }
    (best.0 > 0.0).then_some(best.1)
}

/// Mean envelope on a grid of `period` from `phase`, between `from` and `to`.
fn comb_mean(env: &Envelope, period: f64, phase: f64, from: f64, to: f64) -> f64 {
    let first = ((from - phase) / period).ceil() as i64;
    let mut t = phase + first as f64 * period;
    let (mut sum, mut n) = (0.0, 0usize);
    while t < to {
        sum += f64::from(env.at_time(t));
        n += 1;
        t += period;
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f64
    }
}

/// The period within `spread` of `period`, and the phase, that put the most onset on the grid.
fn comb_fit(env: &Envelope, period: f64, spread: f64, from: f64, to: f64) -> (f64, f64, f64) {
    let phase_step = 1.0 / (env.rate * PHASE_STEPS_PER_FRAME);
    let search = |centre: f64, spread: f64, steps: usize| {
        let mut best = (f64::MIN, centre, 0.0);
        for s in 0..=steps {
            let p = centre * (1.0 + spread * (2.0 * s as f64 / steps.max(1) as f64 - 1.0));
            let mut phase = from;
            while phase < from + p {
                let score = comb_mean(env, p, phase, from, to);
                if score > best.0 {
                    best = (score, p, phase);
                }
                phase += phase_step;
            }
        }
        best
    };
    let (_, p, _) = search(period, spread, PERIOD_STEPS);
    let (score, p, phase) = search(p, spread / PERIOD_STEPS as f64 * 2.0, 8);
    (p, phase, score)
}

/// Doubles the tempo while the off-beats are about as strong as the beats, and the doubled tempo
/// is in range. Starts from the slowest octave the range allows, so the faster reading wins ties.
fn choose_octave(env: &Envelope, bpm: f64, duration: f64, opts: &TempoOptions) -> f64 {
    let mut bpm = bpm;
    while bpm > opts.max_bpm {
        bpm /= 2.0;
    }
    while bpm / 2.0 >= opts.min_bpm {
        bpm /= 2.0;
    }
    while bpm < opts.min_bpm {
        bpm *= 2.0;
    }
    while bpm * 2.0 <= opts.max_bpm {
        let period = 60.0 / bpm;
        let (p, phase, on) = comb_fit(env, period, LOCAL_SPREAD / 4.0, 0.0, duration);
        let off = comb_mean(env, p, phase + p / 2.0, 0.0, duration);
        let needed = DOUBLE_SHARE * prior(bpm) / prior(bpm * 2.0);
        if on <= 0.0 || off / on < needed {
            break;
        }
        bpm *= 2.0;
    }
    bpm
}

/// Weighted least squares of time against beat index. Returns (phase at index 0, period).
fn fit_line(points: &[(f64, f64, f64)]) -> Option<(f64, f64)> {
    let w: f64 = points.iter().map(|p| p.2).sum();
    if points.len() < 2 || w <= 0.0 {
        return None;
    }
    let mk = points.iter().map(|p| p.0 * p.2).sum::<f64>() / w;
    let mt = points.iter().map(|p| p.1 * p.2).sum::<f64>() / w;
    let (mut sxx, mut sxy) = (0.0, 0.0);
    for &(k, t, wt) in points {
        sxx += wt * (k - mk) * (k - mk);
        sxy += wt * (k - mk) * (t - mt);
    }
    if sxx <= 0.0 {
        return None;
    }
    let period = sxy / sxx;
    Some((mt - period * mk, period))
}

/// The phase that best fits `points` for a fixed period: their weighted median offset.
fn fit_phase(points: &[(f64, f64, f64)], period: f64) -> Option<f64> {
    let mut offsets: Vec<(f64, f64)> = points.iter().map(|&(k, t, w)| (t - k * period, w)).collect();
    if offsets.is_empty() {
        return None;
    }
    offsets.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = offsets.iter().map(|o| o.1).sum::<f64>() / 2.0;
    let mut acc = 0.0;
    for (offset, w) in &offsets {
        acc += w;
        if acc >= half {
            return Some(*offset);
        }
    }
    offsets.last().map(|o| o.0)
}

/// The peak of the envelope nearest `time`, within a frame either side, to sub-frame precision.
fn envelope_peak(env: &Envelope, time: f64) -> Option<(f64, f64)> {
    let centre = env.index(time).round() as isize;
    let (mut best, mut at) = (0.0f32, -1isize);
    for i in centre - 2..=centre + 2 {
        if i <= 0 || i as usize + 1 >= env.values.len() {
            continue;
        }
        let v = env.values[i as usize];
        if v > best && v >= env.values[i as usize - 1] && v >= env.values[i as usize + 1] {
            best = v;
            at = i;
        }
    }
    if at < 0 || best <= 0.0 {
        return None;
    }
    let (a, b, c) = (
        f64::from(env.values[at as usize - 1]),
        f64::from(best),
        f64::from(env.values[at as usize + 1]),
    );
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-12 { 0.5 * (a - c) / denom } else { 0.0 };
    Some((env.time(at as f64 + shift.clamp(-0.5, 0.5)) + BEAT_AFTER_PEAK, b))
}

/// Mean attack strength on a grid, counting beats without a transient as 0.
fn attack_mean(attacks: &Attacks, period: f64, phase: f64, from: f64, to: f64) -> f64 {
    let first = ((from - phase) / period).ceil() as i64;
    let last = ((to - phase) / period).floor() as i64;
    let total: f64 = (first..=last)
        .filter_map(|k| attacks.near(phase + k as f64 * period, period * ATTACK_REACH))
        .map(|hit| f64::from(hit.strength))
        .sum();
    total / (last - first + 1).max(1) as f64
}

/// Whether the grid at `phase` sits on the off-beats, so the beat is half a period later.
/// Each band votes with the log ratio of its onsets on the grid to those half a beat away.
fn is_offbeat(inputs: &Inputs, period: f64, phase: f64, from: f64, to: f64) -> bool {
    let half = phase + period / 2.0;
    let log_ratio = |on: f64, off: f64| ((on + VOTE_FLOOR) / (off + VOTE_FLOOR)).ln();
    let mut vote = OFFBEAT_BIAS;
    for (env, weight) in inputs.bands.iter().zip(OFFBEAT_WEIGHTS) {
        vote += weight * log_ratio(comb_mean(env, period, phase, from, to), comb_mean(env, period, half, from, to));
    }
    let attacks = log_ratio(
        attack_mean(inputs.attacks, period, phase, from, to),
        attack_mean(inputs.attacks, period, half, from, to),
    );
    vote + OFFBEAT_ATTACK_WEIGHT * attacks > 0.0
}

/// Fits one constant-tempo segment over `from..to`, starting from a rough period.
fn fit_segment(inputs: &Inputs, rough_period: f64, spread: f64, from: f64, to: f64, opts: &TempoOptions) -> Option<Segment> {
    let (period, mut phase, score) = comb_fit(inputs.onset, rough_period, spread, from, to);
    if score <= 0.0 {
        return None;
    }
    if is_offbeat(inputs, period, phase, from, to) {
        phase += period / 2.0;
    }

    let first = ((from - phase) / period).ceil() as i64;
    let last = ((to - phase) / period).floor() as i64;
    let mut points = Vec::new();
    for k in first..=last {
        let predicted = phase + k as f64 * period;
        let found = if opts.high_precision {
            inputs
                .attacks
                .near(predicted, period * ATTACK_REACH)
                .map(|hit| (hit.time, f64::from(hit.strength)))
        } else {
            envelope_peak(inputs.onset, predicted)
        };
        if let Some((time, weight)) = found {
            points.push((k as f64, time, weight));
        }
    }
    let (mut phase0, mut period) = fit_line(&points).unwrap_or((phase, period));
    if period.is_nan() || period <= 0.0 || (period / rough_period - 1.0).abs() > spread * 2.0 + 0.01 {
        phase0 = phase;
        period = rough_period;
    }
    // Refit without the hits that sit off the line.
    let kept: Vec<_> = points
        .iter()
        .copied()
        .filter(|&(k, t, _)| (t - (phase0 + k * period)).abs() <= period * OUTLIER_SHARE)
        .collect();
    if let Some((p0, p)) = fit_line(&kept) {
        if (p / period - 1.0).abs() < 0.01 {
            phase0 = p0;
            period = p;
        }
    }

    let bpm = 60.0 / period;
    let snapped = if opts.round_bpm && (bpm - bpm.round()).abs() < WHOLE_BPM_TOLERANCE {
        bpm.round()
    } else {
        (bpm * 100.0).round() / 100.0
    };
    let period = 60.0 / snapped;
    let fitted = if kept.is_empty() { &points } else { &kept };
    let phase0 = fit_phase(fitted, period).unwrap_or(phase0);
    Some(Segment { from, period, phase: phase0 })
}

/// Local tempo per window, for finding where a track changes tempo.
fn local_bpms(env: &Envelope, bpm: f64, duration: f64) -> Vec<(f64, f64, Option<f64>)> {
    let mut out = Vec::new();
    let mut start = 0.0;
    let floor = comb_mean(env, 60.0 / bpm, 0.0, 0.0, duration) * 0.5;
    while start + WINDOW_SECS / 2.0 < duration {
        let end = (start + WINDOW_SECS).min(duration);
        let (p, _, score) = comb_fit(env, 60.0 / bpm, LOCAL_SPREAD, start, end);
        out.push((start, end, (score > floor).then_some(60.0 / p)));
        start += WINDOW_HOP_SECS;
    }
    out
}

/// Runs of windows that agree on a tempo, as (from, to, bpm). Short runs join their neighbours.
fn tempo_runs(windows: &[(f64, f64, Option<f64>)], bpm: f64) -> Vec<(f64, f64, f64)> {
    let mut runs: Vec<(f64, f64, Vec<f64>)> = Vec::new();
    for &(from, to, local) in windows {
        let Some(local) = local else { continue };
        match runs.last_mut() {
            Some(run) if (local / median(&run.2) - 1.0).abs() <= SAME_TEMPO => {
                run.1 = to;
                run.2.push(local);
            }
            _ => runs.push((from, to, vec![local])),
        }
    }
    let mut merged: Vec<(f64, f64, f64, usize)> = Vec::new();
    for (from, to, values) in runs {
        let tempo = median(&values);
        if values.len() < MIN_RUN {
            if let Some(last) = merged.last_mut() {
                last.1 = to;
            }
            continue;
        }
        match merged.last_mut() {
            Some(last) if (tempo / last.2 - 1.0).abs() <= SAME_TEMPO => last.1 = to,
            _ => merged.push((from, to, tempo, values.len())),
        }
    }
    if merged.is_empty() {
        return vec![(0.0, f64::INFINITY, bpm)];
    }
    merged.iter().map(|&(a, b, t, _)| (a, b, t)).collect()
}

fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Lays beats from each segment's start to the next one's, dropping any that crowd a boundary.
fn lay_beats(segments: &[Segment], duration: f64) -> (Vec<f64>, Vec<f64>) {
    let mut beats: Vec<f64> = Vec::new();
    let mut tempos = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        let to = segments.get(i + 1).map_or(duration, |next| next.from);
        let from = if i == 0 { 0.0 } else { seg.from };
        let mut k = ((from - seg.phase) / seg.period).ceil() as i64;
        loop {
            let t = seg.phase + k as f64 * seg.period;
            if t >= to || t >= duration {
                break;
            }
            if t >= 0.0 && beats.last().is_none_or(|&last| t - last > seg.period * 0.5) {
                beats.push(t);
                tempos.push(seg.bpm());
            }
            k += 1;
        }
    }
    (beats, tempos)
}

pub fn estimate(inputs: &Inputs, opts: &TempoOptions) -> Option<Grid> {
    let duration = inputs.duration;
    if duration < 2.0 {
        return None;
    }
    let rough = coarse_bpm(inputs.onset, opts)?;
    let bpm = choose_octave(inputs.onset, rough, duration, opts);

    let mut segments = Vec::new();
    if opts.dynamic {
        let runs = tempo_runs(&local_bpms(inputs.onset, bpm, duration), bpm);
        let count = runs.len();
        for (i, &(from, to, tempo)) in runs.iter().enumerate() {
            let from = if i == 0 { 0.0 } else { from };
            let to = if i + 1 == count { duration } else { to.min(duration) };
            let spread = if count == 1 { FINE_SPREAD } else { SAME_TEMPO };
            if let Some(seg) = fit_segment(inputs, 60.0 / tempo, spread, from, to, opts) {
                segments.push(seg);
            }
        }
        // Segment boundaries sit halfway between where one run ends and the next begins.
        for i in 1..segments.len() {
            let prev_end = runs[i - 1].1.min(duration);
            let start = runs[i].0;
            segments[i].from = if start < prev_end { (start + prev_end) / 2.0 } else { start };
        }
        segments.dedup_by(|b, a| (a.period / b.period - 1.0).abs() < 1e-6 && (a.phase - b.phase).abs() < 1e-3);
    }
    if segments.is_empty() {
        segments.push(fit_segment(inputs, 60.0 / bpm, FINE_SPREAD, 0.0, duration, opts)?);
    }
    let (beats, tempos) = lay_beats(&segments, duration);
    if beats.len() < 2 {
        return None;
    }
    let longest = segments
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let to = segments.get(i + 1).map_or(duration, |n| n.from);
            (to - s.from, s.bpm())
        })
        .fold((f64::MIN, bpm), |a, b| if b.0 > a.0 { b } else { a });
    Some(Grid {
        bpm: longest.1,
        segments,
        beats,
        tempos,
    })
}
