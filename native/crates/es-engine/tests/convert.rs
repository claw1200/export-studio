//! Converting with the real ffmpeg. Skipped when it is not installed.

use std::path::Path;

use rb_anlz::decode::{self, Tags};
use rb_library::fixture;
use rb_library::{Guard, Library};
use es_engine::analysis::{apply_analysis, Analysis, Options as AnalysisOptions};
use es_engine::convert::{clean_preset, convert_tracks, delay_offset_ms, ffmpeg_path, Mode};
use es_engine::import::{import_files, Options};
use serde_json::json;

fn wav(path: &Path) {
    let data = 16000u32;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend([1, 0, 1, 0]);
    bytes.extend(8000u32.to_le_bytes());
    bytes.extend(16000u32.to_le_bytes());
    bytes.extend([2, 0, 16, 0]);
    bytes.extend(b"data");
    bytes.extend(data.to_le_bytes());
    bytes.extend((0..data).map(|i| (i % 200) as u8));
    std::fs::write(path, bytes).unwrap();
}

fn scene() -> (tempfile::TempDir, Library, String, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    let mut lib = Library::open_live(&path, Guard::Off).unwrap();
    let share = lib.share_dir();
    let file = dir.path().join("Song.wav");
    wav(&file);
    let id = import_files(&mut lib, &share, &[file.to_string_lossy().into_owned()], &Options { artwork: false, playlist: None }).unwrap().ids[0].clone();
    lib.write(|tx| tx.set_hot_cue(&id, 1, 500)).unwrap();
    (dir, lib, id, file)
}

#[test]
fn update_repoints_the_track_and_copy_makes_a_new_one_with_the_cues() {
    if ffmpeg_path().is_err() {
        return;
    }
    let (dir, mut lib, id, wav_file) = scene();
    let share = lib.share_dir();
    let preset = clean_preset(&json!({ "format": "flac", "bitDepth": 16, "sampleRate": 44100 })).unwrap();

    let copied = convert_tracks(&mut lib, &share, std::slice::from_ref(&id), &preset, Mode::Copy, &|_, _, _| {});
    assert!(copied.failed.is_empty(), "{:?}", copied.failed);
    assert_ne!(copied.ids[0], id);
    let copy = lib.track_files(&copied.ids[0]).unwrap().unwrap();
    assert!(copy.folder_path.ends_with("Song.flac") && Path::new(&copy.folder_path).is_file());
    assert_eq!(lib.cues(&copied.ids[0]).unwrap().len(), 1);
    assert_eq!(Path::new(&lib.track_files(&id).unwrap().unwrap().folder_path), wav_file);

    let moved = convert_tracks(&mut lib, &share, std::slice::from_ref(&id), &preset, Mode::UpdateDelete, &|_, _, _| {});
    assert!(moved.failed.is_empty(), "{:?}", moved.failed);
    assert_eq!(moved.ids, std::slice::from_ref(&id));
    assert_eq!(moved.originals.iter().map(Path::new).collect::<Vec<_>>(), [wav_file.as_path()]);
    assert!(lib.track_files(&id).unwrap().unwrap().folder_path.ends_with("Song (1).flac"));
    assert!(wav_file.is_file(), "the old file is left for the caller to remove");
    drop(dir);
}

#[test]
fn a_delay_moves_the_grid_and_the_detail_waveforms() {
    if ffmpeg_path().is_err() {
        return;
    }
    let (_dir, mut lib, id, wav_file) = scene();
    let share = lib.share_dir();
    let analysis = Analysis {
        beats: vec![(1, 12800, 0), (2, 12800, 469), (3, 12800, 938)],
        waveform: (0..8 * 150).map(|i| (i * 7 % 251) as u8).collect(),
        key: None,
        bpm: 128.0,
        duration: 1.0,
    };
    apply_analysis(&mut lib, &share, &id, &analysis, AnalysisOptions { overwrite: true, grid: true, key: false }).unwrap();
    let relative = lib.track_files(&id).unwrap().unwrap().analysis_path.unwrap();
    let read = |suffix: &str| Tags::read(&rb_anlz::file(&share, &relative, suffix).unwrap()).unwrap();
    let before = decode::rgb(&read("EXT")).unwrap();
    let before_bands = decode::bands(&read("2EX"), "PWV7").unwrap();

    let preset = clean_preset(&json!({ "format": "mp3", "bitrate": 128 })).unwrap();
    let out = convert_tracks(&mut lib, &share, std::slice::from_ref(&id), &preset, Mode::Update, &|_, _, _| {});
    assert!(out.failed.is_empty(), "{:?}", out.failed);
    let mp3 = lib.track_files(&id).unwrap().unwrap().folder_path;
    let offset = delay_offset_ms(&wav_file, Path::new(&mp3)).unwrap();
    assert!(offset > 0, "an MP3 starts later than its WAV");

    assert_eq!(decode::beatgrid(&read("DAT")).unwrap().times, [0, 469, 938].map(|t| t + u32::try_from(offset).unwrap()));
    let columns = usize::try_from((offset * 150 + 500) / 1000).unwrap();
    let after = decode::rgb(&read("EXT")).unwrap();
    assert_eq!(after.length, before.length + columns);
    assert!(after.heights[..columns].iter().chain(&after.colors[..columns * 3]).all(|v| *v == 0), "the new start is silent");
    assert_eq!(after.heights[columns..], before.heights[..]);
    assert_eq!(after.colors[columns * 3..], before.colors[..]);
    let bands = decode::bands(&read("2EX"), "PWV7").unwrap();
    assert_eq!(bands.length, before_bands.length + columns);
    assert!(bands.bands[..columns * 3].iter().all(|v| *v == 0), "the new start is silent");
    assert_eq!(bands.bands[columns * 3..], before_bands.bands[..]);
}

#[test]
fn a_missing_file_fails_that_track_only() {
    let (_dir, mut lib, id, file) = scene();
    let share = lib.share_dir();
    std::fs::remove_file(&file).unwrap();
    let preset = clean_preset(&json!({ "format": "mp3", "bitrate": 128 })).unwrap();
    let out = convert_tracks(&mut lib, &share, std::slice::from_ref(&id), &preset, Mode::Update, &|_, _, _| {});
    assert!(out.ids.is_empty());
    assert_eq!(out.failed, [(id, "The audio file is missing".to_owned())]);
}
