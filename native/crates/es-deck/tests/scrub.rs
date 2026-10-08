mod common;

use common::*;
use es_deck::FADE_FRAMES;

/// A 440 Hz tone at half scale.
fn tone(name: &str, seconds: u32) -> std::path::PathBuf {
    wav(name, RATE * seconds, |i| {
        ((i as f32 * 2.0 * std::f32::consts::PI * 440.0 / RATE as f32).sin() * 16_000.0) as i16
    })
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|v| v * v).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

const STEP_MS: f64 = 8.0;
const STEP_FRAMES: usize = (RATE as f64 * STEP_MS / 1000.0) as usize;

/// Drags from `from_ms` at normal speed for `steps` pointer events, pulling the audio in step.
/// Returns the audio and the last pointer position.
fn drag(rig: &Rig, from_ms: f64, steps: usize) -> (Vec<f32>, f64) {
    let mut audio = Vec::new();
    let mut at = from_ms;
    for k in 0..steps {
        at = from_ms + k as f64 * STEP_MS;
        rig.engine.scrub_to(0, at, k as f64 * STEP_MS);
        audio.extend(rig.pull(STEP_FRAMES));
    }
    (audio, at)
}

#[test]
fn a_paused_drag_is_heard_and_lands_exactly_under_the_pointer() {
    let rig = Rig::new();
    rig.load(&tone("scrubpaused", 6));
    rig.engine.seek(0, 1000.0);
    rig.settle_ring();
    rig.engine.scrub_begin(0);
    let (audio, last) = drag(&rig, 1000.0, 60);
    let moving = &audio[audio.len() / 2..];
    assert!(
        rms(moving) > 0.1,
        "a normal-speed drag is audible, rms {}",
        rms(moving)
    );

    rig.engine.scrub_end(0);
    let landed = (last * f64::from(RATE) / 1000.0).round() as u64;
    let snap = rig.engine.snapshot(0).unwrap();
    assert!(!snap.scrubbing && !snap.playing);
    assert_eq!(snap.position_frames, landed, "lands on the pointer");
    let after = rig.pull(4096);
    assert_eq!(
        after[after.len() - 2],
        0.0,
        "silent once let go while paused"
    );
    assert_eq!(rig.position(), landed, "and stays there");
}

#[test]
fn letting_go_of_a_drag_while_playing_plays_on_from_the_pointer() {
    let rig = Rig::new();
    rig.load(&tone("scrubplaying", 6));
    rig.engine.play(0);
    let _ = rig.pull(4096);
    rig.engine.scrub_begin(0);
    let (_, last) = drag(&rig, 2000.0, 20);
    rig.engine.scrub_end(0);
    let landed = (last * f64::from(RATE) / 1000.0).round() as u64;
    assert!(
        rig.engine.snapshot(0).unwrap().playing,
        "still playing after the drag"
    );
    rig.settle_ring();
    let _ = rig.pull(4096);
    let at = rig.position();
    assert!(
        at > landed && at <= landed + 4096,
        "plays on from {landed}, now at {at}"
    );
}

#[test]
fn taking_hold_of_a_playing_deck_silences_it_at_once_without_a_click() {
    let rig = Rig::new();
    rig.load(&dc("scrubhold", RATE * 4));
    rig.engine.play(0);
    let _ = rig.pull(4096);
    rig.engine.scrub_begin(0);
    let held = rig.pull(2048);
    let fade = 26_000.0 / 32_768.0 / f32::from(FADE_FRAMES) + 1e-4;
    assert!(
        largest_step(&held) <= fade,
        "stepped {}",
        largest_step(&held)
    );
    let tail = &held[held.len() - 512..];
    assert!(
        tail.iter().all(|v| v.abs() < 1e-3),
        "a held deck goes quiet within a few blocks, not a ring of playback later"
    );
}
