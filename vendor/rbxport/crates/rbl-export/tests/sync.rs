//! Exporting twice to the same stick.
//!
//! The first export writes everything. The second is a sync: it copies what
//! changed, leaves what did not, and takes off what is no longer selected —
//! while every track keeps the id a player already cached it under.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_export::{export, verify, Manifest, SourcePlaylist, SourceTrack};

/// Writes a dummy audio file and returns a track that points at it.
fn track(dir: &std::path::Path, id: u64, title: &str, artist: &str) -> SourceTrack {
    let path = dir.join(format!("source-{id}.mp3"));
    std::fs::write(&path, vec![id as u8; 2048]).unwrap();
    SourceTrack {
        id,
        source_path: path,
        title: title.into(),
        artist: artist.into(),
        album: "Single".into(),
        genre: "House".into(),
        key: "Am".into(),
        bpm_x100: 12_800,
        duration_sec: 300,
        date_added: "2026-09-06".into(),
        analysis: vec![("DAT".into(), rbl_anlz::AnlzBuilder::new().path("/x.mp3").finish())],
        ..SourceTrack::default()
    }
}

fn one_list(tracks: &[SourceTrack]) -> Vec<SourcePlaylist> {
    vec![SourcePlaylist { name: "Set".into(), track_indices: (0..tracks.len()).collect(), ..Default::default() }]
}

fn ids(destination: &std::path::Path) -> Vec<(u64, u32)> {
    let mut out: Vec<(u64, u32)> = Manifest::load(destination)
        .expect("a manifest")
        .tracks
        .iter()
        .map(|t| (t.library_id, t.export_id))
        .collect();
    out.sort_unstable();
    out
}

#[test]
fn a_second_export_of_the_same_tracks_copies_nothing() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![
        track(src.path(), 1, "All U Need", "TRIODE"),
        track(src.path(), 2, "The Abyss", "ARTBAT"),
    ];

    let first = export(dest.path(), &tracks, &one_list(&tracks)).unwrap();
    assert_eq!(first.tracks, 2);
    assert_eq!(first.reused, 0);
    assert_eq!(first.bytes_copied, 4096);
    assert_eq!(first.analysis_files, 2);
    assert_eq!((first.playlists_added, first.playlists_removed), (1, 0));

    let second = export(dest.path(), &tracks, &one_list(&tracks)).unwrap();
    assert_eq!(second.tracks, 2, "the stick still holds both");
    assert_eq!(second.reused, 2);
    assert_eq!(second.bytes_copied, 0, "nothing changed, so nothing was copied");
    assert_eq!(second.bytes_reused, 4096);
    assert_eq!(second.analysis_files, 0, "the analysis was already there");
    assert_eq!(second.removed, 0);
    assert_eq!((second.playlists_added, second.playlists_removed), (0, 0));

    // And the stick is still a stick.
    let check = verify(dest.path()).unwrap();
    assert!(check.is_ok());
    assert_eq!(check.tracks, 2);
    assert_eq!(check.playlist_entries, 2);
}

#[test]
fn a_changed_source_is_copied_again() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![
        track(src.path(), 1, "All U Need", "TRIODE"),
        track(src.path(), 2, "The Abyss", "ARTBAT"),
    ];
    export(dest.path(), &tracks, &[]).unwrap();

    // A re-encode: different length.
    std::fs::write(&tracks[0].source_path, vec![9u8; 8192]).unwrap();

    let second = export(dest.path(), &tracks, &[]).unwrap();
    assert_eq!(second.reused, 1, "only the untouched track is left alone");
    assert_eq!(second.bytes_copied, 8192);

    let on_stick = dest.path().join("Contents/TRIODE/Single/source-1.mp3");
    assert_eq!(std::fs::metadata(&on_stick).unwrap().len(), 8192, "the stick has the new audio");
}

