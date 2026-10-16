//! Typed read operations exposed by the application layer.
//!
//! The N-API adapter serializes these results for the existing Electron contract. Rust callers
//! work with domain and read-model types rather than JSON text.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use es_backup::Backups;
use es_db::{Guard, Library as Database};
use serde::Serialize;
use serde_json::{json, Value};

use crate::Result;

/// One selected library and its current read snapshot.
pub struct LibrarySession {
    database: Database,
    source: PathBuf,
    writable: bool,
    backed_up: bool,
}

pub struct LibraryService {
    session: Option<LibrarySession>,
    backups: Backups,
}

#[derive(Debug)]
pub enum ServiceEvent {
    BackupProgress(Value),
    BackupChanged,
    LibraryReloaded,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub db_path: String,
    pub snapshot_time: f64,
    pub rekordbox_running: bool,
    pub source_changed: bool,
    pub writable: bool,
}

impl LibraryService {
    #[must_use]
    pub fn new() -> Self {
        let root = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map_or_else(|| PathBuf::from("."), PathBuf::from)
            .join(".exportstudio")
            .join("backups");
        Self::with_backup_root(root)
    }

    fn with_backup_root(root: PathBuf) -> Self {
        let verify = |path: &Path| {
            es_db::verify(path)
                .map(|counts| es_backup::Counts {
                    tracks: counts.tracks,
                    playlists: counts.playlists,
                })
                .map_err(|error| es_backup::Error::Message(error.to_string()))
        };
        Self {
            session: None,
            backups: Backups::new(root, verify, es_db::rekordbox_running),
        }
    }

    pub fn open(&mut self, source: PathBuf, writable: bool, backed_up: bool) -> Result<()> {
        self.session = None;
        self.session = Some(LibrarySession::open(source, writable, backed_up)?);
        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        let (source, writable, backed_up) = self
            .session
            .as_ref()
            .map(|session| {
                (
                    session.source().to_path_buf(),
                    session.is_writable(),
                    session.backed_up(),
                )
            })
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        self.open(source, writable, backed_up)
    }

    pub fn close(&mut self) -> Option<LibrarySession> {
        self.session.take()
    }

    #[must_use]
    pub fn session(&self) -> Option<&LibrarySession> {
        self.session.as_ref()
    }

    pub fn session_mut(&mut self) -> Option<&mut LibrarySession> {
        self.session.as_mut()
    }

