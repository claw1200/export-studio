//! Read-only conflict inspection over the same reconciled set the exporter writes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rb_model::conflict::{Diagnostic, EntityKind, ConflictKind, FieldEvidence, Inspection, PlanSummary, Resolution, Severity};

use crate::{incremental, reconcile, snapshot::{self, Playlist, Rewritten, Snapshot}, ExportError, ExportOptions, Manifest, Result, SourceMyTag, SourcePlaylist, SourceTrack};

const REVIEW_PROTOCOL_VERSION: u32 = 1;

fn diagnostic(
    id: String,
    kind: ConflictKind,
    severity: Severity,
    entity: EntityKind,
    entity_id: impl ToString,
    name: impl Into<String>,
    formats: Vec<String>,
    fields: Vec<String>,
    detail: impl Into<String>,
    allowed_resolutions: Vec<Resolution>,
) -> Diagnostic {
    Diagnostic {
        id,
        kind,
        severity,
        entity,
        entity_id: entity_id.to_string(),
        name: name.into(),
        formats,
        fields,
        evidence: Vec::new(),
        detail: detail.into(),
        allowed_resolutions,
    }
}

/// Inspect the reconciled export without recovering, staging, or writing the volume.
pub fn inspect_volume(
    destination: &Path,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    tags: &[SourceMyTag],
    options: &ExportOptions<'_>,
) -> Result<Inspection> {
    let destination = destination.canonicalize().map_err(|error| {
        ExportError::Io(std::io::Error::new(error.kind(), format!("Volume is not mounted: {}", destination.display())))
    })?;
    let _lease = rbl_core::durable::read_lease(&destination)?;
    let request = incremental::request(tracks, playlists, tags, options)?;
    if recovery_pending(&destination) {
        let token = incremental::digest(&(&destination, request, "recovery-required"))?;
        return Ok(Inspection {
            token,
            diagnostics: vec![diagnostic(
                "export:recovery".into(), ConflictKind::RecoveryRequired, Severity::Blocked,
                EntityKind::Export, "publication", "Interrupted USB export", Vec::new(), Vec::new(),
                "An interrupted export must be recovered before changes can be inspected.", Vec::new(),
            )],
            plan: None,
        });
    }

    let mut previous = Manifest::load(&destination);
    let db_id = options.sync.map_or(0, |source| source.db_id);
    upgrade_previous_db_id(&destination, &mut previous, db_id);
    let signatures = incremental::databases(&destination)?;
    let mut before = match previous.as_ref()
        .filter(|manifest| manifest.generation.as_ref().is_some_and(|g| g.databases == signatures))
        .and_then(|manifest| manifest.baseline.clone())
        .map_or_else(|| Snapshot::read(&destination), Ok) {
            Ok(snapshot) => snapshot,
            Err(ExportError::UnsupportedSchema(detail)) => return Ok(blocked_inspection(
                &destination, &request, ConflictKind::UnsupportedSchema, "device:schema", "Unsupported device database", detail,
            )?),
            Err(ExportError::IntegrityFailure(detail)) => return Ok(blocked_inspection(
                &destination, &request, ConflictKind::IntegrityFailure, "device:integrity", "Device database could not be read safely", detail,
            )?),
            Err(error) => return Err(error),
        };
    let record = reconcile::playlist_namespace(&destination, &mut before, previous.as_ref(), db_id);
    let rewritten = reconcile::rewritten(record.as_ref(), &before, previous.as_ref(), tracks, playlists, db_id);
    let plan = match crate::plan::from_baseline(&destination, &before, previous.as_ref(), tracks, playlists, options, record.as_ref()) {
        Ok(plan) => plan,
        Err(ExportError::Conflict(detail)) => return Ok(blocked_inspection(
            &destination, &request, ConflictKind::ExportPreflight, "export:preflight", "Export is blocked", detail,
        )?),
        Err(error) => return Err(error),
    };
    inspect_state(
        &destination, tracks, playlists, tags, options, &before, previous.as_ref(),
        &signatures, &plan.tracks, &plan.playlists, &rewritten,
        &plan.device_additions,
        Some(PlanSummary { tracks: plan.tracks.len(), playlists: plan.playlists.len(), estimated_peak_bytes: (!plan.unknown_size).then_some(plan.peak_bytes), approximate: plan.approximate }),
    )
}

fn blocked_inspection(
    destination: &Path,
    request: &str,
    kind: ConflictKind,
    id: &str,
    name: &str,
    detail: String,
) -> Result<Inspection> {
    let token = incremental::digest(&(destination, request, id, &detail))?;
    Ok(Inspection {
        token,
        diagnostics: vec![diagnostic(id.into(), kind, Severity::Blocked, EntityKind::Export, id, name, Vec::new(), Vec::new(), detail, Vec::new())],
        plan: None,
    })
}

pub(crate) fn upgrade_previous_db_id(destination: &Path, previous: &mut Option<Manifest>, db_id: u64) {
    if let Some(manifest) = previous.as_mut().filter(|manifest| manifest.db_id == 0 && db_id != 0) {
        if crate::sync_record::read(destination).is_some_and(|record| record.db_id == db_id) {
            manifest.db_id = db_id;
        }
    }
}

