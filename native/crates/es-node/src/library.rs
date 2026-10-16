//! The rekordbox master library for the app. Reads come from a snapshot; edits open the live
//! database for one transaction and then refresh the snapshot. Each call runs on a worker thread
//! and resolves to JSON text in the shapes the interface already consumes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use es_backup::Backups;
use crate::usb::UsbOp;
use es_db::{Library, TrackEdit, Tx};
use es_engine::library::{LibraryService, LibrarySession};
use napi::bindgen_prelude::AsyncTask;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::{Env, JsFunction, Task};
use napi_derive::napi;
use serde::Serialize;
use serde_json::{json, Value};

const WRITE_BLOCKED: &str = "Close rekordbox before editing the library";

pub(crate) type State = LibrarySession;

type Events = ThreadsafeFunction<(String, String), ErrorStrategy::Fatal>;

/// What every call shares: the open library, where events go, and the backup folder.
pub(crate) struct Core {
    pub(crate) devices: crate::usb::Devices,
    service: Mutex<LibraryService>,
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
            service: Mutex::new(LibraryService::default()),
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

pub(crate) fn json_of(value: &impl Serialize) -> napi::Result<String> {
    serde_json::to_string(value).map_err(failure)
}

fn info(state: &State) -> String {
    json!({
        "dbPath": state.source(),
        "snapshotTime": state.database().snapshot_time(),
        "rekordboxRunning": es_db::rekordbox_running(),
        "sourceChanged": state.database().source_changed(),
        "writable": state.is_writable(),
    })
    .to_string()
}

/// Run something against the live database, then re-read the snapshot so later reads see it.
pub(crate) fn with_live<T>(state: &mut State, work: impl FnOnce(&mut Library) -> napi::Result<T>) -> napi::Result<T> {
    state
        .with_live(|live| work(live).map_err(|error| es_engine::Error::Other(error.to_string())))
        .map_err(failure)
}

/// Run one edit in a single transaction.
fn edit<T>(state: &mut State, change: impl FnOnce(&mut Tx<'_>) -> es_db::Result<T>) -> napi::Result<T> {
    state.with_live(|live| Ok(live.write(change)?)).map_err(failure)
}

fn track_ids(state: &State, playlist: &str) -> napi::Result<String> {
    json_of(&reads(state).playlist_tracks(playlist).map_err(failure)?)
}

fn playlists(state: &State) -> napi::Result<serde_json::Value> {
    serde_json::to_value(reads(state).playlists().map_err(failure)?).map_err(failure)
}

fn tracks_of(state: &State, ids: &[String]) -> napi::Result<String> {
    json_of(&reads(state).tracks_by_id(ids).map_err(failure)?)
}

fn reads(state: &State) -> es_engine::library::LibraryRead<'_> {
    es_engine::library::LibraryRead::new(state.database())
}

fn cues_json(state: &State, id: &str) -> napi::Result<String> {
    json_of(&reads(state).cues(id).map_err(failure)?)
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

/// The edited fields a file's own tags can hold; rating and colour live only in the library.
fn file_tag_edit(change: &TrackEdit) -> es_tags::Edit {
    es_tags::Edit {
        title: change.title.clone(),
        artist: change.artist.clone(),
        album: change.album.clone(),
        genre: change.genre.clone(),
        label: change.label.clone(),
        remixer: change.remixer.clone(),
        original_artist: change.original_artist.clone(),
        composer: change.composer.clone(),
        comment: change.comment.clone(),
        year: change.year.map(|y| u32::try_from(y).unwrap_or(0)),
        track_no: change.track_no.map(|n| u32::try_from(n).unwrap_or(0)),
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

/// Header checks for a batch of collection tracks; a file whose size and mtime still match the caller's cache is not read again.
fn inspect_tracks(state: &State, items: &str) -> napi::Result<String> {
    use es_engine::inspect::{inspect_file, same_fingerprint};
    let list: Vec<Value> = serde_json::from_str(items).map_err(|_| failure("tracks must be a list"))?;
    let mut out = Vec::new();
    for item in list.iter().take(80) {
        let id = item.get("id").and_then(Value::as_str).filter(|i| !i.is_empty()).ok_or_else(|| failure("tracks must be a list of track ids"))?;
        let files = state.database().track_files(id).map_err(failure)?.ok_or_else(|| failure(format!("Unknown track {id}")))?;
        if !es_engine::relocate::is_local(&files.folder_path) {
            out.push(json!({ "id": id, "size": 0, "mtime": null, "same": false, "issues": [], "audio": null }));
            continue;
        }
        let path = std::path::Path::new(&files.folder_path);
        if let (Some(size), Some(mtime)) = (item.get("size"), item.get("mtime")) {
            let hinted = size.as_i64().or_else(|| size.as_str().and_then(|s| s.trim().parse().ok()));
            let mtime = match mtime {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                other => Some(other.to_string()),
            };
            if let Some((size, mtime)) = same_fingerprint(path, hinted, mtime.as_deref()) {
                out.push(json!({ "id": id, "size": size, "mtime": mtime, "same": true, "issues": [], "audio": null }));
                continue;
            }
        }
        let found = inspect_file(path);
        let mut entry = serde_json::to_value(&found).map_err(failure)?;
        entry["id"] = json!(id);
        entry["same"] = json!(false);
        out.push(entry);
    }
    json_of(&out)
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
    TrackRelocate(String),
    TrackFindMissing(Vec<String>, String),
    TrackInspect(String),
    FilesRead(Vec<String>),
    BeatgridSet(String, String),
    Convert { ids: Vec<String>, preset: String, mode: String },
    ApplyAnalysis(String, String),
    MediaResolve { kind: String, id: String, size: String },
    CueSetHot(String, i64, i64),
    CueAddMemory(String, i64),
    CueDelete(String, String),
    CueSetColor(String, String, Option<i64>),
    TrackUpdate(Vec<String>, String, bool),
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

    fn perform(&self, service: &mut LibraryService) -> napi::Result<String> {
        match &self.op {
            Op::Open(path) => {
                let source = path
                    .clone()
                    .map(PathBuf::from)
                    .or_else(Library::installed_path)
                    .ok_or_else(|| failure("No rekordbox library found on this computer"))?;
                let writable = service.session().is_some_and(|s| s.is_writable() && s.source() == source.as_path());
                let backed_up = service.session().is_some_and(State::backed_up);
                service.open(source, writable, backed_up).map_err(failure)?;
                return service.session().map(info).ok_or_else(|| failure("Library not open"));
            }
            Op::Reload => {
                service.reload().map_err(failure)?;
                return service.session().map(info).ok_or_else(|| failure("Library not open"));
            }
            Op::BackupList | Op::BackupCreate | Op::BackupDelete(_) | Op::BackupSettings | Op::BackupSetSettings { .. } => {
                return self.backup_op(service.session().map(|s| s.source().to_path_buf()));
            }
            Op::BackupRestore(id) => {
                let source = service
                    .session()
                    .map(|s| s.source().to_path_buf())
                    .or_else(Library::installed_path)
                    .ok_or_else(|| failure("No rekordbox library found on this computer"))?;
                let backed_up = service.session().is_some_and(State::backed_up);
                service.close();
                let restored = self.core.backups.restore(id, &source, &|event| self.core.emit("backup.progress", &event));
                self.core.emit("backup.progress", &json!({ "stage": "done", "reason": "restore" }));
                self.core.emit("backup.changed", &json!({}));
                let _ = service.open(source, false, backed_up);
                self.core.emit("library.reloaded", &json!({}));
                return json_of(&restored.map_err(failure)?);
            }
            Op::DailyBackup => {
                let Some(state) = service.session() else { return Ok("false".into()) };
                let due = self.core.backups.settings()["auto"].as_bool().unwrap_or(true) && self.core.auto_due();
                if due && !es_db::rekordbox_running() {
                    self.core.back_up(state.source(), "daily")?;
                }
                return Ok(due.to_string());
            }
            _ => {}
        }
        if let Op::FilesRead(paths) = &self.op {
            return json_of(&es_engine::browse::file_rows(paths));
        }
        if let Op::Usb(op) = &self.op {
            return crate::usb::perform(&self.core, service, op);
        }
        let state = service.session_mut().ok_or_else(|| failure("Library not open"))?;
        match &self.op {
            Op::SetWritable(on) => {
                if *on && !state.backed_up() && self.core.backups.settings()["auto"].as_bool().unwrap_or(true) {
                    if es_db::rekordbox_running() {
                        return Err(failure(WRITE_BLOCKED));
                    }
                    if self.core.auto_due() {
                        self.core.back_up(state.source(), "before-edit")?;
                    }
                    state.mark_backed_up();
                }
                state.set_writable(*on);
                Ok(info(state))
            }
            Op::Info => Ok(info(state)),
            Op::Tracks => json_of(&reads(state).tracks().map_err(failure)?),
            Op::Playlists => json_of(&reads(state).playlists().map_err(failure)?),
            Op::Histories => json_of(&reads(state).histories().map_err(failure)?),
            Op::MyTags => json_of(&reads(state).my_tags().map_err(failure)?),
            Op::PlaylistMembers => json_of(&reads(state).playlist_members().map_err(failure)?),
            Op::PlaylistTracks(id) => json_of(&reads(state).playlist_tracks(id).map_err(failure)?),
            Op::HistoryTracks(id) => json_of(&reads(state).history_tracks(id).map_err(failure)?),
            Op::TrackDetail(id) => json_of(&reads(state).track_detail(id).map_err(failure)?),
            Op::TrackPreviews(ids) => {
                let (device, own): (Vec<String>, Vec<String>) = ids.iter().cloned().partition(|i| i.starts_with("usb:"));
                let mut by_id: std::collections::HashMap<String, es_index::detail::Preview> = std::collections::HashMap::new();
                for row in reads(state).previews(&own).map_err(failure)? {
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
            Op::TrackRelocate(moves) => {
                let parsed: serde_json::Map<String, serde_json::Value> = serde_json::from_str(moves).map_err(failure)?;
                let moves: Vec<(String, String)> =
                    parsed.into_iter().map(|(id, path)| (id, path.as_str().unwrap_or_default().to_owned())).collect();
                let share = state.database().share_dir();
                let ids = with_live(state, |live| es_engine::relocate::relocate(live, &share, &moves).map_err(failure))?;
                tracks_of(state, &ids)
            }
            Op::TrackFindMissing(ids, directory) => {
                let found = reads(state).find_missing(ids, std::path::Path::new(directory)).map_err(failure)?;
                json_of(&json!({
                    "matches": found.matches.iter().map(|m| json!({ "id": m.id, "path": m.path })).collect::<Vec<_>>(),
                    "scanned": found.scanned,
                }))
            }
            Op::TrackInspect(items) => inspect_tracks(state, items),
            Op::FilesRead(_) => unreachable!("handled above"),
            Op::BeatgridSet(id, body) => {
                let input: Value = serde_json::from_str(body).map_err(failure)?;
                let numbers = |key: &str| -> napi::Result<Vec<f64>> {
                    input.get(key).and_then(Value::as_array).map(|a| a.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect()).ok_or_else(|| failure(format!("{key} must be a list")))
                };
                let grid = es_engine::analysis::Grid {
                    times: numbers("times")?.iter().map(|t| t.round_ties_even() as i64).collect(),
                    beats: numbers("beats")?.iter().map(|b| *b as i64).collect(),
                    bpms: numbers("bpms")?,
                };
                let moves: Vec<es_db::CueMove> = input
                    .get("cues")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|m| {
                                Some(es_db::CueMove {
                                    id: m.get("id").map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))?,
                                    start_ms: m.get("start")?.as_f64()?.round_ties_even() as i64,
                                    end_ms: m.get("end").and_then(Value::as_f64).map(|e| e.round_ties_even() as i64),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let share = state.database().share_dir();
                let bpm = with_live(state, |live| es_engine::analysis::set_beatgrid(live, &share, id, &grid, &moves).map_err(failure))?;
                let reads = reads(state);
                json_of(&json!({
                    "beatgrid": reads.beatgrid(id).map_err(failure)?,
                    "cues": reads.cues(id).map_err(failure)?,
                    "bpm": bpm,
                }))
            }
            Op::Convert { ids, preset, mode } => {
                let mode = es_engine::convert::Mode::parse(mode).map_err(failure)?;
                let preset = es_engine::convert::clean_preset(&serde_json::from_str(preset).map_err(failure)?).map_err(failure)?;
                let share = state.database().share_dir();
                let core = &self.core;
                let outcome = with_live(state, |live| {
                    Ok(es_engine::convert::convert_tracks(live, &share, ids, &preset, mode, &|done, total, id| {
                        let mut event = json!({ "done": done, "total": total });
                        if let Some(id) = id {
                            event["id"] = json!(id);
                        }
                        core.emit("convert.progress", &event);
                    }))
                })?;
                let tracks = if outcome.ids.is_empty() { "[]".to_owned() } else { tracks_of(state, &outcome.ids)? };
                Ok(format!(
                    r#"{{"tracks":{},"ids":{},"originals":{},"failed":{}}}"#,
                    tracks,
                    json_of(&outcome.ids)?,
                    json_of(&outcome.originals)?,
                    json_of(&outcome.failed.iter().map(|(id, error)| json!({ "id": id, "error": error })).collect::<Vec<_>>())?
                ))
            }
            Op::ApplyAnalysis(id, body) => {
                use base64::Engine as _;
                let input: Value = serde_json::from_str(body).map_err(failure)?;
                let result = input.get("result").cloned().unwrap_or(Value::Null);
                let options = input.get("options").cloned().unwrap_or(Value::Null);
                let beats = result
                    .get("beats")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|b| {
                                let at = |i: usize| b.get(i).and_then(Value::as_i64).unwrap_or(0);
                                (at(0) as u16, at(1) as u16, at(2) as u32)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let waveform = base64::engine::general_purpose::STANDARD
                    .decode(result.get("waveform").and_then(Value::as_str).unwrap_or(""))
                    .map_err(failure)?;
                let key = result.get("key").filter(|k| !k.is_null()).map(|k| {
                    (k.get("tonic").and_then(Value::as_i64).unwrap_or(0), k.get("minor").and_then(Value::as_bool).unwrap_or(false))
                });
                let analysis = es_engine::analysis::Analysis {
                    beats,
                    waveform,
                    key,
                    bpm: result.get("bpm").and_then(Value::as_f64).unwrap_or(0.0),
                    duration: result.get("duration").and_then(Value::as_f64).unwrap_or(0.0),
                };
                let flag = |name: &str, default: bool| options.get(name).map_or(default, |v| v.as_bool().unwrap_or(false));
                let chosen = es_engine::analysis::Options { overwrite: flag("overwrite", false), grid: flag("grid", true), key: flag("key", true) };
                let share = state.database().share_dir();
                with_live(state, |live| es_engine::analysis::apply_analysis(live, &share, id, &analysis, chosen).map_err(failure))?;
                let rows: Vec<Value> = serde_json::from_str(&tracks_of(state, std::slice::from_ref(id))?).map_err(failure)?;
                json_of(&rows.into_iter().next().unwrap_or(Value::Null))
            }
            Op::MediaResolve { kind, id, size } => {
                let found = reads(state).resolve_media(kind, id, size).map_err(failure)?;
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
                let share = state.database().share_dir();
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
            Op::TrackUpdate(ids, fields, write_tags) => {
                let change = parse_edit(fields)?;
                let mut unique = ids.clone();
                unique.dedup();
                if change != TrackEdit::default() && !unique.is_empty() {
                    edit(state, |tx| tx.update_tracks(&unique, &change))?;
                }
                let mut errors: Vec<String> = Vec::new();
                if *write_tags && !unique.is_empty() {
                    let file_edit = file_tag_edit(&change);
                    if file_edit != es_tags::Edit::default() {
                        let mut sizes = Vec::new();
                        for id in &unique {
                            let Some(files) = state.database().track_files(id).map_err(failure)? else { continue };
                            let path = std::path::Path::new(&files.folder_path);
                            match es_tags::write(path, &file_edit) {
                                Ok(()) => {
                                    if let Ok(meta) = path.metadata() {
                                        sizes.push((id.clone(), i64::try_from(meta.len()).unwrap_or(0)));
                                    }
                                }
                                Err(e) => {
                                    let name = path.file_name().map_or_else(|| files.folder_path.clone(), |n| n.to_string_lossy().into_owned());
                                    errors.push(format!("{name}: {e}"));
                                }
                            }
                        }
                        if !sizes.is_empty() {
                            edit(state, |tx| sizes.iter().try_for_each(|(id, size)| tx.set_file_size(id, *size)))?;
                        }
                    }
                }
                Ok(format!(r#"{{"tracks":{},"tagErrors":{}}}"#, tracks_of(state, &unique)?, json_of(&errors)?))
            }
            Op::TrackImport { paths, playlist, artwork } => {
                let share = state.database().share_dir();
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
        let mut service = self.core.service.lock().map_err(|_| failure("the library lock was poisoned"))?;
        self.perform(&mut service)
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
    pub fn files_read(&self, paths: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::FilesRead(paths))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn beatgrid_set(&self, id: String, body: String) -> AsyncTask<Run> {
        self.run(Op::BeatgridSet(id, body))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_apply_analysis(&self, id: String, body: String) -> AsyncTask<Run> {
        self.run(Op::ApplyAnalysis(id, body))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_convert(&self, ids: Vec<String>, preset: String, mode: String) -> AsyncTask<Run> {
        self.run(Op::Convert { ids, preset, mode })
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_relocate(&self, moves: String) -> AsyncTask<Run> {
        self.run(Op::TrackRelocate(moves))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_find_missing(&self, ids: Vec<String>, directory: String) -> AsyncTask<Run> {
        self.run(Op::TrackFindMissing(ids, directory))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn track_inspect(&self, tracks: String) -> AsyncTask<Run> {
        self.run(Op::TrackInspect(tracks))
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
    pub fn track_update(&self, ids: Vec<String>, fields: String, write_tags: Option<bool>) -> AsyncTask<Run> {
        self.run(Op::TrackUpdate(ids, fields, write_tags.unwrap_or(false)))
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
