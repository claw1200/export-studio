use std::collections::HashMap;

use rusqlite::{params_from_iter, Row};

use crate::live::{HIDDEN_PLAYLIST, MEMBERS_FROM, MEMBERS_WHERE, TRACK_LIVE};
use crate::{Error, Library, Result};

/// A live track with its names resolved. Text columns that are NULL read as empty.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub original_artist: String,
    pub composer: String,
    pub key: String,
    pub bpm_x100: i64,
    pub length: i64,
    pub rating: i64,
    pub color_id: String,
    pub comment: String,
    pub year: i64,
    pub stock_date: String,
    pub date_created: String,
    pub created_at: String,
    pub play_count: i64,
    pub bit_rate: i64,
    pub sample_rate: i64,
    pub bit_depth: i64,
    pub file_type: i64,
    pub folder_path: String,
    pub file_size: i64,
    pub image_path: String,
    pub track_no: i64,
    pub analysis_path: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryNode {
    pub id: String,
    pub name: String,
    pub parent_id: String,
    pub seq: i64,
    pub is_folder: bool,
    pub date: String,
    pub count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TagNode {
    pub id: String,
    pub name: String,
    pub parent_id: String,
    pub seq: i64,
    pub is_folder: bool,
}

const FOLDER: i64 = 1;
const SMART: i64 = 4;
const CHUNK: usize = 500;

/// One stored cue of a track.
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub id: String,
    pub kind: i64,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub color: Option<i64>,
    pub color_index: Option<i64>,
    pub comment: String,
}

/// Where a track's audio and analysis live.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackFiles {
    pub folder_path: String,
    pub analysis_path: Option<String>,
    pub image_path: Option<String>,
}

fn text(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    Ok(row.get::<_, Option<String>>(index)?.unwrap_or_default())
}

fn number(row: &Row<'_>, index: usize) -> rusqlite::Result<i64> {
    Ok(row.get::<_, Option<i64>>(index)?.unwrap_or(0))
}

