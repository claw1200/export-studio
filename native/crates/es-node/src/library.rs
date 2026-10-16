//! Reads of the rekordbox master library for the app. Each call runs on a worker thread and
//! resolves to JSON text of the shapes the interface already consumes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use es_db::Library;
use napi::bindgen_prelude::AsyncTask;
use napi::{Env, Task};
use napi_derive::napi;
use serde::Serialize;
use serde_json::json;

struct State {
    lib: Library,
    source: PathBuf,
}

type Shared = Arc<Mutex<Option<State>>>;

fn failure(reason: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(reason.to_string())
}

fn open(source: PathBuf) -> napi::Result<State> {
    let lib = Library::open_snapshot(&source).map_err(failure)?;
    Ok(State { lib, source })
}

fn json_of(value: &impl Serialize) -> napi::Result<String> {
    serde_json::to_string(value).map_err(failure)
}

fn info(state: &State) -> String {
    json!({
        "dbPath": state.source,
        "snapshotTime": state.lib.snapshot_time(),
        "rekordboxRunning": es_db::rekordbox_running(),
        "sourceChanged": state.lib.source_changed(),
        "writable": false,
    })
    .to_string()
}

enum Op {
    Open(Option<String>),
    Reload,
    Info,
    Tracks,
    Playlists,
    Histories,
    MyTags,
    PlaylistMembers,
    PlaylistTracks(String),
    HistoryTracks(String),
}

pub struct Run {
    shared: Shared,
    op: Op,
}

impl Task for Run {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<String> {
        let mut slot = self.shared.lock().map_err(|_| failure("the library lock was poisoned"))?;
        match &self.op {
            Op::Open(path) => {
                let source = path
                    .clone()
                    .map(PathBuf::from)
                    .or_else(Library::installed_path)
                    .ok_or_else(|| failure("No rekordbox library found on this computer"))?;
                *slot = None;
                let state = open(source)?;
                let out = info(&state);
                *slot = Some(state);
                return Ok(out);
            }
            Op::Reload => {
                let source = slot.as_ref().map(|s| s.source.clone()).ok_or_else(|| failure("Library not open"))?;
                *slot = None;
                let state = open(source)?;
                let out = info(&state);
                *slot = Some(state);
                return Ok(out);
            }
            _ => {}
        }
        let state = slot.as_ref().ok_or_else(|| failure("Library not open"))?;
        let lib = &state.lib;
        match &self.op {
            Op::Info => Ok(info(state)),
            Op::Tracks => json_of(&es_index::track_rows(lib, &lib.share_dir(), None).map_err(failure)?),
            Op::Playlists => json_of(&es_index::playlist_nodes(lib).map_err(failure)?),
            Op::Histories => json_of(&es_index::history_nodes(lib).map_err(failure)?),
            Op::MyTags => json_of(&es_index::tag_nodes(lib).map_err(failure)?),
            Op::PlaylistMembers => json_of(&lib.playlist_member_ids().map_err(failure)?),
            Op::PlaylistTracks(id) => match lib.playlist_track_ids(id) {
                Ok(ids) => json_of(&ids),
                // Smart playlists are not evaluated yet; the caller falls back to the Python engine.
                Err(e @ es_db::Error::Unsupported(_)) => Err(napi::Error::new(napi::Status::GenericFailure, format!("unsupported: {e}"))),
                Err(e) => Err(failure(e)),
            },
            Op::HistoryTracks(id) => json_of(&lib.history_track_ids(id).map_err(failure)?),
            Op::Open(_) | Op::Reload => unreachable!("handled above"),
        }
    }

    fn resolve(&mut self, _env: Env, output: String) -> napi::Result<String> {
        Ok(output)
    }
}

/// A read-only snapshot of the master library.
#[napi]
pub struct RekordboxLibrary {
    shared: Shared,
}

#[napi]
impl RekordboxLibrary {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self { shared: Arc::new(Mutex::new(None)) }
    }

    fn run(&self, op: Op) -> AsyncTask<Run> {
        AsyncTask::new(Run { shared: Arc::clone(&self.shared), op })
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn open(&self, path: Option<String>) -> AsyncTask<Run> {
        self.run(Op::Open(path))
    }

    #[napi(ts_return_type = "Promise<string>")]
    pub fn reload(&self) -> AsyncTask<Run> {
        self.run(Op::Reload)
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
}

impl Default for RekordboxLibrary {
    fn default() -> Self {
        Self::new()
    }
}
