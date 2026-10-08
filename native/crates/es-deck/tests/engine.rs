mod common;

use common::*;
use es_deck::FADE_FRAMES;

/// The most a fade may move the level in one frame, for the constant test signal.
fn fade_step() -> f32 {
    26_000.0 / 32_768.0 / f32::from(FADE_FRAMES) + 1e-4
}

#[test]
fn play_and_pause_fade_instead_of_clicking() {
    let rig = Rig::new();
    rig.load(&dc("playpause", RATE * 3));
    rig.engine.play(0);
    let start = rig.pull(4096);
    assert!(
        largest_step(&start) <= fade_step(),
        "start stepped {}",
        largest_step(&start)
    );
    assert!(
        value(start[start.len() - 2]) > 25_000,
        "should reach full level"
    );
    rig.engine.pause(0);
    let stop = rig.pull(4096);
    assert!(
        largest_step(&stop) <= fade_step(),
        "stop stepped {}",
        largest_step(&stop)
    );
    assert_eq!(stop[stop.len() - 2], 0.0, "paused deck is silent");
}

#[test]
fn a_seek_while_playing_fades_out_and_back_in() {
    let rig = Rig::new();
    rig.load(&dc("seekfade", RATE * 5));
    rig.engine.play(0);
    let _ = rig.pull(4096);
    rig.engine.seek(0, 3000.0);
    let around = rig.pull(8192);
    assert!(
        largest_step(&around) <= fade_step(),
        "seek stepped {}",
        largest_step(&around)
    );
    let dips = around.chunks_exact(2).any(|f| f[0].abs() < 0.05);
    assert!(dips, "the join should pass through silence");
}

#[test]
fn a_seek_lands_on_the_exact_frame() {
    let rig = Rig::new();
    rig.load(&ramp("seekexact", RATE * 4));
    rig.engine.seek(0, 1234.5);
    let frame = (1234.5_f64 * f64::from(RATE) / 1000.0).round() as u64;
    assert_eq!(
        rig.position(),
        frame,
        "a paused seek shows where it landed at once"
    );
    rig.settle_ring();
    rig.engine.play(0);
    let out = rig.pull(4096);
    // The first audible frame is the seek target, and past the fade-in the counter continues
    // from it exactly.
    let first = first_audible(&out);
    let i = first + usize::from(FADE_FRAMES) + 10;
    assert_eq!(
        value(out[i * 2]),
        ((frame + (i - first) as u64) % 30_000) as i64
    );
}

#[test]
fn releasing_cue_lands_exactly_on_the_cue_point() {
    let rig = Rig::new();
    rig.load(&ramp("stopat", RATE * 4));
    rig.engine.play(0);
    let _ = rig.pull(8192);
    rig.engine.stop_at(0, 500.0);
    let _ = rig.pull(4096);
    assert_eq!(rig.position(), 24_000);
    assert!(!rig.engine.snapshot(0).unwrap().playing);
}

#[test]
fn playing_to_the_end_fades_and_stops() {
    let rig = Rig::new();
    let frames = RATE / 4;
    rig.load(&dc("end", frames));
    rig.engine.play(0);
    let out = rig.pull(frames as usize + 4096);
    assert!(
        largest_step(&out) <= fade_step(),
        "end stepped {}",
        largest_step(&out)
    );
    assert!(!rig.engine.snapshot(0).unwrap().playing);
    assert_eq!(out[out.len() - 2], 0.0);
}

#[test]
fn a_loop_seam_plays_straight_through() {
    let rig = Rig::new();
    rig.load(&ramp("loop", RATE * 4));
    let (from, to) = (24_000_u64, 30_000_u64);
    rig.engine.set_loop(0, 500.0, 625.0);
    rig.settle_ring();
    rig.engine.play(0);
    let out = rig.pull(20_000);
    let values: Vec<i64> = out.chunks_exact(2).map(|f| value(f[0])).collect();
    // After the fade-in, every frame is the next frame of the loop, wrapping at the out point.
    let open = first_audible(&out) + usize::from(FADE_FRAMES);
    let mut expected = values[open];
    for (i, &v) in values.iter().enumerate().skip(open) {
        assert_eq!(v, expected % 30_000, "frame {i}");
        expected += 1;
        if expected as u64 >= to {
            expected = from as i64;
        }
    }
    assert!(
        values.iter().skip(open).any(|&v| v == from as i64),
        "the loop wrapped"
    );
}

#[test]
fn exit_plays_on_past_the_out_point_and_reloop_returns_to_the_in_point() {
    let rig = Rig::new();
    rig.load(&ramp("loopexit", RATE * 4));
    let (from, to) = (24_000_u64, 30_000_u64);
    rig.engine.set_loop(0, 500.0, 625.0);
    rig.settle_ring();
    rig.engine.play(0);
    let _ = rig.pull(9000);

    rig.engine.set_looping(0, false);
    rig.settle_ring();
    let _ = rig.pull(12_000);
    let at = rig.position();
    assert!(at > to + 4000, "carried on past the out point, at {at}");
    let snap = rig.engine.snapshot(0).unwrap();
    assert!(!snap.looping);
    assert_eq!(
        (snap.loop_in_frames, snap.loop_out_frames),
        (from, to),
        "the loop is kept for RELOOP"
    );

    rig.engine.set_looping(0, true);
    assert_eq!(rig.position(), from, "RELOOP goes to the in point");
    rig.settle_ring();
    let out = rig.pull(16_000);
    // Past the fade out of where it was and the fade in at the in point.
    let values: Vec<i64> = out
        .chunks_exact(2)
        .skip(4096)
        .map(|f| value(f[0]))
        .collect();
    let outside = values
        .iter()
        .position(|&v| !(from as i64..to as i64).contains(&v));
    assert_eq!(
        outside,
        None,
        "stays inside the loop: {:?}",
        outside.map(|i| values[i])
    );
}

#[test]
fn the_newest_load_wins() {
    let rig = Rig::new();
    let a = ramp("load-a", RATE);
    let b = dc("load-b", RATE * 2);
    rig.engine.load(0, &a, 1);
    rig.engine.load(0, &b, 2);
    wait_for(|| rig.engine.snapshot(0).unwrap().loaded);
    let snap = rig.engine.snapshot(0).unwrap();
    assert_eq!(snap.load_id, 2);
    assert_eq!(snap.total_frames, u64::from(RATE * 2));
}

#[test]
fn a_missing_file_reports_an_error() {
    let rig = Rig::new();
    rig.engine
        .load(0, std::path::Path::new("/nonexistent/track.mp3"), 7);
    wait_for(|| !rig.events.lock().unwrap().is_empty());
    let events = rig.events.lock().unwrap();
    assert!(matches!(
        events[0],
        es_deck::DeckEvent::Error { load_id: 7, .. }
    ));
}