pub(crate) fn inspect_state(
    destination: &Path,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    tags: &[SourceMyTag],
    options: &ExportOptions<'_>,
    before: &Snapshot,
    previous: Option<&Manifest>,
    signatures: &BTreeMap<String, Option<String>>,
    planned_tracks: &[SourceTrack],
    planned_playlists: &[SourcePlaylist],
    rewritten: &Rewritten,
    device_additions: &BTreeSet<u32>,
    plan: Option<PlanSummary>,
) -> Result<Inspection> {
    let canonical_destination = destination.canonicalize().map_err(|error| {
        ExportError::Io(std::io::Error::new(error.kind(), format!("Volume is not mounted: {}", destination.display())))
    })?;
    let request = incremental::request(tracks, playlists, tags, options)?;
    let inputs = incremental::inputs(tracks)?;
    let analysis = snapshot::analysis_stamp(destination, before)?;
    let token = incremental::digest(&(
        &canonical_destination,
        &rbl_core::durable::root_fingerprint(&canonical_destination)?,
        &request,
        REVIEW_PROTOCOL_VERSION,
        format!("{:?}", options.root),
        options.verify_all,
        signatures,
        &inputs,
        &analysis,
        options.settings_files,
        previous,
        before,
        planned_tracks,
        planned_playlists,
    ))?;
    let diagnostics = collect(destination, before, previous, planned_tracks, planned_playlists, tags, rewritten, device_additions, options.sync.map_or(0, |sync| sync.db_id), &inputs)?;
    Ok(Inspection { token, diagnostics, plan })
}

fn recovery_pending(destination: &Path) -> bool {
    destination.join(".exportstudio-publication").exists()
}

pub fn validate_decisions(
    inspection: &Inspection,
    decisions: Option<&rb_model::conflict::Decisions>,
) -> Result<BTreeSet<String>> {
    let Some(decisions) = decisions else {
        if inspection.diagnostics.iter().any(|d| d.severity != Severity::Information) {
            return Err(ExportError::ReviewRequired(inspection.clone()));
        }
        return Ok(BTreeSet::new());
    };
    if decisions.token != inspection.token {
        return Err(ExportError::ReviewRequired(inspection.clone()));
    }
    let mut accepted = BTreeSet::new();
    for (id, resolution) in &decisions.resolutions {
        let Some(conflict) = inspection.diagnostics.iter().find(|d| &d.id == id) else {
            return Err(ExportError::ReviewRequired(inspection.clone()));
        };
        if conflict.severity != Severity::DecisionRequired || !conflict.allowed_resolutions.contains(resolution) {
            return Err(ExportError::ReviewRequired(inspection.clone()));
        }
        accepted.insert(id.clone());
    }
    if inspection.diagnostics.iter().any(|d| d.severity == Severity::Blocked)
        || inspection.diagnostics.iter().any(|d| d.severity == Severity::DecisionRequired && !accepted.contains(&d.id))
    {
        return Err(ExportError::ReviewRequired(inspection.clone()));
    }
    Ok(accepted)
}

pub(crate) fn apply_library_playlist_decisions(
    tracks: &[SourceTrack],
    playlists: &mut [SourcePlaylist],
    source_tracks: &[SourceTrack],
    source_playlists: &[SourcePlaylist],
    inspection: &Inspection,
    decisions: Option<&rb_model::conflict::Decisions>,
) -> Result<()> {
    let Some(decisions) = decisions else { return Ok(()) };
    for diagnostic in &inspection.diagnostics {
        if diagnostic.kind != ConflictKind::PlaylistChanged
            || decisions.resolutions.get(&diagnostic.id) != Some(&Resolution::UseLibrary)
        {
            continue;
        }
        let device_id = diagnostic.entity_id.parse::<u32>().map_err(|_| ExportError::ReviewRequired(inspection.clone()))?;
        let Some(target) = playlists.iter_mut().find(|playlist| playlist.device_id == device_id) else {
            return Err(ExportError::ReviewRequired(inspection.clone()));
        };
        let Some(source) = source_playlists.iter().find(|playlist| playlist.id == target.id) else {
            return Err(ExportError::ReviewRequired(inspection.clone()));
        };
        let mut members = Vec::with_capacity(source.track_indices.len());
        for source_index in &source.track_indices {
            let Some(source_track) = source_tracks.get(*source_index) else {
                return Err(ExportError::ReviewRequired(inspection.clone()));
            };
            let key = crate::track_key(source_track.id, &source_track.source_path.to_string_lossy());
            let Some(target_index) = tracks.iter().position(|track| crate::track_key(track.id, &track.source_path.to_string_lossy()) == key) else {
                return Err(ExportError::ReviewRequired(inspection.clone()));
            };
            members.push(target_index);
        }
        target.name.clone_from(&source.name);
        target.parent_id = source.parent_id;
        target.folder = source.folder;
        target.track_indices = members;
        target.entries_changed_ms = source.entries_changed_ms;
        target.device_only = false;
    }
    Ok(())
}

