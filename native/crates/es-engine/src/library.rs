//! Typed read operations exposed by the application layer.
//!
//! The N-API adapter serializes these results for the existing Electron contract. Rust callers
//! work with domain and read-model types rather than JSON text.

use std::path::{Path, PathBuf};

use es_db::{Guard, Library as Database};

use crate::Result;

/// One selected library and its current read snapshot.
pub struct LibrarySession {
    database: Database,
    source: PathBuf,
    writable: bool,
    backed_up: bool,
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

        let session = LibrarySession::open(path, false, false).unwrap();
        let reads = LibraryRead::new(session.database());
        let tracks = reads.tracks().unwrap();
        let playlists = reads.playlists().unwrap();

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Test track");
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "Test playlist");
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
}
