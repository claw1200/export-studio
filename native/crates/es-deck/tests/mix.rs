mod common;

use common::*;

#[test]
fn a_deck_gain_scales_only_that_deck() {
    let rig = Rig::with_decks(2);
    rig.load_on(0, &dc("gain-a", RATE * 3));
    rig.load_on(1, &dc("gain-b", RATE * 3));
    // Half each, so the sum stays clear of clipping.
    rig.engine.set_deck_gain(0, 0.5);
    rig.engine.set_deck_gain(1, 0.5);
    rig.engine.play_together(&[0, 1]);
    let both = rig.pull(8192);
    let full = value(both[both.len() - 2]);
    assert!(
        (full - 26_000).abs() < 100,
        "two halves make the whole: {full}"
    );

    rig.engine.set_deck_gain(1, 0.0);
    let one = rig.pull(8192);
    let alone = value(one[one.len() - 2]);
    assert!(
        (alone * 2 - full).abs() < 200,
        "silencing deck B should leave deck A alone: {alone} of {full}"
    );
    assert!(
        largest_step(&one) < 0.01,
        "the level glides rather than steps"
    );

    rig.engine.set_deck_gain(0, 0.0);
    let none = rig.pull(8192);
    assert_eq!(none[none.len() - 2], 0.0, "both faders down is silent");
}

#[test]
fn decks_started_together_stay_together() {
    let rig = Rig::with_decks(2);
    rig.load_on(0, &ramp("together-a", RATE * 4));
    rig.load_on(1, &ramp("together-b", RATE * 4));
    rig.engine.seek(0, 1000.0);
    rig.engine.seek(1, 2000.0);
    rig.settle_ring();
    rig.engine.play_together(&[0, 1]);
    let _ = rig.pull(4096);
    let a = rig.engine.snapshot(0).unwrap();
    let b = rig.engine.snapshot(1).unwrap();
    assert!(a.playing && b.playing);
    assert_eq!(
        b.position_frames - a.position_frames,
        u64::from(RATE),
        "the decks keep the second between them they started with"
    );
}

#[test]
fn starting_together_skips_an_empty_deck() {
    let rig = Rig::with_decks(2);
    rig.load_on(0, &dc("together-empty", RATE * 2));
    rig.engine.play_together(&[0, 1]);
    assert!(rig.engine.snapshot(0).unwrap().playing);
    assert!(!rig.engine.snapshot(1).unwrap().playing);
}

#[test]
fn a_killed_low_takes_the_bass_out_of_only_that_deck() {
    let rig = Rig::with_decks(2);
    // Far below the isolator's crossover: a DC level stands in for the deepest bass.
    rig.load_on(0, &dc("low-a", RATE * 3));
    rig.load_on(1, &dc("low-b", RATE * 3));
    rig.engine.set_deck_gain(0, 0.5);
    rig.engine.set_deck_gain(1, 0.5);
    rig.engine.set_deck_low(0, Some(1.0));
    rig.engine.play_together(&[0, 1]);
    let both = rig.pull(8192);
    let full = value(both[both.len() - 2]);
    assert!(
        (full - 26_000).abs() < 100,
        "with the low up the isolator passes the bass: {full}"
    );

    rig.engine.set_deck_low(0, Some(0.0));
    let killed = rig.pull(16_384);
    let left = value(killed[killed.len() - 2]);
    assert!(
        (left * 2 - full).abs() < 200,
        "killing deck A's low leaves deck B's bass alone: {left} of {full}"
    );
    assert!(
        largest_step(&killed) < 0.01,
        "the low glides out rather than stepping"
    );

    rig.engine.set_deck_low(0, None);
    let back = rig.pull(16_384);
    assert_eq!(
        value(back[back.len() - 2]),
        full,
        "out of the path, the deck plays untouched"
    );
}
