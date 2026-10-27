//! Importing a disposable stick into a library, using a stick the native exporter wrote.

use std::path::{Path, PathBuf};
use std::process::Command;

use rb_library::fixture::{self, Rows};
use rb_library::{Guard, Library};
use es_engine::device_import::{import_device, preview, Options, PlaylistPolicy, TrackStatus};
use serde_json::json;
use tempfile::TempDir;

fn exporter() -> Option<PathBuf> {
    Some(es_engine::sync::exporter().expect("Build es-usb and set EXPORTSTUDIO_USB_EXPORT before running import tests"))
}

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

struct World {
    dir: TempDir,
    volume: PathBuf,
    lib: Library,
}

/// A stick holding two tracks in the playlist "Set" (inside "Folder"), and an empty library with a DBID of 4242.
/// A stick whose playlists were made on the stick itself.
fn world() -> Option<World> {
    let w = synced_world()?;
    let path = w.volume.join("PIONEER/exportstudio/manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for row in manifest["playlists"].as_array_mut().unwrap() {
        row["device_only"] = json!(true);
    }
    std::fs::write(&path, manifest.to_string()).unwrap();
    Some(w)
}

/// A stick this library synced its playlists 1 ("Folder") and 2 ("Set") to.
fn synced_world() -> Option<World> {
    let exporter = exporter()?;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let volume = dir.path().join("STICK");
    std::fs::create_dir_all(&volume).unwrap();
    let mut tracks = Vec::new();
    for (id, title, artist, seconds) in [(101u64, "First", "Alpha", 1u32), (102, "Second", "Beta", 2)] {
        let file = source.join(format!("{title}.wav"));
        wav(&file, seconds);
        tracks.push(json!({
            "id": id, "sourcePath": file, "title": title, "artist": artist, "album": "Album", "genre": "House",
            "bpmX100": 12800, "durationSec": seconds, "rating": 4, "year": 2020, "bitrate": 128, "sampleRate": 8000,
            "fileSize": 36 + 16000 * seconds, "metadata": { "trackNumber": 1, "bitDepth": 16, "analysed": 105 },
            "cues": [{ "kind": 0, "timeMs": 500 }, { "kind": 1, "timeMs": 100, "colorCode": 10, "comment": "drop" }],
        }));
    }
    let job = json!({
        "schemaVersion": 4,
        "destination": volume, "dbId": 4242, "tracks": tracks,
        "playlists": [
            { "id": 1, "name": "Folder", "parentId": 0, "folder": true, "trackIds": [] },
            { "id": 2, "name": "Set", "parentId": 1, "folder": false, "trackIds": [102, 101] },
        ],
    });
    let job_path = dir.path().join("job.json");
    std::fs::write(&job_path, job.to_string()).unwrap();
    let out = Command::new(exporter).arg(&job_path).output().unwrap();
    assert!(out.status.success(), "export failed: {}", String::from_utf8_lossy(&out.stdout));

    let path = fixture::build(&dir.path().join("lib"), 5).unwrap_or_else(|_| {
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        fixture::build(&dir.path().join("lib"), 5).unwrap()
    });
    let lib = Library::open_live(&path, Guard::Off).unwrap();
    Some(World { dir, volume, lib })
}

fn options(world: &World, policy: PlaylistPolicy) -> Options {
    Options { folder: None, music: Some(world.dir.path().join("Music")), policy }
}

fn run(w: &mut World, policy: PlaylistPolicy) -> es_engine::device_import::Outcome {
    let device = rb_device::read::read(&w.volume).unwrap().unwrap();
    let share = w.dir.path().join("lib/share");
    let opts = options(w, policy);
    import_device(&mut w.lib, &share, &w.volume, &device, &opts, &|_| {}).unwrap()
}

