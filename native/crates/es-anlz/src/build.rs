//! Building analysis files (`ANLZ0000.DAT`, `.EXT`, `.2EX`) from our own analysis.
//!
//! The analyser supplies the beat grid and, per 1/150 s, the peak and RMS of the whole signal and of its
//! low, mid and high bands. Each waveform tag's values are fitted to those features; the coefficients come
//! from least squares against a few hundred files rekordbox analysed.

#![allow(clippy::approx_constant)]

use crate::decode::TAG_HEADER;

/// Values per detail column: peak and RMS of the whole signal, then of the low, mid and high bands.
pub const FEATURES: usize = 8;
const PREVIEW: usize = 400;
const TINY_PREVIEW: usize = 100;
const COLOR_PREVIEW: usize = 1200;
const VBR_ENTRIES: usize = 400;
const DETAIL_RATE: u32 = 150;
const SILENCE: f64 = 1.0 / 255.0;
const DB_FLOOR: f64 = -60.0;

#[derive(Clone, Copy)]
enum Transform {
    Sqrt,
    Lin,
    Db,
}

use Transform::{Db, Lin, Sqrt};

type Lane = (Transform, [f64; FEATURES], f64, f64);

const PWAV_H: Lane = (Sqrt, [-14.02, 15.97, 13.10, 7.54, 4.05, 3.72, 3.85, -2.83], -1.95, 31.0);
const PWAV_W: Lane = (Lin, [-1.48, 3.86, -2.58, -2.09, -0.60, -4.74, 1.43, 2.78], 5.17, 7.0);
const PWV2_H: Lane = (Sqrt, [1.37, -21.80, 3.54, 23.21, 3.45, 9.53, 0.72, 1.51], 0.0, 15.0);
const PWV3_H: Lane = (Lin, [25.29, 5.01, 2.72, 1.14, 0.95, 1.05, 4.15, 2.47], -6.28, 31.0);
const PWV3_W: Lane = (Sqrt, [4.92, -1.08, -11.27, 0.19, -0.26, 3.46, 0.43, 0.15], 5.11, 7.0);
const PWV4: [Lane; 6] = [
    (Lin, [112.16, 25.17, 7.06, -17.46, 3.21, -9.88, 3.70, 0.18], -0.74, 127.0),
    (Lin, [-63.49, 8.18, -14.55, -13.52, -7.43, 6.34, -8.79, -18.18], 221.95, 255.0),
    (Lin, [22.86, -5.73, 105.72, -3.62, 4.98, 41.16, -3.21, -20.09], -0.76, 127.0),
    (Lin, [-7.30, -20.70, 122.10, 34.91, 3.05, -22.41, 2.26, 14.73], -0.47, 127.0),
    (Lin, [-29.73, 14.26, 11.20, -16.15, 83.41, 86.89, 11.10, -56.97], -0.62, 127.0),
    (Lin, [-33.40, -7.57, 9.58, 19.14, 9.77, -18.40, 66.78, 174.44], 0.71, 127.0),
];
const PWV5_H: Lane = (Lin, [21.02, 0.47, 5.93, 4.91, 0.58, 6.49, 1.72, 14.28], -5.70, 31.0);
const PWV5_R: Lane = (Db, [-3.89, -0.45, 8.66, 6.83, -1.62, -7.16, 0.98, -1.05], 2.87, 7.0);
const PWV5_G: Lane = (Db, [-5.36, -10.93, -3.27, 2.59, 11.23, 6.14, -1.29, 1.98], 4.40, 7.0);
const PWV5_B: Lane = (Db, [-0.47, -14.73, -5.18, 4.91, -2.06, 4.87, 7.76, 5.57], 6.87, 7.0);
const PWV6: [Lane; 3] = [
    (Lin, [9.36, -49.95, -6.82, 102.51, -8.26, 47.31, -0.59, 37.32], 2.11, 127.0),
    (Sqrt, [10.97, -24.28, 1.17, 10.32, -18.08, 87.55, -0.60, 9.58], -3.75, 127.0),
    (Sqrt, [7.69, -29.15, 3.82, 15.02, -2.71, 5.17, -6.46, 99.37], -3.85, 127.0),
];
const PWV7: [Lane; 3] = [
    (Sqrt, [-11.08, 21.93, 105.01, -14.33, -10.01, 3.71, 11.19, -8.91], -3.34, 127.0),
    (Lin, [6.07, -1.04, 1.67, 2.93, 38.22, 85.53, -6.94, 9.27], 6.48, 127.0),
    (Lin, [-15.90, 10.01, 13.31, -5.97, -18.48, 23.04, 61.39, 123.84], -4.69, 127.0),
];

/// The waveform tags each analysis file holds.
pub const WAVE_TAGS: [(&str, &[&str]); 3] = [("DAT", &["PWAV", "PWV2"]), ("EXT", &["PWV3", "PWV5", "PWV4"]), ("2EX", &["PWV7", "PWV6"])];

