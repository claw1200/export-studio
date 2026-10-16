//! Every change the app makes to `master.db`. Each edit runs in one transaction and is stamped the
//! way rekordbox stamps its own: a new local update count, written to each touched row.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::types::Value;
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::live::HIDDEN_PLAYLIST;
use crate::xml::{self, XmlOp};
use crate::{Error, Result};

const NOW: &str = "strftime('%Y-%m-%d %H:%M:%f +00:00','now')";
const TODAY: &str = "strftime('%Y-%m-%d','now','localtime')";

/// Hot cue slots; 4 is a loop, not a slot.
const HOT_SLOTS: [i64; 8] = [1, 2, 3, 5, 6, 7, 8, 9];
const CUE_PALETTE_SIZE: i64 = 62;
const CUE_FRAMES_PER_SECOND: i64 = 150;

fn clamp_msec(msec: i64, length_seconds: i64) -> i64 {
    let msec = msec.max(0);
    if length_seconds > 0 { msec.min(length_seconds * 1000) } else { msec }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnRemove {
    /// Rows are deleted with the track.
    Delete,
    /// Rows stay, as rekordbox leaves them.
    Keep,
}

/// Every table that points at a track by `ContentID`, and what removing the track does to it.
/// Adding a table here is the only way to teach remove and restore about it; a test fails for any
/// table with a `ContentID` column that is missing.
pub const DEPENDENTS: &[(&str, OnRemove)] = &[
    ("djmdSongPlaylist", OnRemove::Delete),
    ("djmdSongMyTag", OnRemove::Delete),
    ("djmdSongHotCueBanklist", OnRemove::Delete),
    ("djmdSongRelatedTracks", OnRemove::Delete),
    ("djmdSongSampler", OnRemove::Delete),
    ("djmdActiveCensor", OnRemove::Delete),
    ("contentActiveCensor", OnRemove::Delete),
    ("djmdSongTagList", OnRemove::Delete),
    ("djmdSongRequestList", OnRemove::Delete),
    ("djmdCloudExportSongPlaylist", OnRemove::Delete),
    // rekordbox keeps cues; we delete them so a re-import does not bring an old grid's cues back.
    ("djmdCue", OnRemove::Delete),
    ("contentCue", OnRemove::Delete),
    ("djmdSongHistory", OnRemove::Keep),
    ("djmdMixerParam", OnRemove::Keep),
    ("contentFile", OnRemove::Keep),
];

/// Fields of a track that can be edited. `None` leaves a field alone; for text, empty clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackEdit {
    pub title: Option<String>,
    pub comment: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub rating: Option<i64>,
    /// 0 clears the colour.
    pub color: Option<i64>,
    pub artist: Option<String>,
    pub remixer: Option<String>,
    pub original_artist: Option<String>,
    pub composer: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
}

/// Stream facts read from an audio file; zero or missing values are left unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stream {
    pub length: Option<i64>,
    pub bit_rate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
}

/// What a file looks like on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub name: String,
    pub size: i64,
    /// rekordbox's file type code: 1 MP3, 4 M4A, 5 FLAC, 11 WAV, 12 AIFF.
    pub file_type: i64,
}

/// A track to add to the collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTrack {
    pub uuid: String,
    /// The stored path, with forward slashes.
    pub path: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_type: i64,
    pub stream: Stream,
    /// Tags to set after the row exists.
    pub edit: TrackEdit,
    pub image_path: Option<String>,
}

/// A cue to place on a track.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewCue {
    /// 0 is a memory cue, 1 and up are hot cues and loops.
    pub kind: i64,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub color_index: Option<i64>,
    pub comment: String,
    pub active_loop: bool,
}

/// What a track had on a device that a library copy of it should get.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceData {
    /// Where the track's analysis files now are, relative to the share folder, when they were copied.
    pub analysis_path: Option<String>,
    pub bpm_x100: i64,
    /// A key name as the library writes it; ignored when the library has no such key.
    pub key: String,
    pub rating: i64,
    pub color: i64,
    pub cues: Vec<NewCue>,
}

/// The `Analysed` value rekordbox writes once a track has been analysed.
const ANALYSED: i64 = 105;

/// Artist-like columns share the artist table.
const NAMED: [(&str, &str); 7] = [
    ("ArtistID", "djmdArtist"),
    ("RemixerID", "djmdArtist"),
    ("OrgArtistID", "djmdArtist"),
    ("ComposerID", "djmdArtist"),
    ("AlbumID", "djmdAlbum"),
    ("GenreID", "djmdGenre"),
    ("LabelID", "djmdLabel"),
];

pub struct Tx<'a> {
    tx: Transaction<'a>,
    usn: i64,
    touched: bool,
    xml: Vec<XmlOp>,
}