#[test]
fn a_rewrite_of_the_same_length_is_still_noticed() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![track(src.path(), 1, "All U Need", "TRIODE")];
    export(dest.path(), &tracks, &[]).unwrap();

    // Same byte count, different bytes — size alone cannot see this, which is
    // why the modification time is kept to nanoseconds.
    std::fs::write(&tracks[0].source_path, vec![7u8; 2048]).unwrap();

    let second = export(dest.path(), &tracks, &[]).unwrap();
    assert_eq!(second.reused, 0);
    assert_eq!(second.bytes_copied, 2048);
    let on_stick = dest.path().join("Contents/TRIODE/Single/source-1.mp3");
    assert_eq!(std::fs::read(&on_stick).unwrap()[0], 7, "the stick has the new bytes");
}

#[test]
fn a_track_dropped_from_the_selection_leaves_the_stick() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![
        track(src.path(), 1, "All U Need", "TRIODE"),
        track(src.path(), 2, "The Abyss", "ARTBAT"),
    ];
    export(dest.path(), &tracks, &one_list(&tracks)).unwrap();
    let gone = dest.path().join("Contents/ARTBAT/Single/source-2.mp3");
    assert!(gone.is_file());

    let kept = vec![tracks[0].clone()];
    let second = export(dest.path(), &kept, &one_list(&kept)).unwrap();
    assert_eq!(second.tracks, 1);
    assert_eq!(second.removed, 1);
    assert!(!gone.exists(), "its audio is off the stick");
    assert!(
        !dest.path().join("Contents/ARTBAT").exists(),
        "and the directory it emptied went with it"
    );
    assert!(dest.path().join("Contents/TRIODE/Single/source-1.mp3").is_file());

    let check = verify(dest.path()).unwrap();
    assert!(check.is_ok());
    assert_eq!(check.tracks, 1);
    assert_eq!(check.playlist_entries, 1);
}

#[test]
fn export_ids_survive_a_removal_so_a_deck_does_not_repoint() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![
        track(src.path(), 10, "One", "A"),
        track(src.path(), 20, "Two", "B"),
        track(src.path(), 30, "Three", "C"),
    ];
    export(dest.path(), &tracks, &[]).unwrap();
    assert_eq!(ids(dest.path()), vec![(10, 1), (20, 2), (30, 3)]);

    // Drop the middle one. Reassigning by position would slide Three onto id 2
    // and every cached waveform on the deck would name the wrong track.
    let kept = vec![tracks[0].clone(), tracks[2].clone()];
    export(dest.path(), &kept, &[]).unwrap();
    assert_eq!(ids(dest.path()), vec![(10, 1), (30, 3)]);

    // A new track takes the freed id rather than growing the range forever.
    let mut grown = kept.clone();
    grown.insert(1, track(src.path(), 40, "Four", "D"));
    export(dest.path(), &grown, &[]).unwrap();
    assert_eq!(ids(dest.path()), vec![(10, 1), (30, 3), (40, 2)]);
}

#[test]
fn renaming_an_artist_moves_the_audio_and_leaves_no_copy_behind() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let mut tracks = vec![track(src.path(), 1, "All U Need", "TRIODE")];
    export(dest.path(), &tracks, &[]).unwrap();
    assert!(dest.path().join("Contents/TRIODE/Single/source-1.mp3").is_file());

    tracks[0].artist = "Triode Live".into();
    let second = export(dest.path(), &tracks, &[]).unwrap();
    assert_eq!(second.reused, 0, "a different place on the stick means a copy");
    assert!(dest.path().join("Contents/Triode Live/Single/source-1.mp3").is_file());
    assert!(
        !dest.path().join("Contents/TRIODE").exists(),
        "the old path must not linger unreferenced"
    );

    assert!(verify(dest.path()).unwrap().is_ok());
}

#[test]
fn a_rename_that_only_changes_case_keeps_the_track() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let mut tracks = vec![track(src.path(), 1, "All U Need", "TRIODE")];
    export(dest.path(), &tracks, &[]).unwrap();

    // On a case-insensitive disk — which a Mac and a FAT32 stick both are —
    // the new path and the old path are the same file. Tidying away "the old
    // one" would delete the audio the databases point at.
    tracks[0].artist = "Triode".into();
    export(dest.path(), &tracks, &[]).unwrap();

    let check = verify(dest.path()).unwrap();
    assert!(check.is_ok(), "missing: {:?}", check.missing_audio);
    assert_eq!(check.audio_present, 1);
}