type Row = [f64; FEATURES];

fn lane(spec: &Lane, row: &Row) -> u16 {
    let (transform, weights, bias, top) = spec;
    let mut value = *bias;
    for (x, w) in row.iter().zip(weights) {
        let t = match transform {
            Sqrt => x.sqrt(),
            Lin => *x,
            Db => (20.0 * (x + 1e-4).log10()).max(DB_FLOOR) / -DB_FLOOR + 1.0,
        };
        value += t * w;
    }
    if row[0] < SILENCE {
        return 0;
    }
    value.round_ties_even().clamp(0.0, *top) as u16
}

/// Detail features pooled into `columns` columns: the loudest peak, and the RMS.
fn pool(features: &[Row], columns: usize) -> Vec<Row> {
    let n = features.len();
    if n == 0 {
        return vec![[0.0; FEATURES]; columns];
    }
    let starts: Vec<usize> = (0..columns).map(|i| (i * n / columns).min(n - 1)).collect();
    (0..columns)
        .map(|i| {
            let lo = starts[i];
            let hi = starts.get(i + 1).copied().unwrap_or(n);
            // numpy's reduceat takes the single element at `lo` when the span is empty
            let span = if hi > lo { &features[lo..hi] } else { &features[lo..=lo] };
            let mut out = [0.0; FEATURES];
            for k in (0..FEATURES).step_by(2) {
                out[k] = span.iter().map(|r| r[k]).fold(f64::MIN, f64::max);
                let sum: f64 = span.iter().map(|r| r[k + 1] * r[k + 1]).sum();
                out[k + 1] = (sum / (hi.saturating_sub(lo)).max(1) as f64).sqrt();
            }
            out
        })
        .collect()
}

