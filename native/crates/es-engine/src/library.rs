//! Typed read operations exposed by the application layer.
//!
//! The N-API adapter serializes these results for the existing Electron contract. Rust callers
//! work with domain and read-model types rather than JSON text.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use es_backup::Backups;
use es_db::{Guard, Library as Database, TrackEdit, Tx};
use serde::Serialize;
use serde_json::{json, Value};

use crate::Result;

/// One selected library and its current read snapshot.
pub struct LibrarySession {
    database: Database,
    live: Option<Database>,
    source: PathBuf,
    writable: bool,
    backed_up: bool,
    guard: Guard,
    snapshot_refresh_pending: bool,
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
    ConvertProgress {
        done: usize,
        total: usize,
        id: Option<String>,
    },
    UsbImportProgress {
        done: usize,
        total: usize,
        name: String,
    },
    UsbSyncProgress(crate::sync::Progress),
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

#[derive(Debug)]
pub struct RemovedTracks {
    pub ids: Vec<String>,
    pub playlists: Vec<es_index::Node>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackUpdateResult {
    pub tracks: Vec<es_index::TrackRow>,
    pub tag_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TrackImportFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct TrackImportResult {
    pub tracks: Vec<es_index::TrackRow>,
    pub ids: Vec<String>,
    pub skipped: usize,
    pub failed: Vec<TrackImportFailure>,
    pub playlist: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatgridResult {
    pub beatgrid: Option<es_index::detail::Grid>,
    pub cues: Vec<es_index::detail::Cue>,
    pub bpm: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TrackConvertFailure {
    pub id: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct TrackConvertResult {
    pub tracks: Vec<es_index::TrackRow>,
    pub ids: Vec<String>,
    pub originals: Vec<String>,
    pub failed: Vec<TrackConvertFailure>,
}

#[derive(Debug, Clone)]
pub struct InspectionRequest {
    pub id: String,
    pub cached_size: Option<i64>,
    pub cached_mtime: Option<String>,
    pub has_cache: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInspection {
    pub id: String,
    pub size: u64,
    pub mtime: Option<String>,
    pub same: bool,
    pub issues: Vec<crate::inspect::Issue>,
    pub audio: Option<crate::inspect::Audio>,
}

fn tag_edit(change: &TrackEdit) -> es_tags::Edit {
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
        year: change.year.map(|year| u32::try_from(year).unwrap_or(0)),
        track_no: change
            .track_no
            .map(|number| u32::try_from(number).unwrap_or(0)),
    }
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
                    session.is_writable() && !es_db::rekordbox_running(),
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
        Ok(LibraryInfo {
            db_path: session.source().to_string_lossy().into_owned(),
            snapshot_time: session.snapshot_time(),
            rekordbox_running: es_db::rekordbox_running(),
            source_changed: session.source_changed(),
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

    pub fn playlist_selection(&self) -> Result<(Vec<String>, u64)> {
        let database = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?
            .database();
        let ids = database
            .playlists()?
            .into_iter()
            .map(|playlist| playlist.id)
            .collect();
        Ok((ids, u64::try_from(database.db_id()?).unwrap_or(0)))
    }

    pub fn inspect_tracks(&self, requests: &[InspectionRequest]) -> Result<Vec<TrackInspection>> {
        let database = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?
            .database();
        let mut inspected = Vec::new();
        for request in requests.iter().take(80) {
            let files = database
                .track_files(&request.id)?
                .ok_or_else(|| crate::Error::Other(format!("Unknown track {}", request.id)))?;
            if !crate::relocate::is_local(&files.folder_path) {
                inspected.push(TrackInspection {
                    id: request.id.clone(),
                    size: 0,
                    mtime: None,
                    same: false,
                    issues: Vec::new(),
                    audio: None,
                });
                continue;
            }
            let path = Path::new(&files.folder_path);
            if request.has_cache {
                if let Some((size, mtime)) = crate::inspect::same_fingerprint(
                    path,
                    request.cached_size,
                    request.cached_mtime.as_deref(),
                ) {
                    inspected.push(TrackInspection {
                        id: request.id.clone(),
                        size,
                        mtime,
                        same: true,
                        issues: Vec::new(),
                        audio: None,
                    });
                    continue;
                }
            }
            let inspection = crate::inspect::inspect_file(path);
            inspected.push(TrackInspection {
                id: request.id.clone(),
                size: inspection.size,
                mtime: inspection.mtime,
                same: false,
                issues: inspection.issues,
                audio: inspection.audio,
            });
        }
        Ok(inspected)
    }

    fn edit<T>(&mut self, change: impl FnOnce(&mut Tx<'_>) -> es_db::Result<T>) -> Result<T> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        session.with_live(|database| Ok(database.write(change)?))
    }

    pub fn playlist_add(&mut self, id: &str, track_ids: &[String]) -> Result<Vec<String>> {
        self.edit(|tx| tx.add_to_playlist(id, track_ids))?;
        self.playlist_tracks(id)
    }

    pub fn playlist_remove(&mut self, id: &str, track_ids: &[String]) -> Result<Vec<String>> {
        self.edit(|tx| tx.remove_from_playlist(id, track_ids))?;
        self.playlist_tracks(id)
    }

    pub fn playlist_move(
        &mut self,
        id: &str,
        track_ids: &[String],
        index: usize,
    ) -> Result<Vec<String>> {
        self.edit(|tx| tx.move_in_playlist(id, track_ids, index))?;
        self.playlist_tracks(id)
    }

    pub fn playlist_create(
        &mut self,
        name: &str,
        parent: Option<&str>,
        folder: bool,
        track_ids: &[String],
    ) -> Result<(String, Vec<es_index::Node>)> {
        let fallback = if folder { "New folder" } else { "New playlist" };
        let name = if name.trim().is_empty() {
            fallback
        } else {
            name.trim()
        };
        let parent = parent.filter(|parent| *parent != "root");
        let id = self.edit(|tx| tx.create_playlist(name, parent, folder, track_ids))?;
        Ok((id, self.playlists()?))
    }

    pub fn playlist_rename(&mut self, id: &str, name: &str) -> Result<Vec<es_index::Node>> {
        let name = name.trim();
        if name.is_empty() {
            return Err(crate::Error::Other("A playlist needs a name".into()));
        }
        self.edit(|tx| tx.rename_playlist(id, name))?;
        self.playlists()
    }

    pub fn playlist_delete(&mut self, id: &str) -> Result<Vec<es_index::Node>> {
        self.edit(|tx| tx.delete_playlist(id))?;
        self.playlists()
    }

    pub fn tag_add(
        &mut self,
        tag_id: &str,
        track_ids: &[String],
    ) -> Result<Vec<es_index::TrackRow>> {
        self.edit(|tx| tx.add_tag(tag_id, track_ids))?;
        self.tracks_by_id(track_ids)
    }

    pub fn tag_remove(
        &mut self,
        tag_id: &str,
        track_ids: &[String],
    ) -> Result<Vec<es_index::TrackRow>> {
        self.edit(|tx| tx.remove_tag(tag_id, track_ids))?;
        self.tracks_by_id(track_ids)
    }

    pub fn cue_set_hot(
        &mut self,
        id: &str,
        kind: i64,
        ms: i64,
    ) -> Result<Vec<es_index::detail::Cue>> {
        self.edit(|tx| tx.set_hot_cue(id, kind, ms))?;
        self.cues(id)
    }

    pub fn cue_add_memory(&mut self, id: &str, ms: i64) -> Result<Vec<es_index::detail::Cue>> {
        self.edit(|tx| tx.add_memory_cue(id, ms))?;
        self.cues(id)
    }

    pub fn cue_delete(&mut self, id: &str, cue_id: &str) -> Result<Vec<es_index::detail::Cue>> {
        self.edit(|tx| tx.delete_cue(id, cue_id))?;
        self.cues(id)
    }

    pub fn cue_set_color(
        &mut self,
        id: &str,
        cue_id: &str,
        color: Option<i64>,
    ) -> Result<Vec<es_index::detail::Cue>> {
        self.edit(|tx| tx.set_cue_color(id, cue_id, color))?;
        self.cues(id)
    }

    pub fn remove_tracks(&mut self, ids: &[String]) -> Result<RemovedTracks> {
        let removed = self.edit(|tx| tx.remove_tracks(ids))?;
        let share = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?
            .database()
            .share_dir();
        for item in &removed {
            if let Some(path) = &item.analysis_path {
                if let Err(error) = es_anlz::discard(&es_anlz::files(&share, path)) {
                    eprintln!(
                        "[es-engine] could not delete analysis files for {}: {error}",
                        item.id
                    );
                }
            }
        }
        let mut ids: Vec<String> = removed.into_iter().map(|item| item.id).collect();
        ids.sort_unstable();
        Ok(RemovedTracks {
            ids,
            playlists: self.playlists()?,
        })
    }

    pub fn track_update(
        &mut self,
        ids: &[String],
        change: &TrackEdit,
        write_tags: bool,
    ) -> Result<TrackUpdateResult> {
        let mut unique = ids.to_vec();
        unique.dedup();
        if change != &TrackEdit::default() && !unique.is_empty() {
            self.edit(|tx| tx.update_tracks(&unique, change))?;
        }

        let mut tag_errors = Vec::new();
        if write_tags && !unique.is_empty() {
            let file_edit = tag_edit(change);
            if file_edit != es_tags::Edit::default() {
                let mut sizes = Vec::new();
                for id in &unique {
                    let Some(files) = self
                        .session
                        .as_ref()
                        .ok_or_else(|| crate::Error::Other("Library not open".into()))?
                        .database()
                        .track_files(id)?
                    else {
                        continue;
                    };
                    let path = Path::new(&files.folder_path);
                    match es_tags::write(path, &file_edit) {
                        Ok(()) => {
                            if let Ok(metadata) = path.metadata() {
                                sizes
                                    .push((id.clone(), i64::try_from(metadata.len()).unwrap_or(0)));
                            }
                        }
                        Err(error) => {
                            let name = path.file_name().map_or_else(
                                || files.folder_path.clone(),
                                |name| name.to_string_lossy().into_owned(),
                            );
                            tag_errors.push(format!("{name}: {error}"));
                        }
                    }
                }
                if !sizes.is_empty() {
                    self.edit(|tx| {
                        sizes
                            .iter()
                            .try_for_each(|(id, size)| tx.set_file_size(id, *size))
                    })?;
                }
            }
        }

        Ok(TrackUpdateResult {
            tracks: self.tracks_by_id(&unique)?,
            tag_errors,
        })
    }

    pub fn track_import(
        &mut self,
        paths: &[String],
        playlist: Option<String>,
        artwork: bool,
    ) -> Result<TrackImportResult> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        let share = session.database().share_dir();
        let options = crate::import::Options {
            artwork,
            playlist: playlist.clone(),
        };
        let outcome = self
            .session
            .as_mut()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?
            .with_live(|database| crate::import::import_files(database, &share, paths, &options))?;
        let playlist = playlist.map(|id| self.playlist_tracks(&id)).transpose()?;
        let failed = outcome
            .failed
            .into_iter()
            .map(|failure| TrackImportFailure {
                path: failure.path,
                error: failure.error,
            })
            .collect();
        Ok(TrackImportResult {
            tracks: self.tracks_by_id(&outcome.added)?,
            ids: outcome.ids,
            skipped: outcome.skipped,
            failed,
            playlist,
        })
    }

    pub fn track_relocate(
        &mut self,
        moves: &[(String, String)],
    ) -> Result<Vec<es_index::TrackRow>> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        let share = session.database().share_dir();
        let ids =
            session.with_live(|database| crate::relocate::relocate(database, &share, moves))?;
        self.tracks_by_id(&ids)
    }

    pub fn beatgrid_set(
        &mut self,
        id: &str,
        grid: &crate::analysis::Grid,
        cues: &[es_db::CueMove],
    ) -> Result<BeatgridResult> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        let share = session.database().share_dir();
        let bpm = session.with_live(|database| {
            crate::analysis::set_beatgrid(database, &share, id, grid, cues)
        })?;
        let reads = self.read_model()?;
        Ok(BeatgridResult {
            beatgrid: reads.beatgrid(id)?,
            cues: reads.cues(id)?,
            bpm,
        })
    }

    pub fn track_apply_analysis(
        &mut self,
        id: &str,
        analysis: &crate::analysis::Analysis,
        options: crate::analysis::Options,
    ) -> Result<Option<es_index::TrackRow>> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        let share = session.database().share_dir();
        session.with_live(|database| {
            crate::analysis::apply_analysis(database, &share, id, analysis, options)
        })?;
        Ok(self.tracks_by_id(&[id.to_owned()])?.into_iter().next())
    }

    pub fn track_convert(
        &mut self,
        ids: &[String],
        preset: &crate::convert::Preset,
        mode: crate::convert::Mode,
        emit: &impl Fn(ServiceEvent),
    ) -> Result<TrackConvertResult> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        let share = session.database().share_dir();
        let outcome = session.with_live(|database| {
            Ok(crate::convert::convert_tracks(
                database,
                &share,
                ids,
                preset,
                mode,
                &|done, total, id| {
                    emit(ServiceEvent::ConvertProgress {
                        done,
                        total,
                        id: id.map(str::to_owned),
                    });
                },
            ))
        })?;
        Ok(TrackConvertResult {
            tracks: self.tracks_by_id(&outcome.ids)?,
            ids: outcome.ids,
            originals: outcome.originals,
            failed: outcome
                .failed
                .into_iter()
                .map(|(id, error)| TrackConvertFailure { id, error })
                .collect(),
        })
    }

    pub fn device_import(
        &mut self,
        volume: &Path,
        device: &es_device::Device,
        options: &crate::device_import::Options,
        emit: &impl Fn(ServiceEvent),
    ) -> Result<crate::device_import::Outcome> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?;
        let share = session.database().share_dir();
        session.with_live(|database| {
            crate::device_import::import_device(
                database,
                &share,
                volume,
                device,
                options,
                &|progress| {
                    emit(ServiceEvent::UsbImportProgress {
                        done: progress.done,
                        total: progress.total,
                        name: progress.name,
                    });
                },
            )
        })
    }

    pub fn usb_sync(
        &self,
        volume: &Path,
        playlist_ids: &[String],
        overwrite: bool,
        review: Option<&Value>,
        emit: &impl Fn(ServiceEvent),
    ) -> Result<crate::sync::Outcome> {
        let database = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Other("Library not open".into()))?
            .database();
        crate::sync::sync_volume_reviewed(database, volume, playlist_ids, overwrite, review, &|progress| {
            emit(ServiceEvent::UsbSyncProgress(progress));
        })
    }

    pub fn usb_sync_review(&self, volume: &Path, playlist_ids: &[String]) -> Result<Value> {
        let database = self.session.as_ref().ok_or_else(|| crate::Error::Other("Library not open".into()))?.database();
        crate::sync::review_volume(database, volume, playlist_ids)
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
        let profile = std::env::var_os("EXPORTSTUDIO_PROFILE_LIBRARY_OPEN").is_some();
        let settings_started = SystemTime::now();
        let settings = self.backups().settings();
        let interval = settings["interval"].as_i64().unwrap_or(12) as f64 * 3600.0;
        let settings_ms = settings_started.elapsed().map_or(0.0, |duration| duration.as_secs_f64() * 1000.0);
        let lookup_started = SystemTime::now();
        let created = self.backups().latest_automatic_created();
        let lookup_ms = lookup_started.elapsed().map_or(0.0, |duration| duration.as_secs_f64() * 1000.0);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |duration| duration.as_secs_f64());
        let due = match created {
            None => true,
            Some(created) => now - created >= interval,
        };
        if profile {
            eprintln!(
                "[library-profile] backup.auto_due settings_ms={settings_ms:.1} latest_lookup_ms={lookup_ms:.1} interval_hours={:.1} latest_age_hours={:.2} due={due}",
                interval / 3600.0,
                created.map_or(-1.0, |created| (now - created) / 3600.0),
            );
        }
        due
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
            .set_writable(writable)
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
        let live = if writable {
            Some(Database::open_live(&source, Guard::RefuseWhileRunning)?)
        } else {
            None
        };
        Ok(Self {
            database,
            live,
            source,
            writable,
            backed_up,
            guard: Guard::RefuseWhileRunning,
            snapshot_refresh_pending: false,
        })
    }