fn collect(
    destination: &Path,
    current: &Snapshot,
    previous: Option<&Manifest>,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    tags: &[SourceMyTag],
    rewritten: &Rewritten,
    device_additions: &BTreeSet<u32>,
    db_id: u64,
    inputs: &BTreeMap<String, Option<incremental::FileRevision>>,
) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    if previous.as_ref().is_none_or(|m| m.baseline.is_none()) {
        if current.legacy.is_some() || current.one.is_some() {
            diagnostics.push(diagnostic(
                "export:baseline-unknown".into(), ConflictKind::BaselineUnknown, Severity::Information,
                EntityKind::Device, "usb", "Existing USB state has no trusted Export Studio baseline",
                Vec::new(), Vec::new(), "Unowned content will be retained unless an explicit safe rewrite is selected.", Vec::new(),
            ));
        }
    }
    if let Some(previous) = previous {
        if previous.db_id != db_id && (!previous.tracks.is_empty() || !previous.playlists.is_empty()) {
            diagnostics.push(diagnostic(
                "device:ownership".into(), ConflictKind::OwnershipMismatch, Severity::Blocked,
                EntityKind::Device, "usb", "USB belongs to a different master library", Vec::new(), Vec::new(),
                "Import or reconcile this device in Rekordbox before changing its ownership.", Vec::new(),
            ));
        }
        if let Some(baseline) = &previous.baseline {
            if current.identity.iter().any(|(id, owner)| baseline.identity.get(id).is_some_and(|old| old != owner)) {
                diagnostics.push(diagnostic(
                    "device:identity".into(), ConflictKind::IdentityChanged, Severity::Blocked,
                    EntityKind::Device, "usb", "Device track identities changed", Vec::new(), Vec::new(),
                    "Reconcile the device before reusing its previous selection.", Vec::new(),
                ));
            }
            collect_device_edits(current, baseline, tracks, playlists, previous, device_additions, &mut diagnostics);
            collect_tag_edits(current, baseline, tracks, tags, previous, &mut diagnostics);
            collect_cue_edits(current, baseline, tracks, previous, &mut diagnostics);
            collect_analysis_edits(destination, tracks, previous, &mut diagnostics)?;
            }
    }
            collect_history_losses(current, tracks, previous, db_id, &mut diagnostics);
    collect_unowned_device_content(current, previous, tracks, playlists, &mut diagnostics);
    collect_format_differences(current, rewritten, tracks, playlists, &mut diagnostics);
    if let Some(previous) = previous {
        collect_asset_changes(tracks, previous, inputs, &mut diagnostics);
    }
    for track in tracks.iter().filter(|track| !track.source_path.is_file()) {
        diagnostics.push(diagnostic(
            format!("asset:track:{}", track.id), ConflictKind::AssetUnavailable, Severity::Information,
            EntityKind::Track, track.id, &track.title, Vec::new(), vec!["audio".into()],
            if track.source_unavailable { "The original source is unavailable; the existing device copy is retained." } else { "The source file is unavailable; this track is skipped while the rest of the export continues." }, Vec::new(),
        ));
    }
    diagnostics.sort_by(|a, b| a.id.cmp(&b.id));
    let mut merged: Vec<Diagnostic> = Vec::with_capacity(diagnostics.len());
    for diagnostic in diagnostics {
        if let Some(previous) = merged.last_mut().filter(|previous| previous.id == diagnostic.id) {
            previous.formats.extend(diagnostic.formats);
            previous.formats.sort();
            previous.formats.dedup();
            previous.fields.extend(diagnostic.fields);
            previous.fields.sort();
            previous.fields.dedup();
            previous.evidence.extend(diagnostic.evidence);
            previous.evidence.sort_by(|left, right| left.field.cmp(&right.field));
            previous.allowed_resolutions.extend(diagnostic.allowed_resolutions);
            previous.allowed_resolutions.sort();
            previous.allowed_resolutions.dedup();
            if diagnostic.severity == Severity::Blocked {
                previous.severity = Severity::Blocked;
            } else if diagnostic.severity == Severity::DecisionRequired && previous.severity == Severity::Information {
                previous.severity = Severity::DecisionRequired;
            }
            if previous.severity == Severity::Blocked {
                previous.allowed_resolutions.clear();
            }
        } else {
            merged.push(diagnostic);
        }
    }
    Ok(merged)
}
fn collect_unowned_device_content(
    current: &Snapshot,
    previous: Option<&Manifest>,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    out: &mut Vec<Diagnostic>,
) {
    let baseline = previous.and_then(|manifest| manifest.baseline.as_ref());
    let planned_tracks = selected_track_device_ids(tracks, previous);
    let planned_playlists: BTreeSet<u32> = playlists.iter().filter_map(|playlist| (playlist.device_id != 0).then_some(playlist.device_id)).collect();
    for (format, library, old) in [
        ("Device Library", current.legacy.as_ref(), baseline.and_then(|snapshot| snapshot.legacy.as_ref())),
        ("OneLibrary", current.one.as_ref(), baseline.and_then(|snapshot| snapshot.one.as_ref())),
    ] {
        let Some(library) = library else { continue };
        for track in &library.tracks {
            if old.is_some_and(|old| old.tracks.iter().any(|row| row.id == track.id)) || planned_tracks.contains(&track.id) { continue; }
            out.push(diagnostic(
                format!("device_only:{format}:track:{}", track.id), ConflictKind::DeviceOnlyContent,
                Severity::Blocked, EntityKind::Track, track.id, &track.title, vec![format.into()], Vec::new(),
                "This device-only track is not part of the reconciled export and would be lost.", Vec::new(),
            ));
        }
        for playlist in &library.playlists {
            if old.is_some_and(|old| old.playlists.iter().any(|row| row.id == playlist.id)) || planned_playlists.contains(&playlist.id) { continue; }
            out.push(diagnostic(
                format!("device_only:{format}:playlist:{}", playlist.id), ConflictKind::DeviceOnlyContent,
                Severity::Blocked, if playlist.folder { EntityKind::Folder } else { EntityKind::Playlist },
                playlist.id, &playlist.name, vec![format.into()], Vec::new(),
                "This device-only playlist is not part of the reconciled export and would be lost.", Vec::new(),
            ));
        }
    }
}

