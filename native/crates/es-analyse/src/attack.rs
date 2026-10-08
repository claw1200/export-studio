//! Where transients start, at millisecond resolution, for putting beats exactly on the hit.

/// Seconds per attack step.
pub const STEP: f64 = 0.001;
/// Steps the energy rise is measured across.
const RISE_SPAN: usize = 3;
/// Pre-emphasis, so the click of a kick or snare stands out from sustained bass.
const EMPHASIS: f32 = 0.97;
/// A rise this many dB or more counts as a hit.
const MIN_RISE_DB: f32 = 3.0;
/// The onset is the first step whose rise reaches this share of the strongest one.
const ONSET_SHARE: f32 = 0.5;
/// How far before the strongest rise the onset may start.
const ONSET_REACH: usize = 8;
/// Where rekordbox puts the beat relative to the start of the transient, measured on a few
/// hundred of its grids.
const BEAT_AFTER_ONSET: f64 = 0.003;

pub struct Attacks {
    /// dB rise over the previous `RISE_SPAN` steps, per step.
    rise: Vec<f32>,
    /// Seconds per step: [`STEP`] rounded to whole samples.
    step: f64,
}

/// A transient found near a predicted beat.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub time: f64,
    /// The dB rise, as a confidence for the line fit.
    pub strength: f32,
}

impl Attacks {
    pub fn new(mono: &[f32], sample_rate: f64) -> Self {
        let block = (sample_rate * STEP).round().max(1.0) as usize;
        let mut prev = 0.0f32;
        let mut db = Vec::with_capacity(mono.len() / block + 1);
        for chunk in mono.chunks(block) {
            let mut energy = 0.0f64;
            for &x in chunk {
                let y = x - EMPHASIS * prev;
                prev = x;
                energy += f64::from(y * y);
            }
            db.push((10.0 * (energy / chunk.len() as f64 + 1e-10).log10()) as f32);
        }
        let rise = (0..db.len())
            .map(|j| {
                if j < RISE_SPAN {
                    0.0
                } else {
                    (db[j] - db[j - RISE_SPAN]).max(0.0)
                }
            })
            .collect();
        Self {
            rise,
            step: block as f64 / sample_rate,
        }
    }

    /// The start of the strongest transient within `reach` seconds of `time`.
    pub fn near(&self, time: f64, reach: f64) -> Option<Hit> {
        let centre = (time / self.step).round() as isize;
        let span = (reach / self.step).round() as isize;
        let lo = (centre - span).max(0) as usize;
        let hi = ((centre + span).max(0) as usize).min(self.rise.len().saturating_sub(1));
        if lo >= hi {
            return None;
        }
        let (peak_at, peak) = (lo..=hi)
            .map(|j| (j, self.rise[j]))
            .fold((lo, 0.0f32), |best, cur| if cur.1 > best.1 { cur } else { best });
        if peak < MIN_RISE_DB {
            return None;
        }
        let mut onset = peak_at;
        let floor = peak * ONSET_SHARE;
        let earliest = peak_at.saturating_sub(ONSET_REACH);
        while onset > earliest && self.rise[onset - 1] >= floor {
            onset -= 1;
        }
        // The rise at step j spans the RISE_SPAN steps before it, so the hit starts there.
        let start = onset.saturating_sub(RISE_SPAN - 1);
        Some(Hit {
            time: start as f64 * self.step + BEAT_AFTER_ONSET,
            strength: peak,
        })
    }
}