#[test]
fn tracks_are_copied_imported_with_their_cues_and_put_in_new_playlists() {
    let Some(mut w) = world() else { return };
    let out = run(&mut w, PlaylistPolicy::SkipExisting);
    assert_eq!((out.copied, out.reused, out.tracks, out.playlists, out.failed.len()), (2, 0, 2, 2, 0));

    let music = w.dir.path().join("Music/PioneerDJ/Imported from Device/Contents");
    assert!(music.join("Alpha/Album/First.wav").is_file());
    assert!(music.join("Beta/Album/Second.wav").is_file());

    let tracks = w.lib.tracks(None).unwrap();
    let first = tracks.iter().find(|t| t.title == "First").unwrap();
    assert_eq!((first.artist.as_str(), first.bpm_x100, first.rating), ("Alpha", 12800, 4));
    let mut cues: Vec<_> = w.lib.cues(&first.id).unwrap().into_iter().map(|c| (c.kind, c.start_ms, c.comment)).collect();
    cues.sort();
    assert_eq!(cues, [(0, 500, String::new()), (1, 100, "drop".to_owned())]);

    let nodes = w.lib.playlists().unwrap();
    let set = nodes.iter().find(|n| n.name == "Set").unwrap();
    let order: Vec<_> = w.lib.playlist_track_ids(&set.id).unwrap();
    let titles: Vec<_> = order.iter().map(|id| tracks.iter().find(|t| &t.id == id).unwrap().title.as_str()).collect();
    assert_eq!(titles, ["Second", "First"]);
    assert_eq!(nodes.iter().find(|n| n.name == "Folder").map(|f| f.id.as_str()), Some(set.parent_id.as_str()));
}

#[test]
fn a_second_import_reuses_the_copies_and_makes_no_duplicates() {
    let Some(mut w) = world() else { return };
    run(&mut w, PlaylistPolicy::SkipExisting);
    let again = run(&mut w, PlaylistPolicy::SkipExisting);
    assert_eq!((again.copied, again.reused, again.playlists), (0, 2, 0));
    assert_eq!(w.lib.tracks(None).unwrap().len(), 2);
}

#[test]
fn a_track_removed_from_a_playlist_is_not_put_back_by_the_default_policy() {
    let Some(mut w) = world() else { return };
    run(&mut w, PlaylistPolicy::SkipExisting);
    let set = w.lib.playlists().unwrap().into_iter().find(|n| n.name == "Set").unwrap();
    let first = w.lib.tracks(None).unwrap().into_iter().find(|t| t.title == "First").unwrap();
    w.lib.write(|tx| tx.remove_tracks(std::slice::from_ref(&first.id))).unwrap();

    run(&mut w, PlaylistPolicy::SkipExisting);
    let members = w.lib.playlist_track_ids(&set.id).unwrap();
    assert_eq!(members.len(), 1, "the playlist keeps only what it had");
    assert!(w.lib.tracks(None).unwrap().iter().any(|t| t.title == "First"), "the track itself is back in the collection");
}

#[test]
fn merging_adds_the_sticks_tracks_to_a_playlist_that_already_exists() {
    let Some(mut w) = world() else { return };
    run(&mut w, PlaylistPolicy::SkipExisting);
    let set = w.lib.playlists().unwrap().into_iter().find(|n| n.name == "Set").unwrap();
    let first = w.lib.tracks(None).unwrap().into_iter().find(|t| t.title == "First").unwrap();
    w.lib.write(|tx| tx.remove_from_playlist(&set.id, std::slice::from_ref(&first.id))).unwrap();
    run(&mut w, PlaylistPolicy::Merge);
    assert_eq!(w.lib.playlist_track_ids(&set.id).unwrap().len(), 2);
}

#[test]
fn create_new_puts_everything_in_a_folder_named_for_the_stick() {
    let Some(mut w) = world() else { return };
    run(&mut w, PlaylistPolicy::SkipExisting);
    let out = run(&mut w, PlaylistPolicy::CreateNew);
    assert_eq!(out.playlists, 3);
    let nodes = w.lib.playlists().unwrap();
    assert!(nodes.iter().any(|n| n.name.starts_with("Imported from ")));
    assert_eq!(nodes.iter().filter(|n| n.name == "Set").count(), 2);
}

