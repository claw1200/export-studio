//! Keeping time: a deck's audio lands where the playhead says, through seeks, starts and moves
//! onto and off the time-stretcher. Two decks played together only stay together if each does.
mod common;

use common::*;
use es_deck::FADE_FRAMES;

const BURST_EVERY: u32 = RATE / 2;
const BURST_LEN: u32 = 480;
/// Close enough to call on time: well under what anyone could hear as a flam.
const TOLERANCE_MS: f64 = 1.0;

/// A deck set up a particular way before it plays: its name, tempo and setup.
type Path<'a> = (&'a str, f64, &'a dyn Fn(&Rig));

/// 1 kHz bursts, 10 ms long, every half second.
fn bursts(name: &str, frames: u32) -> std::path::PathBuf {
    wav(name, frames, |i| {
        if i % BURST_EVERY < BURST_LEN {
            ((f64::from(i) * 2.0 * std::f64::consts::PI * 1000.0 / f64::from(RATE)).sin()
                * 20_000.0) as i16
        } else {
            0
        }
    })
}

/// Output frames where a burst starts after silence.
fn onsets(out: &[f32]) -> Vec<usize> {
    let mut found = Vec::new();
    let mut quiet = 0usize;
    for (i, frame) in out.chunks_exact(2).enumerate() {
        if frame[0].abs() > 0.15 {
            if quiet > 2000 {
                found.push(i);
            }
            quiet = 0;
        } else {
            quiet += 1;
        }
    }
    found
}

/// Plays, does `switch` three seconds in, and returns how far each burst after it is off the
/// beat the bursts before it set, in ms.
fn lag_after(name: &str, tempo: f64, before: impl Fn(&Rig), switch: impl Fn(&Rig)) -> Vec<f64> {
    let rig = Rig::new();
    rig.load(&bursts(name, RATE * 12));
    before(&rig);
    rig.settle_ring();
    rig.engine.play(0);
    let mut out = rig.pull(RATE as usize * 3);
    let at = out.len() / 2;
    switch(&rig);
    out.extend(rig.pull(RATE as usize * 3));
    let found = onsets(&out);
    let step = f64::from(BURST_EVERY) / tempo;
    let first = found[0] as f64;
    found
        .iter()
        .filter(|&&o| o > at)
        .map(|&o| {
            let k = ((o as f64 - first) / step).round();
            (o as f64 - first - k * step) / f64::from(RATE) * 1000.0
        })
        .collect()
}

fn assert_on_time(what: &str, lags: &[f64]) {
    assert!(
        lags.len() >= 4,
        "{what}: too few bursts heard after the switch: {lags:?}"
    );
    for lag in lags {
        assert!(
            lag.abs() < TOLERANCE_MS,
            "{what}: a burst was {lag:+.2} ms off the beat: {lags:?}"
        );
    }
}

#[test]
fn shifting_the_key_onto_the_stretcher_keeps_time() {
    let lags = lag_after("t-key-on", 1.0, |_| {}, |r| r.engine.set_key_shift(0, 1));
    assert_on_time("key 0 to +1", &lags);
}

#[test]
fn clearing_the_key_shift_off_the_stretcher_keeps_time() {
    let lags = lag_after(
        "t-key-off",
        1.0,
        |r| r.engine.set_key_shift(0, 1),
        |r| r.engine.set_key_shift(0, 0),
    );
    assert_on_time("key +1 to 0", &lags);
}

#[test]
fn switching_master_tempo_keeps_time_both_ways() {
    let on = lag_after(
        "t-mt-on",
        1.04,
        |r| r.engine.set_tempo(0, 1.04),
        |r| r.engine.set_master_tempo(0, true),
    );
    assert_on_time("master tempo on", &on);
    let off = lag_after(
        "t-mt-off",
        1.04,
        |r| {
            r.engine.set_tempo(0, 1.04);
            r.engine.set_master_tempo(0, true);
        },
        |r| r.engine.set_master_tempo(0, false),
    );
    assert_on_time("master tempo off", &off);
}

#[test]
fn every_path_starts_on_the_frame_it_was_sent_to() {
    let paths: [Path; 4] = [
        ("t-start-direct", 1.0, &|_| {}),
        ("t-start-key", 1.0, &|r| r.engine.set_key_shift(0, 1)),
        ("t-start-vari", 1.04, &|r| r.engine.set_tempo(0, 1.04)),
        ("t-start-mt", 1.04, &|r| {
            r.engine.set_tempo(0, 1.04);
            r.engine.set_master_tempo(0, true);
        }),
    ];
    for (name, tempo, setup) in paths {
        let rig = Rig::new();
        rig.load(&bursts(name, RATE * 6));
        setup(&rig);
        // A quarter of a second before the second burst.
        let lead = RATE / 4;
        rig.engine.seek(
            0,
            f64::from(BURST_EVERY * 2 - lead) * 1000.0 / f64::from(RATE),
        );
        rig.settle_ring();
        rig.engine.play(0);
        let out = rig.pull(RATE as usize);
        let first = out
            .chunks_exact(2)
            .position(|f| f[0].abs() > 0.15)
            .expect("no burst");
        let off = (first as f64 - f64::from(lead) / tempo) / f64::from(RATE) * 1000.0;
        assert!(
            off.abs() < TOLERANCE_MS,
            "{name}: burst heard {off:+.2} ms from where it belongs"
        );
    }
}

