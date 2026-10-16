//! Copying a stick's playlists and tracks into the library.
//!
//! Tracks the library already has are reused, and the rest are copied next to where rekordbox would put
//! them, imported, and given the analysis and cues they had on the stick. What happens to a playlist
//! the library already has is an explicit choice ([`PlaylistPolicy`]), not a side effect of its name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use es_db::{DeviceData, Library, NewCue, PlaylistKind, TrackEdit};
use es_device::paths::volume_path;
use es_device::{Cue as DeviceCue, Device, Playlist, Track as DeviceTrack};

use crate::import::{import_files, Options as FileOptions};
use crate::place::{candidate, destination, place_copy};
use crate::{Error, Result};

/// What to do with a stick playlist when the library already has one with that name in that place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaylistPolicy {
    /// Leave the library's playlist alone and add only the playlists it lacks.
    #[default]
    SkipExisting,
    /// Add the stick's tracks to the library's playlist.
    Merge,
    /// Create every playlist anew, inside a folder named for the stick.
    CreateNew,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Copy new audio here as `<Artist>/<Album>/<file>` instead of rekordbox's own import folder.
    pub folder: Option<PathBuf>,
    /// The music folder for rekordbox's own import location; the user's `Music` when `None`.
    pub music: Option<PathBuf>,
    pub policy: PlaylistPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackStatus {
    /// The library already has this track.
    Reuse,
    Copy,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPlaylist {
    pub name: String,
    pub folder: bool,
    pub depth: usize,
    pub tracks: usize,
    /// The library already has one in that place.
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTrack {
    pub title: String,
    pub artist: String,
    pub status: TrackStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub destination: PathBuf,
    pub layout: &'static str,
    pub playlists: Vec<PlannedPlaylist>,
    pub tracks: Vec<PlannedTrack>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub playlists: usize,
    pub tracks: usize,
    pub copied: usize,
    pub reused: usize,
    pub failed: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub name: String,
}

/// What the library already holds that a stick playlist may stand for.
struct Known {
    existing: Vec<es_db::PlaylistNode>,
    policy: PlaylistPolicy,
    /// Stick playlist export id -> the library playlist it was synced from.
    origins: HashMap<i64, u64>,
}

enum Match<'a> {
    Existing(&'a es_db::PlaylistNode),
    /// Synced from the library, which has since deleted it.
    Retired,
    New,
}

impl Known {
    fn new(lib: &Library, volume: &Path, policy: PlaylistPolicy) -> Result<Self> {
        let db_id = u64::try_from(lib.db_id()?).unwrap_or(0);
        Ok(Self { existing: lib.playlists()?, policy, origins: es_device::selection::playlist_origins(volume, db_id) })
    }

    /// `parent` is the library playlist the row would go under; `None` when that folder is itself new.
    fn lookup(&self, row: &Playlist, parent: Option<&str>, name: &str) -> Match<'_> {
        if self.policy == PlaylistPolicy::CreateNew {
            return Match::New;
        }
        if let Some(origin) = self.origins.get(&row.export_id) {
            let wanted = origin.to_string();
            return self.existing.iter().find(|n| n.id == wanted).map_or(Match::Retired, Match::Existing);
        }
        parent.and_then(|p| find(&self.existing, p, row.folder, name)).map_or(Match::New, Match::Existing)
    }
}

type Children<'a> = HashMap<i64, Vec<&'a Playlist>>;

fn children(device: &Device) -> Children<'_> {
    let known: std::collections::HashSet<i64> = device.playlists.iter().map(|p| p.export_id).collect();
    let mut out: Children<'_> = HashMap::new();
    for row in &device.playlists {
        let parent = if known.contains(&row.parent_id) { row.parent_id } else { 0 };
        out.entry(parent).or_default().push(row);
    }
    for rows in out.values_mut() {
        rows.sort_by_key(|row| (row.seq, row.export_id));
    }
    out
}

/// Tracks in the order the playlists list them, each once.
fn ordered_tracks(children: &Children<'_>, device: &Device) -> Vec<i64> {
    let mut seen = Vec::new();
    let mut stack: Vec<&Playlist> = children.get(&0).map(|rows| rows.iter().rev().copied().collect()).unwrap_or_default();
    while let Some(row) = stack.pop() {
        if row.folder {
            if let Some(inner) = children.get(&row.export_id) {
                stack.extend(inner.iter().rev().copied());
            }
        }
        for id in &row.track_ids {
            if !seen.contains(id) && device.track(*id).is_some() {
                seen.push(*id);
            }
        }
    }
    seen
}

