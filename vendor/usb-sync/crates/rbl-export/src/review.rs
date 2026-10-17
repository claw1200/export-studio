use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::snapshot::Library;
use crate::{ExportError, Manifest, Result, SourcePlaylist, SourceTrack};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Exclusion {
    pub key: String,
    pub entity: String,
    pub library_id: u64,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action { Restore, Keep }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decisions {
    pub token: String,
    pub resolutions: BTreeMap<String, Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub recovery_required: bool,
    pub token: String,
    pub conflicts: Vec<Conflict>,
    pub exclusions: Vec<Exclusion>,
}

pub fn inspect(root: &Path, tracks: &[SourceTrack], playlists: &[SourcePlaylist], db_id: u64) -> Result<Review> {
    let _lease = rbl_core::durable::read_lease(root)?;
    if root.join(crate::PUBLICATION).exists() {
        return Ok(Review { recovery_required: true, token: String::new(), conflicts: Vec::new(), exclusions: Vec::new() });
    }
    let previous = Manifest::load(root);
    let before = crate::snapshot::Snapshot::read(root)?;
    inspect_snapshot(root, &before, previous.as_ref(), tracks, playlists, db_id)
}

fn inspect_snapshot(root: &Path, before: &crate::snapshot::Snapshot, previous: Option<&Manifest>, tracks: &[SourceTrack], playlists: &[SourcePlaylist], db_id: u64) -> Result<Review> {
    let rewritten = crate::reconcile::rewritten(root, before, previous, tracks, playlists, db_id);
    before.check_baseline_except(previous, db_id, &rewritten)?;
    let mut conflicts: BTreeMap<String, Conflict> = BTreeMap::new();
    if let Some(previous) = previous {
        for (format, current, baseline) in [
            ("Device Library", before.legacy.as_ref(), previous.baseline.as_ref().and_then(|b| b.legacy.as_ref())),
            ("OneLibrary", before.one.as_ref(), previous.baseline.as_ref().and_then(|b| b.one.as_ref())),
        ] {
            let (Some(current), Some(baseline)) = (current, baseline) else { continue };
            for mut conflict in deletions(format, current, baseline, &rewritten.tracks, &rewritten.playlists) {
                conflict.library_id = if conflict.entity == "track" {
                    previous.tracks.iter().find(|t| t.export_id == conflict.device_id).map(|t| t.library_id)
                        .or_else(|| previous.baseline.as_ref()?.identity.get(&conflict.device_id).filter(|(db, _)| *db == db_id).map(|(_, id)| *id))
                } else {
                    previous.playlists.iter().find(|p| p.export_id == conflict.device_id && !p.device_only).map(|p| p.library_id)
                }.unwrap_or(0);
                if conflict.library_id == 0 {
                    return Err(ExportError::Conflict(format!("{} was deleted on the USB but its library identity cannot be verified", conflict.name)));
                }
                conflict.key = format!("{}:{}", if conflict.entity == "track" { "track" } else { "playlist" }, conflict.library_id);
                if previous.exclusions.iter().any(|e| e.key == conflict.key) { continue; }
                if let Some(existing) = conflicts.get_mut(&conflict.key) {
                    existing.format.push_str(" / ");
                    existing.format.push_str(format);
                } else { conflicts.insert(conflict.key.clone(), conflict); }
            }
        }
    }
    let bytes = serde_json::to_vec(&(root, before, previous, tracks, playlists, db_id))
        .map_err(|e| ExportError::Conflict(e.to_string()))?;
    Ok(Review {
        recovery_required: false,
        token: format!("{:016x}", crate::manifest::hash(&bytes)),
        conflicts: conflicts.into_values().collect(),
        exclusions: previous.map_or_else(Vec::new, |p| p.exclusions.clone()),
    })
}

pub(crate) struct Prepared {
    pub previous: Option<Manifest>,
    pub tracks: Vec<SourceTrack>,
    pub playlists: Vec<SourcePlaylist>,
    pub exclusions: Vec<Exclusion>,
}

pub(crate) fn resolve(root: &Path, before: &crate::snapshot::Snapshot, previous: Option<&Manifest>, tracks: &[SourceTrack], playlists: &[SourcePlaylist], db_id: u64, decisions: Option<&Decisions>) -> Result<Prepared> {
    let review = inspect_snapshot(root, before, previous, tracks, playlists, db_id)?;
    if decisions.is_some_and(|d| d.token != review.token) {
        return Err(ExportError::Conflict("The USB or sync selection changed. Review its changes again before syncing.".into()));
    }
    if let Some(decisions) = decisions {
        if decisions.resolutions.keys().any(|key| !review.conflicts.iter().any(|c| &c.key == key) && !review.exclusions.iter().any(|e| &e.key == key)) {
            return Err(ExportError::Conflict("The sync review contains an unknown item. Review the USB again.".into()));
        }
    }
    let mut exclusions = review.exclusions;
    if let Some(decisions) = decisions {
        exclusions.retain(|e| decisions.resolutions.get(&e.key) != Some(&Action::Restore));
    }
    for conflict in &review.conflicts {
        match decisions.and_then(|d| d.resolutions.get(&conflict.key)) {
            Some(Action::Restore) => {}
            Some(Action::Keep) => exclusions.push(Exclusion {
                key: conflict.key.clone(), entity: conflict.entity.clone(), library_id: conflict.library_id,
                name: conflict.name.clone(), path: conflict.path.clone(),
            }),
            None => return Err(ExportError::Conflict(format!("'{}' was deleted on {}. Review USB changes before syncing.", conflict.name, conflict.format))),
        }
    }
    let mut accepted = previous.cloned();
    if let Some(baseline) = accepted.as_mut().and_then(|p| p.baseline.as_mut()) {
        for (library, current) in [(baseline.legacy.as_mut(), before.legacy.as_ref()), (baseline.one.as_mut(), before.one.as_ref())] {
            let (Some(library), Some(current)) = (library, current) else { continue };
            let missing: BTreeSet<_> = review.conflicts.iter().filter(|c| c.entity == "track" && !current.tracks.iter().any(|t| t.id == c.device_id)).map(|c| c.device_id).collect();
            library.tracks.retain(|track| !missing.contains(&track.id));
            library.playlists.retain(|playlist| !review.conflicts.iter().any(|c| c.entity != "track" && c.device_id == playlist.id && !current.playlists.iter().any(|p| p.id == playlist.id)));
            for playlist in &mut library.playlists {
                playlist.tracks.retain(|id| !missing.contains(id));
            }
        }
    }
    let (tracks, playlists) = filter(tracks, playlists, &exclusions, db_id);
    if let Some(decisions) = decisions {
        if review.conflicts.iter().any(|c| c.entity != "track" && decisions.resolutions.get(&c.key) == Some(&Action::Restore) && !playlists.iter().any(|p| p.id == c.library_id)) {
            return Err(ExportError::Conflict("A playlist cannot be restored while its parent folder is excluded on this USB. Restore the folder as well.".into()));
        }
    }
    Ok(Prepared { previous: accepted, tracks, playlists, exclusions })
}

pub(crate) fn filter(tracks: &[SourceTrack], playlists: &[SourcePlaylist], exclusions: &[Exclusion], db_id: u64) -> (Vec<SourceTrack>, Vec<SourcePlaylist>) {
    if exclusions.is_empty() { return (tracks.to_vec(), playlists.to_vec()); }
    let excluded_tracks: BTreeSet<_> = exclusions.iter().filter(|e| e.entity == "track").map(|e| e.library_id).collect();
    let mut excluded_playlists: BTreeSet<_> = exclusions.iter().filter(|e| e.entity != "track").map(|e| e.library_id).collect();
    loop {
        let children: Vec<_> = playlists.iter().filter(|p| p.parent_id != 0 && excluded_playlists.contains(&p.parent_id)).map(|p| p.id).collect();
        let count = excluded_playlists.len();
        excluded_playlists.extend(children);
        if excluded_playlists.len() == count { break; }
    }
    let removed_members: BTreeSet<_> = playlists.iter().filter(|p| excluded_playlists.contains(&p.id)).flat_map(|p| p.track_indices.iter().copied()).collect();
    let kept_members: BTreeSet<_> = playlists.iter().filter(|p| !excluded_playlists.contains(&p.id)).flat_map(|p| p.track_indices.iter().copied()).collect();
    let mut remap = BTreeMap::new();
    let mut kept = Vec::new();
    for (index, track) in tracks.iter().enumerate() {
        let identity = track.device.as_ref().map_or(track.id, |d| if d.master_content_id != 0 { d.master_content_id } else { track.id });
        let other_library = track.device.as_ref().is_some_and(|d| d.master_db_id != 0 && d.master_db_id != db_id);
        if (other_library || !excluded_tracks.contains(&identity)) && (!removed_members.contains(&index) || kept_members.contains(&index)) {
            remap.insert(index, kept.len());
            kept.push(track.clone());
        }
    }
    let playlists = playlists.iter().filter(|p| !excluded_playlists.contains(&p.id)).map(|p| {
        let mut playlist = p.clone();
        playlist.track_indices = p.track_indices.iter().filter_map(|i| remap.get(i).copied()).collect();
        playlist
    }).collect();
    (kept, playlists)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub key: String,
    pub format: String,
    pub entity: String,
    pub device_id: u32,
    pub library_id: u64,
    pub name: String,
    pub artist: String,
    pub path: String,
}

pub(crate) fn deletions(
    format: &str,
    current: &Library,
    baseline: &Library,
    tracks: &BTreeSet<u32>,
    playlists: &BTreeSet<u32>,
) -> Vec<Conflict> {
    let mut out = Vec::new();
    for track in &baseline.tracks {
        if tracks.contains(&track.id) && !current.tracks.iter().any(|row| row.id == track.id) {
            out.push(Conflict {
                key: format!("{format}:track:{}", track.id),
                format: format.into(), entity: "track".into(), device_id: track.id,
                library_id: 0, name: track.title.clone(), artist: track.artist.clone(), path: track.path.clone(),
            });
        }
    }
    for playlist in &baseline.playlists {
        if playlists.contains(&playlist.id) && !current.playlists.iter().any(|row| row.id == playlist.id) {
            let mut names = vec![playlist.name.clone()];
            let mut parent = playlist.parent;
            let mut seen = BTreeSet::new();
            while parent != 0 && seen.insert(parent) {
                let Some(folder) = baseline.playlists.iter().find(|row| row.id == parent) else { break };
                names.push(folder.name.clone());
                parent = folder.parent;
            }
            names.reverse();
            out.push(Conflict {
                key: format!("{format}:playlist:{}", playlist.id),
                format: format.into(), entity: if playlist.folder { "folder" } else { "playlist" }.into(),
                device_id: playlist.id, library_id: 0, name: playlist.name.clone(), artist: String::new(), path: names.join(" / "),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{Playlist, Track};

    #[test]
    fn empty_review_state_preserves_existing_receipt_hashes() {
        let manifest: Manifest = serde_json::from_value(serde_json::json!({ "version": 2, "written": "", "tracks": [] })).unwrap();
        let expected = crate::incremental::digest(&(manifest.db_id, &manifest.tracks, &manifest.playlists, &manifest.loose, &manifest.baseline, manifest.generation.as_ref().map(|g| &g.device_sources))).unwrap();
        assert_eq!(crate::incremental::manifest_structure(&manifest).unwrap(), expected);
        let tracks: Vec<SourceTrack> = Vec::new();
        let playlists: Vec<SourcePlaylist> = Vec::new();
        let tags: Vec<crate::SourceMyTag> = Vec::new();
        let options = crate::ExportOptions::default();
        let expected = crate::incremental::digest(&(&tracks, &playlists, &tags, options.compatibility, options.keep_unlisted, format!("{:?}", options.sync), format!("{:?}", options.defaults), crate::incremental::TRANSFORM_VERSION)).unwrap();
        assert_eq!(crate::incremental::request(&tracks, &playlists, &tags, &options).unwrap(), expected);
        let decisions = Decisions { token: "read-only review".into(), resolutions: BTreeMap::new() };
        let options = crate::ExportOptions { review: Some(&decisions), ..Default::default() };
        assert_eq!(crate::incremental::request(&tracks, &playlists, &tags, &options).unwrap(), expected);
    }

    #[test]
    fn folder_exclusions_keep_shared_tracks_and_remap_membership() {
        let tracks = vec![SourceTrack { id: 1, ..Default::default() }, SourceTrack { id: 2, ..Default::default() }];
        let playlists = vec![
            SourcePlaylist { id: 10, folder: true, ..Default::default() },
            SourcePlaylist { id: 11, parent_id: 10, track_indices: vec![0, 1], ..Default::default() },
            SourcePlaylist { id: 20, track_indices: vec![1], ..Default::default() },
        ];
        let excluded = vec![Exclusion { entity: "folder".into(), library_id: 10, ..Default::default() }];
        let (tracks, playlists) = filter(&tracks, &playlists, &excluded, 123);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, 2);
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].track_indices, [0]);
    }

    #[test]
    fn track_exclusions_do_not_match_another_librarys_numeric_id() {
        let tracks = vec![SourceTrack { id: 1, device: Some(crate::DeviceTrack { master_db_id: 999, master_content_id: 1, ..Default::default() }), ..Default::default() }];
        let excluded = vec![Exclusion { entity: "track".into(), library_id: 1, ..Default::default() }];
        assert_eq!(filter(&tracks, &[], &excluded, 123).0.len(), 1);
    }

    #[test]
    fn names_only_selected_deletions_and_keeps_the_database_identity() {
        let baseline = Library {
            tracks: vec![Track { id: 7, title: "Missing track".into(), artist: "Artist".into(), ..Default::default() }, Track { id: 8, ..Default::default() }],
            playlists: vec![Playlist { id: 1, name: "Sets".into(), folder: true, ..Default::default() }, Playlist { id: 2, parent: 1, name: "Friday".into(), ..Default::default() }],
        };
        let current = Library::default();
        let conflicts = deletions("Device Library", &current, &baseline, &BTreeSet::from([7]), &BTreeSet::from([2]));
        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].name, "Missing track");
        assert_eq!(conflicts[0].artist, "Artist");
        assert_eq!(conflicts[1].path, "Sets / Friday");
        assert_ne!(conflicts[0].key, deletions("OneLibrary", &current, &baseline, &BTreeSet::from([7]), &BTreeSet::new())[0].key);
    }
}