/// A track taken out of the collection, with the analysis files it had so the caller can delete them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub id: String,
    pub analysis_path: Option<String>,
}

pub(crate) fn run<T>(conn: &mut Connection, xml_path: &Path, edit: impl FnOnce(&mut Tx<'_>) -> Result<T>) -> Result<T> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let counter: i64 = tx.query_row(
        "SELECT COALESCE(int_1, 0) FROM agentRegistry WHERE registry_id = 'localUpdateCount'",
        [],
        |row| row.get(0),
    )?;
    let mut scope = Tx { tx, usn: counter + 1, touched: false, xml: Vec::new() };
    let out = edit(&mut scope)?;
    if scope.touched {
        scope.tx.execute(
            &format!("UPDATE agentRegistry SET int_1 = ?1, updated_at = {NOW} WHERE registry_id = 'localUpdateCount'"),
            [scope.usn],
        )?;
    }
    let xml_ops = std::mem::take(&mut scope.xml);
    scope.tx.commit()?;
    if !xml_ops.is_empty() && xml_path.is_file() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let text = std::fs::read_to_string(xml_path)?;
        let updated = xml::apply(&text, &xml_ops, now);
        let tmp = xml_path.with_extension("xml.tmp");
        std::fs::write(&tmp, updated)?;
        std::fs::rename(&tmp, xml_path)?;
    }
    Ok(out)
}

fn unused_id(tx: &Transaction<'_>, table: &str) -> Result<String> {
    unused_value(tx, table, "ID")
}

/// A random 28-bit number no row of `table` holds in `column`.
fn unused_value(tx: &Transaction<'_>, table: &str, column: &str) -> Result<String> {
    for _ in 0..1000 {
        let id = i64::try_from(Uuid::new_v4().as_u128() >> 100).unwrap_or(0);
        if id < 100 {
            continue;
        }
        let used: i64 =
            tx.query_row(&format!("SELECT count(*) FROM {table} WHERE {column} = ?1"), [id.to_string()], |r| r.get(0))?;
        if used == 0 {
            return Ok(id.to_string());
        }
    }
    Err(Error::Refused("could not find an unused id".into()))
}