fn tag(kind: &str, header: &[u8], body: &[u8]) -> Vec<u8> {
    let len_header = u32::try_from(TAG_HEADER + header.len()).unwrap_or(0);
    let total = len_header + u32::try_from(body.len()).unwrap_or(0);
    let mut out = kind.as_bytes().to_vec();
    out.extend(len_header.to_be_bytes());
    out.extend(total.to_be_bytes());
    out.extend(header);
    out.extend(body);
    out
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

/// One beat: its number in the bar, the tempo times 100 and its time in milliseconds.
pub type Beat = (u16, u16, u32);

#[must_use]
pub fn beatgrid_tag(beats: &[Beat]) -> Vec<u8> {
    let body: Vec<u8> = beats.iter().flat_map(|(n, t, ms)| [n.to_be_bytes().as_slice(), &t.to_be_bytes(), &ms.to_be_bytes()].concat()).collect();
    tag("PQTZ", &words(&[0, 0x80000, u32::try_from(beats.len()).unwrap_or(0)]), &body)
}

fn cue_tags() -> Vec<Vec<u8>> {
    [1u32, 0].iter().map(|kind| tag("PCOB", &[kind.to_be_bytes().as_slice(), &[0, 0, 0, 0], &(-1i32).to_be_bytes()].concat(), &[])).collect()
}

fn extended_cue_tags() -> Vec<Vec<u8>> {
    [1u32, 0].iter().map(|kind| tag("PCO2", &[kind.to_be_bytes().as_slice(), &[0, 0, 0, 0]].concat(), &[])).collect()
}

/// Every waveform tag written, by kind. `features` run 0 to 1.
#[must_use]
pub fn wave_tags(features: &[Row]) -> Vec<(&'static str, Vec<u8>)> {
    let preview = pool(features, PREVIEW);
    let tiny = pool(features, TINY_PREVIEW);
    let color = pool(features, COLOR_PREVIEW);
    let n = u32::try_from(features.len()).unwrap_or(0);

    let pwav: Vec<u8> = preview.iter().map(|r| (lane(&PWAV_H, r) | (lane(&PWAV_W, r) << 5)) as u8).collect();
    let pwv2: Vec<u8> = tiny.iter().map(|r| lane(&PWV2_H, r) as u8).collect();
    let pwv3: Vec<u8> = features.iter().map(|r| (lane(&PWV3_H, r) | (lane(&PWV3_W, r) << 5)) as u8).collect();
    let pwv5: Vec<u8> = features
        .iter()
        .flat_map(|r| ((lane(&PWV5_R, r) << 13) | (lane(&PWV5_G, r) << 10) | (lane(&PWV5_B, r) << 7) | (lane(&PWV5_H, r) << 2)).to_be_bytes())
        .collect();
    let pwv4: Vec<u8> = color
        .iter()
        .flat_map(|r| {
            let mut six: Vec<u8> = PWV4.iter().map(|l| lane(l, r) as u8).collect();
            // Silence is bright in rekordbox's second colour-preview byte, not dark.
            if r[0] < SILENCE {
                six[1] = 255;
            }
            six
        })
        .collect();
    let pwv6: Vec<u8> = color.iter().flat_map(|r| PWV6.iter().map(|l| lane(l, r) as u8)).collect();
    let pwv7: Vec<u8> = features.iter().flat_map(|r| PWV7.iter().map(|l| lane(l, r) as u8)).collect();

    let rate = DETAIL_RATE << 16;
    vec![
        ("PWAV", tag("PWAV", &words(&[PREVIEW as u32, 0x10000]), &pwav)),
        ("PWV2", tag("PWV2", &words(&[TINY_PREVIEW as u32, 0x10000]), &pwv2)),
        ("PWV3", tag("PWV3", &words(&[1, n, rate]), &pwv3)),
        ("PWV5", tag("PWV5", &words(&[2, n, rate | 0x305]), &pwv5)),
        ("PWV4", tag("PWV4", &words(&[6, COLOR_PREVIEW as u32, 0]), &pwv4)),
        ("PWV6", tag("PWV6", &words(&[3, COLOR_PREVIEW as u32]), &pwv6)),
        ("PWV7", tag("PWV7", &words(&[3, n, rate]), &pwv7)),
    ]
}

fn file(tags: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"PMAI".to_vec();
    out.extend(words(&[0x1C, 0, 1, 0x10000, 0x10000, 0]));
    for t in tags {
        out.extend(t);
    }
    let total = u32::try_from(out.len()).unwrap_or(0);
    out[8..12].copy_from_slice(&total.to_be_bytes());
    out
}

/// DAT, EXT and 2EX for a track with no analysis yet, laid out as rekordbox writes them.
#[must_use]
pub fn new_files(audio_name: &str, beats: &[Beat], features: &[Row]) -> [(&'static str, Vec<u8>); 3] {
    let path = crate::write::path_tag(&format!("?/{audio_name}"));
    let waves = wave_tags(features);
    let wave = |kind: &str| waves.iter().find(|(k, _)| *k == kind).map(|(_, t)| t.clone()).unwrap_or_default();
    let vbr = tag("PVBR", &[0; 4], &vec![0; 4 * (VBR_ENTRIES + 1)]);
    let dat: Vec<Vec<u8>> = [vec![path.clone(), vbr, beatgrid_tag(beats), wave("PWAV"), wave("PWV2")], cue_tags()].concat();
    let ext: Vec<Vec<u8>> = [vec![path.clone(), wave("PWV3")], cue_tags(), extended_cue_tags(), vec![wave("PWV5"), wave("PWV4")]].concat();
    let ex2 = vec![path, wave("PWV7"), wave("PWV6")];
    [("DAT", file(&dat)), ("EXT", file(&ext)), ("2EX", file(&ex2))]
}

/// Rows of eight features from the analyser's bytes, each scaled to 0 to 1.
#[must_use]
pub fn features_from_bytes(raw: &[u8]) -> Vec<Row> {
    raw.chunks_exact(FEATURES).map(|c| std::array::from_fn(|i| f64::from(c[i]) / 255.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{self, Tags};

    fn features(n: usize) -> Vec<Row> {
        (0..n).map(|i| std::array::from_fn(|k| ((i * 7 + k * 13) % 255) as f64 / 255.0)).collect()
    }

    #[test]
    fn built_files_read_back_with_their_grid_and_waveforms() {
        let beats = [(1u16, 12800u16, 0u32), (2, 12800, 469), (3, 12800, 938)];
        let rows = features(300);
        let files = new_files("song.wav", &beats, &rows);
        let dat = Tags::parse(files[0].1.clone()).unwrap();
        let grid = decode::beatgrid(&dat).unwrap();
        assert_eq!((grid.times, grid.beats), (vec![0, 469, 938], vec![1, 2, 3]));
        let ext = Tags::parse(files[1].1.clone()).unwrap();
        assert_eq!(decode::rgb(&ext).unwrap().length, 300);
        let ex2 = Tags::parse(files[2].1.clone()).unwrap();
        assert_eq!(decode::bands(&ex2, "PWV7").unwrap().length, 300);
        assert_eq!(decode::bands(&ex2, "PWV6").unwrap().length, COLOR_PREVIEW);
        for (_, data) in &files {
            assert_eq!(u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize, data.len());
        }
    }

    #[test]
    fn silence_stays_dark_and_pooling_handles_short_signals() {
        let tags = wave_tags(&features(3));
        assert_eq!(tags.len(), 7);
        let silent = wave_tags(&[[0.0; FEATURES]; 4]);
        let pwv3 = &silent.iter().find(|(k, _)| *k == "PWV3").unwrap().1;
        assert!(pwv3[pwv3.len() - 4..].iter().all(|b| *b == 0));
    }
}
