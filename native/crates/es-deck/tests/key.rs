mod common;

use common::*;

/// A sine at `hz`, at most about two thirds of full scale.
fn sine(name: &str, frames: u32, hz: f64) -> std::path::PathBuf {
    wav(name, frames, |i| {
        ((f64::from(i) * 2.0 * std::f64::consts::PI * hz / f64::from(RATE)).sin() * 20_000.0) as i16
    })
}

/// Cycles per second of the left channel, from its upward zero crossings.
fn frequency(out: &[f32]) -> f64 {
    let left: Vec<f32> = out.chunks_exact(2).map(|f| f[0]).collect();
    let crossings = left
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    crossings as f64 * f64::from(RATE) / left.len() as f64
}

/// Plays a 440 Hz sine with the key moved, and measures how far the playhead went, and the pitch
/// heard, over a second.
fn play_shifted(name: &str, semitones: i32, tempo: f64, master_tempo: bool) -> (f64, f64) {
    let rig = Rig::new();
    rig.load(&sine(name, RATE * 8, 440.0));
    rig.engine.set_master_tempo(0, master_tempo);
    rig.engine.set_tempo(0, tempo);
    rig.engine.set_key_shift(0, semitones);
    rig.settle_ring();
    rig.engine.play(0);
    // Past the fade in and the stretcher's priming.
    let _ = rig.pull(RATE as usize / 4);
    let from = rig.position();
    let out = rig.pull(RATE as usize);
    let moved = (rig.position() - from) as f64 / f64::from(RATE);
    (moved, frequency(&out))
}

fn semitones_up(hz: f64, semitones: i32) -> f64 {
    hz * 2_f64.powf(f64::from(semitones) / 12.0)
}

#[test]
fn a_key_shift_moves_the_pitch_and_not_the_speed() {
    let (moved, hz) = play_shifted("key-up", 1, 1.0, false);
    assert!((moved - 1.0).abs() < 0.03, "moved {moved} s in a second");
    let want = semitones_up(440.0, 1);
    assert!((hz - want).abs() < 6.0, "heard {hz} Hz, wanted {want}");

    let (moved, hz) = play_shifted("key-down", -2, 1.0, false);
    assert!((moved - 1.0).abs() < 0.03, "moved {moved} s in a second");
    let want = semitones_up(440.0, -2);
    assert!((hz - want).abs() < 6.0, "heard {hz} Hz, wanted {want}");
}

#[test]
fn with_master_tempo_the_key_shift_is_all_the_pitch_moves() {
    let (moved, hz) = play_shifted("key-mt", 2, 1.1, true);
    assert!((moved - 1.1).abs() < 0.03, "moved {moved} s in a second");
    let want = semitones_up(440.0, 2);
    assert!((hz - want).abs() < 6.0, "heard {hz} Hz, wanted {want}");
}

#[test]
fn without_master_tempo_the_key_shift_adds_to_the_speed_change() {
    let (moved, hz) = play_shifted("key-vari", 1, 1.1, false);
    assert!((moved - 1.1).abs() < 0.03, "moved {moved} s in a second");
    let want = semitones_up(440.0 * 1.1, 1);
    assert!((hz - want).abs() < 7.0, "heard {hz} Hz, wanted {want}");
}

#[test]
fn back_at_no_shift_the_file_plays_exactly() {
    let rig = Rig::new();
    rig.load(&ramp("key-exact", RATE * 4));
    rig.engine.play(0);
    let _ = rig.pull(4096);
    rig.engine.set_key_shift(0, 3);
    let _ = rig.pull(9000);
    rig.engine.set_key_shift(0, 0);
    // The requeue fades out and back in; past that the frames are the file's own again.
    let _ = rig.pull(4096);
    let out = rig.pull(4096);
    let values: Vec<i64> = out.chunks_exact(2).map(|f| value(f[0])).collect();
    for pair in values.windows(2) {
        assert_eq!(
            pair[1],
            (pair[0] + 1) % 30_000,
            "frames are the file's, one after another"
        );
    }
}

#[test]
fn the_key_shift_stays_within_an_octave() {
    let rig = Rig::new();
    rig.load(&dc("key-range", RATE));
    rig.engine.set_key_shift(0, 40);
    assert_eq!(rig.engine.snapshot(0).unwrap().key_shift, 12);
    rig.engine.set_key_shift(0, -40);
    assert_eq!(rig.engine.snapshot(0).unwrap().key_shift, -12);
}