fn collect_device_edits(
    current: &Snapshot,
    baseline: &Snapshot,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    previous: &Manifest,
    device_additions: &BTreeSet<u32>,
    out: &mut Vec<Diagnostic>,
) {
    let planned_track_ids = selected_track_device_ids(tracks, Some(previous));
    for (format, now, then) in [
        ("Device Library", current.legacy.as_ref(), baseline.legacy.as_ref()),
        ("OneLibrary", current.one.as_ref(), baseline.one.as_ref()),
    ] {
        let (Some(now), Some(then)) = (now, then) else { continue };
        for row in &now.tracks {
            let Some(old) = then.tracks.iter().find(|old| old.id == row.id) else { continue };
            let mut fields = snapshot::track_differences(old, row);
            if format == "OneLibrary" && row.rating > 5 && old.rating <= 5 && row.rating == old.rating * 51 {
                fields.retain(|field| *field != "rating");
            }
            if fields.is_empty() { continue; }
            let source = tracks.iter().find(|track| track.device.as_ref().is_some_and(|device| device.id == row.id));
            let selected = source.is_some_and(|track| !track.source_unavailable && !track.device.as_ref().is_some_and(|device| device.preserve));
            let unchanged_target = source.is_some_and(|track| {
                source_matches_track(track, row, format)
                    && !fields.contains(&"file path")
                    && !fields.contains(&"analysis path")
            });
            let severity = if unchanged_target || !selected && planned_track_ids.contains(&row.id) {
                Severity::Information
            } else {
                Severity::DecisionRequired
            };
            let resolutions = if severity == Severity::DecisionRequired {
                vec![if selected { Resolution::UseLibrary } else { Resolution::DiscardDeviceEdits }]
            } else { Vec::new() };
            let mut item = diagnostic(
                format!("metadata:{format}:track:{}", row.id), ConflictKind::MetadataChanged,
                severity, EntityKind::Track, row.id, &row.title,
                vec![format.to_owned()], fields.iter().map(|field| (*field).to_owned()).collect(),
                if severity == Severity::Information { "This device-side difference is preserved by the current plan." } else if selected { "This track changed on the device and the selected library record differs." } else { "This device-side edit would be lost when the track leaves the sync selection." },
                resolutions,
            );
            item.evidence = fields.iter().map(|field| FieldEvidence {
                field: (*field).to_owned(),
                baseline: Some(track_field(old, field)),
                device: Some(track_field(row, field)),
                library: source.map(|track| source_field(track, field, format)),
            }).collect();
            out.push(item);
        }
        for old in &then.tracks {
            if now.tracks.iter().any(|row| row.id == old.id) { continue; }
            let selected_library_id = previous.tracks.iter().find(|row| row.export_id == old.id).map(|row| row.library_id);
            if selected_library_id.is_some_and(|id| tracks.iter().any(|track| track.id == id)) {
                let mut item = diagnostic(
                    format!("deletion:{format}:track:{}", old.id), ConflictKind::DeviceDeletion,
                    Severity::DecisionRequired, EntityKind::Track, old.id, &old.title, vec![format.to_owned()], Vec::new(),
                    "This library track was deleted from the device and will be restored.", vec![Resolution::RestoreFromLibrary],
                );
                item.evidence.push(FieldEvidence { field: "existence".into(), baseline: Some("present".into()), device: Some("deleted".into()), library: Some("selected".into()) });
                out.push(item);
            }
        }
        for row in &now.playlists {
            let Some(old) = then.playlists.iter().find(|old| old.id == row.id) else { continue };
            let fields = playlist_differences(old, row);
            if fields.is_empty() { continue; }
            let target = playlists.iter().find(|playlist| playlist.device_id == row.id);
            let selected = target.is_some_and(|playlist| !playlist.device_only);
            let matches_target = target.is_some_and(|playlist| playlist_matches_target(playlist, playlists, tracks, row));
            let reconciled_addition = device_additions.contains(&row.id);
            let severity = if matches_target || reconciled_addition { Severity::Information } else { Severity::DecisionRequired };
            let mut item = diagnostic(
                format!("playlist:{format}:{}", row.id), ConflictKind::PlaylistChanged,
                severity, if row.folder { EntityKind::Folder } else { EntityKind::Playlist },
                row.id, &row.name, vec![format.to_owned()], fields,
                if reconciled_addition && !matches_target { "The library playlist changed; the exporter will use it and retain device-added tracks separately." } else if severity == Severity::Information { "This device-side playlist is preserved by the current plan." } else if selected { "This selected playlist changed on the device; the library version can replace it." } else { "This device-side playlist edit would be lost when it leaves the sync selection." },
                if severity == Severity::Information { Vec::new() } else { vec![if selected { Resolution::UseLibrary } else { Resolution::DiscardDeviceEdits }] },
            );
            item.evidence = playlist_evidence(old, row, target, tracks);
            out.push(item);
        }
        for old in &then.playlists {
            if now.playlists.iter().any(|row| row.id == old.id) { continue; }
            let selected_library_id = previous.playlists.iter().find(|row| row.export_id == old.id).map(|row| row.library_id);
            if selected_library_id.is_some_and(|id| playlists.iter().any(|playlist| playlist.id == id)) {
                let mut item = diagnostic(
                    format!("deletion:{format}:playlist:{}", old.id), ConflictKind::DeviceDeletion,
                    Severity::DecisionRequired, if old.folder { EntityKind::Folder } else { EntityKind::Playlist },
                    old.id, &old.name, vec![format.to_owned()], Vec::new(),
                    "This selected playlist was deleted from the device and will be restored.", vec![Resolution::RestoreFromLibrary],
                );
                item.evidence.push(FieldEvidence { field: "existence".into(), baseline: Some("present".into()), device: Some("deleted".into()), library: Some("selected".into()) });
                out.push(item);
            }
        }
    }
}

fn source_matches_track(source: &SourceTrack, device: &snapshot::Track, format: &str) -> bool {
    let library_rating = if format == "OneLibrary" {
        u32::from(snapshot::onelibrary_stars(i64::from(source.rating)))
    } else {
        u32::from(source.rating)
    };
    source.title == device.title
        && source.artist == device.artist
        && source.album == device.album
        && source.genre == device.genre
        && source.label == device.label
        && source.key == device.key
        && source.comment == device.comment
        && source.bpm_x100 == device.bpm
        && library_rating == device.rating
        && u32::from(source.color_id) == device.color
}

fn track_field(track: &snapshot::Track, field: &str) -> String {
    match field {
        "title" => track.title.clone(),
        "artist" => track.artist.clone(),
        "album" => track.album.clone(),
        "genre" => track.genre.clone(),
        "label" => track.label.clone(),
        "key" => track.key.clone(),
        "comment" => track.comment.clone(),
        "file path" => track.path.clone(),
        "analysis path" => track.analysis.clone(),
        "BPM" => track.bpm.to_string(),
        "rating" => track.rating.to_string(),
        "colour" => track.color.to_string(),
        _ => String::new(),
    }
}