#[test]
fn a_track_from_another_library_is_never_mistaken_for_one_here() {
    let Some(w) = world() else { return };
    // This library has a track with id 101, but the stick was exported by library 4242 and this one is another.
    {
        let conn = fixture::connect(w.lib.path()).unwrap();
        conn.execute("UPDATE djmdProperty SET DBID = '9999'", []).unwrap();
        let path = w.dir.path().join("elsewhere.wav");
        wav(&path, 1);
        conn.execute("INSERT INTO djmdContent (ID, Title, FolderPath) VALUES ('101', 'Unrelated', ?1)", [path.to_string_lossy().into_owned()]).unwrap();
        Rows(&conn).playlist("1", "x", 0).unwrap();
    }
    let device = rb_device::read::read(&w.volume).unwrap().unwrap();
    let plan = preview(&w.lib, &w.volume, &device, &options(&w, PlaylistPolicy::SkipExisting)).unwrap();
    assert!(plan.tracks.iter().all(|t| t.status == TrackStatus::Copy), "{:?}", plan.tracks);
}

#[test]
fn a_track_this_library_exported_is_reused_not_copied() {
    let Some(mut w) = world() else { return };
    let path = w.dir.path().join("mine.wav");
    wav(&path, 1);
    {
        let conn = fixture::connect(w.lib.path()).unwrap();
        conn.execute("INSERT INTO djmdContent (ID, Title, FolderPath) VALUES ('101', 'Mine', ?1)", [path.to_string_lossy().into_owned()]).unwrap();
    }
    let out = run(&mut w, PlaylistPolicy::SkipExisting);
    assert_eq!((out.reused, out.copied), (1, 1));
    assert_eq!(w.lib.tracks(None).unwrap().len(), 2);
}

#[test]
fn missing_audio_is_reported_and_the_rest_still_imports() {
    let Some(mut w) = world() else { return };
    let device = rb_device::read::read(&w.volume).unwrap().unwrap();
    let gone = device.tracks.iter().find(|t| t.title == "Second").unwrap();
    std::fs::remove_file(rb_device::paths::volume_path(&gone.path, &w.volume).unwrap()).unwrap();
    let plan = preview(&w.lib, &w.volume, &device, &options(&w, PlaylistPolicy::SkipExisting)).unwrap();
    assert_eq!(plan.tracks.iter().filter(|t| t.status == TrackStatus::Missing).count(), 1);
    let out = run(&mut w, PlaylistPolicy::SkipExisting);
    assert_eq!((out.copied, out.failed.len()), (1, 1));
    assert_eq!(out.failed[0].1, "Audio file is missing on the USB");
}

#[test]
fn a_playlist_the_library_deleted_is_not_recreated_from_the_stick() {
    let Some(mut w) = synced_world() else { return };
    // The library still has playlist 1 but deleted playlist 2.
    {
        let conn = fixture::connect(w.lib.path()).unwrap();
        Rows(&conn).playlist("1", "Folder", 0).unwrap();
    }
    let device = rb_device::read::read(&w.volume).unwrap().unwrap();
    let plan = preview(&w.lib, &w.volume, &device, &options(&w, PlaylistPolicy::SkipExisting)).unwrap();
    assert_eq!(plan.tracks.len(), 2, "its tracks are still offered");
    assert!(plan.playlists.iter().all(|p| p.name != "Set"), "{:?}", plan.playlists);

    let out = run(&mut w, PlaylistPolicy::SkipExisting);
    assert_eq!((out.playlists, out.tracks), (0, 2));
    assert!(w.lib.playlists().unwrap().iter().all(|n| n.name != "Set"));

    let out = run(&mut w, PlaylistPolicy::CreateNew);
    assert!(out.playlists >= 2, "asking for new playlists still makes them");
}

#[test]
fn a_renamed_library_playlist_still_counts_as_the_one_on_the_stick() {
    let Some(mut w) = synced_world() else { return };
    {
        let conn = fixture::connect(w.lib.path()).unwrap();
        Rows(&conn).playlist("2", "Renamed", 0).unwrap();
    }
    let out = run(&mut w, PlaylistPolicy::Merge);
    let renamed = w.lib.playlists().unwrap().into_iter().find(|n| n.id == "2").unwrap();
    assert_eq!(w.lib.playlist_track_ids(&renamed.id).unwrap().len(), 2);
    assert!(w.lib.playlists().unwrap().iter().all(|n| n.name != "Set"));
    assert_eq!(out.tracks, 2);
}

