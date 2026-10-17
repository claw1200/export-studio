//! Syncing a playlist onto a stick with the real exporter, then reading the stick back. Skipped without `es-usb`.

use std::path::Path;

use es_db::fixture::{self, Rows};
use es_db::{Guard, Library};
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
    if es_engine::sync::exporter().is_err() {
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
    let result = sync_volume(&lib, &volume, &["10".to_owned()], false, &|p| {
        if let Progress::Step { phase, .. } = p {
            phases.borrow_mut().push(phase);
        }
    })
    .unwrap();
    assert_eq!((result.tracks, result.playlists), (2, 1));
    assert!(!phases.borrow().is_empty());

    let device = es_device::read::read(&volume).unwrap().unwrap();
    assert_eq!(device.tracks.len(), 2);
    let mix = device.playlists.iter().find(|p| p.name == "Mix").unwrap();
    assert_eq!(mix.track_ids.len(), 2);

    let paths = ["PIONEER/rekordbox/export.pdb", "PIONEER/rekordbox/exportLibrary.db", "PIONEER/exportstudio/manifest.json"];
    let before: Vec<_> = paths.iter().map(|path| std::fs::read(volume.join(path)).unwrap()).collect();
    let review = es_engine::sync::review_volume(&lib, &volume, &["10".to_owned()]).unwrap();
    assert_eq!(review["recoveryRequired"], false);
    assert!(review["conflicts"].as_array().unwrap().is_empty());
    assert!(!review["token"].as_str().unwrap().is_empty());
    let after: Vec<_> = paths.iter().map(|path| std::fs::read(volume.join(path)).unwrap()).collect();
    assert_eq!(after, before, "review must not publish or rewrite USB data");
    let decisions = serde_json::json!({ "token": review["token"], "resolutions": {} });
    es_engine::sync::sync_volume_reviewed(&lib, &volume, &["10".to_owned()], false, Some(&decisions), &|_| {}).unwrap();
}

#[test]
fn syncing_nothing_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    let lib = Library::open_live(&path, Guard::Off).unwrap();
    let volume = dir.path().join("STICK");
    std::fs::create_dir_all(&volume).unwrap();
    assert!(sync_volume(&lib, &volume, &[], false, &|_| {}).is_err());
}