fn source_field(track: &SourceTrack, field: &str, format: &str) -> String {
    match field {
        "title" => track.title.clone(),
        "artist" => track.artist.clone(),
        "album" => track.album.clone(),
        "genre" => track.genre.clone(),
        "label" => track.label.clone(),
        "key" => track.key.clone(),
        "comment" => track.comment.clone(),
        "file path" => track.source_path.to_string_lossy().into_owned(),
        "analysis path" => track.analysis_source.as_ref().map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
        "BPM" => track.bpm_x100.to_string(),
        "rating" => if format == "OneLibrary" { snapshot::onelibrary_stars(i64::from(track.rating)).to_string() } else { track.rating.to_string() },
        "colour" => track.color_id.to_string(),
        _ => String::new(),
    }
}

fn playlist_evidence(old: &Playlist, device: &Playlist, library: Option<&SourcePlaylist>, tracks: &[SourceTrack]) -> Vec<FieldEvidence> {
    let values = [
        ("name", Some(old.name.clone()), Some(device.name.clone()), library.map(|row| row.name.clone())),
        ("parent", Some(old.parent.to_string()), Some(device.parent.to_string()), library.map(|row| row.parent_id.to_string())),
        ("kind", Some(if old.folder { "folder" } else { "playlist" }.into()), Some(if device.folder { "folder" } else { "playlist" }.into()), library.map(|row| if row.folder { "folder".into() } else { "playlist".into() })),
        ("membership_or_order", Some(format!("{:?}", old.tracks)), Some(format!("{:?}", device.tracks)), library.map(|row| {
            let entries: Vec<String> = row.track_indices.iter().filter_map(|index| tracks.get(*index))
                .map(|track| crate::track_key(track.id, &track.source_path.to_string_lossy())).collect();
            format!("{entries:?}")
        })),
    ];
    values.into_iter().filter(|(_, before, after, _)| before != after).map(|(field, baseline, current, library)| FieldEvidence {
        field: field.into(), baseline, device: current, library,
    }).collect()
}

fn playlist_matches_target(
    target: &SourcePlaylist,
    all: &[SourcePlaylist],
    tracks: &[SourceTrack],
    device: &Playlist,
) -> bool {
    let parent = if target.parent_id == 0 { 0 } else {
        all.iter().find(|playlist| playlist.id == target.parent_id).map_or(0, |playlist| playlist.device_id)
    };
    let members: Vec<u32> = target.track_indices.iter().filter_map(|index| tracks.get(*index))
        .filter_map(|track| track.device.as_ref().map(|device| device.id)).collect();
    target.name == device.name && target.folder == device.folder && parent == device.parent && members == device.tracks
}

fn selected_track_device_ids(tracks: &[SourceTrack], previous: Option<&Manifest>) -> BTreeSet<u32> {
    let mut ids: BTreeSet<u32> = tracks.iter().filter_map(|track| track.device.as_ref().map(|device| device.id)).collect();
    ids.extend(previous.into_iter().flat_map(|manifest| &manifest.tracks).filter(|old| tracks.iter().any(|track| {
        crate::track_key(track.id, &track.source_path.to_string_lossy()) == old.key()
    })).map(|old| old.export_id));
    ids
}

fn playlist_differences(old: &Playlist, current: &Playlist) -> Vec<String> {
    let mut fields = Vec::new();
    if old.name != current.name { fields.push("name".into()); }
    if old.parent != current.parent { fields.push("parent".into()); }
    if old.folder != current.folder { fields.push("kind".into()); }
    if old.tracks != current.tracks { fields.push("membership_or_order".into()); }
    fields
}

fn collect_tag_edits(
    current: &Snapshot,
    baseline: &Snapshot,
    tracks: &[SourceTrack],
    tags: &[SourceMyTag],
    previous: &Manifest,
    out: &mut Vec<Diagnostic>,
) {
    for tag in &current.my_tags {
        let Some(old) = baseline.my_tags.iter().find(|old| old.id == tag.id) else { continue };
        if old == tag { continue; }
        let severity = if tags.iter().any(|source| source.id == tag.id && source != tag) {
            Severity::DecisionRequired
        } else {
            Severity::Information
        };
        let mut item = diagnostic(
            format!("tag:{}", tag.id), ConflictKind::MetadataChanged, severity, EntityKind::Tag,
            tag.id, &tag.name, vec!["OneLibrary".into()], vec!["name_or_hierarchy".into()],
            if severity == Severity::DecisionRequired { "This My Tag changed on the device and the library version differs." } else { "The device-only tag is preserved by the current plan." },
            if severity == Severity::DecisionRequired { vec![Resolution::UseLibrary] } else { Vec::new() },
        );
        item.evidence.push(FieldEvidence {
            field: "name_or_hierarchy".into(),
            baseline: Some(format!("name={}, parent={}, attribute={}", old.name, old.parent, old.attribute)),
            device: Some(format!("name={}, parent={}, attribute={}", tag.name, tag.parent, tag.attribute)),
            library: tags.iter().find(|source| source.id == tag.id).map(|source| format!("name={}, parent={}, attribute={}", source.name, source.parent, source.attribute)),
        });
        out.push(item);
    }
    let old_memberships = &baseline.tag_memberships;
    let membership_ids: BTreeSet<u32> = old_memberships.keys().chain(current.tag_memberships.keys()).copied().collect();
    for device_id in membership_ids {
        let current_tags = current.tag_memberships.get(&device_id).cloned().unwrap_or_default();
        if old_memberships.get(&device_id).is_some_and(|old| *old == current_tags) { continue; }
        let library_id = current.identity.get(&device_id)
            .filter(|(master, _)| *master == previous.db_id)
            .map(|(_, content)| *content)
            .or_else(|| previous.tracks.iter().find(|track| track.export_id == device_id).map(|track| track.library_id))
            .unwrap_or(0);
        let selected = tracks.iter().find(|track| {
            track.id == library_id || track.device.as_ref().is_some_and(|device| device.id == device_id)
        });
        let expected = selected.map(|track| track.my_tags.iter().copied().collect::<BTreeSet<_>>());
        if expected.as_ref().is_some_and(|expected| expected.iter().copied().collect::<Vec<_>>() == current_tags) { continue; }
        let resolution = if selected.is_some() { Resolution::UseLibrary } else { Resolution::DiscardDeviceEdits };
        let name = current.one.iter().flat_map(|library| &library.tracks).find(|track| track.id == device_id)
            .map_or_else(|| format!("USB track {device_id}"), |track| track.title.clone());
        let mut item = diagnostic(
            format!("tag_membership:{device_id}"), ConflictKind::MetadataChanged, Severity::DecisionRequired,
            EntityKind::Track, library_id, name, vec!["OneLibrary".into()], vec!["my_tag_membership".into()],
            "My Tag membership changed on the device.", vec![resolution],
        );
        item.evidence.push(FieldEvidence {
            field: "my_tag_membership".into(),
            baseline: old_memberships.get(&device_id).map(|old| format!("{old:?}")),
            device: Some(format!("{current_tags:?}")),
            library: expected.as_ref().map(|expected| format!("{expected:?}")),
        });
        out.push(item);
    }
}

