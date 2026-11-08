//! Beat sync: a deck started while its partner plays lands on the partner's beat, however long its
//! new audio takes to arrive.
mod common;

use common::*;

/// 120 BPM: a beat every half second.
const BEAT: u64 = RATE as u64 / 2;
/// Close enough to call in time: well under what anyone could hear as a flam.
const TOLERANCE: f64 = RATE as f64 / 1000.0;

fn grid(bpm: f64, seconds: u32) -> (Vec<f64>, Vec<f64>) {
    let beats = (f64::from(seconds) * bpm / 60.0) as usize;
    let times = (0..beats).map(|i| i as f64 * 60_000.0 / bpm).collect();
    (times, vec![bpm; beats])
}

fn two_decks(name: &str, bpm_b: f64) -> Rig {
    let rig = Rig::with_decks(2);
    rig.load_on(0, &dc(&format!("{name}-a"), RATE * 20));
    rig.load_on(1, &dc(&format!("{name}-b"), RATE * 20));
    let (times, bpms) = grid(120.0, 20);
    rig.engine.set_beatgrid(0, &times, &bpms);
    let (times, bpms) = grid(bpm_b, 20);
    rig.engine.set_beatgrid(1, &times, &bpms);
    rig
}

fn position(rig: &Rig, deck: usize) -> u64 {
    rig.engine.snapshot(deck).unwrap().position_frames
}

/// How far `b` is off `a`'s beat, in frames of `a`, from -½ to ½ a beat.
fn phase_error(rig: &Rig, beat_b: f64) -> f64 {
    let beats_a = position(rig, 0) as f64 / BEAT as f64;
    let beats_b = position(rig, 1) as f64 / beat_b;
    let off = (beats_b - beats_a).rem_euclid(1.0);
    let off = if off > 0.5 { off - 1.0 } else { off };
    off * BEAT as f64
}

#[test]
fn a_follower_started_over_a_playing_master_lands_on_its_beat() {
    for (i, start_ms) in [1_337.0, 2_610.0, 4_444.0].into_iter().enumerate() {
        let rig = two_decks(&format!("s-join-{i}"), 120.0);
        rig.engine.set_sync(Some((0, 1)));
        rig.engine.play(0);
        let _ = rig.pull(RATE as usize);
        // Moved just before it starts, so its audio is not decoded yet when it does.
        rig.engine.seek(1, start_ms);
        rig.engine.play(1);
        let _ = rig.pull(RATE as usize);
        let error = phase_error(&rig, BEAT as f64);
        assert!(
            error.abs() < TOLERANCE,
            "from {start_ms} ms: off by {error} frames"
        );
    }
}

#[test]
fn a_follower_at_another_tempo_keeps_the_masters_and_its_beat() {
    let rig = two_decks("s-tempo", 126.0);
    rig.engine.set_sync(Some((0, 1)));
    let tempo = rig.engine.snapshot(1).unwrap().tempo;
    assert!(
        (tempo - 120.0 / 126.0).abs() < 1e-9,
        "takes the master's tempo: {tempo}"
    );
    rig.engine.play(0);
    let _ = rig.pull(RATE as usize / 2);
    rig.engine.seek(1, 3_100.0);
    rig.engine.play(1);
    let _ = rig.pull(RATE as usize * 3);
    let beat_b = f64::from(RATE) * 60.0 / 126.0;
    let error = phase_error(&rig, beat_b);
    assert!(error.abs() < TOLERANCE, "off by {error} frames");
}

#[test]
fn a_master_started_under_a_playing_follower_joins_its_beat() {
    let rig = two_decks("s-master-joins", 120.0);
    rig.engine.set_sync(Some((0, 1)));
    rig.engine.seek(1, 1_210.0);
    rig.engine.play(1);
    let _ = rig.pull(RATE as usize);
    let follower_before = position(&rig, 1);
    rig.engine.seek(0, 5_000.0);
    rig.engine.play(0);
    let _ = rig.pull(RATE as usize);
    let error = phase_error(&rig, BEAT as f64);
    assert!(error.abs() < TOLERANCE, "off by {error} frames");
    assert_eq!(
        position(&rig, 1),
        follower_before + u64::from(RATE),
        "the deck already playing was left alone"
    );
}

