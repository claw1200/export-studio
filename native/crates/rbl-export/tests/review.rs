//! Typed review decisions are token-bound and committed with the export generation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;

use rb_model::conflict::{ConflictKind, Decisions, Resolution, Severity};
use rbl_export::{ExportOptions, SourceMyTag, SourcePlaylist, SourceTrack, SyncSource};

fn source(root: &Path, db_id: u64) -> (Vec<SourceTrack>, Vec<SourcePlaylist>, SyncSource) {
    let audio = root.join("track.mp3");
    std::fs::write(&audio, b"disposable audio fixture").unwrap();
    let track = SourceTrack { id: 42, source_path: audio, title: "Library title".into(), artist: "Library artist".into(), ..Default::default() };
    let playlist = SourcePlaylist { id: 9, name: "Library mix".into(), track_indices: vec![0], ..Default::default() };
    let sync = SyncSource { db_id, automatic: false, tree: vec![rbl_export::SyncNode { id: 9, parent: 0, attribute: 0 }] };
    (vec![track], vec![playlist], sync)
}

fn export(volume: &Path, tracks: &[SourceTrack], playlists: &[SourcePlaylist], sync: &SyncSource, decisions: Option<&Decisions>) -> rbl_export::Result<rbl_export::ExportReport> {
    export_with_tags(volume, tracks, playlists, &[], sync, decisions)
}

fn export_with_tags(volume: &Path, tracks: &[SourceTrack], playlists: &[SourcePlaylist], tags: &[SourceMyTag], sync: &SyncSource, decisions: Option<&Decisions>) -> rbl_export::Result<rbl_export::ExportReport> {
    rbl_export::export_cancellable(
        volume,
        tracks,
        playlists,
        tags,
        &ExportOptions { sync: Some(sync), decisions, ..Default::default() },
        &mut |_| {},
        &|| false,
    )
}

fn edit_one_library(volume: &Path, statement: &str) {
    let connection = rusqlite::Connection::open(volume.join("PIONEER/rekordbox/exportLibrary.db")).unwrap();
    connection.pragma_update(None, "cipher", "sqlcipher").unwrap();
    connection.pragma_update(None, "legacy", 4).unwrap();
    connection.pragma_update(None, "key", rbl_onelibrary::key::passphrase().unwrap()).unwrap();
    connection.execute_batch(statement).unwrap();
}

#[test]
fn edited_selected_metadata_requires_an_explicit_library_resolution_and_receipt() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    edit_one_library(volume.path(), "UPDATE content SET title='Device title' WHERE content_id=1");

    let options = ExportOptions { sync: Some(&sync), ..Default::default() };
    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &options).unwrap();
    assert_eq!(inspection.plan.as_ref().unwrap().tracks, 1);
    let database_path = volume.path().join("PIONEER/rekordbox/exportLibrary.db");
    let before = std::fs::read(&database_path).unwrap();
    let conflict = inspection.diagnostics.iter().find(|item| item.kind == ConflictKind::MetadataChanged && item.fields.contains(&"title".to_owned())).unwrap();
    assert_eq!(conflict.severity, Severity::DecisionRequired);
    assert_eq!(conflict.allowed_resolutions, [Resolution::UseLibrary]);
    let evidence = conflict.evidence.iter().find(|evidence| evidence.field == "title").unwrap();
    assert_eq!(evidence.baseline.as_deref(), Some("Library title"));
    assert_eq!(evidence.device.as_deref(), Some("Device title"));
    assert_eq!(evidence.library.as_deref(), Some("Library title"));
    let decisions = Decisions { token: inspection.token, resolutions: [(conflict.id.clone(), Resolution::UseLibrary)].into() };

    assert!(matches!(export(volume.path(), &tracks, &playlists, &sync, None), Err(rbl_export::ExportError::ReviewRequired(_))));
    assert_eq!(std::fs::read(&database_path).unwrap(), before);
    let unsupported = Decisions { token: decisions.token.clone(), resolutions: [(conflict.id.clone(), Resolution::RestoreFromLibrary)].into() };
    assert!(matches!(export(volume.path(), &tracks, &playlists, &sync, Some(&unsupported)), Err(rbl_export::ExportError::ReviewRequired(_))));
    assert_eq!(std::fs::read(&database_path).unwrap(), before);

    let report = export(volume.path(), &tracks, &playlists, &sync, Some(&decisions)).unwrap();
    assert!(report.verified);
    let receipt = rbl_export::Manifest::load(volume.path()).unwrap().review.unwrap();
    assert_eq!(receipt.token, decisions.token);
    assert_eq!(receipt.resolutions, decisions.resolutions);
    let database = rbl_onelibrary::ExportLibrary::open_read_only(&volume.path().join("PIONEER/rekordbox/exportLibrary.db")).unwrap();
    let title: String = database.connection().query_row("SELECT title FROM content WHERE content_id=1", [], |row| row.get(0)).unwrap();
    assert_eq!(title, "Library title");
}