#[test]
fn a_skipped_track_does_not_shift_the_playlist_under_the_ones_after_it() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![
        track(src.path(), 1, "Missing", "A"),
        track(src.path(), 2, "Present", "B"),
    ];
    // The first track's audio is gone by the time the export runs.
    std::fs::remove_file(&tracks[0].source_path).unwrap();

    let report = export(dest.path(), &tracks, &one_list(&tracks)).unwrap();
    assert_eq!(report.tracks, 1);
    assert_eq!(report.skipped, vec!["Missing".to_owned()]);

    // The playlist named indices 0 and 1; only index 1 was written, and it must
    // be the track that survived, not the one that took its place in the list.
    let bytes = std::fs::read(dest.path().join("PIONEER/rekordbox/export.pdb")).unwrap();
    let pdb = rbl_pdb::Pdb::parse(&bytes).unwrap();
    let entries = pdb
        .table(rbl_pdb::PageType::PlaylistEntries)
        .map(|t| pdb.playlist_entries(t))
        .unwrap_or_default();
    assert_eq!(entries.len(), 1);
    let rows = pdb.track_rows(pdb.table(rbl_pdb::PageType::Tracks).unwrap());
    assert_eq!(rows.len(), 1);
    assert_eq!(entries[0].track_id, rows[0].id, "the entry names the track that was written");
    assert_eq!(rows[0].title, "Present");
}

#[test]
fn a_stick_with_no_manifest_of_ours_is_written_in_full() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![track(src.path(), 1, "All U Need", "TRIODE")];
    export(dest.path(), &tracks, &[]).unwrap();

    // As if rekordbox itself had written the stick: our record is not there.
    std::fs::remove_file(Manifest::path(dest.path())).unwrap();

    let second = export(dest.path(), &tracks, &[]).unwrap();
    assert_eq!(second.reused, 0, "with nothing to compare against, everything is written");
    assert_eq!(second.bytes_copied, 2048);
    assert!(verify(dest.path()).unwrap().is_ok());
}

#[test]
fn a_file_deleted_off_the_stick_is_put_back() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![track(src.path(), 1, "All U Need", "TRIODE")];
    export(dest.path(), &tracks, &[]).unwrap();

    let audio = dest.path().join("Contents/TRIODE/Single/source-1.mp3");
    std::fs::remove_file(&audio).unwrap();
    let saved = rbl_export::Manifest::load(dest.path()).unwrap();
    let anlz = dest.path().join(saved.tracks[0].anlz_dir.trim_start_matches('/')).join("ANLZ0000.DAT");
    std::fs::remove_file(&anlz).unwrap();

    // The manifest still says both are there. Trusting it would leave a stick
    // whose database names files that do not exist.
    let second = export(dest.path(), &tracks, &[]).unwrap();
    assert_eq!(second.reused, 0);
    assert_eq!(second.analysis_files, 1);
    assert!(audio.is_file());
    assert!(anlz.is_file());
    assert!(verify(dest.path()).unwrap().is_ok());
}

#[test]
fn a_sync_keeps_the_settings_the_stick_already_carries() {
    use rbl_onelibrary::settings::StickSettings;

    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![track(src.path(), 1, "All U Need", "TRIODE")];
    export(dest.path(), &tracks, &one_list(&tracks)).unwrap();

    // What the device panel writes between two syncs.
    let db = dest.path().join("PIONEER/rekordbox/exportLibrary.db");
    let mut settings = StickSettings::read(&db).unwrap();
    settings.device_name = "FRIDAY".to_owned();
    settings.colors[2].name = "Peak time".to_owned();
    let genre = settings.categories.iter_mut().find(|s| s.menu_item == 1).unwrap();
    genre.visible = true;
    genre.seq = 11;
    settings.sub_column = Some(11);
    settings.write(&db).unwrap();

    // The database is rebuilt, and every one of those survives.
    export(dest.path(), &tracks, &one_list(&tracks)).unwrap();
    let after = StickSettings::read(&db).unwrap();
    assert_eq!(after, settings);
}