    #[must_use]
    pub fn database(&self) -> &Database {
        self.live.as_ref().unwrap_or(&self.database)
    }

    #[must_use]
    pub fn snapshot_time(&self) -> f64 {
        self.database.snapshot_time()
    }

    #[must_use]
    pub fn source_changed(&self) -> bool {
        !self.writable && (self.snapshot_refresh_pending || self.database.source_changed())
    }

    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.writable
    }

    pub fn set_writable(&mut self, writable: bool) -> Result<()> {
        if self.writable == writable && (writable || !self.snapshot_refresh_pending) {
            return Ok(());
        }
        if writable {
            if self.snapshot_refresh_pending {
                self.database = Database::open_snapshot(&self.source)?;
                self.snapshot_refresh_pending = false;
            }
            let live = Database::open_live(&self.source, self.guard)?;
            self.live = Some(live);
            self.writable = true;
        } else {
            self.live = None;
            self.writable = false;
            self.snapshot_refresh_pending = true;
            self.database = Database::open_snapshot(&self.source)?;
            self.snapshot_refresh_pending = false;
        }
        Ok(())
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
        if self.guard == Guard::RefuseWhileRunning && es_db::rekordbox_running() {
            return Err(es_db::Error::RekordboxRunning.into());
        }
        let live = self
            .live
            .as_mut()
            .ok_or_else(|| crate::Error::Other("The writable library connection is unavailable".into()))?;
        work(live)
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

    fn writable_fixture_service(directory: &Path, path: &Path) -> LibraryService {
        let mut service = LibraryService::with_backup_root(directory.join("backups"));
        service.session = Some(LibrarySession {
            database: Database::open_snapshot(path).unwrap(),
            live: Some(Database::open_live(path, Guard::Off).unwrap()),
            source: path.to_path_buf(),
            writable: true,
            backed_up: true,
            guard: Guard::Off,
            snapshot_refresh_pending: false,
        });
        service
    }

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

    #[test]
    #[ignore = "profiles startup reads from the installed library without writing to it"]
    fn profile_installed_library_open_and_initial_reads() {
        let source = Database::installed_path().expect("no home directory");
        let directory = tempfile::tempdir().unwrap();
        let mut service = LibraryService::with_backup_root(directory.path().join("backups"));
        let total = std::time::Instant::now();

        let stage = std::time::Instant::now();
        service.open(source, false, false).unwrap();
        eprintln!("[library-profile] service.open_ms={:.1}", stage.elapsed().as_secs_f64() * 1000.0);

        let stage = std::time::Instant::now();
        let tracks = service.tracks().unwrap();
        eprintln!("[library-profile] tracks_ms={:.1} rows={}", stage.elapsed().as_secs_f64() * 1000.0, tracks.len());

        let stage = std::time::Instant::now();
        let playlists = service.playlists().unwrap();
        eprintln!("[library-profile] playlists_ms={:.1} rows={}", stage.elapsed().as_secs_f64() * 1000.0, playlists.len());

        let stage = std::time::Instant::now();
        let histories = service.histories().unwrap();
        eprintln!("[library-profile] histories_ms={:.1} rows={}", stage.elapsed().as_secs_f64() * 1000.0, histories.len());

        let stage = std::time::Instant::now();
        let tags = service.my_tags().unwrap();
        eprintln!("[library-profile] my_tags_ms={:.1} rows={}", stage.elapsed().as_secs_f64() * 1000.0, tags.len());

        let stage = std::time::Instant::now();
        let members = service.playlist_members().unwrap();
        eprintln!("[library-profile] playlist_members_ms={:.1} rows={}", stage.elapsed().as_secs_f64() * 1000.0, members.len());

        let stage = std::time::Instant::now();
        let volumes = es_device::volumes::list(None);
        eprintln!("[library-profile] usb_list_ms={:.1} volumes={}", stage.elapsed().as_secs_f64() * 1000.0, volumes.len());

        let stage = std::time::Instant::now();
        let backup_created = service.daily_backup(&|_| {}).unwrap();
        eprintln!(
            "[library-profile] daily_backup_ms={:.1} due={backup_created}",
            stage.elapsed().as_secs_f64() * 1000.0
        );
        eprintln!("[library-profile] open_and_hydration_total_ms={:.1}", total.elapsed().as_secs_f64() * 1000.0);
    }

    #[test]
    fn playlist_service_write_commits_and_refreshes_its_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        {
            let connection = fixture::connect(&path).unwrap();
            Rows(&connection).track("track-1", "Test track").unwrap();
            Rows(&connection)
                .playlist("playlist-1", "Test playlist", 0)
                .unwrap();
        }
        let mut service = writable_fixture_service(directory.path(), &path);

        let ids = service
            .playlist_add("playlist-1", &["track-1".into()])
            .unwrap();

        assert_eq!(ids, ["track-1"]);
        assert_eq!(service.playlist_tracks("playlist-1").unwrap(), ["track-1"]);
    }