#[test]
fn a_cue_hold_plays_where_it_is_until_play_latches_it() {
    let rig = two_decks("s-cue", 120.0);
    rig.engine.set_sync(Some((0, 1)));
    rig.engine.play(0);
    let _ = rig.pull(RATE as usize);
    rig.engine.seek(1, 1_100.0);
    rig.engine.play_unsynced(1);
    let _ = rig.pull(RATE as usize / 2);
    assert!(
        phase_error(&rig, BEAT as f64).abs() > TOLERANCE,
        "the hold was not moved onto the beat"
    );
    rig.engine.align(1);
    let _ = rig.pull(RATE as usize / 2);
    let error = phase_error(&rig, BEAT as f64);
    assert!(
        error.abs() < TOLERANCE,
        "off by {error} frames once latched"
    );
}

#[test]
fn turning_sync_on_while_both_play_lines_them_up() {
    let rig = two_decks("s-toggle", 120.0);
    rig.engine.seek(1, 777.0);
    rig.engine.play_together(&[0, 1]);
    let _ = rig.pull(RATE as usize / 2);
    rig.engine.set_sync(Some((0, 1)));
    let _ = rig.pull(RATE as usize / 2);
    let error = phase_error(&rig, BEAT as f64);
    assert!(error.abs() < TOLERANCE, "off by {error} frames");
}

#[test]
fn playing_both_together_starts_them_in_phase() {
    let rig = two_decks("s-together", 120.0);
    rig.engine.set_sync(Some((0, 1)));
    rig.engine.seek(1, 3_210.0);
    rig.engine.play_together(&[0, 1]);
    let _ = rig.pull(RATE as usize);
    let error = phase_error(&rig, BEAT as f64);
    assert!(error.abs() < TOLERANCE, "off by {error} frames");
}

#[test]
fn without_sync_a_deck_starts_where_it_was() {
    let rig = two_decks("s-off", 120.0);
    rig.engine.play(0);
    let _ = rig.pull(RATE as usize / 2);
    rig.engine.seek(1, 1_100.0);
    rig.engine.play(1);
    let _ = rig.pull(4096);
    let at = position(&rig, 1);
    assert!(
        at > u64::from(RATE) * 11 / 10,
        "it plays on from 1.1 s: {at}"
    );
    assert!(phase_error(&rig, BEAT as f64).abs() > TOLERANCE);
}

#[test]
fn handing_the_master_over_moves_neither_deck() {
    let rig = two_decks("s-hand-over", 120.0);
    rig.engine.seek(1, 333.0);
    rig.engine.play_together(&[0, 1]);
    let _ = rig.pull(RATE as usize / 2);
    rig.engine.set_sync(Some((0, 1)));
    let _ = rig.pull(RATE as usize / 2);
    let before = (position(&rig, 0), position(&rig, 1));
    rig.engine.set_sync(Some((1, 0)));
    let _ = rig.pull(RATE as usize / 2);
    let half = u64::from(RATE) / 2;
    assert_eq!(
        (position(&rig, 0), position(&rig, 1)),
        (before.0 + half, before.1 + half)
    );
}

#[test]
fn a_follower_at_the_start_of_its_track_moves_forward_onto_the_beat() {
    // Each start puts the master's nearest beat behind deck B, before its first frame.
    for (i, master_ms) in [1_100.0, 1_200.0, 1_400.0].into_iter().enumerate() {
        let rig = two_decks(&format!("s-from-start-{i}"), 120.0);
        rig.engine.set_sync(Some((0, 1)));
        rig.engine.seek(0, master_ms);
        rig.engine.play(0);
        let _ = rig.pull(RATE as usize / 4);
        rig.engine.play(1);
        let _ = rig.pull(RATE as usize / 2);
        let error = phase_error(&rig, BEAT as f64);
        assert!(
            error.abs() < TOLERANCE,
            "with the master from {master_ms} ms: off by {error} frames"
        );
    }
}