/// Text from a column whose declared type is not enforced, so it may hold a number.
fn loose(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    use rusqlite::types::ValueRef;
    Ok(match row.get_ref(index)? {
        ValueRef::Null => String::new(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(n) => n.to_string(),
        ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
    })
}

fn id_filter(ids: Option<&[String]>, column: &str) -> (String, Vec<String>) {
    match ids {
        None => (String::new(), Vec::new()),
        Some(ids) => (format!(" AND {column} IN ({})", vec!["?"; ids.len()].join(",")), ids.to_vec()),
    }
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
            "SELECT c.ID, c.Title, ar.Name, al.Name, ge.Name, la.Name, re.Name, oa.Name, co.Name, ke.ScaleName, \
                    c.BPM, c.Length, c.Rating, c.ColorID, c.Commnt, c.ReleaseYear, c.StockDate, c.DateCreated, \
                    c.created_at, c.DJPlayCount, c.BitRate, c.SampleRate, c.BitDepth, c.FileType, c.FolderPath, \
                    c.FileSize, c.ImagePath, c.TrackNo, c.AnalysisDataPath \
             FROM djmdContent c \
             LEFT JOIN djmdArtist ar ON ar.ID = c.ArtistID \
             LEFT JOIN djmdAlbum al ON al.ID = c.AlbumID \
             LEFT JOIN djmdGenre ge ON ge.ID = c.GenreID \
             LEFT JOIN djmdLabel la ON la.ID = c.LabelID \
             LEFT JOIN djmdArtist re ON re.ID = c.RemixerID \
             LEFT JOIN djmdArtist oa ON oa.ID = c.OrgArtistID \
             LEFT JOIN djmdArtist co ON co.ID = c.ComposerID \
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
                label: text(row, 5)?,
                remixer: text(row, 6)?,
                original_artist: text(row, 7)?,
                composer: text(row, 8)?,
                key: text(row, 9)?,
                bpm_x100: number(row, 10)?,
                length: number(row, 11)?,
                rating: number(row, 12)?,
                color_id: loose(row, 13)?,
                comment: text(row, 14)?,
                year: number(row, 15)?,
                stock_date: text(row, 16)?,
                date_created: text(row, 17)?,
                created_at: loose(row, 18)?,
                play_count: number(row, 19)?,
                bit_rate: number(row, 20)?,
                sample_rate: number(row, 21)?,
                bit_depth: number(row, 22)?,
                file_type: number(row, 23)?,
                folder_path: text(row, 24)?,
                file_size: number(row, 25)?,
                image_path: text(row, 26)?,
                track_no: number(row, 27)?,
                analysis_path: row.get::<_, Option<String>>(28)?.filter(|p| !p.is_empty()),
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

    /// Ids and removed flags of every row stored at `path`.
    pub fn tracks_at(&self, path: &str) -> Result<Vec<(String, bool)>> {
        let mut stmt =
            self.conn().prepare("SELECT ID, rb_local_deleted FROM djmdContent WHERE FolderPath = ?1 ORDER BY ID")?;
        let rows = stmt.query_map([path], |row| Ok((row.get::<_, String>(0)?, number(row, 1)? != 0)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Where a track's files are, for a track in the library or removed from it. `None` for an unknown id.
    pub fn track_files(&self, id: &str) -> Result<Option<TrackFiles>> {
        let row = self.conn().query_row(
            "SELECT FolderPath, AnalysisDataPath, ImagePath FROM djmdContent WHERE ID = ?1",
            [id],
            |row| {
                Ok(TrackFiles {
                    folder_path: text(row, 0)?,
                    analysis_path: row.get::<_, Option<String>>(1)?.filter(|p| !p.is_empty()),
                    image_path: row.get::<_, Option<String>>(2)?.filter(|p| !p.is_empty()),
                })
            },
        );
        match row {
            Ok(files) => Ok(Some(files)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// A track's live cues, in the order they were stored.
    pub fn cues(&self, content_id: &str) -> Result<Vec<Cue>> {
        let mut stmt = self.conn().prepare(
            "SELECT ID, Kind, InMsec, OutMsec, Color, ColorTableIndex, Comment FROM djmdCue \
             WHERE ContentID = ?1 AND rb_local_deleted = 0 ORDER BY rowid",
        )?;
        let rows = stmt.query_map([content_id], |row| {
            Ok(Cue {
                id: row.get(0)?,
                kind: number(row, 1)?,
                start_ms: number(row, 2)?,
                end_ms: row.get(3)?,
                color: row.get(4)?,
                color_index: row.get(5)?,
                comment: text(row, 6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Cue kinds per track (0 is a memory cue, 1 and up are hot cues and loops), live cues on live tracks only.
    pub fn cue_kinds(&self, ids: Option<&[String]>) -> Result<HashMap<String, Vec<i64>>> {
        let (filter, params) = id_filter(ids, "q.ContentID");
        let mut stmt = self.conn().prepare(&format!(
            "SELECT q.ContentID, q.Kind FROM djmdCue q JOIN djmdContent c ON c.ID = q.ContentID \
             WHERE q.rb_local_deleted = 0 AND {TRACK_LIVE}{filter}"
        ))?;
        let mut out: HashMap<String, Vec<i64>> = HashMap::new();
        for row in stmt.query_map(params_from_iter(params), |row| Ok((row.get::<_, String>(0)?, number(row, 1)?)))? {
            let (id, kind) = row?;
            out.entry(id).or_default().push(kind);
        }
        Ok(out)
    }

    /// My Tag ids per track, live links on live tracks only.
    pub fn my_tag_ids(&self, ids: Option<&[String]>) -> Result<HashMap<String, Vec<String>>> {
        let (filter, params) = id_filter(ids, "sm.ContentID");
        let mut stmt = self.conn().prepare(&format!(
            "SELECT sm.ContentID, sm.MyTagID FROM djmdSongMyTag sm JOIN djmdContent c ON c.ID = sm.ContentID \
             WHERE sm.rb_local_deleted = 0 AND {TRACK_LIVE}{filter} ORDER BY sm.TrackNo"
        ))?;
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        for row in stmt.query_map(params_from_iter(params), |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
            let (id, tag) = row?;
            out.entry(id).or_default().push(tag);
        }
        Ok(out)
    }

    pub fn playlists(&self) -> Result<Vec<PlaylistNode>> {
        let conn = self.conn();
        let mut counts = HashMap::<String, i64>::new();
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
            let kind = match number(row, 4)? {
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

    /// Content ids that sit in at least one live normal playlist.
    pub fn playlist_member_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT DISTINCT sp.ContentID FROM {MEMBERS_FROM} \
             WHERE {MEMBERS_WHERE} AND COALESCE(p.Attribute, 0) = 0 AND p.ID <> ?1"
        ))?;
        let rows = stmt.query_map([HIDDEN_PLAYLIST], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Content ids in a normal playlist, in order.
    pub fn playlist_track_ids(&self, playlist_id: &str) -> Result<Vec<String>> {
        let attribute = self.conn().query_row(
            "SELECT Attribute FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
            [playlist_id],
            |row| row.get::<_, Option<i64>>(0),
        );
        match attribute {
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::NotFound(format!("playlist {playlist_id}"))),
            Err(e) => return Err(e.into()),
            Ok(Some(FOLDER)) => return Ok(Vec::new()),
            Ok(Some(SMART)) => return Err(Error::Unsupported("smart playlists")),
            Ok(_) => {}
        }
        let mut stmt = self.conn().prepare(&format!(
            "SELECT sp.ContentID FROM {MEMBERS_FROM} WHERE {MEMBERS_WHERE} AND sp.PlaylistID = ?1 ORDER BY sp.TrackNo"
        ))?;
        let rows = stmt.query_map([playlist_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn histories(&self) -> Result<Vec<HistoryNode>> {
        let conn = self.conn();
        let mut counts = HashMap::<String, i64>::new();
        let mut stmt = conn.prepare(
            "SELECT sh.HistoryID, count(*) FROM djmdSongHistory sh JOIN djmdContent c ON c.ID = sh.ContentID \
             WHERE sh.rb_local_deleted = 0 AND c.rb_local_deleted = 0 GROUP BY sh.HistoryID",
        )?;
        for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))? {
            let (id, count) = row?;
            counts.insert(id, count);
        }
        let mut stmt = conn.prepare(
            "SELECT ID, Name, ParentID, Seq, Attribute, DateCreated FROM djmdHistory \
             WHERE rb_local_deleted = 0 ORDER BY ParentID, Seq",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let is_folder = number(row, 4)? == FOLDER;
            Ok(HistoryNode {
                count: if is_folder { None } else { Some(counts.get(&id).copied().unwrap_or(0)) },
                id,
                name: text(row, 1)?,
                parent_id: text(row, 2)?,
                seq: number(row, 3)?,
                is_folder,
                date: text(row, 5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn history_track_ids(&self, history_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn().prepare(
            "SELECT sh.ContentID FROM djmdSongHistory sh JOIN djmdContent c ON c.ID = sh.ContentID \
             WHERE sh.HistoryID = ?1 AND sh.rb_local_deleted = 0 AND c.rb_local_deleted = 0 ORDER BY sh.TrackNo",
        )?;
        let rows = stmt.query_map([history_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn my_tags(&self) -> Result<Vec<TagNode>> {
        let mut stmt = self.conn().prepare(
            "SELECT ID, Name, ParentID, Seq, Attribute FROM djmdMyTag WHERE rb_local_deleted = 0 ORDER BY ParentID, Seq",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TagNode {
                id: row.get(0)?,
                name: text(row, 1)?,
                parent_id: text(row, 2)?,
                seq: number(row, 3)?,
                is_folder: number(row, 4)? == FOLDER,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
