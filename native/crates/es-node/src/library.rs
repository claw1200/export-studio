//! The rekordbox master library for the app. Reads come from a snapshot; edits open the live
//! database for one transaction and then refresh the snapshot. Each call runs on a worker thread
//! and resolves to JSON text in the shapes the interface already consumes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::usb::UsbOp;
use es_db::{Library, TrackEdit};
use es_engine::library::{LibraryService, ServiceEvent};
use napi::bindgen_prelude::AsyncTask;
use napi::threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::{Env, JsFunction, Task};
use napi_derive::napi;
use serde::Serialize;
use serde_json::{json, Value};

type Events = ThreadsafeFunction<(String, String), ErrorStrategy::Fatal>;

/// What every call shares: the open library, where events go, and the backup folder.
pub(crate) struct Core {
    pub(crate) devices: crate::usb::Devices,
    service: Mutex<LibraryService>,
    events: Mutex<Option<Events>>,
}

impl Core {
    fn new() -> Self {
        Self {
            devices: crate::usb::Devices::default(),
            service: Mutex::new(LibraryService::new()),
            events: Mutex::new(None),
        }
    }

    pub(crate) fn emit(&self, event: &str, data: &Value) {
        if let Ok(guard) = self.events.lock() {
            if let Some(handler) = guard.as_ref() {
                handler.call((event.to_owned(), data.to_string()), ThreadsafeFunctionCallMode::NonBlocking);
            }
        }
    }

    pub(crate) fn emit_service_event(&self, event: ServiceEvent) {
        match event {
            ServiceEvent::BackupProgress(data) => self.emit("backup.progress", &data),
            ServiceEvent::BackupChanged => self.emit("backup.changed", &json!({})),
            ServiceEvent::LibraryReloaded => self.emit("library.reloaded", &json!({})),
            ServiceEvent::ConvertProgress { done, total, id } => {
                let mut data = json!({ "done": done, "total": total });
                if let Some(id) = id {
                    data["id"] = json!(id);
                }
                self.emit("convert.progress", &data);
            }
            ServiceEvent::UsbImportProgress { done, total, name } => self.emit(
                "usb.progress",
                &json!({ "phase": "import", "done": done, "total": total, "name": name }),
            ),
            ServiceEvent::UsbSyncProgress(progress) => {
                let data = match progress {
                    es_engine::sync::Progress::Step { phase, done, total, name } => {
                        json!({ "phase": phase, "done": done, "total": total, "name": name })
                    }
                    es_engine::sync::Progress::Done { tracks, playlists } => {
                        json!({ "phase": "done", "tracks": tracks, "playlists": playlists })
                    }
                };
                self.emit("usb.progress", &data);
            }
        }
    }
}

pub(crate) fn failure(reason: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(reason.to_string())
}