#[test]
fn both_usb_libraries_preserve_selected_and_unset_hot_cue_colours() {
    for kind in [rb_device::LibraryKind::OneLibrary, rb_device::LibraryKind::DeviceLibrary] {
        let Some(mut w) = world() else { return };
        let job_path = w.dir.path().join("job.json");
        let mut job: serde_json::Value = serde_json::from_slice(&std::fs::read(&job_path).unwrap()).unwrap();
        w.volume = w.dir.path().join("COLOUR-STICK");
        std::fs::create_dir_all(&w.volume).unwrap();
        job["destination"] = json!(w.volume);
        let analysis = w.dir.path().join("source/ANLZ0000.EXT");
        let mut bytes = b"PMAI".to_vec();
        bytes.extend(28u32.to_be_bytes());
        bytes.extend(28u32.to_be_bytes());
        bytes.extend([0; 16]);
        std::fs::write(&analysis, &bytes).unwrap();
        std::fs::write(analysis.with_extension("DAT"), &bytes).unwrap();
        for track in job["tracks"].as_array_mut().unwrap() {
            track["analysis"] = json!([analysis.with_extension("DAT"), analysis]);
            track["cues"] = json!([
                { "kind": 0, "timeMs": 50, "color": 5 },
                { "kind": 1, "timeMs": 100, "colorCode": 46, "comment": "Selected" },
                { "kind": 2, "timeMs": 200, "colorCode": 0 },
                { "kind": 5, "timeMs": 300, "colorCode": 8, "comment": "Slot D" },
            ]);
        }
        std::fs::write(&job_path, job.to_string()).unwrap();
        let exported = Command::new(exporter().unwrap()).arg(&job_path).output().unwrap();
        assert!(exported.status.success(), "{}", String::from_utf8_lossy(&exported.stdout));

        let device = rb_device::read::read_kind(&w.volume, kind).unwrap().unwrap();
        let share = w.dir.path().join("lib/share");
        let opts = options(&w, PlaylistPolicy::SkipExisting);
        let result = import_device(&mut w.lib, &share, &w.volume, &device, &opts, &|_| {}).unwrap();
        assert_eq!(result.copied, 2);
        assert!(result.failed.is_empty(), "{:?}", result.failed);

        let track = w.lib.tracks(None).unwrap().into_iter().find(|t| t.title == "First").unwrap();
        let cues = es_index::detail::cues_for(&w.lib, &share, &track.id).unwrap();
        let selected = cues.iter().find(|c| c.slot == 1).unwrap();
        assert_eq!((selected.color_index, selected.color_code, selected.rgb.as_deref()), (Some(46), Some(46), Some("#ff005c")));
        let unset = cues.iter().find(|c| c.slot == 2).unwrap();
        assert_eq!((unset.color_index, unset.color_code, unset.rgb.as_deref()), (None, Some(0), Some("#00c4ff")));
        let slot_d = cues.iter().find(|c| c.slot == 5).unwrap();
        assert_eq!((slot_d.color_index, slot_d.color_code, slot_d.rgb.as_deref()), (Some(8), Some(8), Some("#00c4ff")));
        assert_eq!(cues.iter().find(|c| c.slot == 0).unwrap().color_index, None);

        w.lib.write(|tx| tx.set_cue_color(&track.id, &selected.id, None)).unwrap();
        let previews = es_index::detail::previews(&w.lib, &share, &[track.id]).unwrap();
        let cleared = previews[0].cues.iter().find(|c| c.slot == 1).unwrap();
        assert_eq!((cleared.color_index, cleared.color_code, cleared.rgb.as_deref()), (None, Some(46), Some("#ff005c")));
        let slot_d = previews[0].cues.iter().find(|c| c.slot == 5).unwrap();
        assert_eq!(slot_d.color_code, Some(8));
    }
}