/// The library's copy of a stick track, when the stick says it came from this library and the file is still there.
fn library_copy(lib: &Library, db_id: i64, track: &DeviceTrack) -> Option<String> {
    let master = track.master_id?;
    if db_id <= 0 || track.master_db_id != Some(db_id) {
        return None;
    }
    let id = master.to_string();
    let found = lib.tracks(Some(std::slice::from_ref(&id))).ok()?;
    found.first().filter(|t| Path::new(&t.folder_path).is_file()).map(|t| t.id.clone())
}

fn target_root(options: &Options) -> (PathBuf, bool) {
    match &options.folder {
        Some(folder) => (folder.clone(), false),
        None => {
            let music = options.music.clone().unwrap_or_else(|| {
                std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map_or_else(|| PathBuf::from("."), |h| PathBuf::from(h).join("Music"))
            });
            (destination(&music), true)
        }
    }
}

fn audio_of(volume: &Path, track: &DeviceTrack) -> Option<PathBuf> {
    volume_path(&track.path, volume).ok().filter(|p| p.is_file())
}

fn title_of(track: &DeviceTrack) -> String {
    if !track.title.is_empty() {
        track.title.clone()
    } else if !track.filename.is_empty() {
        track.filename.clone()
    } else {
        "Untitled".into()
    }
}

/// What an import would do, without changing the library or copying anything.
pub fn preview(lib: &Library, volume: &Path, device: &Device, options: &Options) -> Result<Plan> {
    let db_id = lib.db_id()?;
    let kids = children(device);
    let (root, contents) = target_root(options);
    let tracks = ordered_tracks(&kids, device)
        .into_iter()
        .filter_map(|id| device.track(id))
        .map(|track| PlannedTrack {
            title: title_of(track),
            artist: track.artist.clone(),
            status: if library_copy(lib, db_id, track).is_some() {
                TrackStatus::Reuse
            } else if audio_of(volume, track).is_none() {
                TrackStatus::Missing
            } else {
                TrackStatus::Copy
            },
        })
        .collect();

    let known = Known::new(lib, volume, options.policy)?;
    let mut playlists = Vec::new();
    plan_playlists(&kids, 0, Some("root"), 0, &known, &mut playlists);
    Ok(Plan { destination: root, layout: if contents { "Contents/Artist/Album" } else { "Artist/Album" }, playlists, tracks })
}

fn find<'a>(nodes: &'a [es_db::PlaylistNode], parent: &str, folder: bool, name: &str) -> Option<&'a es_db::PlaylistNode> {
    let kind = if folder { PlaylistKind::Folder } else { PlaylistKind::Playlist };
    nodes.iter().find(|n| n.parent_id == parent && n.kind == kind && (if n.name.trim().is_empty() { "Untitled" } else { n.name.trim() }) == name)
}

fn name_of(row: &Playlist) -> String {
    let name = row.name.trim();
    if name.is_empty() { "Untitled".into() } else { name.into() }
}

fn plan_playlists(kids: &Children<'_>, parent_export: i64, parent: Option<&str>, depth: usize, known: &Known, out: &mut Vec<PlannedPlaylist>) {
    for row in kids.get(&parent_export).map(Vec::as_slice).unwrap_or_default() {
        let name = name_of(row);
        let found = match known.lookup(row, parent, &name) {
            Match::Retired => {
                if row.folder {
                    plan_playlists(kids, row.export_id, None, depth, known, out);
                }
                continue;
            }
            Match::Existing(node) => Some(node),
            Match::New => None,
        };
        out.push(PlannedPlaylist { name, folder: row.folder, depth, tracks: if row.folder { 0 } else { row.track_ids.len() }, exists: found.is_some() });
        if row.folder {
            plan_playlists(kids, row.export_id, found.map(|n| n.id.as_str()), depth + 1, known, out);
        }
    }
}

/// Hot cue slots on a stick count from A = 1; the library skips 4, which it keeps for loops.
fn library_kind(slot: u32, is_loop: bool) -> i64 {
    match slot {
        0 if is_loop => 4,
        0 => 0,
        1..=3 => i64::from(slot),
        _ => i64::from(slot) + 1,
    }
}

fn from_device_cue(cue: &DeviceCue) -> NewCue {
    NewCue {
        kind: cue.kind,
        start_ms: cue.start_ms,
        end_ms: cue.end_ms,
        color_index: cue.color_index,
        comment: cue.comment.clone(),
        active_loop: cue.active_loop,
    }
}

