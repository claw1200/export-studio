//! Everything a USB export needs from the library for a set of playlists.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params_from_iter, Row};

use crate::live::TRACK_LIVE;
use crate::read::PlaylistKind;
use crate::{Error, Library, Result};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportCue {
    pub kind: i64,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub color: Option<i64>,
    pub color_index: Option<i64>,
    pub comment: String,
    pub active_loop: bool,
    pub beat_loop: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub key: String,
    pub comment: String,
    pub date_added: String,
    pub release_date: String,
    pub bpm_x100: i64,
    pub length: i64,
    pub rating: i64,
    pub color: i64,
    pub year: i64,
    pub bit_rate: i64,
    pub sample_rate: i64,
    pub bit_depth: i64,
    pub file_size: i64,
    pub file_name: String,
    pub folder_path: String,
    pub analysis_path: Option<String>,
    pub image_path: Option<String>,
    pub track_no: i64,
    pub disc_no: i64,
    pub hot_cue_auto: bool,
    pub analysed: i64,
    pub play_count: i64,
    pub date_created: String,
    pub isrc: String,
    pub my_tags: Vec<String>,
    pub cues: Vec<ExportCue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPlaylist {
    pub id: String,
    pub name: String,
    /// `None` at the top level, or when the parent is not part of this export.
    pub parent_id: Option<String>,
    pub seq: i64,
    pub folder: bool,
    pub track_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportTag {
    pub id: String,
    pub seq: i64,
    pub name: String,
    pub attribute: i64,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bundle {
    pub db_id: String,
    pub playlists: Vec<ExportPlaylist>,
    pub tracks: Vec<ExportTrack>,
    pub tags: Vec<ExportTag>,
}

fn text(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    use rusqlite::types::ValueRef;
    Ok(match row.get_ref(index)? {
        ValueRef::Null => String::new(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(n) => n.to_string(),
        ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
    })
}

fn number(row: &Row<'_>, index: usize) -> rusqlite::Result<i64> {
    Ok(row.get::<_, Option<i64>>(index).unwrap_or(None).unwrap_or(0))
}

impl Library {
    /// The playlists to export, with the folders above them and the folders' contents, and every track in them.
    pub fn export_bundle(&self, playlist_ids: &[String]) -> Result<Bundle> {
        let nodes = self.playlists()?;
        let by_id: BTreeMap<&str, &crate::PlaylistNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for node in &nodes {
            children.entry(node.parent_id.as_str()).or_default().push(node.id.as_str());
        }
        let mut include: BTreeSet<&str> = BTreeSet::new();
        for id in playlist_ids {
            let Some(selected) = by_id.get(id.as_str()) else { return Err(Error::NotFound(format!("playlist {id}"))) };
            let mut cursor = Some(*selected);
            while let Some(node) = cursor {
                include.insert(node.id.as_str());
                cursor = by_id.get(node.parent_id.as_str()).copied();
            }
            if selected.kind == PlaylistKind::Folder {
                let mut stack = vec![selected.id.as_str()];
                while let Some(current) = stack.pop() {
                    for child in children.get(current).map(Vec::as_slice).unwrap_or_default() {
                        include.insert(child);
                        stack.push(child);
                    }
                }
            }
        }
        let ordered: Vec<&crate::PlaylistNode> = nodes.iter().filter(|n| include.contains(n.id.as_str())).collect();
        if !ordered.iter().any(|n| n.kind != PlaylistKind::Folder) {
            return Err(Error::Refused("Select a playlist to sync".into()));
        }

        let mut playlists = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut known = BTreeSet::new();
        for node in ordered {
            let members = match node.kind {
                PlaylistKind::Folder => Vec::new(),
                PlaylistKind::Playlist | PlaylistKind::Smart => self.playlist_track_ids(&node.id)?,
            };
            for id in &members {
                if known.insert(id.clone()) {
                    seen.push(id.clone());
                }
            }
            playlists.push(ExportPlaylist {
                id: node.id.clone(),
                name: if node.name.is_empty() { "Untitled".into() } else { node.name.clone() },
                parent_id: Some(node.parent_id.clone()).filter(|p| p != "root" && include.contains(p.as_str())),
                seq: node.seq,
                folder: node.kind == PlaylistKind::Folder,
                track_ids: members,
            });
        }
        let tracks = self.export_tracks(&seen)?;
        let tags = self.export_tags()?;
        Ok(Bundle { db_id: self.db_id()?.to_string(), playlists, tracks, tags })
    }

    fn export_tags(&self) -> Result<Vec<ExportTag>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT ID, Seq, Name, Attribute, ParentID FROM djmdMyTag WHERE rb_local_deleted = 0 ORDER BY ParentID, Seq")?;
        let rows = stmt.query_map([], |row| {
            let parent = text(row, 4)?;
            Ok(ExportTag {
                id: text(row, 0)?,
                seq: number(row, 1)?,
                name: text(row, 2)?,
                attribute: number(row, 3)?,
                parent_id: Some(parent).filter(|p| !p.is_empty() && p != "root"),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The live tracks among `ids`, in the order given, with cues and tags.
    pub fn export_tracks(&self, ids: &[String]) -> Result<Vec<ExportTrack>> {
        let mut by_id: BTreeMap<String, ExportTrack> = BTreeMap::new();
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = self.conn().prepare(&format!(
                "SELECT c.ID, c.Title, ar.Name, al.Name, ge.Name, la.Name, ke.ScaleName, c.Commnt, c.StockDate, c.ReleaseDate, \
                        c.BPM, c.Length, c.Rating, c.ColorID, c.ReleaseYear, c.BitRate, c.SampleRate, c.BitDepth, c.FileSize, \
                        c.FileNameL, c.FolderPath, c.AnalysisDataPath, c.ImagePath, c.TrackNo, c.DiscNo, c.HotCueAutoLoad, \
                        c.Analysed, c.DJPlayCount, c.DateCreated, c.ISRC \
                 FROM djmdContent c \
                 LEFT JOIN djmdArtist ar ON ar.ID = c.ArtistID LEFT JOIN djmdAlbum al ON al.ID = c.AlbumID \
                 LEFT JOIN djmdGenre ge ON ge.ID = c.GenreID LEFT JOIN djmdLabel la ON la.ID = c.LabelID \
                 LEFT JOIN djmdKey ke ON ke.ID = c.KeyID \
                 WHERE {TRACK_LIVE} AND c.ID IN ({marks})"
            ))?;
            let rows = stmt.query_map(params_from_iter(chunk), |row| {
                let auto = text(row, 25)?;
                Ok(ExportTrack {
                    id: text(row, 0)?,
                    title: text(row, 1)?,
                    artist: text(row, 2)?,
                    album: text(row, 3)?,
                    genre: text(row, 4)?,
                    label: text(row, 5)?,
                    key: text(row, 6)?,
                    comment: text(row, 7)?,
                    date_added: text(row, 8)?,
                    release_date: text(row, 9)?,
                    bpm_x100: number(row, 10)?,
                    length: number(row, 11)?,
                    rating: number(row, 12)?,
                    color: text(row, 13)?.parse().unwrap_or(0),
                    year: number(row, 14)?,
                    bit_rate: number(row, 15)?,
                    sample_rate: number(row, 16)?,
                    bit_depth: number(row, 17)?,
                    file_size: number(row, 18)?,
                    file_name: text(row, 19)?,
                    folder_path: text(row, 20)?,
                    analysis_path: Some(text(row, 21)?).filter(|p| !p.is_empty()),
                    image_path: Some(text(row, 22)?).filter(|p| !p.is_empty()),
                    track_no: number(row, 23)?,
                    disc_no: number(row, 24)?,
                    hot_cue_auto: auto.is_empty() || !matches!(auto.to_uppercase().as_str(), "0" | "OFF"),
                    analysed: number(row, 26)?,
                    play_count: number(row, 27)?,
                    date_created: text(row, 28)?,
                    isrc: text(row, 29)?,
                    ..ExportTrack::default()
                })
            })?;
            for row in rows {
                let track = row?;
                by_id.insert(track.id.clone(), track);
            }
            let mut cues = self.conn().prepare(&format!(
                "SELECT ContentID, Kind, InMsec, OutMsec, Color, ColorTableIndex, Comment, ActiveLoop, BeatLoopSize FROM djmdCue \
                 WHERE rb_local_deleted = 0 AND ContentID IN ({marks}) ORDER BY rowid"
            ))?;
            for row in cues.query_map(params_from_iter(chunk), |row| {
                Ok((
                    text(row, 0)?,
                    ExportCue {
                        kind: number(row, 1)?,
                        start_ms: number(row, 2)?,
                        end_ms: row.get::<_, Option<i64>>(3).unwrap_or(None).filter(|e| *e >= 0),
                        color: row.get::<_, Option<i64>>(4).unwrap_or(None).filter(|c| *c >= 0),
                        color_index: row.get::<_, Option<i64>>(5).unwrap_or(None),
                        comment: text(row, 6)?,
                        active_loop: number(row, 7)? != 0,
                        beat_loop: number(row, 8)?,
                    },
                ))
            })? {
                let (id, cue) = row?;
                if let Some(track) = by_id.get_mut(&id) {
                    track.cues.push(cue);
                }
            }
            let mut tags = self.conn().prepare(&format!(
                "SELECT ContentID, MyTagID FROM djmdSongMyTag WHERE rb_local_deleted = 0 AND ContentID IN ({marks}) ORDER BY TrackNo"
            ))?;
            for row in tags.query_map(params_from_iter(chunk), |row| Ok((text(row, 0)?, text(row, 1)?)))? {
                let (id, tag) = row?;
                if let Some(track) = by_id.get_mut(&id) {
                    track.my_tags.push(tag);
                }
            }
        }
        Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
    }
}