    fn read_model(&self) -> Result<LibraryRead<'_>> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        Ok(LibraryRead::new(session.database()))
    }

    pub fn info(&self) -> Result<LibraryInfo> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        let database = session.database();
        Ok(LibraryInfo {
            db_path: session.source().to_string_lossy().into_owned(),
            snapshot_time: database.snapshot_time(),
            rekordbox_running: es_db::rekordbox_running(),
            source_changed: database.source_changed(),
            writable: session.is_writable(),
        })
    }

    pub fn tracks(&self) -> Result<Vec<es_index::TrackRow>> {
        self.read_model()?.tracks()
    }

    pub fn tracks_by_id(&self, ids: &[String]) -> Result<Vec<es_index::TrackRow>> {
        self.read_model()?.tracks_by_id(ids)
    }

    pub fn playlists(&self) -> Result<Vec<es_index::Node>> {
        self.read_model()?.playlists()
    }

    pub fn histories(&self) -> Result<Vec<es_index::Node>> {
        self.read_model()?.histories()
    }

    pub fn my_tags(&self) -> Result<Vec<es_index::Node>> {
        self.read_model()?.my_tags()
    }

    pub fn playlist_members(&self) -> Result<Vec<String>> {
        self.read_model()?.playlist_members()
    }

    pub fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<String>> {
        self.read_model()?.playlist_tracks(playlist_id)
    }

    pub fn history_tracks(&self, history_id: &str) -> Result<Vec<String>> {
        self.read_model()?.history_tracks(history_id)
    }

    pub fn track_detail(&self, id: &str) -> Result<es_index::detail::Detail> {
        self.read_model()?.track_detail(id)
    }

    pub fn cues(&self, id: &str) -> Result<Vec<es_index::detail::Cue>> {
        self.read_model()?.cues(id)
    }

    pub fn beatgrid(&self, id: &str) -> Result<Option<es_index::detail::Grid>> {
        self.read_model()?.beatgrid(id)
    }

    pub fn previews(&self, ids: &[String]) -> Result<Vec<es_index::detail::Preview>> {
        self.read_model()?.previews(ids)
    }

    pub fn resolve_media(&self, kind: &str, id: &str, size: &str) -> Result<Option<PathBuf>> {
        self.read_model()?.resolve_media(kind, id, size)
    }

    pub fn find_missing(&self, ids: &[String], directory: &Path) -> Result<crate::relocate::Found> {
        self.read_model()?.find_missing(ids, directory)
    }

    fn backups(&self) -> &Backups {
        &self.backups
    }

    fn source(&self) -> Option<PathBuf> {
        self.session
            .as_ref()
            .map(|session| session.source().to_path_buf())
            .or_else(Database::installed_path)
    }

    fn auto_due(&self) -> bool {
        let interval = self.backups().settings()["interval"].as_i64().unwrap_or(12) as f64 * 3600.0;
        match self
            .backups()
            .latest_automatic()
            .and_then(|manifest| manifest["created"].as_f64())
        {
            None => true,
            Some(created) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0.0, |duration| duration.as_secs_f64());
                now - created >= interval
            }
        }
    }

    fn back_up(
        &self,
        source: &Path,
        reason: &str,
        emit: &impl Fn(ServiceEvent),
    ) -> Result<Option<Value>> {
        let made = self.backups().create(source, reason, &|progress| {
            emit(ServiceEvent::BackupProgress(progress))
        })?;
        emit(ServiceEvent::BackupProgress(
            json!({ "stage": "done", "reason": reason }),
        ));
        if made.is_some() {
            emit(ServiceEvent::BackupChanged);
        }
        Ok(made)
    }

    pub fn backup_list(&self) -> Vec<Value> {
        let source = self.source();
        self.backups().list(source.as_deref())
    }

    #[must_use]
    pub fn backup_settings(&self) -> Value {
        self.backups().settings()
    }

    pub fn backup_set_settings(
        &self,
        auto: Option<bool>,
        keep: Option<i64>,
        interval: Option<i64>,
    ) -> Result<Value> {
        Ok(self.backups().set_settings(auto, keep, interval)?)
    }

    pub fn backup_create(&self, emit: &impl Fn(ServiceEvent)) -> Result<Option<Value>> {
        let source = self.source().ok_or_else(|| {
            crate::Error::Other("No rekordbox library found on this computer".into())
        })?;
        self.back_up(&source, "manual", emit)
    }

    pub fn backup_delete(&self, id: &str) -> Result<Vec<Value>> {
        self.backups().delete(id)?;
        Ok(self.backup_list())
    }

    pub fn backup_restore(&mut self, id: &str, emit: &impl Fn(ServiceEvent)) -> Result<Value> {
        let source = self.source().ok_or_else(|| {
            crate::Error::Other("No rekordbox library found on this computer".into())
        })?;
        let backed_up = self.session.as_ref().is_some_and(LibrarySession::backed_up);
        self.close();
        let restored = self.backups().restore(id, &source, &|progress| {
            emit(ServiceEvent::BackupProgress(progress))
        });
        emit(ServiceEvent::BackupProgress(
            json!({ "stage": "done", "reason": "restore" }),
        ));
        emit(ServiceEvent::BackupChanged);
        let _ = self.open(source, false, backed_up);
        emit(ServiceEvent::LibraryReloaded);
        Ok(restored?)
    }

    pub fn daily_backup(&self, emit: &impl Fn(ServiceEvent)) -> Result<bool> {
        let Some(session) = self.session.as_ref() else {
            return Ok(false);
        };
        let due = self.backups().settings()["auto"].as_bool().unwrap_or(true) && self.auto_due();
        if due && !es_db::rekordbox_running() {
            self.back_up(session.source(), "daily", emit)?;
        }
        Ok(due)
    }

    pub fn set_writable(&mut self, writable: bool, emit: &impl Fn(ServiceEvent)) -> Result<()> {
        let Some(session) = self.session.as_ref() else {
            return Err(crate::Error::Other("Library not open".into()));
        };
        if writable
            && !session.backed_up()
            && self.backups().settings()["auto"].as_bool().unwrap_or(true)
        {
            if es_db::rekordbox_running() {
                return Err(crate::Error::Other(
                    "Close rekordbox before editing the library".into(),
                ));
            }
            if self.auto_due() {
                self.back_up(session.source(), "before-edit", emit)?;
            }
            self.session
                .as_mut()
                .expect("session checked above")
                .mark_backed_up();
        }
        self.session
            .as_mut()
            .expect("session checked above")
            .set_writable(writable);
        Ok(())
    }
}

impl Default for LibraryService {
    fn default() -> Self {
        Self::new()
    }
}

impl LibrarySession {
    pub fn open(source: PathBuf, writable: bool, backed_up: bool) -> Result<Self> {
        let database = Database::open_snapshot(&source)?;
        Ok(Self {
            database,
            source,
            writable,
            backed_up,
        })
    }

    #[must_use]
    pub fn database(&self) -> &Database {
        &self.database
    }

    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.writable
    }

    pub fn set_writable(&mut self, writable: bool) {
        self.writable = writable;
    }

    #[must_use]
    pub fn backed_up(&self) -> bool {
        self.backed_up
    }

    pub fn mark_backed_up(&mut self) {
        self.backed_up = true;
    }

    /// Run an edit against the guarded live database, then refresh the read snapshot.
    pub fn with_live<T>(&mut self, work: impl FnOnce(&mut Database) -> Result<T>) -> Result<T> {
        if !self.writable {
            return Err(crate::Error::Other(
                "Unlock editing to make changes to the library".into(),
            ));
        }
        let mut live = Database::open_live(&self.source, Guard::RefuseWhileRunning)?;
        let out = work(&mut live)?;
        drop(live);
        self.database = Database::open_snapshot(&self.source)?;
        Ok(out)
    }
}

