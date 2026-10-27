//! Syncing a playlist onto a disposable stick with the real exporter, then reading it back.

use std::path::Path;

use rb_library::fixture::{self, Rows};
use rb_library::{Guard, Library};
use es_engine::import::{import_files, Options};
use es_engine::sync::{sync_volume, Progress};
use std::cell::RefCell;

fn wav(path: &Path, seconds: u32) {
    let data = 8000u32 * 2 * seconds;
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
    bytes.extend(vec![seconds as u8; data as usize]);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_playlist_synced_to_a_stick_reads_back_with_its_tracks() {
    es_engine::sync::exporter().expect("Build es-usb and set EXPORTSTUDIO_USB_EXPORT before running sync tests");
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        Rows(&conn).playlist("10", "Mix", 0).unwrap();
    }
    let mut lib = Library::open_live(&path, Guard::Off).unwrap();
    let share = lib.share_dir();
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let mut files = Vec::new();
    for (name, seconds) in [("One.wav", 1), ("Two.wav", 2)] {
        let file = music.join(name);
        wav(&file, seconds);
        files.push(file.to_string_lossy().into_owned());
    }
    let out = import_files(&mut lib, &share, &files, &Options { artwork: false, playlist: Some("10".into()) }).unwrap();
    assert_eq!(out.added.len(), 2);

    let volume = dir.path().join("STICK");
    std::fs::create_dir_all(&volume).unwrap();
    let phases = RefCell::new(Vec::new());
    let result = sync_volume(&lib, &volume, &["10".to_owned()], None, None, &|p| {
        if let Progress::Step { phase, .. } = p {
            phases.borrow_mut().push(phase);
        }
    })
    .unwrap();
    assert_eq!((result.tracks, result.playlists), (2, 1));
    assert!(!phases.borrow().is_empty());

    let device = rb_device::read::read(&volume).unwrap().unwrap();
    assert_eq!(device.tracks.len(), 2);
    let mix = device.playlists.iter().find(|p| p.name == "Mix").unwrap();
    assert_eq!(mix.track_ids.len(), 2);
    let review = es_engine::sync::inspect_volume(&lib, &volume, &["10".to_owned()], None).unwrap();
    assert!(review.diagnostics.is_empty());
    assert_eq!(review.plan.as_ref().unwrap().tracks, 2);

    let manifest_path = volume.join("PIONEER/exportstudio/manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let tracks_before = manifest["tracks"].clone();
    let playlists_before = manifest["playlists"].clone();
    manifest.as_object_mut().unwrap().remove("generation");
    for playlist in manifest["playlists"].as_array_mut().unwrap() {
        playlist.as_object_mut().unwrap().remove("library_entries");
    }
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let foreign_path = volume.join("PIONEER/rbxport/manifest.json");
    std::fs::create_dir_all(foreign_path.parent().unwrap()).unwrap();
    std::fs::write(&foreign_path, b"foreign application state").unwrap();
    sync_volume(&lib, &volume, &["10".to_owned()], None, None, &|_| {}).unwrap();
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    for (before, after) in tracks_before.as_array().unwrap().iter().zip(after["tracks"].as_array().unwrap()) {
        assert_eq!(before["export_id"], after["export_id"]);
    }
    assert_eq!(after["tracks"].as_array().unwrap().len(), 2);
    assert_eq!(after["playlists"][0]["export_id"], playlists_before[0]["export_id"]);
    assert_eq!(std::fs::read(&foreign_path).unwrap(), b"foreign application state");
    assert!(!volume.join(".exportstudio-publication").exists());
}

#[test]
fn syncing_nothing_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    let lib = Library::open_live(&path, Guard::Off).unwrap();
    let volume = dir.path().join("STICK");
    std::fs::create_dir_all(&volume).unwrap();
    assert!(sync_volume(&lib, &volume, &[], None, None, &|_| {}).is_err());
}

#[test]
fn a_converted_sync_leaves_the_library_alone_and_the_next_changes_nothing() {
    es_engine::sync::exporter().expect("Build es-usb and set EXPORTSTUDIO_USB_EXPORT before running sync tests");
    if es_engine::convert::ffmpeg_path().is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        Rows(&conn).playlist("10", "Mix", 0).unwrap();
    }
    let mut lib = Library::open_live(&path, Guard::Off).unwrap();
    let share = lib.share_dir();
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let file = music.join("One.wav");
    wav(&file, 1);
    let out = import_files(&mut lib, &share, &[file.to_string_lossy().into_owned()], &Options { artwork: false, playlist: Some("10".into()) }).unwrap();
    let volume = dir.path().join("STICK");
    std::fs::create_dir_all(&volume).unwrap();
    let mp3 = es_engine::convert::clean_preset(&serde_json::json!({ "format": "mp3", "bitrate": 128 })).unwrap();
    let playlists = ["10".to_owned()];

    sync_volume(&lib, &volume, &playlists, None, Some(&mp3), &|_| {}).unwrap();
    let device = rb_device::read::read(&volume).unwrap().unwrap();
    assert!(device.tracks[0].path.ends_with("/One.mp3"), "{}", device.tracks[0].path);
    assert_eq!(device.tracks[0].bitrate, 128);
    assert_eq!(lib.track_files(&out.ids[0]).unwrap().unwrap().folder_path, file.to_string_lossy().replace('\\', "/"));
    assert_eq!(std::fs::read_dir(&music).unwrap().count(), 1, "nothing is written beside the library's file");

    let plan = es_engine::sync::inspect_volume(&lib, &volume, &playlists, Some(&mp3)).unwrap().plan.unwrap();
    assert_eq!((plan.copy, plan.convert, plan.keep), (0, 0, 1));
    let phases = RefCell::new(Vec::new());
    sync_volume(&lib, &volume, &playlists, None, Some(&mp3), &|p| {
        if let Progress::Step { phase, .. } = p {
            phases.borrow_mut().push(phase);
        }
    })
    .unwrap();
    assert!(!phases.borrow().iter().any(|p| p == "convert" || p == "copy"), "{:?}", phases.borrow());

    let plan = es_engine::sync::inspect_volume(&lib, &volume, &playlists, None).unwrap().plan.unwrap();
    assert_eq!((plan.copy, plan.convert), (1, 0), "without conversion the original goes back");
}
