//! Builds a disposable stick with the native exporter and reads it back.

use std::path::{Path, PathBuf};
use std::process::Command;

use rb_device::{read::read, read::read_kind, LibraryKind};
use serde_json::json;

fn exporter() -> Option<PathBuf> {
    let name = if cfg!(windows) { "es-usb.exe" } else { "es-usb" };
    let path = std::env::var_os("EXPORTSTUDIO_USB_EXPORT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug").join(name),
        PathBuf::from,
    );
    assert!(path.is_file(), "Build es-usb and set EXPORTSTUDIO_USB_EXPORT before running stick tests");
    Some(path)
}

fn wav(path: &Path) {
    let data = 16000u32 * 2;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend([1, 0, 1, 0]);
    bytes.extend(16000u32.to_le_bytes());
    bytes.extend(32000u32.to_le_bytes());
    bytes.extend([2, 0, 16, 0]);
    bytes.extend(b"data");
    bytes.extend(data.to_le_bytes());
    bytes.extend(vec![0u8; data as usize]);
    std::fs::write(path, bytes).unwrap();
}

fn stick() -> Option<(tempfile::TempDir, PathBuf)> {
    let exporter = exporter()?;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let artwork = source.join("cover.jpg");
    std::fs::write(&artwork, b"artwork fixture").unwrap();
    let volume = dir.path().join("STICK");
    std::fs::create_dir_all(&volume).unwrap();
    let mut tracks = Vec::new();
    for (id, title, artist) in [(101u64, "First", "Alpha"), (102, "Second", "Beta")] {
        let file = source.join(format!("{title}.wav"));
        wav(&file);
        tracks.push(json!({
            "id": id, "sourcePath": file, "title": title, "artist": artist, "album": "Album", "genre": "House",
            "bpmX100": 12800, "durationSec": 1, "rating": 3, "year": 2020, "bitrate": 512, "sampleRate": 16000,
            "fileSize": 32044, "metadata": { "trackNumber": 2, "bitDepth": 16, "analysed": 105 },
            "artwork": if id == 101 { Some(&artwork) } else { None },
            "cues": [
                { "kind": 0, "timeMs": 500 },
                { "kind": 1, "timeMs": 100, "colorCode": 10, "comment": "drop" }
            ],
        }));
    }
    let job = json!({
        "schemaVersion": 2, "destination": volume, "dbId": 4242, "tracks": tracks,
        "playlists": [
            { "id": 1, "name": "Folder", "parentId": 0, "folder": true, "trackIds": [] },
            { "id": 2, "name": "Set", "parentId": 1, "folder": false, "trackIds": [102, 101] },
        ],
    });
    let job_path = dir.path().join("job.json");
    std::fs::write(&job_path, job.to_string()).unwrap();
    let out = Command::new(exporter).arg(&job_path).output().unwrap();
    assert!(out.status.success(), "export failed: {}", String::from_utf8_lossy(&out.stdout));
    Some((dir, volume))
}

#[test]
fn a_stick_reads_back_with_its_playlists_in_order() {
    let Some((_dir, volume)) = stick() else { return };
    let device = read(&volume).unwrap().expect("a library");
    assert_eq!(device.kind, LibraryKind::OneLibrary);
    assert_eq!(device.tracks.len(), 2);
    let first = device.tracks.iter().find(|t| t.title == "First").unwrap();
    assert_eq!((first.artist.as_str(), first.album.as_str(), first.genre.as_str()), ("Alpha", "Album", "House"));
    assert_eq!((first.bpm_x100, first.sample_rate, first.file_type), (12800, 16000, 11));
    assert_eq!(first.master_id, Some(101));

    let set = device.playlists.iter().find(|p| p.name == "Set").unwrap();
    let titles: Vec<_> = set.track_ids.iter().map(|id| device.track(*id).unwrap().title.as_str()).collect();
    assert_eq!(titles, ["Second", "First"]);
    let folder = device.playlists.iter().find(|p| p.name == "Folder").unwrap();
    assert!(folder.folder);
    assert_eq!(set.parent_id, folder.export_id);
}

#[test]
fn cues_come_from_the_database_with_their_times() {
    let Some((_dir, volume)) = stick() else { return };
    let device = read(&volume).unwrap().unwrap();
    let cues = &device.tracks[0].cues;
    let mut summary: Vec<_> = cues.iter().map(|c| (c.kind, c.start_ms, c.comment.as_str())).collect();
    summary.sort_unstable();
    assert_eq!(summary, [(0, 500, ""), (1, 100, "drop")]);
}

#[test]
fn the_older_database_reads_the_same_library() {
    let Some((_dir, volume)) = stick() else { return };
    let one = read_kind(&volume, LibraryKind::OneLibrary).unwrap().unwrap();
    let old = read_kind(&volume, LibraryKind::DeviceLibrary).unwrap().expect("export.pdb is written too");
    let titles = |d: &rb_device::Device| {
        let mut t: Vec<_> = d.tracks.iter().map(|t| (t.title.clone(), t.artist.clone(), t.bpm_x100)).collect();
        t.sort();
        t
    };
    assert_eq!(titles(&old), titles(&one));
    let set = old.playlists.iter().find(|p| p.name == "Set").unwrap();
    let names: Vec<_> = set.track_ids.iter().map(|id| old.track(*id).unwrap().title.as_str()).collect();
    assert_eq!(names, ["Second", "First"]);
    assert_eq!(old.tracks[0].master_id, Some(101).or(old.tracks[0].master_id));
}

#[test]
fn both_device_libraries_keep_their_artwork_paths() {
    let Some((_dir, volume)) = stick() else { return };
    for kind in [LibraryKind::OneLibrary, LibraryKind::DeviceLibrary] {
        let device = read_kind(&volume, kind).unwrap().unwrap();
        let first = device.tracks.iter().find(|track| track.title == "First").unwrap();
        assert!(!first.artwork.is_empty(), "missing artwork in {kind:?}");
        let path = rb_device::paths::volume_path(&first.artwork, &volume).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"artwork fixture");
        let second = device.tracks.iter().find(|track| track.title == "Second").unwrap();
        assert!(second.artwork.is_empty());
    }
}
