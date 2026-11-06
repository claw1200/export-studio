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
