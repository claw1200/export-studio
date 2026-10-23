use std::path::Path;

use rb_library::fixture;
use rb_library::{Guard, Library};
use es_engine::import::{import_files, Options};
use es_engine::relocate::{find_missing, relocate};

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
    bytes.extend(vec![0u8; data as usize]);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_moved_file_is_found_and_the_track_pointed_at_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    let mut lib = Library::open_live(&path, Guard::Off).unwrap();
    let share = lib.share_dir();
    let old = dir.path().join("Old Song.wav");
    wav(&old);
    let out = import_files(&mut lib, &share, &[old.to_string_lossy().into_owned()], &Options { artwork: false, playlist: None }).unwrap();
    let id = out.ids[0].clone();

    let new_dir = dir.path().join("moved/deeper");
    std::fs::create_dir_all(&new_dir).unwrap();
    std::fs::rename(&old, new_dir.join("old song.WAV")).unwrap();

    let found = find_missing(&lib, std::slice::from_ref(&id), &dir.path().join("moved")).unwrap();
    assert_eq!(found.matches.len(), 1);
    let target = found.matches[0].path.clone().expect("found by name, ignoring case");

    let moved = relocate(&mut lib, &share, &[(id.clone(), target.clone())]).unwrap();
    assert_eq!(moved, std::slice::from_ref(&id));
    let files = lib.track_files(&id).unwrap().unwrap();
    assert_eq!(Path::new(&files.folder_path), Path::new(&target));
    assert_eq!(files.file_name, "old song.WAV");
    assert!(find_missing(&lib, &[id], &dir.path().join("moved")).unwrap().matches.is_empty());
}

#[test]
fn relocating_to_a_missing_file_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    let mut lib = Library::open_live(&path, Guard::Off).unwrap();
    let share = lib.share_dir();
    let file = dir.path().join("a.wav");
    wav(&file);
    let id = import_files(&mut lib, &share, &[file.to_string_lossy().into_owned()], &Options { artwork: false, playlist: None }).unwrap().ids[0].clone();
    let err = relocate(&mut lib, &share, &[(id.clone(), dir.path().join("nope.wav").to_string_lossy().into_owned())]).unwrap_err();
    assert!(err.to_string().contains("Can't find nope.wav"));
    assert_eq!(Path::new(&lib.track_files(&id).unwrap().unwrap().folder_path), file);
}