fn collect_cue_edits(current: &Snapshot, baseline: &Snapshot, tracks: &[SourceTrack], previous: &Manifest, out: &mut Vec<Diagnostic>) {
    let old: BTreeSet<_> = baseline.cues.iter().collect();
    let now: BTreeSet<_> = current.cues.iter().collect();
    let kept = selected_track_device_ids(tracks, Some(previous));
    let added: Vec<_> = now.difference(&old).copied().collect();
    let removed: Vec<_> = old.difference(&now).copied().collect();
    let unowned_rows: Vec<_> = added.iter().chain(&removed).filter(|row| snapshot::cue_content(row).is_none()).collect();
    if !unowned_rows.is_empty() {
        out.push(diagnostic(
            format!("cue_grid:unknown:{}", crate::manifest::hash(unowned_rows.iter().flat_map(|row| row.bytes()).collect::<Vec<_>>().as_slice())),
            ConflictKind::CueGridChanged, Severity::Blocked, EntityKind::Export, "unknown-cue-rows",
            "Unrecognized device cue data", vec!["OneLibrary".into()], vec!["unmapped_cue_rows".into()],
            "Changed cue rows could not be mapped to device track identities; the sync cannot prove they will be retained.", Vec::new(),
        ));
    }
    let mut by_track = BTreeMap::<u32, (usize, usize)>::new();
    for row in added {
        if let Some(id) = snapshot::cue_content(row) { by_track.entry(id).or_default().0 += 1; }
    }
    for row in removed {
        if let Some(id) = snapshot::cue_content(row) { by_track.entry(id).or_default().1 += 1; }
    }
    for (id, (added_count, removed_count)) in by_track {
        let title = current.one.iter().flat_map(|library| &library.tracks).find(|track| track.id == id).map_or_else(|| format!("USB track {id}"), |track| track.title.clone());
        let source = tracks.iter().find(|track| track.device.as_ref().is_some_and(|device| device.id == id));
        let retained = kept.contains(&id);
        let severity = if retained || added_count == 0 { Severity::Information } else { Severity::DecisionRequired };
        let resolutions = if severity == Severity::Information { Vec::new() } else { vec![Resolution::DiscardDeviceEdits] };
        let library_cues = source.and_then(|track| track.cues.as_ref()).map(Vec::len);
        let detail = if !kept.contains(&id) {
            if added_count > 0 { "Device cue edits would be lost with this removed track." } else { "Device cue rows were removed on the device." }
        } else if source.is_some_and(|track| track.cues.is_some()) {
            "The library's cue list replaces device edits for this retained track."
        } else {
            "The library has no cue list for this track, so usable device cue data is retained."
        };
        let baseline_rows: Vec<_> = baseline.cues.iter().filter(|row| snapshot::cue_content(row) == Some(id)).collect();
        let current_rows: Vec<_> = current.cues.iter().filter(|row| snapshot::cue_content(row) == Some(id)).collect();
        let mut item = diagnostic(
            format!("cue_grid:track:{id}"), ConflictKind::CueGridChanged, severity,
            EntityKind::Track, id, title, vec!["OneLibrary".into()], vec!["cues".into()],
            detail,
            resolutions,
        );
        item.evidence.push(FieldEvidence {
            field: "cue_rows".into(),
            baseline: Some(format!("{} rows", baseline_rows.len())),
            device: Some(format!("{} rows; {added_count} added, {removed_count} removed", current_rows.len())),
            library: library_cues.map(|count| format!("{count} source cues")),
        });
        out.push(item);
    }
}

fn collect_analysis_edits(
    destination: &Path,
    tracks: &[SourceTrack],
    previous: &Manifest,
    out: &mut Vec<Diagnostic>,
) -> Result<()> {
    let Some(generation) = previous.generation.as_ref() else { return Ok(()) };
    let selected = selected_track_device_ids(tracks, Some(previous));
    for track in &previous.tracks {
        if track.anlz_dir.is_empty() { continue; }
        let mut changed = Vec::new();
        for extension in &track.analysis_extensions {
            let relative = format!("{}/ANLZ0000.{extension}", track.anlz_dir.trim_start_matches('/'));
            let Some(baseline) = generation.outputs.get(&relative) else { continue };
            let current = incremental::copy_revision_fresh(&crate::checked_under(destination, &relative)?)?;
            if &current != baseline {
                changed.push(FieldEvidence {
                    field: format!("ANLZ0000.{extension}"),
                    baseline: baseline.as_ref().map(|revision| format!("{} bytes, mtime {}", revision.size, revision.modified)),
                    device: current.as_ref().map(|revision| format!("{} bytes, mtime {}", revision.size, revision.modified)),
                    library: None,
                });
            }
        }
        if changed.is_empty() { continue; }
        let retained = selected.contains(&track.export_id);
        let name = tracks.iter().find(|source| source.id == track.library_id).map(|source| source.title.clone()).unwrap_or_else(|| track.source.clone());
        let mut item = diagnostic(
            format!("cue_grid:track:{}", track.export_id), ConflictKind::CueGridChanged,
            if retained { Severity::Information } else { Severity::DecisionRequired },
            EntityKind::Track, track.library_id, name, vec!["ANLZ".into()],
            changed.iter().map(|evidence| evidence.field.clone()).collect(),
            if retained { "Device analysis changed. The library version is used when present; otherwise usable device analysis is retained." } else { "Device analysis changed and would be removed with this track." },
            if retained { Vec::new() } else { vec![Resolution::DiscardDeviceEdits] },
        );
        item.evidence = changed;
        out.push(item);
    }
    Ok(())
}