pub struct LibraryRead<'a> {
    database: &'a Database,
    share: std::path::PathBuf,
}

impl<'a> LibraryRead<'a> {
    #[must_use]
    pub fn new(database: &'a Database) -> Self {
        Self {
            database,
            share: database.share_dir(),
        }
    }

    pub fn tracks(&self) -> Result<Vec<es_index::TrackRow>> {
        Ok(es_index::track_rows(self.database, &self.share, None)?)
    }

    pub fn tracks_by_id(&self, ids: &[String]) -> Result<Vec<es_index::TrackRow>> {
        Ok(es_index::track_rows(self.database, &self.share, Some(ids))?)
    }

    pub fn playlists(&self) -> Result<Vec<es_index::Node>> {
        Ok(es_index::playlist_nodes(self.database)?)
    }

    pub fn histories(&self) -> Result<Vec<es_index::Node>> {
        Ok(es_index::history_nodes(self.database)?)
    }

    pub fn my_tags(&self) -> Result<Vec<es_index::Node>> {
        Ok(es_index::tag_nodes(self.database)?)
    }

    pub fn playlist_members(&self) -> Result<Vec<String>> {
        Ok(self.database.playlist_member_ids()?)
    }

    pub fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<String>> {
        Ok(self.database.playlist_track_ids(playlist_id)?)
    }

    pub fn history_tracks(&self, history_id: &str) -> Result<Vec<String>> {
        Ok(self.database.history_track_ids(history_id)?)
    }

    pub fn track_detail(&self, id: &str) -> Result<es_index::detail::Detail> {
        Ok(es_index::detail::track_detail(
            self.database,
            &self.share,
            id,
        )?)
    }

    pub fn cues(&self, id: &str) -> Result<Vec<es_index::detail::Cue>> {
        Ok(es_index::detail::cues_for(self.database, &self.share, id)?)
    }

    pub fn beatgrid(&self, id: &str) -> Result<Option<es_index::detail::Grid>> {
        Ok(es_index::detail::beatgrid_for(
            self.database,
            &self.share,
            id,
        )?)
    }

    pub fn previews(&self, ids: &[String]) -> Result<Vec<es_index::detail::Preview>> {
        Ok(es_index::detail::previews(self.database, &self.share, ids)?)
    }

    pub fn resolve_media(&self, kind: &str, id: &str, size: &str) -> Result<Option<PathBuf>> {
        Ok(es_index::media::resolve_media(
            self.database,
            &self.share,
            kind,
            id,
            size,
        )?)
    }

    pub fn find_missing(&self, ids: &[String], directory: &Path) -> Result<crate::relocate::Found> {
        crate::relocate::find_missing(self.database, ids, directory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use es_db::fixture::{self, Rows};

    #[test]
    fn reads_return_typed_library_projections() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        {
            let connection = fixture::connect(&path).unwrap();
            Rows(&connection).track("track-1", "Test track").unwrap();
            Rows(&connection)
                .playlist("playlist-1", "Test playlist", 0)
                .unwrap();
        }

        let mut service = LibraryService::with_backup_root(directory.path().join("backups"));
        service.open(path.clone(), false, false).unwrap();
        let tracks = service.tracks().unwrap();
        let playlists = service.playlists().unwrap();
        let info = service.info().unwrap();

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Test track");
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "Test playlist");
        assert_eq!(info.db_path, path.to_string_lossy());
        assert!(!info.writable);
    }

    #[test]
    fn read_only_session_rejects_live_work() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        let mut session = LibrarySession::open(path, false, false).unwrap();

        let error = session.with_live(|_| Ok(())).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Unlock editing to make changes to the library"
        );
    }

    #[test]
    fn service_reload_preserves_session_policy() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        let mut service = LibraryService::with_backup_root(directory.path().join("backups"));

        service.open(path.clone(), true, true).unwrap();
        service.reload().unwrap();

        let session = service.session().unwrap();
        assert_eq!(session.source(), path);
        assert!(session.is_writable());
        assert!(session.backed_up());
        assert!(service.close().is_some());
        assert!(service.session().is_none());
    }

    #[test]
    fn failed_open_clears_the_previous_session() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        let mut service = LibraryService::with_backup_root(directory.path().join("backups"));
        service.open(path, false, false).unwrap();

        assert!(service
            .open(directory.path().join("missing.db"), false, false)
            .is_err());
        assert!(service.session().is_none());
    }

    #[test]
    fn daily_backup_without_a_session_is_a_noop() {
        let directory = tempfile::tempdir().unwrap();
        let service = LibraryService::with_backup_root(directory.path().join("backups"));

        assert!(!service.daily_backup(&|_| {}).unwrap());
        assert!(!directory.path().join("backups").exists());
    }
}