/// Give a freshly imported track the analysis files, cues and numbers it had on the stick.
fn carry(lib: &mut Library, share: &Path, volume: &Path, track: &DeviceTrack, content_id: &str, placed: &Path) -> Result<()> {
    let uuid = lib.track_uuid(content_id)?.ok_or_else(|| Error::Other("the new track has no UUID".into()))?;
    let relative = format!("/PIONEER/USBANLZ/{}/{}/ANLZ0000.DAT", uuid.chars().take(3).collect::<String>(), uuid.chars().skip(3).collect::<String>());
    let dat = share.join(relative.trim_start_matches('/'));
    let name = placed.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let path_tag = es_anlz::write::path_tag(&format!("?/{name}"));

    let source = if track.analysis.is_empty() { None } else { volume_path(&track.analysis, volume).ok() };
    let mut written: Vec<PathBuf> = Vec::new();
    let mut ext_bytes: Option<Vec<u8>> = None;
    let outcome = (|| -> Result<()> {
        let Some(source) = &source else { return Ok(()) };
        for suffix in ["DAT", "EXT", "2EX"] {
            let from = source.with_extension(suffix);
            if !from.is_file() {
                continue;
            }
            let data = std::fs::read(&from).map_err(|e| Error::Other(e.to_string()))?;
            let patched = es_anlz::write::replace_tags(&data, &[("PPTH", path_tag.clone())]).map_err(|e| Error::Other(e.to_string()))?;
            let to = dat.with_extension(suffix);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::Other(e.to_string()))?;
            }
            std::fs::write(&to, patched).map_err(|e| Error::Other(e.to_string()))?;
            written.push(to);
            if suffix == "EXT" {
                ext_bytes = Some(data);
            }
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        discard(&written);
        return Err(e);
    }

    let cues: Vec<NewCue> = if track.cues.is_empty() {
        ext_bytes
            .and_then(|bytes| es_anlz::decode::Tags::parse(bytes).ok())
            .map(|tags| {
                es_anlz::decode::extended_cues(&tags)
                    .into_iter()
                    .map(|c| NewCue {
                        kind: library_kind(c.slot, c.is_loop),
                        start_ms: i64::from(c.start_ms),
                        end_ms: c.loop_end_ms.filter(|_| c.is_loop).map(i64::from),
                        color_index: Some(i64::from(c.color_id)).filter(|c| *c != 0),
                        comment: c.comment,
                        active_loop: false,
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        track.cues.iter().map(from_device_cue).collect()
    };
    let data = DeviceData {
        analysis_path: (!written.is_empty()).then_some(relative),
        bpm_x100: track.bpm_x100,
        key: track.key.clone(),
        rating: track.rating,
        color: track.color,
        cues,
    };
    if let Err(e) = lib.write(|tx| tx.apply_device_data(content_id, &data)) {
        discard(&written);
        return Err(e.into());
    }
    fill_from_device(lib, content_id, track, placed)
}

/// Where the audio file's own tags left a field empty, use what the stick knew about the track.
fn fill_from_device(lib: &mut Library, content_id: &str, track: &DeviceTrack, placed: &Path) -> Result<()> {
    let Some(current) = lib.tracks(Some(&[content_id.to_owned()]))?.into_iter().next() else { return Ok(()) };
    let stem = placed.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let empty_or = |have: &str, device: &str| (have.is_empty() && !device.is_empty()).then(|| device.to_owned());
    let edit = TrackEdit {
        // A title equal to the file name means the file had no title tag.
        title: (current.title == stem && !track.title.is_empty() && track.title != stem).then(|| track.title.clone()),
        artist: empty_or(&current.artist, &track.artist),
        album: empty_or(&current.album, &track.album),
        genre: empty_or(&current.genre, &track.genre),
        label: empty_or(&current.label, &track.label),
        comment: empty_or(&current.comment, &track.comment),
        year: (current.year == 0 && (1..=9999).contains(&track.year)).then_some(track.year),
        track_no: (current.track_no == 0 && (1..=9999).contains(&track.track_no)).then_some(track.track_no),
        ..TrackEdit::default()
    };
    if edit != TrackEdit::default() {
        lib.write(|tx| tx.update_tracks(&[content_id.to_owned()], &edit))?;
    }
    Ok(())
}

fn discard(files: &[PathBuf]) {
    for file in files {
        let _ = std::fs::remove_file(file);
    }
}

fn create_playlists(
    tx: &mut es_db::Tx<'_>,
    kids: &Children<'_>,
    parent_export: i64,
    parent: Option<String>,
    known: &Known,
    resolved: &HashMap<i64, String>,
    created: &mut usize,
) -> es_db::Result<()> {
    for row in kids.get(&parent_export).map(Vec::as_slice).unwrap_or_default() {
        let name = name_of(row);
        let ids: Vec<String> = if row.folder { Vec::new() } else { row.track_ids.iter().filter_map(|t| resolved.get(t).cloned()).collect() };
        let (id, is_new) = match known.lookup(row, Some(parent.as_deref().unwrap_or("root")), &name) {
            Match::Retired => {
                if row.folder {
                    create_playlists(tx, kids, row.export_id, None, known, resolved, created)?;
                }
                continue;
            }
            Match::Existing(node) => {
                if known.policy == PlaylistPolicy::Merge && !row.folder && !ids.is_empty() {
                    tx.add_to_playlist(&node.id, &ids)?;
                }
                (node.id.clone(), false)
            }
            Match::New => {
                *created += 1;
                (tx.create_playlist(&name, parent.as_deref(), row.folder, &ids)?, true)
            }
        };
        if row.folder {
            // Nothing inside a folder that was just made can already exist.
            let inner = if is_new { &Known { existing: Vec::new(), policy: known.policy, origins: known.origins.clone() } } else { known };
            create_playlists(tx, kids, row.export_id, Some(id), inner, resolved, created)?;
        }
    }
    Ok(())
}

/// Copy a stick's tracks and playlists into the library.
pub fn import_device(
    lib: &mut Library,
    share: &Path,
    volume: &Path,
    device: &Device,
    options: &Options,
    progress: &dyn Fn(Progress),
) -> Result<Outcome> {
    if !device.playlists.iter().any(|p| !p.folder) {
        return Err(Error::Other("This USB has no playlists to import".into()));
    }
    let db_id = lib.db_id()?;
    let kids = children(device);
    let (root, contents) = target_root(options);
    let order = ordered_tracks(&kids, device);
    let mut outcome = Outcome::default();
    let mut resolved: HashMap<i64, String> = HashMap::new();

    for (index, id) in order.iter().enumerate() {
        let Some(track) = device.track(*id) else { continue };
        let title = title_of(track);
        progress(Progress { done: index, total: order.len(), name: title.clone() });
        if let Some(existing) = library_copy(lib, db_id, track) {
            outcome.reused += 1;
            resolved.insert(*id, existing);
            continue;
        }
        let Some(audio) = audio_of(volume, track) else {
            outcome.failed.push((title, "Audio file is missing on the USB".into()));
            continue;
        };
        let file_name = if track.filename.is_empty() { audio.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { track.filename.clone() };
        let wanted = candidate(&root, &track.artist, &track.album, &file_name, contents);
        let (placed, created) = match place_copy(&audio, &wanted) {
            Ok(done) => done,
            Err(e) => {
                outcome.failed.push((title, e.to_string()));
                continue;
            }
        };
        let imported = import_files(lib, share, &[placed.to_string_lossy().into_owned()], &FileOptions { artwork: true, playlist: None });
        let done = match imported {
            Ok(done) if done.failed.is_empty() && !done.ids.is_empty() => done,
            other => {
                if created {
                    let _ = std::fs::remove_file(&placed);
                }
                let error = match other {
                    Ok(done) => done.failed.first().map_or_else(|| "Couldn’t import the track".to_owned(), |f| f.error.clone()),
                    Err(e) => e.to_string(),
                };
                outcome.failed.push((title, error));
                continue;
            }
        };
        if created { outcome.copied += 1 } else { outcome.reused += 1 }
        let content_id = done.ids[0].clone();
        if !done.added.is_empty() {
            if let Err(e) = carry(lib, share, volume, track, &content_id, &placed) {
                eprintln!("[es-engine] could not carry the device data for {title}: {e}");
            }
        }
        resolved.insert(*id, content_id);
    }
    progress(Progress { done: order.len(), total: order.len(), name: String::new() });

    let known = Known::new(lib, volume, options.policy)?;
    let wrapper_name = format!("Imported from {}", if device.device_name.is_empty() { volume.file_name().map_or_else(|| "device".into(), |n| n.to_string_lossy().into_owned()) } else { device.device_name.clone() });
    let mut created = 0;
    lib.write(|tx| {
        let parent = if options.policy == PlaylistPolicy::CreateNew {
            created += 1;
            Some(tx.create_playlist(&wrapper_name, None, true, &[])?)
        } else {
            None
        };
        create_playlists(tx, &kids, 0, parent, &known, &resolved, &mut created)
    })?;
    outcome.playlists = created;
    outcome.tracks = resolved.len();
    Ok(outcome)
}
