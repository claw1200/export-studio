//! Every change the app makes to `master.db`. Each edit runs in one transaction and is stamped the
//! way rekordbox stamps its own: a new local update count, written to each touched row.

use std::collections::BTreeSet;

use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::{Error, Result};

const NOW: &str = "strftime('%Y-%m-%d %H:%M:%f +00:00','now')";

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

pub struct Tx<'a> {
    tx: Transaction<'a>,
    usn: i64,
    touched: bool,
}

pub(crate) fn run<T>(conn: &mut Connection, edit: impl FnOnce(&mut Tx<'_>) -> Result<T>) -> Result<T> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let counter: i64 = tx.query_row(
        "SELECT COALESCE(int_1, 0) FROM agentRegistry WHERE registry_id = 'localUpdateCount'",
        [],
        |row| row.get(0),
    )?;
    let mut scope = Tx { tx, usn: counter + 1, touched: false };
    let out = edit(&mut scope)?;
    if scope.touched {
        scope.tx.execute(
            &format!("UPDATE agentRegistry SET int_1 = ?1, updated_at = {NOW} WHERE registry_id = 'localUpdateCount'"),
            [scope.usn],
        )?;
    }
    scope.tx.commit()?;
    Ok(out)
}

impl Tx<'_> {
    fn stamp(&mut self, table: &str, id: &str) -> Result<()> {
        self.touched = true;
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

    /// Remove tracks from the collection. Unknown or already removed ids are ignored; returns how many were removed.
    pub fn remove_tracks(&mut self, ids: &[String]) -> Result<usize> {
        let mut playlists = BTreeSet::new();
        let mut removed = 0;
        for id in ids {
            if self.content_state(id)? != Some(false) {
                continue;
            }
            playlists.extend(self.clear_links(id)?);
            self.tx.execute("UPDATE djmdContent SET rb_local_deleted = 1 WHERE ID = ?1", [id])?;
            self.stamp("djmdContent", id)?;
            removed += 1;
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
}