#[test]
fn stale_review_is_rejected_without_changing_device_bytes() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    edit_one_library(volume.path(), "UPDATE content SET title='First device edit' WHERE content_id=1");
    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    let conflict = inspection.diagnostics.iter().find(|item| item.kind == ConflictKind::MetadataChanged).unwrap();
    let decisions = Decisions { token: inspection.token, resolutions: [(conflict.id.clone(), Resolution::UseLibrary)].into() };
    edit_one_library(volume.path(), "UPDATE content SET title='Changed after review' WHERE content_id=1");
    let before = std::fs::read(volume.path().join("PIONEER/rekordbox/exportLibrary.db")).unwrap();
    assert!(matches!(export(volume.path(), &tracks, &playlists, &sync, Some(&decisions)), Err(rbl_export::ExportError::ReviewRequired(_))));
    assert_eq!(std::fs::read(volume.path().join("PIONEER/rekordbox/exportLibrary.db")).unwrap(), before);
}

#[test]
fn a_device_deleted_library_track_is_typed_and_restorable() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    edit_one_library(volume.path(), "DELETE FROM playlist_content WHERE content_id=1; DELETE FROM content WHERE content_id=1");

    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    let conflict = inspection.diagnostics.iter().find(|item| item.kind == ConflictKind::DeviceDeletion && item.entity == rb_model::conflict::EntityKind::Track).unwrap();
    assert_eq!(conflict.severity, Severity::DecisionRequired);
    assert_eq!(conflict.allowed_resolutions, [Resolution::RestoreFromLibrary]);
    let resolutions = inspection.diagnostics.iter().filter(|item| item.severity == Severity::DecisionRequired)
        .map(|item| (item.id.clone(), if item.kind == ConflictKind::DeviceDeletion { Resolution::RestoreFromLibrary } else { Resolution::UseLibrary }))
        .collect();
    let decisions = Decisions { token: inspection.token, resolutions };
    export(volume.path(), &tracks, &playlists, &sync, Some(&decisions)).unwrap();
    let readback = rbl_export::verify(volume.path()).unwrap();
    assert!(readback.is_ok());
    assert_eq!(readback.tracks, 1);
}

#[test]
fn library_ownership_mismatch_is_a_non_overridable_blocker() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    let other_sync = SyncSource { db_id: 999, ..sync };
    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &ExportOptions { sync: Some(&other_sync), ..Default::default() }).unwrap();
    let blocker = inspection.diagnostics.iter().find(|item| item.kind == ConflictKind::OwnershipMismatch).unwrap();
    assert_eq!(blocker.severity, Severity::Blocked);
    assert!(blocker.allowed_resolutions.is_empty());
    let decisions = Decisions { token: inspection.token, resolutions: Default::default() };
    assert!(matches!(export(volume.path(), &tracks, &playlists, &other_sync, Some(&decisions)), Err(rbl_export::ExportError::ReviewRequired(_))));
}

#[test]
fn unsupported_device_schema_is_a_typed_blocker_without_recovery() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    edit_one_library(volume.path(), "UPDATE property SET dbVersion='999'");
    let path = volume.path().join("PIONEER/rekordbox/exportLibrary.db");
    let before = std::fs::read(&path).unwrap();
    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    assert!(inspection.diagnostics.iter().any(|item| item.kind == ConflictKind::UnsupportedSchema && item.severity == Severity::Blocked));
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert!(!volume.path().join(".exportstudio-publication").exists());
}

