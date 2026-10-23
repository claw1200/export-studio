use std::path::{Path, PathBuf};

use rb_library::fixture::{self, Rows};
use rb_library::{Guard, Library};
use es_engine::import::{import_files, Options};
use tempfile::TempDir;

/// A silent mono 16-bit WAV, long enough to read.
fn wav(path: &Path) {
    let samples = 8000u32;
    let data = samples * 2;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(8000u32.to_le_bytes());
    bytes.extend(16000u32.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data.to_le_bytes());
    bytes.extend(vec![0u8; data as usize]);
    std::fs::write(path, bytes).unwrap();
}

struct Scene {
    dir: TempDir,
    lib: Library,
}

fn scene() -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        Rows(&conn).playlist("10", "Mix", 0).unwrap();
    }
    let lib = Library::open_live(&path, Guard::Off).unwrap();
    Scene { dir, lib }
}

fn file(s: &Scene, name: &str) -> String {
    let path: PathBuf = s.dir.path().join(name);
    wav(&path);
    path.to_string_lossy().into_owned()
}

fn run(s: &mut Scene, paths: &[String], playlist: Option<&str>) -> es_engine::import::Outcome {
    let share = s.dir.path().join("share");
    import_files(&mut s.lib, &share, paths, &Options { artwork: true, playlist: playlist.map(str::to_owned) }).unwrap()
}

#[test]
fn a_new_file_becomes_a_track_with_its_file_facts() {
    let mut s = scene();
    let path = file(&s, "Some Song.wav");
    let out = run(&mut s, std::slice::from_ref(&path), None);
    assert_eq!((out.ids.len(), out.added.len(), out.skipped, out.failed.len()), (1, 1, 0, 0));

    let track = s.lib.tracks(None).unwrap().remove(0);
    assert_eq!((track.title.as_str(), track.file_type, track.sample_rate, track.length), ("Some Song", 11, 8000, 1));
    assert_eq!(Path::new(&track.folder_path), Path::new(&path));
    let conn = fixture::connect(s.lib.path()).unwrap();
    let (link, device, master, auto): (i64, String, String, String) = conn
        .query_row("SELECT ContentLink, DeviceID, MasterDBID, HotCueAutoLoad FROM djmdContent", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!((link, device.as_str(), master.as_str(), auto.as_str()), (77, "dev1", "master-1", "on"));
}

#[test]
fn importing_twice_adds_nothing_but_still_fills_the_playlist() {
    let mut s = scene();
    let path = file(&s, "a.wav");
    run(&mut s, std::slice::from_ref(&path), None);
    let again = run(&mut s, &[path.clone(), path], Some("10"));
    assert_eq!((again.added.len(), again.skipped, again.ids.len()), (0, 1, 1));
    assert_eq!(s.lib.tracks(None).unwrap().len(), 1);
    assert_eq!(s.lib.playlist_track_ids("10").unwrap().len(), 1);
}

#[test]
fn a_removed_track_returns_as_the_same_row_and_not_in_its_old_playlists() {
    let mut s = scene();
    let path = file(&s, "b.wav");
    let first = run(&mut s, std::slice::from_ref(&path), Some("10")).ids;
    s.lib.write(|tx| tx.remove_tracks(&first)).unwrap();
    assert!(s.lib.playlist_track_ids("10").unwrap().is_empty());

    let back = run(&mut s, &[path], None);
    assert_eq!(back.ids, first);
    assert_eq!(back.added, first);
    assert!(s.lib.playlist_track_ids("10").unwrap().is_empty(), "importing must not put a track back in a playlist");
    assert_eq!(s.lib.tracks(None).unwrap().len(), 1);
}

#[test]
fn files_that_cannot_be_imported_are_reported() {
    let mut s = scene();
    let out = run(&mut s, &["/nowhere/x.mp3".into(), "relative.wav".into(), "/tmp/notes.txt".into()], None);
    let errors: Vec<_> = out.failed.iter().map(|f| f.error.as_str()).collect();
    assert_eq!(errors, ["File not found", "File not found", "rekordbox can't import .txt files"]);
    assert!(out.ids.is_empty());
}

#[test]
fn a_bad_playlist_rolls_the_whole_import_back() {
    let mut s = scene();
    let path = file(&s, "c.wav");
    let share = s.dir.path().join("share");
    let result = import_files(&mut s.lib, &share, &[path], &Options { artwork: false, playlist: Some("nope".into()) });
    assert!(result.is_err());
    assert!(s.lib.tracks(None).unwrap().is_empty());
}
