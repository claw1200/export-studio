//! Access to rekordbox's `master.db`.
//!
//! Reads go through a snapshot copy so rekordbox can keep running. Writes need a live handle,
//! are refused while rekordbox runs, and all live in [`write`].

mod key;
pub mod export;
pub mod fixture;
pub mod live;
pub mod read;
pub mod write;
mod xml;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use rusqlite::Connection;
use tempfile::TempDir;

pub use read::{Cue, HistoryNode, PlaylistKind, PlaylistNode, TagNode, Track, TrackFiles};
pub use write::{AnalysisWrite, CueMove, DeviceData, FileFacts, NewCue, NewTrack, Removed, Stream, TrackEdit, Tx};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not derive the library key: {0}")]
    Key(String),
    #[error("Close rekordbox before editing the library")]
    RekordboxRunning,
    #[error("this library is open read-only")]
    ReadOnly,
    #[error("refused: {0}")]
    Refused(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("not supported: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Snapshot,
    Live,
}

/// Whether a live handle may write while rekordbox is running. `Off` is for fixtures only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    RefuseWhileRunning,
    Off,
}

pub struct Library {
    conn: Connection,
    path: PathBuf,
    mode: Mode,
    guard: Guard,
    snapshot: Option<Snapshot>,
}

/// What a read-only copy was taken from, so a later change to the source can be noticed.
struct Snapshot {
    _dir: TempDir,
    source: PathBuf,
    signature: Vec<Option<(SystemTime, u64)>>,
    taken: SystemTime,
}

fn signature(source: &Path) -> Vec<Option<(SystemTime, u64)>> {
    let wal = {
        let mut name = source.file_name().unwrap_or_default().to_os_string();
        name.push("-wal");
        source.with_file_name(name)
    };
    [source, wal.as_path()]
        .iter()
        .map(|p| std::fs::metadata(p).ok().and_then(|m| Some((m.modified().ok()?, m.len()))))
        .collect()
}

impl Library {
    /// Where rekordbox keeps `master.db` for this user.
    #[must_use]
    pub fn installed_path() -> Option<PathBuf> {
        #[cfg(windows)]
        let base = std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Pioneer").join("rekordbox"));
        #[cfg(not(windows))]
        let base = std::env::var_os("HOME")
            .map(|dir| PathBuf::from(dir).join("Library").join("Pioneer").join("rekordbox"));
        base.map(|dir| dir.join("master.db"))
    }

    /// A private copy of the database, so reads never contend with rekordbox.
    pub fn open_snapshot(source: &Path) -> Result<Self> {
        let dir = tempfile::Builder::new().prefix("exportstudio-db-").tempdir()?;
        let name = source.file_name().ok_or_else(|| Error::Refused("not a database path".into()))?;
        for suffix in ["", "-wal", "-shm"] {
            let mut file = name.to_os_string();
            file.push(suffix);
            let from = source.with_file_name(&file);
            if from.exists() {
                std::fs::copy(&from, dir.path().join(&file))?;
            }
        }
        let path = dir.path().join(name);
        let conn = unlock(&path)?;
        conn.pragma_update(None, "query_only", true)?;
        let snapshot = Snapshot { _dir: dir, source: source.to_path_buf(), signature: signature(source), taken: SystemTime::now() };
        Ok(Self { conn, path, mode: Mode::Snapshot, guard: Guard::RefuseWhileRunning, snapshot: Some(snapshot) })
    }

    pub fn open_live(path: &Path, guard: Guard) -> Result<Self> {
        if guard == Guard::RefuseWhileRunning && rekordbox_running() {
            return Err(Error::RekordboxRunning);
        }
        let conn = unlock(path)?;
        Ok(Self { conn, path: path.to_path_buf(), mode: Mode::Live, guard, snapshot: None })
    }

    /// When the snapshot was taken, as seconds since the Unix epoch; 0 for a live handle.
    #[must_use]
    pub fn snapshot_time(&self) -> f64 {
        self.snapshot
            .as_ref()
            .and_then(|s| s.taken.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0.0, |d| d.as_secs_f64())
    }

    /// Whether the library the snapshot was copied from has changed since.
    #[must_use]
    pub fn source_changed(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| signature(&s.source) != s.signature)
    }

    /// The folder holding analysis files and artwork.
    #[must_use]
    pub fn share_dir(&self) -> PathBuf {
        self.snapshot.as_ref().map_or_else(|| self.path.clone(), |s| s.source.clone()).with_file_name("share")
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Run `edit` in one transaction, stamped the way rekordbox stamps its own writes.
    pub fn write<T>(&mut self, edit: impl FnOnce(&mut Tx<'_>) -> Result<T>) -> Result<T> {
        if self.mode != Mode::Live {
            return Err(Error::ReadOnly);
        }
        if self.guard == Guard::RefuseWhileRunning {
            if rekordbox_running() {
                return Err(Error::RekordboxRunning);
            }
            if std::env::var_os("ES_DB_TEST").is_some() && Some(self.path.clone()) == Self::installed_path() {
                return Err(Error::Refused("ES_DB_TEST is set; the installed library is never written in tests".into()));
            }
        }
        write::run(&mut self.conn, &self.path.with_file_name("masterPlaylists6.xml"), edit)
    }
}

/// What a database file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub tracks: i64,
    pub playlists: i64,
}

/// Open a database file, check it is intact, and count what is in it.
pub fn verify(path: &Path) -> Result<Counts> {
    let conn = unlock(path)?;
    let rows: Vec<String> = conn
        .prepare("PRAGMA quick_check")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if rows != ["ok"] {
        return Err(Error::Refused(format!("The database is damaged: {}", rows.first().map_or("no result", String::as_str))));
    }
    let count = |sql: &str| conn.query_row(sql, [], |row| row.get::<_, i64>(0));
    Ok(Counts {
        tracks: count("SELECT count(*) FROM djmdContent WHERE rb_local_deleted = 0")?,
        playlists: count("SELECT count(*) FROM djmdPlaylist")?,
    })
}

fn unlock(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "key", key::master_key()?)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))?;
    Ok(conn)
}

#[must_use]
pub fn rekordbox_running() -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq rekordbox.exe"])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("rekordbox.exe"))
    }
    #[cfg(not(windows))]
    {
        Command::new("pgrep").args(["-x", "rekordbox"]).output().is_ok_and(|out| out.status.success())
    }
}