#[test]
fn corrupt_device_database_is_a_typed_integrity_blocker() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    let path = volume.path().join("PIONEER/rekordbox/export.pdb");
    std::fs::write(&path, b"not a DeviceSQL database").unwrap();
    let before = std::fs::read(&path).unwrap();
    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    assert!(inspection.diagnostics.iter().any(|item| item.kind == ConflictKind::IntegrityFailure && item.severity == Severity::Blocked));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn direct_export_refuses_to_recover_a_pending_publication() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let journal = volume.path().join(".exportstudio-publication");
    std::fs::create_dir(&journal).unwrap();
    std::fs::write(journal.join("sentinel"), b"pending").unwrap();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    let result = export(volume.path(), &tracks, &playlists, &sync, None);
    assert!(matches!(result, Err(rbl_export::ExportError::ReviewRequired(_))));
    assert_eq!(std::fs::read(journal.join("sentinel")).unwrap(), b"pending");
}

#[test]
fn changed_source_asset_is_reported_as_informational() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (tracks, playlists, sync) = source(source_dir.path(), 123);
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    std::fs::write(&tracks[0].source_path, b"changed disposable audio fixture").unwrap();
    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    let asset = inspection.diagnostics.iter().find(|item| item.kind == ConflictKind::AssetChanged).unwrap();
    assert_eq!(asset.severity, Severity::Information);
    assert_eq!(asset.fields, ["audio"]);
    assert!(asset.allowed_resolutions.is_empty());
}

#[test]
fn removed_track_cue_edits_require_and_accept_explicit_discard() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let audio_one = source_dir.path().join("one.mp3");
    let audio_two = source_dir.path().join("two.mp3");
    std::fs::write(&audio_one, b"track one fixture").unwrap();
    std::fs::write(&audio_two, b"track two fixture").unwrap();
    let track_one = SourceTrack {
        id: 42,
        source_path: audio_one,
        title: "One".into(),
        analysis: vec![("DAT".into(), rbl_anlz::AnlzBuilder::new().path("/one.mp3").finish())],
        ..Default::default()
    };
    let track_two = SourceTrack { id: 43, source_path: audio_two, title: "Two".into(), ..Default::default() };
    let playlist = |indices| SourcePlaylist { id: 9, name: "Mix".into(), track_indices: indices, ..Default::default() };
    let sync = SyncSource { db_id: 123, automatic: false, tree: vec![rbl_export::SyncNode { id: 9, parent: 0, attribute: 0 }] };
    export(volume.path(), &[track_one.clone(), track_two.clone()], &[playlist(vec![0, 1])], &sync, None).unwrap();
    let manifest = rbl_export::Manifest::load(volume.path()).unwrap();
    let analysis_path = volume.path().join(manifest.tracks.iter().find(|track| track.library_id == 42).unwrap().anlz_dir.trim_start_matches('/')).join("ANLZ0000.DAT");
    let mut analysis = std::fs::read(&analysis_path).unwrap();
    analysis.extend(b"player analysis edit");
    std::fs::write(analysis_path, analysis).unwrap();
    let next_tracks = vec![track_two];
    let next_playlists = vec![playlist(vec![0])];
    let inspection = rbl_export::review::inspect_volume(volume.path(), &next_tracks, &next_playlists, &[], &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    let cue = inspection.diagnostics.iter().find(|item| item.kind == ConflictKind::CueGridChanged).unwrap();
    assert_eq!(cue.severity, Severity::DecisionRequired);
    assert_eq!(cue.allowed_resolutions, [Resolution::DiscardDeviceEdits]);
    assert!(cue.evidence.iter().any(|evidence| evidence.field == "ANLZ0000.DAT"));
    let resolutions = inspection.diagnostics.iter().filter(|item| item.severity == Severity::DecisionRequired)
        .map(|item| (item.id.clone(), item.allowed_resolutions[0])).collect();
    let decisions = Decisions { token: inspection.token, resolutions };
    let report = export(volume.path(), &next_tracks, &next_playlists, &sync, Some(&decisions)).unwrap();
    assert!(report.verified);
    assert_eq!(report.tracks, 1);
}

