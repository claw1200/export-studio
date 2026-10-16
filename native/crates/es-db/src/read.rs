use rusqlite::{params_from_iter, Row};

use crate::live::{HIDDEN_PLAYLIST, MEMBERS_FROM, MEMBERS_WHERE, TRACK_LIVE};
use crate::{Error, Library, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub key: String,
    pub bpm_x100: i64,
    pub length: i64,
    pub rating: i64,
    pub file_type: i64,
    pub folder_path: String,
    pub analysis_path: Option<String>,
    pub stock_date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistKind {
    Folder,
    Playlist,
    Smart,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistNode {
    pub id: String,
    pub name: String,
    pub parent_id: String,
    pub seq: i64,
    pub kind: PlaylistKind,
    /// Tracks in a normal playlist; `None` for folders and smart playlists.
    pub count: Option<i64>,
}

const FOLDER: i64 = 1;
const SMART: i64 = 4;
const CHUNK: usize = 500;

fn text(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    Ok(row.get::<_, Option<String>>(index)?.unwrap_or_default())
}

fn number(row: &Row<'_>, index: usize) -> rusqlite::Result<i64> {
    Ok(row.get::<_, Option<i64>>(index)?.unwrap_or(0))
}

impl Library {
    /// Names of the tables that have a column called `column`; used to check the schema against what we classify.
    pub fn tables_with_column(&self, column: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for name in names {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info(`{name}`)"))?;
            let found = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|c| c == column);
            if found {
                out.push(name);
            }
        }
        Ok(out)
    }

    /// Live tracks, all of them or only those in `ids`.
    pub fn tracks(&self, ids: Option<&[String]>) -> Result<Vec<Track>> {
        let select = format!(
            "SELECT c.ID, c.Title, ar.Name, al.Name, ge.Name, ke.ScaleName, c.BPM, c.Length, c.Rating, \
                    c.FileType, c.FolderPath, c.AnalysisDataPath, c.StockDate \
             FROM djmdContent c \
             LEFT JOIN djmdArtist ar ON ar.ID = c.ArtistID \
             LEFT JOIN djmdAlbum al ON al.ID = c.AlbumID \
             LEFT JOIN djmdGenre ge ON ge.ID = c.GenreID \
             LEFT JOIN djmdKey ke ON ke.ID = c.KeyID \
             WHERE {TRACK_LIVE}"
        );
        let map = |row: &Row<'_>| -> rusqlite::Result<Track> {
            Ok(Track {
                id: row.get(0)?,
                title: text(row, 1)?,
                artist: text(row, 2)?,
                album: text(row, 3)?,
                genre: text(row, 4)?,
                key: text(row, 5)?,
                bpm_x100: number(row, 6)?,
                length: number(row, 7)?,
                rating: number(row, 8)?,
                file_type: number(row, 9)?,
                folder_path: text(row, 10)?,
                analysis_path: row.get(11)?,
                stock_date: text(row, 12)?,
            })
        };
        let conn = self.conn();
        let mut out = Vec::new();
        match ids {
            None => {
                let mut stmt = conn.prepare(&format!("{select} ORDER BY c.ID"))?;
                for row in stmt.query_map([], map)? {
                    out.push(row?);
                }
            }
            Some(ids) => {
                for chunk in ids.chunks(CHUNK) {
                    let marks = vec!["?"; chunk.len()].join(",");
                    let mut stmt = conn.prepare(&format!("{select} AND c.ID IN ({marks}) ORDER BY c.ID"))?;
                    for row in stmt.query_map(params_from_iter(chunk), map)? {
                        out.push(row?);
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn playlists(&self) -> Result<Vec<PlaylistNode>> {
        let conn = self.conn();
        let mut counts = std::collections::HashMap::<String, i64>::new();
        let mut stmt = conn.prepare(&format!(
            "SELECT sp.PlaylistID, count(*) FROM {MEMBERS_FROM} WHERE {MEMBERS_WHERE} GROUP BY sp.PlaylistID"
        ))?;
        for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))? {
            let (id, count) = row?;
            counts.insert(id, count);
        }
        let mut stmt = conn.prepare(
            "SELECT ID, Name, ParentID, Seq, Attribute FROM djmdPlaylist \
             WHERE rb_local_deleted = 0 AND ID <> ?1 ORDER BY ParentID, Seq",
        )?;
        let mut out = Vec::new();
        for row in stmt.query_map([HIDDEN_PLAYLIST], |row| {
            let attribute = number(row, 4)?;
            let kind = match attribute {
                FOLDER => PlaylistKind::Folder,
                SMART => PlaylistKind::Smart,
                _ => PlaylistKind::Playlist,
            };
            Ok(PlaylistNode {
                id: row.get(0)?,
                name: text(row, 1)?,
                parent_id: text(row, 2)?,
                seq: number(row, 3)?,
                kind,
                count: None,
            })
        })? {
            let mut node = row?;
            if node.kind == PlaylistKind::Playlist {
                node.count = Some(counts.get(&node.id).copied().unwrap_or(0));
            }
            out.push(node);
        }
        Ok(out)
    }

    /// Content ids in a normal playlist, in order.
    pub fn playlist_track_ids(&self, playlist_id: &str) -> Result<Vec<String>> {
        let attribute: Option<Option<i64>> = self
            .conn()
            .query_row(
                "SELECT Attribute FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
                [playlist_id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        match attribute {
            None => return Err(Error::NotFound(format!("playlist {playlist_id}"))),
            Some(Some(FOLDER)) => return Ok(Vec::new()),
            Some(Some(SMART)) => return Err(Error::Unsupported("smart playlists")),
            Some(_) => {}
        }
        let mut stmt = self.conn().prepare(&format!(
            "SELECT sp.ContentID FROM {MEMBERS_FROM} WHERE {MEMBERS_WHERE} AND sp.PlaylistID = ?1 ORDER BY sp.TrackNo"
        ))?;
        let rows = stmt.query_map([playlist_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