    #[test]
    fn track_update_service_returns_the_refreshed_row() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        {
            let connection = fixture::connect(&path).unwrap();
            Rows(&connection).track("track-1", "Original").unwrap();
        }
        let mut service = writable_fixture_service(directory.path(), &path);
        let change = TrackEdit {
            title: Some("Updated".into()),
            ..TrackEdit::default()
        };

        let result = service
            .track_update(&["track-1".into()], &change, false)
            .unwrap();

        assert_eq!(result.tracks[0].title, "Updated");
        assert!(result.tag_errors.is_empty());
    }

    #[test]
    fn memory_cue_write_uses_live_session_until_relocked() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        {
            let connection = fixture::connect(&path).unwrap();
            let journal_mode: String = connection
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .unwrap();
            assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
            Rows(&connection).track("track-1", "Test track").unwrap();
        }
        let mut service = writable_fixture_service(directory.path(), &path);
        let snapshot_time = service.session().unwrap().snapshot_time();

        let cues = service.cue_add_memory("track-1", 500).unwrap();

        assert_eq!(cues.len(), 1);
        assert_eq!(service.session().unwrap().snapshot_time(), snapshot_time);
        assert!(!service.session().unwrap().source_changed());

        service.set_writable(false, &|_| {}).unwrap();
        assert_eq!(service.cues("track-1").unwrap().len(), 1);
        assert!(!service.session().unwrap().source_changed());
    }

    #[test]
    fn failed_relock_snapshot_refresh_can_be_retried() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture::build(directory.path(), 1).unwrap();
        {
            let connection = fixture::connect(&path).unwrap();
            Rows(&connection).track("track-1", "Test track").unwrap();
        }
        let mut service = writable_fixture_service(directory.path(), &path);
        let missing_path = directory.path().join("temporarily-missing.db");
        service.session.as_mut().unwrap().source = missing_path;

        assert!(service
            .session_mut()
            .unwrap()
            .set_writable(false)
            .is_err());
        assert!(service.session().unwrap().source_changed());
        assert!(!service.session().unwrap().is_writable());

        service.session.as_mut().unwrap().source = path;
        service.session_mut().unwrap().set_writable(false).unwrap();
        assert!(!service.session().unwrap().source_changed());
        assert_eq!(service.tracks().unwrap().len(), 1);
    }
}
