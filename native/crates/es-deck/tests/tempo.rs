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
    let crossings = left.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    crossings as f64 * f64::from(RATE) / left.len() as f64
}

/// Plays at `tempo` and measures how far the playhead moved, and the pitch heard, over a second.
fn play_at(name: &str, tempo: f64, master_tempo: bool) -> (f64, f64) {
    let rig = Rig::new();
    rig.load(&sine(name, RATE * 8, 440.0));
    rig.engine.set_master_tempo(0, master_tempo);
    rig.engine.set_tempo(0, tempo);
    rig.settle_ring();
    rig.engine.play(0);
    // Past the fade in and, with master tempo, the stretcher's priming.
    let _ = rig.pull(RATE as usize / 4);
    let from = rig.position();
    let out = rig.pull(RATE as usize);
    let moved = (rig.position() - from) as f64 / f64::from(RATE);
    (moved, frequency(&out))
}

#[test]
fn without_master_tempo_speed_and_pitch_move_together() {
    let (moved, hz) = play_at("varispeed", 1.25, false);
    assert!((moved - 1.25).abs() < 0.02, "moved {moved} s in a second");
    assert!((hz - 550.0).abs() < 6.0, "heard {hz} Hz");
}

#[test]
fn master_tempo_changes_speed_and_keeps_the_pitch() {
    let (moved, hz) = play_at("stretch", 1.25, true);
    assert!((moved - 1.25).abs() < 0.03, "moved {moved} s in a second");
    assert!((hz - 440.0).abs() < 6.0, "heard {hz} Hz");
}

#[test]
fn slowing_down_works_both_ways() {
    let (moved, hz) = play_at("slow-varispeed", 0.9, false);
    assert!((moved - 0.9).abs() < 0.02, "moved {moved} s in a second");
    assert!((hz - 396.0).abs() < 6.0, "heard {hz} Hz");
    let (moved, hz) = play_at("slow-stretch", 0.9, true);
    assert!((moved - 0.9).abs() < 0.03, "moved {moved} s in a second");
    assert!((hz - 440.0).abs() < 6.0, "heard {hz} Hz");
}

#[test]
fn back_at_normal_speed_the_file_plays_exactly() {
    let rig = Rig::new();
    rig.load(&ramp("tempo-exact", RATE * 4));
    rig.engine.play(0);
    let _ = rig.pull(4096);
    rig.engine.set_tempo(0, 1.07);
    let _ = rig.pull(9000);
    rig.engine.set_tempo(0, 1.0);
    let _ = rig.pull(16);
    let out = rig.pull(4096);
    let values: Vec<i64> = out.chunks_exact(2).map(|f| value(f[0])).collect();
    for pair in values.windows(2) {
        assert_eq!(pair[1], (pair[0] + 1) % 30_000, "frames are the file's, one after another");
    }
}

#[test]
fn a_tempo_change_is_heard_without_a_click() {
    let rig = Rig::new();
    rig.load(&dc("tempo-click", RATE * 4));
    rig.engine.play(0);
    let _ = rig.pull(4096);
    rig.engine.set_tempo(0, 1.16);
    let out = rig.pull(4096);
    assert!(largest_step(&out) < 1e-3, "stepped {}", largest_step(&out));
}

#[test]
fn switching_master_tempo_fades_rather_than_clicks() {
    let rig = Rig::new();
    rig.load(&dc("master-click", RATE * 4));
    rig.engine.set_tempo(0, 1.1);
    rig.engine.play(0);
    let _ = rig.pull(4096);
    rig.engine.set_master_tempo(0, true);
    let out = rig.pull(RATE as usize / 2);
    let step = 26_000.0 / 32_768.0 / f32::from(es_deck::FADE_FRAMES) + 0.05;
    assert!(largest_step(&out) <= step, "stepped {}", largest_step(&out));
    assert!(rig.engine.snapshot(0).unwrap().master_tempo);
}

#[test]
fn the_tempo_is_kept_in_range() {
    let rig = Rig::new();
    rig.engine.set_tempo(0, 9.0);
    assert_eq!(rig.engine.snapshot(0).unwrap().tempo, es_deck::MAX_TEMPO);
    rig.engine.set_tempo(0, f64::NAN);
    assert_eq!(rig.engine.snapshot(0).unwrap().tempo, 1.0);
}