impl Tx<'_> {
    fn stamp(&mut self, table: &str, id: &str) -> Result<()> {
        self.touched = true;
        if table == "djmdPlaylist" {
            self.xml.push(XmlOp::Touch(id.to_owned()));
        }
        self.tx.execute(
            &format!("UPDATE {table} SET rb_local_usn = ?1, updated_at = {NOW} WHERE ID = ?2"),
            params![self.usn, id],
        )?;
        Ok(())
    }

    fn content_state(&self, id: &str) -> Result<Option<bool>> {
        let row = self
            .tx
            .query_row("SELECT rb_local_deleted FROM djmdContent WHERE ID = ?1", [id], |row| {
                row.get::<_, Option<i64>>(0)
            });
        match row {
            Ok(flag) => Ok(Some(flag.unwrap_or(0) != 0)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete the rows that hang off a track. Returns the playlists that lost an entry.
    fn clear_links(&mut self, content_id: &str) -> Result<BTreeSet<String>> {
        let mut playlists = BTreeSet::new();
        for (table, action) in DEPENDENTS {
            if *action != OnRemove::Delete {
                continue;
            }
            if *table == "djmdSongPlaylist" {
                let mut stmt = self.tx.prepare("SELECT DISTINCT PlaylistID FROM djmdSongPlaylist WHERE ContentID = ?1")?;
                for row in stmt.query_map([content_id], |row| row.get::<_, String>(0))? {
                    playlists.insert(row?);
                }
            }
            self.tx.execute(&format!("DELETE FROM {table} WHERE ContentID = ?1"), [content_id])?;
            self.touched = true;
        }
        Ok(playlists)
    }

    fn renumber(&mut self, playlist_id: &str) -> Result<()> {
        let rows: Vec<(String, Option<i64>)> = {
            let mut stmt = self.tx.prepare(
                "SELECT ID, TrackNo FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND rb_local_deleted = 0 \
                 ORDER BY TrackNo, ID",
            )?;
            let rows = stmt.query_map([playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (index, (id, number)) in rows.iter().enumerate() {
            let wanted = i64::try_from(index + 1).unwrap_or(i64::MAX);
            if *number != Some(wanted) {
                self.tx.execute("UPDATE djmdSongPlaylist SET TrackNo = ?1 WHERE ID = ?2", params![wanted, id])?;
                self.stamp("djmdSongPlaylist", id)?;
            }
        }
        Ok(())
    }

    /// Remove tracks from the collection. Unknown or already removed ids are ignored; the rest are returned.
    /// The analysis columns are cleared so a later import reads the file afresh.
    pub fn remove_tracks(&mut self, ids: &[String]) -> Result<Vec<Removed>> {
        let mut playlists = BTreeSet::new();
        let mut removed = Vec::new();
        for id in ids {
            if self.content_state(id)? != Some(false) {
                continue;
            }
            let analysis_path: Option<String> = self
                .tx
                .query_row("SELECT AnalysisDataPath FROM djmdContent WHERE ID = ?1", [id], |row| row.get(0))?;
            playlists.extend(self.clear_links(id)?);
            self.tx.execute(
                "UPDATE djmdContent SET rb_local_deleted = 1, AnalysisDataPath = NULL, Analysed = 0, FileType = NULL, \
                 BitRate = NULL, SampleRate = NULL, BitDepth = NULL WHERE ID = ?1",
                [id],
            )?;
            self.stamp("djmdContent", id)?;
            removed.push(Removed { id: id.clone(), analysis_path: analysis_path.filter(|p| !p.is_empty()) });
        }
        for playlist in &playlists {
            self.renumber(playlist)?;
            self.stamp("djmdPlaylist", playlist)?;
        }
        Ok(removed)
    }

    /// Bring a removed track back with no links, whatever an earlier tool left behind.
    pub fn restore_track(&mut self, id: &str) -> Result<()> {
        match self.content_state(id)? {
            None => return Err(Error::NotFound(format!("track {id}"))),
            Some(false) => return Err(Error::Refused(format!("track {id} is not removed"))),
            Some(true) => {}
        }
        let playlists = self.clear_links(id)?;
        self.tx.execute("UPDATE djmdContent SET rb_local_deleted = 0 WHERE ID = ?1", [id])?;
        self.stamp("djmdContent", id)?;
        for playlist in &playlists {
            self.renumber(playlist)?;
            self.stamp("djmdPlaylist", playlist)?;
        }
        Ok(())
    }

    fn require_normal_playlist(&self, playlist_id: &str) -> Result<()> {
        let attribute = self.tx.query_row(
            "SELECT Attribute FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
            [playlist_id],
            |row| row.get::<_, Option<i64>>(0),
        );
        match attribute {
            Ok(None | Some(0)) => Ok(()),
            Ok(Some(_)) => Err(Error::Refused("tracks can only be added to or removed from a normal playlist".into())),
            Err(rusqlite::Error::QueryReturnedNoRows) => Err(Error::NotFound(format!("playlist {playlist_id}"))),
            Err(e) => Err(e.into()),
        }
    }

    /// Append tracks a playlist does not already hold. Returns how many were added.
    pub fn add_to_playlist(&mut self, playlist_id: &str, content_ids: &[String]) -> Result<usize> {
        self.require_normal_playlist(playlist_id)?;
        let mut next: i64 = self.tx.query_row(
            "SELECT COALESCE(MAX(TrackNo), 0) FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND rb_local_deleted = 0",
            [playlist_id],
            |row| row.get(0),
        )?;
        let mut added = 0;
        for id in content_ids {
            if self.content_state(id)? != Some(false) {
                return Err(Error::NotFound(format!("live track {id}")));
            }
            let present: i64 = self.tx.query_row(
                "SELECT count(*) FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND ContentID = ?2 AND rb_local_deleted = 0",
                params![playlist_id, id],
                |row| row.get(0),
            )?;
            if present > 0 {
                continue;
            }
            next += 1;
            self.tx.execute(
                &format!(
                    "INSERT INTO djmdSongPlaylist (ID, PlaylistID, ContentID, TrackNo, UUID, rb_data_status, \
                     rb_local_data_status, rb_local_deleted, rb_local_synced, usn, rb_local_usn, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, 0, NULL, ?6, {NOW}, {NOW})"
                ),
                params![Uuid::new_v4().to_string(), playlist_id, id, next, Uuid::new_v4().to_string(), self.usn],
            )?;
            self.touched = true;
            added += 1;
        }
        if added > 0 {
            self.stamp("djmdPlaylist", playlist_id)?;
        }
        Ok(added)
    }

    pub fn remove_from_playlist(&mut self, playlist_id: &str, content_ids: &[String]) -> Result<usize> {
        self.require_normal_playlist(playlist_id)?;
        let mut removed = 0;
        for id in content_ids {
            removed += self.tx.execute(
                "DELETE FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND ContentID = ?2",
                params![playlist_id, id],
            )?;
        }
        if removed > 0 {
            self.touched = true;
            self.renumber(playlist_id)?;
            self.stamp("djmdPlaylist", playlist_id)?;
        }
        Ok(removed)
    }

    /// Reorder so the moved tracks sit together before the track now at `index`.
    pub fn move_in_playlist(&mut self, playlist_id: &str, content_ids: &[String], index: usize) -> Result<()> {
        self.require_normal_playlist(playlist_id)?;
        let songs: Vec<(String, String)> = {
            let mut stmt = self.tx.prepare(
                "SELECT ID, ContentID FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND rb_local_deleted = 0 \
                 ORDER BY TrackNo, ID",
            )?;
            let rows = stmt.query_map([playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let moving: BTreeSet<&String> = content_ids.iter().collect();
        let index = index.min(songs.len());
        let before = songs[..index].iter().filter(|(_, c)| !moving.contains(c)).count();
        let (picked, rest): (Vec<_>, Vec<_>) = songs.iter().cloned().partition(|(_, c)| moving.contains(c));
        let order: Vec<_> = rest[..before].iter().chain(&picked).chain(&rest[before..]).cloned().collect();
        if order == songs {
            return Ok(());
        }
        for (number, (song_id, _)) in order.iter().enumerate() {
            let wanted = i64::try_from(number + 1).unwrap_or(i64::MAX);
            let was = songs.iter().position(|(s, _)| s == song_id).map_or(0, |p| p + 1);
            if was != number + 1 {
                self.tx.execute("UPDATE djmdSongPlaylist SET TrackNo = ?1 WHERE ID = ?2", params![wanted, song_id])?;
                self.stamp("djmdSongPlaylist", song_id)?;
            }
        }
        self.stamp("djmdPlaylist", playlist_id)
    }

    /// A new playlist or folder at the end of `parent` (the top level when `None`). Returns its id.
    pub fn create_playlist(
        &mut self,
        name: &str,
        parent: Option<&str>,
        folder: bool,
        content_ids: &[String],
    ) -> Result<String> {
        let parent_id = parent.filter(|p| !p.is_empty()).unwrap_or("root");
        if parent_id != "root" {
            let attribute = self.tx.query_row(
                "SELECT Attribute FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
                [parent_id],
                |row| row.get::<_, Option<i64>>(0),
            );
            if !matches!(attribute, Ok(Some(1))) {
                return Err(Error::Refused("the parent does not exist or is not a folder".into()));
            }
        }
        let siblings: i64 = self.tx.query_row(
            "SELECT count(*) FROM djmdPlaylist WHERE ParentID = ?1 AND rb_local_deleted = 0",
            [parent_id],
            |row| row.get(0),
        )?;
        let id = unused_id(&self.tx, "djmdPlaylist")?;
        let attribute = i64::from(folder);
        self.tx.execute(
            &format!(
                "INSERT INTO djmdPlaylist (ID, Seq, Name, Attribute, ParentID, UUID, rb_data_status, rb_local_data_status, \
                 rb_local_deleted, rb_local_synced, usn, rb_local_usn, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, 0, 0, NULL, ?7, {NOW}, {NOW})"
            ),
            params![id, siblings + 1, name, attribute, parent_id, Uuid::new_v4().to_string(), self.usn],
        )?;
        self.touched = true;
        self.xml.push(XmlOp::Add { id: id.clone(), parent: parent_id.to_owned(), attribute });
        if !folder && !content_ids.is_empty() {
            self.add_to_playlist(&id, content_ids)?;
        }
        Ok(id)
    }

    pub fn rename_playlist(&mut self, playlist_id: &str, name: &str) -> Result<()> {
        if playlist_id == HIDDEN_PLAYLIST {
            return Err(Error::NotFound(format!("playlist {playlist_id}")));
        }
        let current = self.tx.query_row(
            "SELECT Name FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
            [playlist_id],
            |row| row.get::<_, Option<String>>(0),
        );
        match current {
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::NotFound(format!("playlist {playlist_id}"))),
            Err(e) => return Err(e.into()),
            Ok(current) if current.as_deref() == Some(name) => return Ok(()),
            Ok(_) => {}
        }
        self.tx.execute("UPDATE djmdPlaylist SET Name = ?1 WHERE ID = ?2", params![name, playlist_id])?;
        self.stamp("djmdPlaylist", playlist_id)
    }

    /// Delete a playlist, or a folder with everything inside it, and close the gap among its siblings.
    pub fn delete_playlist(&mut self, playlist_id: &str) -> Result<()> {
        if playlist_id == HIDDEN_PLAYLIST {
            return Err(Error::NotFound(format!("playlist {playlist_id}")));
        }
        let (seq, parent): (Option<i64>, Option<String>) = match self.tx.query_row(
            "SELECT Seq, ParentID FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
            [playlist_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::NotFound(format!("playlist {playlist_id}"))),
            Err(e) => return Err(e.into()),
        };
        let mut doomed = vec![playlist_id.to_owned()];
        let mut at = 0;
        while at < doomed.len() {
            let mut stmt = self.tx.prepare("SELECT ID FROM djmdPlaylist WHERE ParentID = ?1")?;
            let children: Vec<String> = stmt.query_map([&doomed[at]], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
            doomed.extend(children);
            at += 1;
        }
        for id in &doomed {
            self.tx.execute("DELETE FROM djmdSongPlaylist WHERE PlaylistID = ?1", [id])?;
            self.tx.execute("DELETE FROM djmdPlaylist WHERE ID = ?1", [id])?;
            self.xml.push(XmlOp::Remove(id.clone()));
        }
        self.touched = true;
        if let (Some(seq), Some(parent)) = (seq, parent) {
            let later: Vec<(String, i64)> = {
                let mut stmt = self.tx.prepare(
                    "SELECT ID, Seq FROM djmdPlaylist WHERE ParentID = ?1 AND Seq > ?2 AND rb_local_deleted = 0 ORDER BY Seq",
                )?;
                let rows = stmt.query_map(params![parent, seq], |row| Ok((row.get(0)?, row.get(1)?)))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            for (id, was) in later {
                self.tx.execute("UPDATE djmdPlaylist SET Seq = ?1 WHERE ID = ?2", params![was - 1, id])?;
                self.stamp("djmdPlaylist", &id)?;
            }
        }
        Ok(())
    }

    /// The track's UUID and length in seconds, which a new cue needs.
    fn named_id(&mut self, table: &str, name: &str) -> Result<Option<String>> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let found = self.tx.query_row(
            &format!("SELECT ID FROM {table} WHERE Name = ?1 ORDER BY rb_local_deleted LIMIT 1"),
            [name],
            |row| row.get::<_, String>(0),
        );
        match found {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let id = unused_id(&self.tx, table)?;
                self.tx.execute(
                    &format!(
                        "INSERT INTO {table} (ID, Name, UUID, rb_data_status, rb_local_data_status, rb_local_deleted, \
                         rb_local_synced, usn, rb_local_usn, created_at, updated_at) \
                         VALUES (?1, ?2, ?3, 0, 0, 0, 0, NULL, ?4, {NOW}, {NOW})"
                    ),
                    params![id, name, Uuid::new_v4().to_string(), self.usn],
                )?;
                self.touched = true;
                Ok(Some(id))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Set one column of a track, reporting whether the value changed.
    fn set_content(&mut self, id: &str, column: &str, value: rusqlite::types::Value) -> Result<bool> {
        let current: rusqlite::types::Value =
            self.tx.query_row(&format!("SELECT {column} FROM djmdContent WHERE ID = ?1"), [id], |row| row.get(0))?;
        if current == value {
            return Ok(false);
        }
        self.tx.execute(&format!("UPDATE djmdContent SET {column} = ?1 WHERE ID = ?2"), params![value, id])?;
        Ok(true)
    }

    /// Edit fields of live tracks. Artists, albums, genres and labels are found by name and added when new.
    pub fn update_tracks(&mut self, ids: &[String], edit: &TrackEdit) -> Result<()> {
        let resolved = self.resolve_names(edit)?;
        for id in ids {
            if self.content_state(id)? != Some(false) {
                return Err(Error::NotFound(format!("live track {id}")));
            }
            if self.apply_edit(id, edit, &resolved)? {
                self.stamp("djmdContent", id)?;
            }
        }
        Ok(())
    }

    fn resolve_names(&mut self, edit: &TrackEdit) -> Result<Vec<(&'static str, Value)>> {
        let named = [&edit.artist, &edit.remixer, &edit.original_artist, &edit.composer, &edit.album, &edit.genre, &edit.label];
        let mut resolved = Vec::new();
        for ((column, table), name) in NAMED.iter().zip(named) {
            if let Some(name) = name {
                resolved.push((*column, self.named_id(table, name)?.map_or(Value::Null, Value::Text)));
            }
        }
        Ok(resolved)
    }

    /// Set the edited columns of one track; true when any value changed.
    fn apply_edit(&mut self, id: &str, edit: &TrackEdit, resolved: &[(&str, Value)]) -> Result<bool> {
        let text = |s: &str| if s.is_empty() { Value::Null } else { Value::Text(s.to_owned()) };
        let mut changed = false;
        if let Some(title) = &edit.title {
            changed |= self.set_content(id, "Title", Value::Text(title.clone()))?;
        }
        if let Some(comment) = &edit.comment {
            changed |= self.set_content(id, "Commnt", text(comment))?;
        }
        for (column, value) in [("ReleaseYear", edit.year), ("TrackNo", edit.track_no), ("Rating", edit.rating)] {
            if let Some(value) = value {
                changed |= self.set_content(id, column, Value::Integer(value))?;
            }
        }
        if let Some(color) = edit.color {
            let value = if color == 0 { Value::Null } else { Value::Text(color.to_string()) };
            changed |= self.set_content(id, "ColorID", value)?;
        }
        for (column, value) in resolved {
            changed |= self.set_content(id, column, value.clone())?;
        }
        Ok(changed)
    }

    /// Point a live track at another file; the original location follows when it was the same.
    pub fn relocate_track(&mut self, id: &str, stored: &str, file_name: &str, size: i64) -> Result<()> {
        if self.content_state(id)? != Some(false) {
            return Err(Error::NotFound(format!("live track {id}")));
        }
        let old: Option<String> = self.tx.query_row("SELECT FolderPath FROM djmdContent WHERE ID = ?1", [id], |row| row.get(0))?;
        let mut changed = self.set_content(id, "FolderPath", Value::Text(stored.to_owned()))?;
        let org: Option<String> = self.tx.query_row("SELECT OrgFolderPath FROM djmdContent WHERE ID = ?1", [id], |row| row.get(0))?;
        if org == old {
            changed |= self.set_content(id, "OrgFolderPath", Value::Text(stored.to_owned()))?;
        }
        changed |= self.set_content(id, "FileNameL", Value::Text(file_name.to_owned()))?;
        changed |= self.set_content(id, "FileSize", Value::Integer(size))?;
        if changed {
            self.stamp("djmdContent", id)?;
        }
        Ok(())
    }

    /// Ids and removed flags of every row with this stored path.
    pub fn tracks_at(&self, path: &str) -> Result<Vec<(String, bool)>> {
        let mut stmt = self.tx.prepare("SELECT ID, rb_local_deleted FROM djmdContent WHERE FolderPath = ?1 ORDER BY ID")?;
        let rows = stmt.query_map([path], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0) != 0)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn first_or_none<T>(row: rusqlite::Result<T>) -> Result<Option<T>> {
        match row {
            Ok(found) => Ok(Some(found)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Add a track for an audio file. Returns its id.
    pub fn create_track(&mut self, new: &NewTrack) -> Result<String> {
        let id = unused_id(&self.tx, "djmdContent")?;
        let file_id = unused_value(&self.tx, "djmdContent", "rb_file_id")?;
        let (device, master) = Self::first_or_none(self.tx.query_row(
            "SELECT ID, MasterDBID FROM djmdDevice LIMIT 1",
            [],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
        ))?
        .unwrap_or((None, None));
        let link = Self::first_or_none(self.tx.query_row(
            "SELECT rb_local_usn FROM djmdMenuItems WHERE Name = 'TRACK' LIMIT 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        ))?
        .flatten();
        self.tx.execute(
            &format!(
                "INSERT INTO djmdContent (ID, UUID, ContentLink, DateCreated, DeviceID, FileNameL, FileSize, FileType, \
                 FolderPath, HotCueAutoLoad, MasterDBID, MasterSongID, StockDate, rb_file_id, ImagePath, \
                 Length, BitRate, SampleRate, BitDepth, rb_data_status, rb_local_data_status, rb_local_deleted, \
                 rb_local_synced, usn, rb_local_usn, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, {TODAY}, ?4, ?5, ?6, ?7, ?8, 'on', ?9, ?1, {TODAY}, ?10, ?11, ?12, ?13, ?14, ?15, \
                 0, 0, 0, 0, NULL, ?16, {NOW}, {NOW})"
            ),
            params![
                id,
                new.uuid,
                link,
                device,
                new.file_name,
                new.file_size,
                new.file_type,
                new.path,
                master,
                file_id,
                new.image_path,
                new.stream.length,
                new.stream.bit_rate,
                new.stream.sample_rate,
                new.stream.bit_depth,
                self.usn
            ],
        )?;
        self.touched = true;
        let resolved = self.resolve_names(&new.edit)?;
        self.apply_edit(&id, &new.edit, &resolved)?;
        Ok(id)
    }

    /// Bring a removed track back as the same row, re-reading its file facts. Its tags are not touched.
    pub fn revive_track(&mut self, id: &str, file: &FileFacts, stream: &Stream) -> Result<()> {
        self.restore_track(id)?;
        self.set_content(id, "FileNameL", Value::Text(file.name.clone()))?;
        self.set_content(id, "FileSize", Value::Integer(file.size))?;
        self.set_content(id, "FileType", Value::Integer(file.file_type))?;
        for (column, value) in [
            ("BitRate", stream.bit_rate),
            ("SampleRate", stream.sample_rate),
            ("BitDepth", stream.bit_depth),
            ("Length", stream.length),
        ] {
            if let Some(value) = value.filter(|v| *v != 0) {
                self.set_content(id, column, Value::Integer(value))?;
            }
        }
        self.tx.execute(&format!("UPDATE djmdContent SET StockDate = {TODAY} WHERE ID = ?1"), [id])?;
        self.stamp("djmdContent", id)
    }

    fn cue_context(&self, content_id: &str) -> Result<(Option<String>, i64)> {
        match self.tx.query_row(
            "SELECT UUID, COALESCE(Length, 0) FROM djmdContent WHERE ID = ?1",
            [content_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
        ) {
            Ok(found) => Ok(found),
            Err(rusqlite::Error::QueryReturnedNoRows) => Err(Error::NotFound(format!("track {content_id}"))),
            Err(e) => Err(e.into()),
        }
    }

    fn insert_cue(&mut self, content_id: &str, uuid: Option<&str>, kind: i64, msec: i64) -> Result<String> {
        let id = unused_id(&self.tx, "djmdCue")?;
        self.tx.execute(
            &format!(
                "INSERT INTO djmdCue (ID, ContentID, ContentUUID, Kind, InMsec, InFrame, InMpegFrame, InMpegAbs, OutMsec, \
                 OutFrame, OutMpegFrame, OutMpegAbs, Color, UUID, rb_data_status, rb_local_data_status, rb_local_deleted, \
                 rb_local_synced, usn, rb_local_usn, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, -1, 0, 0, 0, -1, ?7, 0, 0, 0, 0, NULL, ?8, {NOW}, {NOW})"
            ),
            params![id, content_id, uuid, kind, msec, msec * CUE_FRAMES_PER_SECOND / 1000, Uuid::new_v4().to_string(), self.usn],
        )?;
        self.touched = true;
        Ok(id)
    }

    /// Give a freshly imported track the analysis, cues and numbers it had on a device.
    pub fn apply_device_data(&mut self, content_id: &str, data: &DeviceData) -> Result<()> {
        let (uuid, length) = self.cue_context(content_id)?;
        if let Some(path) = &data.analysis_path {
            self.set_content(content_id, "AnalysisDataPath", Value::Text(path.clone()))?;
            self.set_content(content_id, "Analysed", Value::Integer(ANALYSED))?;
        }
        for (column, value) in [("BPM", data.bpm_x100), ("Rating", data.rating)] {
            if value > 0 {
                self.set_content(content_id, column, Value::Integer(value))?;
            }
        }
        if data.color > 0 {
            self.set_content(content_id, "ColorID", Value::Text(data.color.to_string()))?;
        }
        if !data.key.is_empty() {
            let key = Self::first_or_none(self.tx.query_row(
                "SELECT ID FROM djmdKey WHERE ScaleName = ?1 AND rb_local_deleted = 0 LIMIT 1",
                [&data.key],
                |row| row.get::<_, String>(0),
            ))?;
            if let Some(key) = key {
                self.set_content(content_id, "KeyID", Value::Text(key))?;
            }
        }
        for cue in &data.cues {
            let start = clamp_msec(cue.start_ms, length);
            let id = self.insert_cue(content_id, uuid.as_deref(), cue.kind, start)?;
            if let Some(end) = cue.end_ms.filter(|e| *e >= 0) {
                let end = clamp_msec(end, length);
                self.tx.execute(
                    "UPDATE djmdCue SET OutMsec = ?1, OutFrame = ?2 WHERE ID = ?3",
                    params![end, end * CUE_FRAMES_PER_SECOND / 1000, id],
                )?;
            }
            self.tx.execute(
                "UPDATE djmdCue SET ColorTableIndex = ?1, Comment = ?2, ActiveLoop = ?3 WHERE ID = ?4",
                params![cue.color_index.filter(|c| *c > 0), cue.comment, i64::from(cue.active_loop), id],
            )?;
        }
        self.stamp("djmdContent", content_id)
    }

    /// Place a hot cue in slot `kind`, replacing the one already there.
    pub fn set_hot_cue(&mut self, content_id: &str, kind: i64, msec: i64) -> Result<()> {
        if !HOT_SLOTS.contains(&kind) {
            return Err(Error::Refused(format!("not a hot cue slot: {kind}")));
        }
        let (uuid, length) = self.cue_context(content_id)?;
        let msec = clamp_msec(msec, length);
        self.tx.execute(
            "DELETE FROM djmdCue WHERE ContentID = ?1 AND Kind = ?2 AND rb_local_deleted = 0",
            params![content_id, kind],
        )?;
        self.insert_cue(content_id, uuid.as_deref(), kind, msec).map(|_| ())
    }

    /// Add a memory cue unless one already sits at that time.
    pub fn add_memory_cue(&mut self, content_id: &str, msec: i64) -> Result<()> {
        let (uuid, length) = self.cue_context(content_id)?;
        let msec = clamp_msec(msec, length);
        let present: i64 = self.tx.query_row(
            "SELECT count(*) FROM djmdCue WHERE ContentID = ?1 AND Kind = 0 AND rb_local_deleted = 0 \
             AND InMsec = ?2 AND (OutMsec IS NULL OR OutMsec < 0)",
            params![content_id, msec],
            |row| row.get(0),
        )?;
        if present == 0 {
            self.insert_cue(content_id, uuid.as_deref(), 0, msec)?;
        }
        Ok(())
    }

    pub fn delete_cue(&mut self, content_id: &str, cue_id: &str) -> Result<()> {
        let removed = self.tx.execute("DELETE FROM djmdCue WHERE ID = ?1 AND ContentID = ?2", params![cue_id, content_id])?;
        if removed > 0 {
            self.touched = true;
        }
        Ok(())
    }

    /// Set or clear a hot cue's palette colour (1 to 62).
    pub fn set_cue_color(&mut self, content_id: &str, cue_id: &str, color_index: Option<i64>) -> Result<()> {
        if let Some(c) = color_index.filter(|c| !(1..=CUE_PALETTE_SIZE).contains(c)) {
            return Err(Error::Refused(format!("not a hot cue colour: {c}")));
        }
        let found = self.tx.query_row(
            "SELECT Kind, ColorTableIndex FROM djmdCue WHERE ID = ?1 AND ContentID = ?2",
            params![cue_id, content_id],
            |row| Ok((row.get::<_, Option<i64>>(0)?.unwrap_or(0), row.get::<_, Option<i64>>(1)?)),
        );
        match found {
            Ok((0, _)) | Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(Error::NotFound(format!("hot cue {cue_id}")))
            }
            Ok((_, current)) if current == color_index => return Ok(()),
            Ok(_) => {}
            Err(e) => return Err(e.into()),
        }
        self.tx.execute("UPDATE djmdCue SET ColorTableIndex = ?1 WHERE ID = ?2", params![color_index, cue_id])?;
        self.stamp("djmdCue", cue_id)
    }

    fn require_tag(&self, tag_id: &str) -> Result<()> {
        match self.tx.query_row(
            "SELECT Attribute FROM djmdMyTag WHERE ID = ?1 AND rb_local_deleted = 0",
            [tag_id],
            |row| row.get::<_, Option<i64>>(0),
        ) {
            Ok(Some(1)) => Err(Error::Refused("pick a tag rather than a tag folder".into())),
            Ok(_) => Ok(()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Err(Error::NotFound(format!("tag {tag_id}"))),
            Err(e) => Err(e.into()),
        }
    }

    /// Give tracks a My Tag they do not already have. Returns how many were tagged.
    pub fn add_tag(&mut self, tag_id: &str, content_ids: &[String]) -> Result<usize> {
        self.require_tag(tag_id)?;
        let tagged: BTreeSet<String> = {
            let mut stmt = self.tx.prepare("SELECT ContentID FROM djmdSongMyTag WHERE MyTagID = ?1")?;
            let rows = stmt.query_map([tag_id], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut next = i64::try_from(tagged.len()).unwrap_or(0);
        let mut added = 0;
        let mut seen = BTreeSet::new();
        for id in content_ids {
            if tagged.contains(id) || !seen.insert(id) {
                continue;
            }
            if self.content_state(id)? != Some(false) {
                return Err(Error::NotFound(format!("live track {id}")));
            }
            next += 1;
            self.tx.execute(
                &format!(
                    "INSERT INTO djmdSongMyTag (ID, MyTagID, ContentID, TrackNo, UUID, rb_data_status, rb_local_data_status, \
                     rb_local_deleted, rb_local_synced, usn, rb_local_usn, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, 0, NULL, ?6, {NOW}, {NOW})"
                ),
                params![Uuid::new_v4().to_string(), tag_id, id, next, Uuid::new_v4().to_string(), self.usn],
            )?;
            self.touched = true;
            added += 1;
        }
        Ok(added)
    }

    pub fn remove_tag(&mut self, tag_id: &str, content_ids: &[String]) -> Result<usize> {
        self.require_tag(tag_id)?;
        let mut removed = 0;
        for id in content_ids {
            removed +=
                self.tx.execute("DELETE FROM djmdSongMyTag WHERE MyTagID = ?1 AND ContentID = ?2", params![tag_id, id])?;
        }
        if removed > 0 {
            self.touched = true;
        }
        Ok(removed)
    }
}