fn collect_history_losses(current: &Snapshot, tracks: &[SourceTrack], previous: Option<&Manifest>, db_id: u64, out: &mut Vec<Diagnostic>) {
    let mut retained = selected_track_device_ids(tracks, previous);
    retained.extend(current.identity.iter().filter_map(|(device_id, (master_db_id, content_id))| {
        (*master_db_id == db_id && tracks.iter().any(|track| track.id != 0 && track.id == *content_id)).then_some(*device_id)
    }));
    let missing: BTreeSet<u32> = current.history.iter().flat_map(|history| &history.tracks).filter(|id| !retained.contains(id)).copied().collect();
    for id in missing {
        let title = current.one.iter().flat_map(|library| &library.tracks).find(|track| track.id == id).map_or_else(|| format!("USB track {id}"), |track| track.title.clone());
        let histories: Vec<_> = current.history.iter().filter(|history| history.tracks.contains(&id)).map(|history| history.name.as_str()).collect();
        let mut item = diagnostic(
            format!("history:track:{id}"), ConflictKind::HistoryReference, Severity::Blocked,
            EntityKind::Track, id, title, vec!["OneLibrary".into()], vec!["history_reference".into()],
            "USB play history still references this track. Import and clear the history before removing it.", Vec::new(),
        );
        item.evidence.push(FieldEvidence { field: "history_reference".into(), baseline: None, device: Some(format!("referenced by {histories:?}")), library: None });
        out.push(item);
    }
}

fn collect_format_differences(
    current: &Snapshot,
    rewritten: &Rewritten,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    out: &mut Vec<Diagnostic>,
) {
    let (Some(legacy), Some(one)) = (&current.legacy, &current.one) else { return };
    let one_tracks: BTreeMap<_, _> = one.tracks.iter().map(|track| (track.id, track)).collect();
    for track in &legacy.tracks {
        let Some(other) = one_tracks.get(&track.id) else {
            let is_rewritten = rewritten.tracks.contains(&track.id)
                || tracks.iter().any(|source| source.device.as_ref().is_some_and(|device| device.id == track.id && device.preserve));
            let mut item = diagnostic(
                format!("format:track:{}:missing-one-library", track.id), ConflictKind::CrossFormatDivergence,
                if is_rewritten { Severity::Information } else { Severity::Blocked }, EntityKind::Track,
                track.id, &track.title, vec!["Device Library".into(), "OneLibrary".into()], vec!["missing_in_one_library".into()],
                if is_rewritten { "The selected track will be written to both formats." } else { "This track exists in only one device format and is not selected for a safe rewrite." }, Vec::new(),
            );
            item.evidence.push(FieldEvidence { field: "existence".into(), baseline: None, device: Some("present".into()), library: Some("absent".into()) });
            out.push(item);
            continue;
        };
        let fields = snapshot::track_differences(track, other);
        if fields.is_empty() { continue; }
        let rewritten = rewritten.tracks.contains(&track.id);
        let mut item = diagnostic(
            format!("format:track:{}", track.id), ConflictKind::CrossFormatDivergence,
            if rewritten { Severity::Information } else { Severity::Blocked }, EntityKind::Track,
            track.id, &track.title, vec!["Device Library".into(), "OneLibrary".into()], fields.iter().map(|field| (*field).to_owned()).collect(),
            if rewritten { "The selected library record will rewrite this track consistently in both device formats." } else { "The two device libraries disagree; ownership cannot be inferred safely." }, Vec::new(),
        );
        item.evidence = fields.iter().map(|field| FieldEvidence {
            field: (*field).into(), baseline: None,
            device: Some(track_field(track, field)), library: Some(track_field(other, field)),
        }).collect();
        out.push(item);
    }
    let legacy_ids: BTreeSet<u32> = legacy.tracks.iter().map(|track| track.id).collect();
    for track in &one.tracks {
        if legacy_ids.contains(&track.id) { continue; }
        let is_rewritten = rewritten.tracks.contains(&track.id)
            || tracks.iter().any(|source| source.device.as_ref().is_some_and(|device| device.id == track.id && device.preserve));
        let mut item = diagnostic(
            format!("format:track:{}:missing-device-library", track.id), ConflictKind::CrossFormatDivergence,
            if is_rewritten { Severity::Information } else { Severity::Blocked }, EntityKind::Track,
            track.id, &track.title, vec!["Device Library".into(), "OneLibrary".into()], vec!["missing_in_device_library".into()],
            if is_rewritten { "The selected track will be written to both formats." } else { "This track exists in only one device format and is not selected for a safe rewrite." }, Vec::new(),
        );
        item.evidence.push(FieldEvidence { field: "existence".into(), baseline: None, device: Some("absent".into()), library: Some("present".into()) });
        out.push(item);
    }
    let ids = snapshot::legacy_playlist_ids(legacy, one, 0);
    let one_playlists: BTreeMap<_, _> = one.playlists.iter().map(|playlist| (playlist.id, playlist)).collect();
    for playlist in &legacy.playlists {
        let id = ids.get(&playlist.id).copied().unwrap_or(playlist.id);
        let Some(other) = one_playlists.get(&id) else {
            let selected = rewritten.playlists.contains(&playlist.id) || rewritten.playlists.contains(&id)
                || playlists.iter().any(|source| source.device_only && source.device_id == playlist.id);
            let mut item = diagnostic(
                format!("format:playlist:{}:missing-one-library", playlist.id), ConflictKind::CrossFormatDivergence,
                if selected { Severity::Information } else { Severity::Blocked },
                if playlist.folder { EntityKind::Folder } else { EntityKind::Playlist }, playlist.id, &playlist.name,
                vec!["Device Library".into(), "OneLibrary".into()], vec!["missing_in_one_library".into()],
                if selected { "The selected playlist will be written to both formats." } else { "This playlist exists in only one device format and is not selected for a safe rewrite." }, Vec::new(),
            );
            item.evidence.push(FieldEvidence { field: "existence".into(), baseline: None, device: Some("present".into()), library: Some("absent".into()) });
            out.push(item);
            continue;
        };
        let fields = playlist_differences(playlist, other);
        if fields.is_empty() { continue; }
        let selected = rewritten.playlists.contains(&playlist.id) || rewritten.playlists.contains(&id);
        let mut item = diagnostic(
            format!("format:playlist:{}", playlist.id), ConflictKind::CrossFormatDivergence,
            if selected { Severity::Information } else { Severity::Blocked },
            if playlist.folder { EntityKind::Folder } else { EntityKind::Playlist }, playlist.id, &playlist.name,
            vec!["Device Library".into(), "OneLibrary".into()], fields.clone(),
            if selected { "The selected library playlist will rewrite this row in both formats." } else { "The device formats disagree and this playlist is not selected for a safe rewrite." }, Vec::new(),
        );
        item.evidence = fields.iter().map(|field| FieldEvidence {
            field: field.clone(), baseline: None,
            device: Some(playlist_field(playlist, field)), library: Some(playlist_field(other, field)),
        }).collect();
        out.push(item);
    }
    let paired_one: BTreeSet<u32> = ids.values().copied().collect();
    for playlist in &one.playlists {
        if paired_one.contains(&playlist.id) { continue; }
        let selected = rewritten.playlists.contains(&playlist.id)
            || playlists.iter().any(|source| source.device_only && source.device_id == playlist.id);
        let mut item = diagnostic(
            format!("format:playlist:{}:missing-device-library", playlist.id), ConflictKind::CrossFormatDivergence,
            if selected { Severity::Information } else { Severity::Blocked },
            if playlist.folder { EntityKind::Folder } else { EntityKind::Playlist }, playlist.id, &playlist.name,
            vec!["Device Library".into(), "OneLibrary".into()], vec!["missing_in_device_library".into()],
            if selected { "The selected playlist will be written to both formats." } else { "This playlist exists in only one device format and is not selected for a safe rewrite." }, Vec::new(),
        );
        item.evidence.push(FieldEvidence { field: "existence".into(), baseline: None, device: Some("absent".into()), library: Some("present".into()) });
        out.push(item);
    }
}

