//! The rekordbox master library for the app. Reads come from a snapshot; edits open the live
//! database for one transaction and then refresh the snapshot. Each call runs on a worker thread
//! and resolves to JSON text in the shapes the interface already consumes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use es_backup::Backups;
use crate::usb::UsbOp;
use es_db::{Guard, Library, TrackEdit, Tx};
use napi::bindgen_prelude::AsyncTask;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::{Env, JsFunction, Task};
use napi_derive::napi;
use serde::Serialize;
use serde_json::{json, Value};

const WRITE_BLOCKED: &str = "Close rekordbox before editing the library";

pub(crate) struct State {
    pub(crate) lib: Library,
    source: PathBuf,
    writable: bool,
    /// Whether this session has already taken (or found a recent) backup before the first edit.
    backed_up: bool,
}

type Events = ThreadsafeFunction<(String, String), ErrorStrategy::Fatal>;

/// What every call shares: the open library, where events go, and the backup folder.
pub(crate) struct Core {
    pub(crate) devices: crate::usb::Devices,
    state: Mutex<Option<State>>,
    events: Mutex<Option<Events>>,
    backups: Backups,
}

fn backup_folder() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join(".exportstudio")
        .join("backups")
}

impl Core {
    fn new() -> Self {
        let verify = |path: &std::path::Path| {
            es_db::verify(path)
                .map(|c| es_backup::Counts { tracks: c.tracks, playlists: c.playlists })
                .map_err(|e| es_backup::Error::Message(e.to_string()))
        };
        Self {
            devices: crate::usb::Devices::default(),
            state: Mutex::new(None),
            events: Mutex::new(None),
            backups: Backups::new(backup_folder(), verify, es_db::rekordbox_running),
        }
    }

    pub(crate) fn emit(&self, event: &str, data: &Value) {
        if let Ok(guard) = self.events.lock() {
            if let Some(handler) = guard.as_ref() {
                handler.call((event.to_owned(), data.to_string()), ThreadsafeFunctionCallMode::NonBlocking);
            }
        }
    }

    /// Back up the library with progress events; automatic reasons skip when nothing changed.
    fn back_up(&self, source: &std::path::Path, reason: &str) -> napi::Result<Option<Value>> {
        let made = self.backups.create(source, reason, &|event| self.emit("backup.progress", &event));
        self.emit("backup.progress", &json!({ "stage": "done", "reason": reason }));
        let made = made.map_err(failure)?;
        if made.is_some() {
            self.emit("backup.changed", &json!({}));
        }
        Ok(made)
    }

    /// Whether the newest automatic backup is older than the chosen interval.
    fn auto_due(&self) -> bool {
        let interval = self.backups.settings()["interval"].as_i64().unwrap_or(12) as f64 * 3600.0;
        match self.backups.latest_automatic().and_then(|m| m["created"].as_f64()) {
            None => true,
            Some(created) => {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
                now - created >= interval
            }
        }
    }
}

pub(crate) fn failure(reason: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(reason.to_string())
}

fn open(source: PathBuf, writable: bool, backed_up: bool) -> napi::Result<State> {
    let lib = Library::open_snapshot(&source).map_err(failure)?;
    Ok(State { lib, source, writable, backed_up })
}

pub(crate) fn json_of(value: &impl Serialize) -> napi::Result<String> {
    serde_json::to_string(value).map_err(failure)
}

fn info(state: &State) -> String {
    json!({
        "dbPath": state.source,
        "snapshotTime": state.lib.snapshot_time(),
        "rekordboxRunning": es_db::rekordbox_running(),
        "sourceChanged": state.lib.source_changed(),
        "writable": state.writable,
    })
    .to_string()
}

/// Run something against the live database, then re-read the snapshot so later reads see it.
pub(crate) fn with_live<T>(state: &mut State, work: impl FnOnce(&mut Library) -> napi::Result<T>) -> napi::Result<T> {
    if !state.writable {
        return Err(failure("Unlock editing to make changes to the library"));
    }
    let mut live = Library::open_live(&state.source, Guard::RefuseWhileRunning).map_err(failure)?;
    let out = work(&mut live)?;
    drop(live);
    state.lib = Library::open_snapshot(&state.source).map_err(failure)?;
    Ok(out)
}