pub(crate) fn json_of(value: &impl Serialize) -> napi::Result<Value> {
    serde_json::to_value(value).map_err(failure)
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
    fn profile_name(&self) -> &'static str {
        match &self.op {
            Op::Open(_) => "library.open",
            Op::Reload => "library.reload",
            Op::Info => "library.info",
            Op::Tracks => "library.tracks",
            Op::Playlists => "library.playlists",
            Op::Histories => "library.histories",
            Op::MyTags => "library.myTags",
            Op::PlaylistMembers => "library.playlistMembers",
            Op::DailyBackup => "backup.daily",
            Op::BackupCreate => "backup.create",
            Op::CueSetHot(_, _, _) => "cue.setHot",
            Op::CueAddMemory(_, _) => "cue.addMemory",
            Op::CueDelete(_, _) => "cue.delete",
            Op::CueSetColor(_, _, _) => "cue.setColor",
            Op::TrackDetail(_) => "track.detail",
            Op::TrackPreviews(_) => "track.previews",
            Op::Usb(UsbOp::List) => "usb.list",
            _ => "library.other",
        }
    }

    /// Backup calls that need nothing but the library's location.
    fn backup_op(&self, service: &LibraryService) -> napi::Result<Value> {
        let emit = |event| self.core.emit_service_event(event);
        match &self.op {
            Op::BackupList => json_of(&service.backup_list()),
            Op::BackupSettings => json_of(&service.backup_settings()),
            Op::BackupSetSettings { auto, keep, interval } => {
                json_of(&service.backup_set_settings(*auto, *keep, *interval).map_err(failure)?)
            }
            Op::BackupDelete(id) => {
                let backups = service.backup_delete(id).map_err(failure)?;
                self.core.emit("backup.changed", &json!({}));
                json_of(&backups)
            }
            Op::BackupCreate => json_of(&service.backup_create(&emit).map_err(failure)?),
            _ => unreachable!("not a backup call"),
        }
    }

    fn perform(&self, service: &mut LibraryService) -> napi::Result<Value> {
        match &self.op {
            Op::Open(path) => {
                let source = path
                    .clone()
                    .map(PathBuf::from)
                    .or_else(Library::installed_path)
                    .ok_or_else(|| failure("No rekordbox library found on this computer"))?;
                let writable = service.session().is_some_and(|s| s.is_writable() && s.source() == source.as_path());
                let backed_up = service.session().is_some_and(|session| session.backed_up());
                service.open(source, writable, backed_up).map_err(failure)?;
                return json_of(&service.info().map_err(failure)?);
            }
            Op::Reload => {
                service.reload().map_err(failure)?;
                return json_of(&service.info().map_err(failure)?);
            }
            Op::BackupList | Op::BackupCreate | Op::BackupDelete(_) | Op::BackupSettings | Op::BackupSetSettings { .. } => {
                return self.backup_op(service);
            }
            Op::BackupRestore(id) => {
                let emit = |event| self.core.emit_service_event(event);
                return json_of(&service.backup_restore(id, &emit).map_err(failure)?);
            }
            Op::DailyBackup => {
                let emit = |event| self.core.emit_service_event(event);
                return Ok(json!(service.daily_backup(&emit).map_err(failure)?));
            }
            _ => {}
        }
        if let Op::FilesRead(paths) = &self.op {
            return json_of(&es_engine::browse::file_rows(paths));
        }
        if let Op::Usb(op) = &self.op {
            return crate::usb::perform(&self.core, service, op);
        }
        match &self.op {
            Op::SetWritable(writable) => {
                let emit = |event| self.core.emit_service_event(event);
                service.set_writable(*writable, &emit).map_err(failure)?;
                return json_of(&service.info().map_err(failure)?);
            }
            Op::Info => return json_of(&service.info().map_err(failure)?),
            Op::Tracks => return json_of(&service.tracks().map_err(failure)?),
            Op::Playlists => return json_of(&service.playlists().map_err(failure)?),
            Op::Histories => return json_of(&service.histories().map_err(failure)?),
            Op::MyTags => return json_of(&service.my_tags().map_err(failure)?),
            Op::PlaylistMembers => return json_of(&service.playlist_members().map_err(failure)?),
            Op::PlaylistTracks(id) => return json_of(&service.playlist_tracks(id).map_err(failure)?),
            Op::HistoryTracks(id) => return json_of(&service.history_tracks(id).map_err(failure)?),
            Op::TrackDetail(id) => return json_of(&service.track_detail(id).map_err(failure)?),
            Op::TrackPreviews(ids) => {
                let (device, own): (Vec<String>, Vec<String>) = ids.iter().cloned().partition(|id| id.starts_with("usb:"));
                let mut by_id: std::collections::HashMap<String, es_index::detail::Preview> = service
                    .previews(&own)
                    .map_err(failure)?
                    .into_iter()
                    .map(|row| (row.id.clone(), row))
                    .collect();
                let devices = &self.core.devices;
                for row in es_index::device::previews(&mut |volume| devices.load(volume).ok().flatten(), &device) {
                    by_id.insert(row.id.clone(), row);
                }
                let mut seen = std::collections::HashSet::new();
                let ordered: Vec<_> = ids.iter().filter(|id| seen.insert(id.as_str())).take(80).filter_map(|id| by_id.remove(id)).collect();
                return json_of(&ordered);
            }
            Op::TrackFindMissing(ids, directory) => {
                let found = service.find_missing(ids, std::path::Path::new(directory)).map_err(failure)?;
                return json_of(&json!({
                    "matches": found.matches.iter().map(|item| json!({ "id": item.id, "path": item.path })).collect::<Vec<_>>(),
                    "scanned": found.scanned,
                }));
            }
            Op::TrackInspect(body) => {
                let items: Vec<Value> = serde_json::from_str(body).map_err(|_| failure("tracks must be a list"))?;
                let mut requests = Vec::new();
                for item in items.iter().take(80) {
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| failure("tracks must be a list of track ids"))?;
                    let has_cache = item.get("size").is_some() && item.get("mtime").is_some();
                    let cached_size = item
                        .get("size")
                        .and_then(|size| size.as_i64().or_else(|| size.as_str().and_then(|value| value.trim().parse().ok())));
                    let cached_mtime = item.get("mtime").and_then(|mtime| match mtime {
                        Value::Null => None,
                        Value::String(value) => Some(value.clone()),
                        other => Some(other.to_string()),
                    });
                    requests.push(es_engine::library::InspectionRequest {
                        id: id.to_owned(),
                        cached_size,
                        cached_mtime,
                        has_cache,
                    });
                }
                return json_of(&service.inspect_tracks(&requests).map_err(failure)?);
            }
            Op::MediaResolve { kind, id, size } => {
                if let Some((volume, export_id)) = es_index::device::parse_track_key(id) {
                    let volume = PathBuf::from(volume);
                    let found = self.core.devices.load(&volume)?.and_then(|device| {
                        let track = device.track(export_id)?;
                        match kind.as_str() {
                            "art" => es_index::device::artwork_file(&volume, track, size),
                            "audio" => es_index::device::audio_file(&volume, track).map(PathBuf::from),
                            _ => None,
                        }
                    });
                    return json_of(&found);
                }
                return json_of(&service.resolve_media(kind, id, size).map_err(failure)?);
            }
            Op::TrackUpdate(ids, fields, write_tags) => {
                let change = parse_edit(fields)?;
                return json_of(&service.track_update(ids, &change, *write_tags).map_err(failure)?);
            }
            Op::TrackImport { paths, playlist, artwork } => {
                return json_of(&service.track_import(paths, playlist.clone(), *artwork).map_err(failure)?);
            }
            Op::TrackRelocate(body) => {
                let parsed: serde_json::Map<String, Value> = serde_json::from_str(body).map_err(failure)?;
                let moves: Vec<(String, String)> = parsed
                    .into_iter()
                    .map(|(id, path)| (id, path.as_str().unwrap_or_default().to_owned()))
                    .collect();
                return json_of(&service.track_relocate(&moves).map_err(failure)?);
            }
            Op::BeatgridSet(id, body) => {
                let input: Value = serde_json::from_str(body).map_err(failure)?;
                let numbers = |key: &str| -> napi::Result<Vec<f64>> {
                    input
                        .get(key)
                        .and_then(Value::as_array)
                        .map(|values| values.iter().map(|value| value.as_f64().unwrap_or(f64::NAN)).collect())
                        .ok_or_else(|| failure(format!("{key} must be a list")))
                };
                let grid = es_engine::analysis::Grid {
                    times: numbers("times")?.iter().map(|time| time.round_ties_even() as i64).collect(),
                    beats: numbers("beats")?.iter().map(|beat| *beat as i64).collect(),
                    bpms: numbers("bpms")?,
                };
                let cues: Vec<es_db::CueMove> = input
                    .get("cues")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                Some(es_db::CueMove {
                                    id: item.get("id").map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned))?,
                                    start_ms: item.get("start")?.as_f64()?.round_ties_even() as i64,
                                    end_ms: item.get("end").and_then(Value::as_f64).map(|end| end.round_ties_even() as i64),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                return json_of(&service.beatgrid_set(id, &grid, &cues).map_err(failure)?);
            }
            Op::ApplyAnalysis(id, body) => {
                use base64::Engine as _;
                let input: Value = serde_json::from_str(body).map_err(failure)?;
                let result = input.get("result").cloned().unwrap_or(Value::Null);
                let options = input.get("options").cloned().unwrap_or(Value::Null);
                let beats = result
                    .get("beats")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .map(|beat| {
                                let at = |index: usize| beat.get(index).and_then(Value::as_i64).unwrap_or(0);
                                (at(0) as u16, at(1) as u16, at(2) as u32)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let waveform = base64::engine::general_purpose::STANDARD
                    .decode(result.get("waveform").and_then(Value::as_str).unwrap_or(""))
                    .map_err(failure)?;
                let key = result.get("key").filter(|value| !value.is_null()).map(|value| {
                    (value.get("tonic").and_then(Value::as_i64).unwrap_or(0), value.get("minor").and_then(Value::as_bool).unwrap_or(false))
                });
                let analysis = es_engine::analysis::Analysis {
                    beats,
                    waveform,
                    key,
                    bpm: result.get("bpm").and_then(Value::as_f64).unwrap_or(0.0),
                    duration: result.get("duration").and_then(Value::as_f64).unwrap_or(0.0),
                };
                let flag = |name: &str, default: bool| options.get(name).map_or(default, |value| value.as_bool().unwrap_or(false));
                let chosen = es_engine::analysis::Options {
                    overwrite: flag("overwrite", false),
                    grid: flag("grid", true),
                    key: flag("key", true),
                };
                return json_of(&service.track_apply_analysis(id, &analysis, chosen).map_err(failure)?);
            }
            Op::Convert { ids, preset, mode } => {
                let mode = es_engine::convert::Mode::parse(mode).map_err(failure)?;
                let preset = es_engine::convert::clean_preset(&serde_json::from_str(preset).map_err(failure)?).map_err(failure)?;
                let emit = |event| self.core.emit_service_event(event);
                return json_of(&service.track_convert(ids, &preset, mode, &emit).map_err(failure)?);
            }
            _ => {}
        }
        match &self.op {
            Op::CueSetHot(id, kind, ms) => return json_of(&service.cue_set_hot(id, *kind, *ms).map_err(failure)?),
            Op::CueAddMemory(id, ms) => return json_of(&service.cue_add_memory(id, *ms).map_err(failure)?),
            Op::CueDelete(id, cue_id) => return json_of(&service.cue_delete(id, cue_id).map_err(failure)?),
            Op::CueSetColor(id, cue_id, color) => {
                return json_of(&service.cue_set_color(id, cue_id, *color).map_err(failure)?);
            }
            Op::RemoveTracks(ids) => {
                let removed = service.remove_tracks(ids).map_err(failure)?;
                return json_of(&json!({ "removed": removed.ids, "playlists": removed.playlists }));
            }
            Op::PlaylistAdd(id, ids) => return json_of(&service.playlist_add(id, ids).map_err(failure)?),
            Op::PlaylistRemove(id, ids) => return json_of(&service.playlist_remove(id, ids).map_err(failure)?),
            Op::PlaylistMove(id, ids, index) => {
                return json_of(&service.playlist_move(id, ids, *index as usize).map_err(failure)?);
            }
            Op::PlaylistCreate { name, parent, folder, ids } => {
                let (id, playlists) = service.playlist_create(name, parent.as_deref(), *folder, ids).map_err(failure)?;
                return json_of(&json!({ "id": id, "playlists": playlists }));
            }
            Op::PlaylistRename(id, name) => return json_of(&service.playlist_rename(id, name).map_err(failure)?),
            Op::PlaylistDelete(id) => return json_of(&service.playlist_delete(id).map_err(failure)?),
            Op::TagAdd(tag_id, ids) => return json_of(&service.tag_add(tag_id, ids).map_err(failure)?),
            Op::TagRemove(tag_id, ids) => return json_of(&service.tag_remove(tag_id, ids).map_err(failure)?),
            _ => {}
        }
        unreachable!("every operation is handled before the session fallback")
    }
}