#[test]
fn changed_tag_and_membership_are_independently_reviewable() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let audio = source_dir.path().join("track.mp3");
    std::fs::write(&audio, b"tag fixture").unwrap();
    let mut track = SourceTrack { id: 42, source_path: audio, title: "Track".into(), ..Default::default() };
    track.my_tags = vec![99];
    let tracks = vec![track];
    let playlists = vec![SourcePlaylist { id: 9, name: "Mix".into(), track_indices: vec![0], ..Default::default() }];
    let tags = vec![SourceMyTag { id: 99, seq: 1, name: "Library tag".into(), attribute: 1, parent: 0 }];
    let sync = SyncSource { db_id: 123, automatic: false, tree: vec![rbl_export::SyncNode { id: 9, parent: 0, attribute: 0 }] };
    export_with_tags(volume.path(), &tracks, &playlists, &tags, &sync, None).unwrap();
    edit_one_library(volume.path(), "UPDATE myTag SET name='Device tag' WHERE myTag_id=99; DELETE FROM myTag_content WHERE content_id=1");

    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &tags, &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    let changed_tag = inspection.diagnostics.iter().find(|item| item.entity == rb_model::conflict::EntityKind::Tag && item.kind == ConflictKind::MetadataChanged).unwrap();
    let changed_membership = inspection.diagnostics.iter().find(|item| item.fields.contains(&"my_tag_membership".to_owned())).unwrap();
    assert_eq!(changed_tag.allowed_resolutions, [Resolution::UseLibrary]);
    assert_eq!(changed_membership.allowed_resolutions, [Resolution::UseLibrary]);
    let decisions = Decisions {
        token: inspection.token,
        resolutions: [(changed_tag.id.clone(), Resolution::UseLibrary), (changed_membership.id.clone(), Resolution::UseLibrary)].into(),
    };
    export_with_tags(volume.path(), &tracks, &playlists, &tags, &sync, Some(&decisions)).unwrap();
    let library = rbl_onelibrary::ExportLibrary::open_read_only(&volume.path().join("PIONEER/rekordbox/exportLibrary.db")).unwrap();
    let name: String = library.connection().query_row("SELECT name FROM myTag WHERE myTag_id=99", [], |row| row.get(0)).unwrap();
    let members: i64 = library.connection().query_row("SELECT COUNT(*) FROM myTag_content WHERE content_id=1 AND myTag_id=99", [], |row| row.get(0)).unwrap();
    assert_eq!(name, "Library tag");
    assert_eq!(members, 1);
}

#[test]
fn use_library_decision_replaces_a_device_edited_playlist() {
    let source_dir = tempfile::tempdir().unwrap();
    let volume = common::stick();
    let (first_tracks, _, sync) = source(source_dir.path(), 123);
    let second_audio = source_dir.path().join("second.mp3");
    std::fs::write(&second_audio, b"second track fixture").unwrap();
    let second = SourceTrack { id: 43, source_path: second_audio, title: "Second".into(), ..Default::default() };
    let tracks = vec![first_tracks[0].clone(), second];
    let playlists = vec![
        SourcePlaylist { id: 9, name: "Mix".into(), track_indices: vec![0], ..Default::default() },
        SourcePlaylist { id: 10, name: "Other".into(), track_indices: vec![1], ..Default::default() },
    ];
    export(volume.path(), &tracks, &playlists, &sync, None).unwrap();
    edit_one_library(volume.path(), "INSERT INTO playlist_content VALUES(1,2,1)");
    let inspection = rbl_export::review::inspect_volume(volume.path(), &tracks, &playlists, &[], &ExportOptions { sync: Some(&sync), ..Default::default() }).unwrap();
    let playlist = inspection.diagnostics.iter().find(|item| item.kind == ConflictKind::PlaylistChanged && item.fields.contains(&"membership_or_order".to_owned())).unwrap();
    assert_eq!(playlist.allowed_resolutions, [Resolution::UseLibrary]);
    let decisions = Decisions { token: inspection.token, resolutions: [(playlist.id.clone(), Resolution::UseLibrary)].into() };
    export(volume.path(), &tracks, &playlists, &sync, Some(&decisions)).unwrap();
    let database = rbl_onelibrary::ExportLibrary::open_read_only(&volume.path().join("PIONEER/rekordbox/exportLibrary.db")).unwrap();
    let count: i64 = database.connection().query_row("SELECT COUNT(*) FROM playlist_content WHERE playlist_id=1", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}