/// Run one edit in a single transaction.
fn edit<T>(state: &mut State, change: impl FnOnce(&mut Tx<'_>) -> es_db::Result<T>) -> napi::Result<T> {
    with_live(state, |live| live.write(change).map_err(failure))
}

fn track_ids(state: &State, playlist: &str) -> napi::Result<String> {
    json_of(&state.lib.playlist_track_ids(playlist).map_err(failure)?)
}

fn playlists(state: &State) -> napi::Result<serde_json::Value> {
    serde_json::to_value(es_index::playlist_nodes(&state.lib).map_err(failure)?).map_err(failure)
}

fn tracks_of(state: &State, ids: &[String]) -> napi::Result<String> {
    json_of(&es_index::track_rows(&state.lib, &state.lib.share_dir(), Some(ids)).map_err(failure)?)
}

fn cues_json(state: &State, id: &str) -> napi::Result<String> {
    json_of(&es_index::detail::cues_for(&state.lib, &state.lib.share_dir(), id).map_err(failure)?)
}

fn clean_name(name: &str, folder: bool) -> String {
    let name = name.trim();
    if name.is_empty() {
        (if folder { "New folder" } else { "New playlist" }).into()
    } else {
        name.into()
    }
}

/// A whole number from what the interface sends: nothing, an empty string or false mean 0.
fn whole(value: &serde_json::Value, label: &str, limit: i64) -> napi::Result<i64> {
    use serde_json::Value;
    let number = match value {
        Value::Null => 0,
        Value::Bool(b) => i64::from(*b),
        Value::Number(n) => n.as_f64().map_or(0, |f| f.trunc() as i64),
        Value::String(s) if s.trim().is_empty() => 0,
        Value::String(s) => s.trim().parse::<i64>().map_err(|_| failure(format!("{label} must be a whole number")))?,
        _ => return Err(failure(format!("{label} must be a whole number"))),
    };
    if (0..=limit).contains(&number) {
        Ok(number)
    } else {
        Err(failure(format!("{label} must be between 0 and {limit}")))
    }
}

fn parse_edit(fields: &str) -> napi::Result<TrackEdit> {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(fields).map_err(failure)?;
    let known = [
        "title", "comment", "year", "trackNo", "rating", "color", "artist", "remixer", "originalArtist", "composer",
        "album", "genre", "label",
    ];
    let mut unknown: Vec<&str> = map.keys().map(String::as_str).filter(|k| !known.contains(k)).collect();
    if !unknown.is_empty() {
        unknown.sort_unstable();
        return Err(failure(format!("These fields can't be edited: {}", unknown.join(", "))));
    }
    let text = |key: &str| {
        map.get(key).map(|v| match v {
            serde_json::Value::Null | serde_json::Value::Bool(false) => String::new(),
            serde_json::Value::String(s) => s.trim().to_owned(),
            other => other.to_string().trim().to_owned(),
        })
    };
    let number = |key: &str, label: &str, limit: i64| map.get(key).map(|v| whole(v, label, limit)).transpose();
    let edit = TrackEdit {
        title: text("title"),
        comment: text("comment"),
        year: number("year", "Year", 9999)?,
        track_no: number("trackNo", "Track number", 9999)?,
        rating: number("rating", "Rating", 5)?,
        color: number("color", "Colour", 8)?,
        artist: text("artist"),
        remixer: text("remixer"),
        original_artist: text("originalArtist"),
        composer: text("composer"),
        album: text("album"),
        genre: text("genre"),
        label: text("label"),
    };
    if edit.title.as_deref() == Some("") {
        return Err(failure("A track needs a title"));
    }
    Ok(edit)
}

enum Op {
    Open(Option<String>),
    Reload,
    SetWritable(bool),
    Info,
    Tracks,
    Playlists,
    Histories,
    MyTags,
    PlaylistMembers,
    PlaylistTracks(String),
    HistoryTracks(String),
    TrackDetail(String),
    TrackPreviews(Vec<String>),
    MediaResolve { kind: String, id: String, size: String },
    CueSetHot(String, i64, i64),
    CueAddMemory(String, i64),
    CueDelete(String, String),
    CueSetColor(String, String, Option<i64>),
    TrackUpdate(Vec<String>, String),
    TrackImport { paths: Vec<String>, playlist: Option<String>, artwork: bool },
    BackupList,
    BackupCreate,
    BackupDelete(String),
    BackupRestore(String),
    BackupSettings,
    BackupSetSettings { auto: Option<bool>, keep: Option<i64>, interval: Option<i64> },
    DailyBackup,
    RemoveTracks(Vec<String>),
    PlaylistAdd(String, Vec<String>),
    PlaylistRemove(String, Vec<String>),
    PlaylistMove(String, Vec<String>, u32),
    PlaylistCreate { name: String, parent: Option<String>, folder: bool, ids: Vec<String> },
    PlaylistRename(String, String),
    PlaylistDelete(String),
    TagAdd(String, Vec<String>),
    TagRemove(String, Vec<String>),
    Usb(UsbOp),
}

pub struct Run {
    core: Arc<Core>,
    op: Op,
}

impl Run {
    /// Backup calls that need nothing but the library's location.
    fn backup_op(&self, open_source: Option<PathBuf>) -> napi::Result<String> {
        let source = open_source.or_else(Library::installed_path);
        let backups = &self.core.backups;
        match &self.op {
            Op::BackupList => json_of(&backups.list(source.as_deref())),
            Op::BackupSettings => json_of(&backups.settings()),
            Op::BackupSetSettings { auto, keep, interval } => json_of(&backups.set_settings(*auto, *keep, *interval).map_err(failure)?),
            Op::BackupDelete(id) => {
                backups.delete(id).map_err(failure)?;
                self.core.emit("backup.changed", &json!({}));
                json_of(&backups.list(source.as_deref()))
            }
            Op::BackupCreate => {
                let source = source.ok_or_else(|| failure("No rekordbox library found on this computer"))?;
                json_of(&self.core.back_up(&source, "manual")?)
            }
            _ => unreachable!("not a backup call"),
        }
    }

    fn perform(&self, slot: &mut Option<State>) -> napi::Result<String> {
        match &self.op {
            Op::Open(path) => {
                let source = path
                    .clone()
                    .map(PathBuf::from)
                    .or_else(Library::installed_path)
                    .ok_or_else(|| failure("No rekordbox library found on this computer"))?;
                let writable = slot.as_ref().is_some_and(|s| s.writable && s.source == source);
                let backed_up = slot.as_ref().is_some_and(|s| s.backed_up);
                *slot = None;
                let state = open(source, writable, backed_up)?;
                let out = info(&state);
                *slot = Some(state);
                return Ok(out);
            }
            Op::Reload => {
                let (source, writable, backed_up) = slot
                    .as_ref()
                    .map(|s| (s.source.clone(), s.writable, s.backed_up))
                    .ok_or_else(|| failure("Library not open"))?;
                *slot = None;
                let state = open(source, writable, backed_up)?;
                let out = info(&state);
                *slot = Some(state);
                return Ok(out);
            }
            Op::BackupList | Op::BackupCreate | Op::BackupDelete(_) | Op::BackupSettings | Op::BackupSetSettings { .. } => {
                return self.backup_op(slot.as_ref().map(|s| s.source.clone()));
            }
            Op::BackupRestore(id) => {
                let source = slot
                    .as_ref()
                    .map(|s| s.source.clone())
                    .or_else(Library::installed_path)
                    .ok_or_else(|| failure("No rekordbox library found on this computer"))?;
                let backed_up = slot.as_ref().is_some_and(|s| s.backed_up);
                *slot = None;
                let restored = self.core.backups.restore(id, &source, &|event| self.core.emit("backup.progress", &event));
                self.core.emit("backup.progress", &json!({ "stage": "done", "reason": "restore" }));
                self.core.emit("backup.changed", &json!({}));
                let reopened = open(source, false, backed_up);
                if let Ok(state) = reopened {
                    *slot = Some(state);
                }
                self.core.emit("library.reloaded", &json!({}));
                return json_of(&restored.map_err(failure)?);
            }
            Op::DailyBackup => {
                let Some(state) = slot.as_ref() else { return Ok("false".into()) };
                let due = self.core.backups.settings()["auto"].as_bool().unwrap_or(true) && self.core.auto_due();
                if due && !es_db::rekordbox_running() {
                    self.core.back_up(&state.source, "daily")?;
                }
                return Ok(due.to_string());
            }
            _ => {}
        }
        if let Op::Usb(op) = &self.op {
            return crate::usb::perform(&self.core, slot, op);
        }
        let state = slot.as_mut().ok_or_else(|| failure("Library not open"))?;
        match &self.op {
            Op::SetWritable(on) => {
                if *on && !state.backed_up && self.core.backups.settings()["auto"].as_bool().unwrap_or(true) {
                    if es_db::rekordbox_running() {
                        return Err(failure(WRITE_BLOCKED));
                    }
                    if self.core.auto_due() {
                        self.core.back_up(&state.source, "before-edit")?;
                    }
                    state.backed_up = true;
                }
                state.writable = *on;
                Ok(info(state))
            }
            Op::Info => Ok(info(state)),
            Op::Tracks => json_of(&es_index::track_rows(&state.lib, &state.lib.share_dir(), None).map_err(failure)?),
            Op::Playlists => json_of(&es_index::playlist_nodes(&state.lib).map_err(failure)?),
            Op::Histories => json_of(&es_index::history_nodes(&state.lib).map_err(failure)?),
            Op::MyTags => json_of(&es_index::tag_nodes(&state.lib).map_err(failure)?),
            Op::PlaylistMembers => json_of(&state.lib.playlist_member_ids().map_err(failure)?),
            Op::PlaylistTracks(id) => match state.lib.playlist_track_ids(id) {
                Ok(ids) => json_of(&ids),
                // Smart playlists are not evaluated yet; the caller falls back to the Python engine.
                Err(e @ es_db::Error::Unsupported(_)) => Err(failure(format!("unsupported: {e}"))),
                Err(e) => Err(failure(e)),
            },
            Op::HistoryTracks(id) => json_of(&state.lib.history_track_ids(id).map_err(failure)?),
            Op::TrackDetail(id) => {
                json_of(&es_index::detail::track_detail(&state.lib, &state.lib.share_dir(), id).map_err(failure)?)
            }
            Op::TrackPreviews(ids) => {
                let (device, own): (Vec<String>, Vec<String>) = ids.iter().cloned().partition(|i| i.starts_with("usb:"));
                let mut by_id: std::collections::HashMap<String, es_index::detail::Preview> = std::collections::HashMap::new();
                for row in es_index::detail::previews(&state.lib, &state.lib.share_dir(), &own).map_err(failure)? {
                    by_id.insert(row.id.clone(), row);
                }
                let devices = &self.core.devices;
                for row in es_index::device::previews(&mut |volume| devices.load(volume).ok().flatten(), &device) {
                    by_id.insert(row.id.clone(), row);
                }
                let mut seen = std::collections::HashSet::new();
                let ordered: Vec<_> = ids.iter().filter(|i| seen.insert(i.as_str())).take(80).filter_map(|i| by_id.remove(i)).collect();
                json_of(&ordered)
            }
            Op::MediaResolve { kind, id, size } => {
                let found = es_index::media::resolve_media(&state.lib, &state.lib.share_dir(), kind, id, size).map_err(failure)?;
                json_of(&found)
            }
            Op::CueSetHot(id, kind, ms) => {
                edit(state, |tx| tx.set_hot_cue(id, *kind, *ms))?;
                cues_json(state, id)
            }
            Op::CueAddMemory(id, ms) => {
                edit(state, |tx| tx.add_memory_cue(id, *ms))?;
                cues_json(state, id)
            }
            Op::CueDelete(id, cue) => {
                edit(state, |tx| tx.delete_cue(id, cue))?;
                cues_json(state, id)
            }
            Op::CueSetColor(id, cue, color) => {
                edit(state, |tx| tx.set_cue_color(id, cue, *color))?;
                cues_json(state, id)
            }
            Op::RemoveTracks(ids) => {
                let removed = edit(state, |tx| tx.remove_tracks(ids))?;
                let share = state.lib.share_dir();
                for item in &removed {
                    if let Some(path) = &item.analysis_path {
                        if let Err(e) = es_anlz::discard(&es_anlz::files(&share, path)) {
                            eprintln!("[es-db] could not delete analysis files for {}: {e}", item.id);
                        }
                    }
                }
                let mut ids: Vec<&str> = removed.iter().map(|r| r.id.as_str()).collect();
                ids.sort_unstable();
                Ok(json!({ "removed": ids, "playlists": playlists(state)? }).to_string())
            }
            Op::PlaylistAdd(id, ids) => {
                edit(state, |tx| tx.add_to_playlist(id, ids))?;
                track_ids(state, id)
            }
            Op::PlaylistRemove(id, ids) => {
                edit(state, |tx| tx.remove_from_playlist(id, ids))?;
                track_ids(state, id)
            }
            Op::PlaylistMove(id, ids, index) => {
                edit(state, |tx| tx.move_in_playlist(id, ids, *index as usize))?;
                track_ids(state, id)
            }
            Op::PlaylistCreate { name, parent, folder, ids } => {
                let name = clean_name(name, *folder);
                let parent = parent.as_deref().filter(|p| *p != "root");
                let id = edit(state, |tx| tx.create_playlist(&name, parent, *folder, ids))?;
                Ok(json!({ "id": id, "playlists": playlists(state)? }).to_string())
            }
            Op::PlaylistRename(id, name) => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(failure("A playlist needs a name"));
                }
                edit(state, |tx| tx.rename_playlist(id, name))?;
                json_of(&playlists(state)?)
            }
            Op::PlaylistDelete(id) => {
                edit(state, |tx| tx.delete_playlist(id))?;
                json_of(&playlists(state)?)
            }
            Op::TagAdd(tag, ids) => {
                edit(state, |tx| tx.add_tag(tag, ids))?;
                tracks_of(state, ids)
            }
            Op::TagRemove(tag, ids) => {
                edit(state, |tx| tx.remove_tag(tag, ids))?;
                tracks_of(state, ids)
            }
            Op::TrackUpdate(ids, fields) => {
                let change = parse_edit(fields)?;
                let mut unique = ids.clone();
                unique.dedup();
                if change != TrackEdit::default() && !unique.is_empty() {
                    edit(state, |tx| tx.update_tracks(&unique, &change))?;
                }
                Ok(format!(r#"{{"tracks":{},"tagErrors":[]}}"#, tracks_of(state, &unique)?))
            }
            Op::TrackImport { paths, playlist, artwork } => {
                let share = state.lib.share_dir();
                let options = es_engine::import::Options { artwork: *artwork, playlist: playlist.clone() };
                let outcome = with_live(state, |live| es_engine::import::import_files(live, &share, paths, &options).map_err(failure))?;
                let failed: Vec<_> = outcome.failed.iter().map(|f| json!({ "path": f.path, "error": f.error })).collect();
                let playlist_ids = match playlist {
                    Some(id) => serde_json::from_str(&track_ids(state, id)?).map_err(failure)?,
                    None => serde_json::Value::Null,
                };
                Ok(format!(
                    r#"{{"tracks":{},"ids":{},"skipped":{},"failed":{},"playlist":{}}}"#,
                    if outcome.added.is_empty() { "[]".to_owned() } else { tracks_of(state, &outcome.added)? },
                    json_of(&outcome.ids)?,
                    outcome.skipped,
                    json_of(&failed)?,
                    playlist_ids
                ))
            }
            Op::Usb(_) => unreachable!("handled above"),
            Op::Open(_)
            | Op::Reload
            | Op::BackupList
            | Op::BackupCreate
            | Op::BackupDelete(_)
            | Op::BackupRestore(_)
            | Op::BackupSettings
            | Op::BackupSetSettings { .. }
            | Op::DailyBackup => unreachable!("handled above"),
        }
    }
}

impl Task for Run {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<String> {
        let mut slot = self.core.state.lock().map_err(|_| failure("the library lock was poisoned"))?;
        self.perform(&mut slot)
    }

    fn resolve(&mut self, _env: Env, output: String) -> napi::Result<String> {
        Ok(output)
    }
}

/// The master library: snapshot reads and guarded edits.
#[napi]
pub struct RekordboxLibrary {
    core: Arc<Core>,
}

#[napi]
impl RekordboxLibrary {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self { core: Arc::new(Core::new()) }
    }

    fn run(&self, op: Op) -> AsyncTask<Run> {
        AsyncTask::new(Run { core: Arc::clone(&self.core), op })
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn open(&self, path: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Open(path))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn reload(&self) -> AsyncTask<Run> {
        self.run(Op::Reload)
    }

    /// Whether edits are allowed; the app sets it when the user unlocks editing.
    #[napi(ts_return_type = "Promise<string>")]
    pub fn set_writable(&self, on: bool) -> AsyncTask<Run> {
        self.run(Op::SetWritable(on))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn info(&self) -> AsyncTask<Run> {
        self.run(Op::Info)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn tracks(&self) -> AsyncTask<Run> {
        self.run(Op::Tracks)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlists(&self) -> AsyncTask<Run> {
        self.run(Op::Playlists)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn histories(&self) -> AsyncTask<Run> {
        self.run(Op::Histories)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn my_tags(&self) -> AsyncTask<Run> {
        self.run(Op::MyTags)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_members(&self) -> AsyncTask<Run> {
        self.run(Op::PlaylistMembers)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_tracks(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::PlaylistTracks(id))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn history_tracks(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::HistoryTracks(id))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_detail(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::TrackDetail(id))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_previews(&self, ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::TrackPreviews(ids))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn media_resolve(&self, kind: String, id: String, size: String) -> AsyncTask<Run> {
        self.run(Op::MediaResolve { kind, id, size })
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn cue_set_hot(&self, id: String, kind: i64, ms: i64) -> AsyncTask<Run> {
        self.run(Op::CueSetHot(id, kind, ms))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn cue_add_memory(&self, id: String, ms: i64) -> AsyncTask<Run> {
        self.run(Op::CueAddMemory(id, ms))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn cue_delete(&self, id: String, cue_id: String) -> AsyncTask<Run> {
        self.run(Op::CueDelete(id, cue_id))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn cue_set_color(&self, id: String, cue_id: String, color_index: Option<i64>) -> AsyncTask<Run> {
        self.run(Op::CueSetColor(id, cue_id, color_index))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_update(&self, ids: Vec<String>, fields: String) -> AsyncTask<Run> {
        self.run(Op::TrackUpdate(ids, fields))
    }

    /// Where backup progress and library events go: `(event, json)`.
    #[napi]
    pub fn on_event(
        &self,
        env: Env,
        #[napi(ts_arg_type = "(event: string, data: string) => void")] callback: JsFunction,
    ) -> napi::Result<()> {
        let mut handler: Events = callback.create_threadsafe_function(0, |ctx: ThreadSafeCallContext<(String, String)>| {
            let (event, data) = ctx.value;
            Ok(vec![ctx.env.create_string(&event)?, ctx.env.create_string(&data)?])
        })?;
        // Events must not keep the process alive on their own.
        handler.unref(&env)?;
        *self.core.events.lock().map_err(|_| failure("the event lock was poisoned"))? = Some(handler);
        Ok(())
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn backup_list(&self) -> AsyncTask<Run> {
        self.run(Op::BackupList)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn backup_create(&self) -> AsyncTask<Run> {
        self.run(Op::BackupCreate)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn backup_delete(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::BackupDelete(id))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn backup_restore(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::BackupRestore(id))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn backup_settings(&self) -> AsyncTask<Run> {
        self.run(Op::BackupSettings)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn backup_set_settings(&self, auto: Option<bool>, keep: Option<i64>, interval: Option<i64>) -> AsyncTask<Run> {
        self.run(Op::BackupSetSettings { auto, keep, interval })
    }

    /// Takes a daily backup when one is due; resolves to whether it was due.
    #[napi(ts_return_type = "Promise<string>")]
    pub fn daily_backup(&self) -> AsyncTask<Run> {
        self.run(Op::DailyBackup)
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_import(&self, paths: Vec<String>, playlist_id: Option<String>, artwork: bool) -> AsyncTask<Run> {
        self.run(Op::TrackImport { paths, playlist: playlist_id, artwork })
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn remove_tracks(&self, ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::RemoveTracks(ids))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_add(&self, id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::PlaylistAdd(id, track_ids))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_remove(&self, id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::PlaylistRemove(id, track_ids))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_move(&self, id: String, track_ids: Vec<String>, index: u32) -> AsyncTask<Run> {
        self.run(Op::PlaylistMove(id, track_ids, index))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_create(
        &self,
        name: String,
        parent_id: Option<String>,
        folder: bool,
        track_ids: Vec<String>,
    ) -> AsyncTask<Run> {
        self.run(Op::PlaylistCreate { name, parent: parent_id, folder, ids: track_ids })
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_rename(&self, id: String, name: String) -> AsyncTask<Run> {
        self.run(Op::PlaylistRename(id, name))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn playlist_delete(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::PlaylistDelete(id))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn tag_add(&self, tag_id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::TagAdd(tag_id, track_ids))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn tag_remove(&self, tag_id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::TagRemove(tag_id, track_ids))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_list(&self) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::List))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_eject(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Eject(volume)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_format(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Format(volume)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_playlists(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Playlists(volume)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_libraries(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Libraries(volume)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_selection(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Selection(volume)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_tracks(&self, volume: String, playlist_id: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Tracks(volume, playlist_id)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_detail(&self, key: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Detail(key)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_media(&self, key: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Media(key)))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_import_preview(&self, volume: String, folder: Option<String>, policy: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::ImportPreview { volume, folder, policy }))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_import(&self, volume: String, folder: Option<String>, policy: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Import { volume, folder, policy }))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn usb_sync(&self, volume: String, playlist_ids: Vec<String>, overwrite: bool) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Sync { volume, playlists: playlist_ids, overwrite }))
    }
}

impl Default for RekordboxLibrary {
    fn default() -> Self {
        Self::new()
    }
}