#[test]
fn the_first_callback_after_a_seek_plays() {
    let rig = Rig::new();
    rig.load(&dc("t-seek-first", RATE * 4));
    rig.engine.seek(0, 1000.0);
    rig.settle_ring();
    rig.engine.play(0);
    let out = rig.sink.pull(256);
    assert_eq!(
        out.chunks_exact(2).position(|f| f[0] != 0.0),
        Some(0),
        "the new position's audio was queued behind the old, so there is no silent buffer"
    );
}

#[test]
fn master_tempo_starts_even_at_the_fastest_tempo() {
    let rig = Rig::new();
    rig.load(&dc("t-mt-fastest", RATE * 6));
    rig.engine.set_tempo(0, 2.0);
    rig.engine.set_master_tempo(0, true);
    rig.engine.seek(0, 500.0);
    rig.settle_ring();
    rig.engine.play(0);
    let out = rig.pull(RATE as usize / 2);
    assert!(
        out.chunks_exact(2).any(|f| f[0].abs() > 0.3),
        "the ring holds enough to prime the stretcher at double speed"
    );
}

#[test]
fn a_deck_moved_just_before_starting_together_still_starts_with_the_other() {
    let rig = Rig::with_decks(2);
    rig.load_on(0, &dc("t-together-a", RATE * 6));
    rig.load_on(1, &dc("t-together-b", RATE * 6));
    // Deck B is moved and started at once, before its new position could have been decoded.
    rig.engine.seek(1, 2000.0);
    rig.engine.play_together(&[0, 1]);
    let _ = rig.pull(4096);
    let a = rig.engine.snapshot(0).unwrap().position_frames;
    let b = rig.engine.snapshot(1).unwrap().position_frames;
    assert_eq!(
        b - a,
        u64::from(RATE) * 2,
        "both decks moved the same distance from where they started"
    );
}

#[test]
fn a_nudge_while_playing_keeps_time() {
    // A whole burst later: the bursts keep landing on the same beat if the jump took no time.
    let lags = lag_after("t-nudge-play", 1.0, |_| {}, |r| r.engine.nudge(0, 500.0));
    assert_on_time("nudge forward a beat", &lags);
}

#[test]
fn a_nudge_while_paused_moves_by_exactly_that_much() {
    let rig = Rig::new();
    rig.load(&dc("t-nudge-paused", RATE * 4));
    rig.engine.seek(0, 1000.0);
    rig.engine.nudge(0, -250.0);
    assert_eq!(rig.position(), u64::from(RATE) * 3 / 4);
    rig.engine.nudge(0, 10_000.0);
    assert_eq!(rig.position(), u64::from(RATE) * 4, "never past the end");
}

#[test]
fn nudging_one_deck_onto_another_lines_them_up_exactly() {
    let rig = Rig::with_decks(2);
    rig.load_on(0, &dc("t-line-up-a", RATE * 8));
    rig.load_on(1, &dc("t-line-up-b", RATE * 8));
    rig.engine.seek(0, 1000.0);
    rig.engine.seek(1, 1100.0);
    rig.engine.play_together(&[0, 1]);
    let _ = rig.pull(8192);
    // Deck B is 100 ms ahead of deck A; bring it back while both play.
    rig.engine.nudge(1, -100.0);
    let _ = rig.pull(8192);
    let a = rig.engine.snapshot(0).unwrap().position_frames;
    let b = rig.engine.snapshot(1).unwrap().position_frames;
    assert_eq!(
        b, a,
        "deck B landed exactly level with deck A, with no time lost to the jump"
    );
}

#[test]
fn a_paused_deck_moved_twice_plays_from_the_second_move() {
    let rig = Rig::new();
    rig.load(&ramp("t-moved-twice", RATE * 4));
    // Both moves queue before the decode thread reaches the first.
    rig.engine.seek(0, 1000.0);
    rig.engine.seek(0, 2000.0);
    rig.engine.play(0);
    let out = rig.pull(4096);
    // Past the fade-in, where the frame plays at full level.
    let past_fade = first_audible(&out) + usize::from(FADE_FRAMES) * 2;
    let heard = value(out[past_fade * 2]);
    let expected = (i64::from(RATE) * 2 + past_fade as i64 - first_audible(&out) as i64) % 30_000;
    assert!(
        (heard - expected).abs() <= 1,
        "playing from 2 s, not 1 s: heard {heard}, expected {expected}"
    );
}