impl Task for Run {
    type Output = Value;
    type JsValue = napi::JsUnknown;

    fn compute(&mut self) -> napi::Result<Value> {
        let profile = std::env::var_os("EXPORTSTUDIO_PROFILE_LIBRARY_OPEN").is_some();
        let operation = self.profile_name();
        let waiting = Instant::now();
        let mut service = self.core.service.lock().map_err(|_| failure("the library lock was poisoned"))?;
        let lock_wait_ms = waiting.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let result = self.perform(&mut service);
        if profile {
            eprintln!(
                "[library-profile] native op={operation} lock_wait_ms={lock_wait_ms:.1} operation_ms={:.1}",
                started.elapsed().as_secs_f64() * 1000.0,
            );
        }
        result
    }

    fn resolve(&mut self, env: Env, output: Value) -> napi::Result<napi::JsUnknown> {
        env.to_js_value(&output)
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

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn open(&self, path: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Open(path))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn reload(&self) -> AsyncTask<Run> {
        self.run(Op::Reload)
    }

    /// Whether edits are allowed; the app sets it when the user unlocks editing.
    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn set_writable(&self, on: bool) -> AsyncTask<Run> {
        self.run(Op::SetWritable(on))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn info(&self) -> AsyncTask<Run> {
        self.run(Op::Info)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn tracks(&self) -> AsyncTask<Run> {
        self.run(Op::Tracks)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlists(&self) -> AsyncTask<Run> {
        self.run(Op::Playlists)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn histories(&self) -> AsyncTask<Run> {
        self.run(Op::Histories)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn my_tags(&self) -> AsyncTask<Run> {
        self.run(Op::MyTags)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_members(&self) -> AsyncTask<Run> {
        self.run(Op::PlaylistMembers)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_tracks(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::PlaylistTracks(id))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn history_tracks(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::HistoryTracks(id))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_detail(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::TrackDetail(id))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_previews(&self, ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::TrackPreviews(ids))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn files_read(&self, paths: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::FilesRead(paths))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn beatgrid_set(&self, id: String, body: String) -> AsyncTask<Run> {
        self.run(Op::BeatgridSet(id, body))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_apply_analysis(&self, id: String, body: String) -> AsyncTask<Run> {
        self.run(Op::ApplyAnalysis(id, body))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_convert(&self, ids: Vec<String>, preset: String, mode: String) -> AsyncTask<Run> {
        self.run(Op::Convert { ids, preset, mode })
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_relocate(&self, moves: String) -> AsyncTask<Run> {
        self.run(Op::TrackRelocate(moves))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_find_missing(&self, ids: Vec<String>, directory: String) -> AsyncTask<Run> {
        self.run(Op::TrackFindMissing(ids, directory))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_inspect(&self, tracks: String) -> AsyncTask<Run> {
        self.run(Op::TrackInspect(tracks))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn media_resolve(&self, kind: String, id: String, size: String) -> AsyncTask<Run> {
        self.run(Op::MediaResolve { kind, id, size })
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn cue_set_hot(&self, id: String, kind: i64, ms: i64) -> AsyncTask<Run> {
        self.run(Op::CueSetHot(id, kind, ms))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn cue_add_memory(&self, id: String, ms: i64) -> AsyncTask<Run> {
        self.run(Op::CueAddMemory(id, ms))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn cue_delete(&self, id: String, cue_id: String) -> AsyncTask<Run> {
        self.run(Op::CueDelete(id, cue_id))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn cue_set_color(&self, id: String, cue_id: String, color_index: Option<i64>) -> AsyncTask<Run> {
        self.run(Op::CueSetColor(id, cue_id, color_index))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
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

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn backup_list(&self) -> AsyncTask<Run> {
        self.run(Op::BackupList)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn backup_create(&self) -> AsyncTask<Run> {
        self.run(Op::BackupCreate)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn backup_delete(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::BackupDelete(id))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn backup_restore(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::BackupRestore(id))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn backup_settings(&self) -> AsyncTask<Run> {
        self.run(Op::BackupSettings)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn backup_set_settings(&self, auto: Option<bool>, keep: Option<i64>, interval: Option<i64>) -> AsyncTask<Run> {
        self.run(Op::BackupSetSettings { auto, keep, interval })
    }

    /// Takes a daily backup when one is due; resolves to whether it was due.
    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn daily_backup(&self) -> AsyncTask<Run> {
        self.run(Op::DailyBackup)
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn track_import(&self, paths: Vec<String>, playlist_id: Option<String>, artwork: bool) -> AsyncTask<Run> {
        self.run(Op::TrackImport { paths, playlist: playlist_id, artwork })
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn remove_tracks(&self, ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::RemoveTracks(ids))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_add(&self, id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::PlaylistAdd(id, track_ids))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_remove(&self, id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::PlaylistRemove(id, track_ids))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_move(&self, id: String, track_ids: Vec<String>, index: u32) -> AsyncTask<Run> {
        self.run(Op::PlaylistMove(id, track_ids, index))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_create(
        &self,
        name: String,
        parent_id: Option<String>,
        folder: bool,
        track_ids: Vec<String>,
    ) -> AsyncTask<Run> {
        self.run(Op::PlaylistCreate { name, parent: parent_id, folder, ids: track_ids })
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_rename(&self, id: String, name: String) -> AsyncTask<Run> {
        self.run(Op::PlaylistRename(id, name))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn playlist_delete(&self, id: String) -> AsyncTask<Run> {
        self.run(Op::PlaylistDelete(id))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn tag_add(&self, tag_id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::TagAdd(tag_id, track_ids))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn tag_remove(&self, tag_id: String, track_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::TagRemove(tag_id, track_ids))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_list(&self) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::List))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_eject(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Eject(volume)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_format(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Format(volume)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_playlists(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Playlists(volume)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_libraries(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Libraries(volume)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_selection(&self, volume: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Selection(volume)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_tracks(&self, volume: String, playlist_id: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Tracks(volume, playlist_id)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_detail(&self, key: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Detail(key)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_media(&self, key: String) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Media(key)))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_import_preview(&self, volume: String, folder: Option<String>, policy: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::ImportPreview { volume, folder, policy }))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_import(&self, volume: String, folder: Option<String>, policy: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Import { volume, folder, policy }))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_sync(&self, volume: String, playlist_ids: Vec<String>, overwrite: bool, review: Option<Value>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::Sync { volume, playlists: playlist_ids, overwrite, review }))
    }

    #[napi(ts_return_type = "Promise<unknown>")]
    pub fn usb_sync_review(&self, volume: String, playlist_ids: Vec<String>) -> AsyncTask<Run> {
        self.run(Op::Usb(UsbOp::SyncReview { volume, playlists: playlist_ids }))
    }
}

impl Default for RekordboxLibrary {
    fn default() -> Self {
        Self::new()
    }
}