fn playlist_field(playlist: &Playlist, field: &str) -> String {
    match field {
        "name" => playlist.name.clone(),
        "parent" => playlist.parent.to_string(),
        "kind" => if playlist.folder { "folder".into() } else { "playlist".into() },
        "membership_or_order" => format!("{:?}", playlist.tracks),
        _ => String::new(),
    }
}

fn collect_asset_changes(
    tracks: &[SourceTrack],
    previous: &Manifest,
    current: &BTreeMap<String, Option<incremental::FileRevision>>,
    out: &mut Vec<Diagnostic>,
) {
    let Some(old) = previous.generation.as_ref() else { return };
    for track in tracks {
        let mut assets = vec![(track.source_path.clone(), "audio")];
        if let Some(analysis) = track.analysis_source.as_ref() {
            assets.extend(["DAT", "EXT", "2EX"].map(|extension| (analysis.with_extension(extension), "analysis")));
        }
        if let Some(artwork) = track.artwork.as_ref() {
            assets.push((artwork.clone(), "artwork"));
            assets.push((artwork.with_file_name("artwork_s.jpg"), "artwork"));
            assets.push((artwork.with_file_name("artwork_m.jpg"), "artwork"));
        }
        for (path, kind) in assets {
            let key = path.to_string_lossy().into_owned();
            let (Some(before), Some(after)) = (old.inputs.get(&key), current.get(&key)) else { continue };
            if before == after { continue; }
            let diagnostic_id = if kind == "audio" {
                format!("asset:track:{}", track.id)
            } else {
                format!("asset:{}:{kind}:{}", track.id, crate::manifest::hash(key.as_bytes()))
            };
            if after.is_none() {
                let blocking = kind == "audio" && !track.source_unavailable;
                let mut item = diagnostic(
                    diagnostic_id,
                    ConflictKind::AssetUnavailable,
                    if blocking { Severity::Blocked } else { Severity::Information },
                    EntityKind::Track, track.id, &track.title, Vec::new(), vec![kind.into()],
                    if blocking { "A source audio asset recorded by the previous export is missing." } else { "A previously available companion asset is missing; the exporter will preserve usable device data when possible." },
                    Vec::new(),
                );
                item.evidence.push(FieldEvidence {
                    field: kind.into(),
                    baseline: before.as_ref().map(|revision| format!("{} bytes, mtime {}", revision.size, revision.modified)),
                    device: Some("missing".into()),
                    library: None,
                });
                out.push(item);
                continue;
            }
            out.push(diagnostic(
                diagnostic_id,
                ConflictKind::AssetChanged, Severity::Information, EntityKind::Track, track.id,
                &track.title, Vec::new(), vec![kind.into()],
                "This source asset changed since the previous export and will be checked or refreshed.", Vec::new(),
            ));
        }
    }
}