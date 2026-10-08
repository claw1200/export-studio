//! The Rust scan against fixtures written by the renderer's TypeScript scan, which this crate
//! replaced, from the same `signal`. Entries may differ by one step, from the last bit of a sine or
//! cosine. Each fixture is little-endian: envelope rate f64, peak i32, entry count u32, entries
//! i16, band rate f64, band count u32, bands u8.

use es_wave::build;

struct Golden {
    env_rate: f64,
    env_peak: i32,
    env: Vec<i16>,
    band_rate: f64,
    bands: Vec<u8>,
}

fn read(name: &str) -> Golden {
    let path = format!("{}/tests/fixtures/{name}.bin", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut at = 0;
    let mut take = |n: usize| {
        let slice = &bytes[at..at + n];
        at += n;
        slice
    };
    let env_rate = f64::from_le_bytes(take(8).try_into().unwrap());
    let env_peak = i32::from_le_bytes(take(4).try_into().unwrap());
    let env_len = u32::from_le_bytes(take(4).try_into().unwrap()) as usize;
    let env = take(env_len * 2)
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    let band_rate = f64::from_le_bytes(take(8).try_into().unwrap());
    let band_len = u32::from_le_bytes(take(4).try_into().unwrap()) as usize;
    let bands = take(band_len).to_vec();
    Golden {
        env_rate,
        env_peak,
        env,
        band_rate,
        bands,
    }
}

/// Noise, a 110 Hz triangle and a 3 kHz square with a swell each second, in whole 16-bit steps,
/// so the input is identical in both languages and every band has something in it.
fn signal(length: usize, sample_rate: i64) -> Vec<f32> {
    let mut seed: u32 = 0x1234_5678;
    let tri_period = (sample_rate as f64 / 110.0).round() as i64;
    let sq_period = (sample_rate as f64 / 3000.0).round() as i64;
    (0..length as i64)
        .map(|n| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let noise = i64::from((seed >> 8) % 4001) - 2000;
            let t = n % tri_period;
            let tri = (if t * 2 < tri_period {
                t
            } else {
                tri_period - t
            }) * 24_000
                / tri_period
                - 6000;
            let sq = if (n % sq_period) * 2 < sq_period {
                3000
            } else {
                -3000
            };
            let swell = 1 + (n % sample_rate) * 4 / sample_rate;
            ((noise + tri + sq) * swell / 4) as f32 / 32768.0
        })
        .collect()
}

fn largest_gap<T: Copy + Into<i32>>(a: &[T], b: &[T]) -> (i32, usize) {
    let mut worst = (0, 0);
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        let gap = ((*x).into() - (*y).into()).abs();
        if gap > worst.0 {
            worst = (gap, i);
        }
    }
    worst
}

fn check(name: &str, sample_rate: i64, seconds: f64, parts: usize) {
    let golden = read(name);
    let mono = signal((sample_rate as f64 * seconds).round() as usize, sample_rate);
    let wave = build(&mono, sample_rate as f64, parts, true).expect("long enough to draw");

    assert_eq!(wave.rate, golden.env_rate, "{name}: envelope rate");
    assert_eq!(wave.data.len(), golden.env.len(), "{name}: envelope length");
    assert!(
        (wave.peak - golden.env_peak).abs() <= 1,
        "{name}: peak {} vs {}",
        wave.peak,
        golden.env_peak
    );
    let (gap, at) = largest_gap(&wave.data, &golden.env);
    assert!(
        gap <= 1,
        "{name}: envelope entry {at} is {} against {}",
        wave.data[at],
        golden.env[at]
    );

    let bands = wave.bands.expect("bands asked for");
    assert_eq!(wave.band_rate, golden.band_rate, "{name}: band rate");
    assert_eq!(bands.len(), golden.bands.len(), "{name}: band length");
    let (gap, at) = largest_gap(&bands, &golden.bands);
    assert!(
        gap <= 1,
        "{name}: band value {at} is {} against {}",
        bands[at],
        golden.bands[at]
    );
    assert!(
        golden.bands.iter().any(|&b| b > 100),
        "{name}: the fixture exercises the bands"
    );
}

#[test]
fn matches_the_typescript_scan_at_44k1_in_four_parts() {
    check("r44100_p4", 44_100, 3.0, 4);
}

#[test]
fn matches_the_typescript_scan_at_48k_in_three_parts() {
    check("r48000_p3", 48_000, 2.0, 3);
}

#[test]
fn matches_the_typescript_scan_at_22k05_in_one_part() {
    check("r22050_p1", 22_050, 2.0, 1);
}

#[test]
fn too_short_to_draw_is_nothing() {
    assert!(build(&[0.0; 1], 44_100.0, 4, true).is_none());
}
