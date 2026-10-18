//! Reading a stick's library from `exportLibrary.db` or `export.pdb`.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, Row};

use crate::model::{Cue, Device, History, LibraryKind, Playlist, Track};
use crate::paths::pioneer_dir;
use crate::{Error, Result};

fn db(e: impl std::fmt::Display) -> Error {
    Error::Db(e.to_string())
}

/// Read the library the player would use: `exportLibrary.db` when the stick has one, else `export.pdb`.
/// `None` when the volume holds neither.
pub fn read(volume: &Path) -> Result<Option<Device>> {
    let (root, _) = pioneer_dir(volume);
    if root.join("rekordbox").join("exportLibrary.db").is_file() {
        return read_kind(volume, LibraryKind::OneLibrary);
    }
    read_kind(volume, LibraryKind::DeviceLibrary)
}

/// Read one specific library from a stick; `None` when the stick has no database of that kind.
pub fn read_kind(volume: &Path, kind: LibraryKind) -> Result<Option<Device>> {
    let (root, root_name) = pioneer_dir(volume);
    match kind {
        LibraryKind::OneLibrary => {
            let path = root.join("rekordbox").join("exportLibrary.db");
            if !path.is_file() {
                return Ok(None);
            }
            let library = rbl_onelibrary::ExportLibrary::open_read_only(&path).map_err(db)?;
            Ok(Some(from_onelibrary(library.connection(), root_name).map_err(db)?))
        }
        LibraryKind::DeviceLibrary => {
            let path = root.join("rekordbox").join("export.pdb");
            if !path.is_file() {
                return Ok(None);
            }
            let bytes = std::fs::read(&path)?;
            Ok(Some(from_pdb(&bytes, root_name)?))
        }
    }
}

fn text(row: &Row<'_>, column: &str) -> rusqlite::Result<String> {
    Ok(row.get::<_, Option<String>>(column)?.unwrap_or_default())
}

fn number(row: &Row<'_>, column: &str) -> rusqlite::Result<i64> {
    Ok(row.get::<_, Option<i64>>(column)?.unwrap_or(0))
}

fn names(conn: &Connection, table: &str, id: &str) -> rusqlite::Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare(&format!("SELECT {id}, name FROM \"{table}\""))?;
    let rows = stmt.query_map([], |row| Ok((number(row, id)?, text(row, "name")?)))?;
    rows.collect()
}

fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let found = stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?.iter().any(|c| c == column);
    Ok(found)
}

fn from_onelibrary(conn: &Connection, root_name: &str) -> rusqlite::Result<Device> {
    let artists = names(conn, "artist", "artist_id")?;
    let albums = names(conn, "album", "album_id")?;
    let genres = names(conn, "genre", "genre_id")?;
    let labels = names(conn, "label", "label_id")?;
    let keys = names(conn, "key", "key_id")?;
    let mut stmt = conn.prepare("SELECT image_id, path FROM image")?;
    let images = stmt.query_map([], |row| Ok((number(row, "image_id")?, text(row, "path")?)))?
        .collect::<rusqlite::Result<HashMap<i64, String>>>()?;

    let mut cues: HashMap<i64, Vec<Cue>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT * FROM cue ORDER BY cue_id")?;
    let rows = stmt.query_map([], |row| {
        let end = row.get::<_, Option<i64>>("outUsec")?;
        let numerator = number(row, "beatLoopNumerator")?;
        let denominator = number(row, "beatLoopDenominator")?;
        Ok((
            number(row, "content_id")?,
            Cue {
                kind: number(row, "kind")?,
                start_ms: number(row, "inUsec")? / 1000,
                end_ms: end.filter(|e| *e >= 0).map(|e| e / 1000),
                color_index: row.get::<_, Option<i64>>("colorTableIndex")?,
                comment: text(row, "cueComment")?,
                active_loop: number(row, "isActiveLoop")? != 0,
                beat_loop: (numerator << 16) | denominator,
            },
        ))
    })?;
    for row in rows {
        let (content, cue) = row?;
        cues.entry(content).or_default().push(cue);
    }

    let master_db = has_column(conn, "content", "masterDbId")?;
    let mut tracks = Vec::new();
    let mut stmt = conn.prepare("SELECT * FROM content ORDER BY content_id")?;
    let rows = stmt.query_map([], |row| {
        let id = number(row, "content_id")?;
        let master = number(row, "masterContentId")?;
        let master_db_id = if master_db { number(row, "masterDbId")? } else { 0 };
        Ok(Track {
            export_id: id,
            master_id: (master != 0).then_some(master),
            master_db_id: (master_db_id != 0).then_some(master_db_id),
            title: text(row, "title")?,
            artist: artists.get(&number(row, "artist_id_artist")?).cloned().unwrap_or_default(),
            album: albums.get(&number(row, "album_id")?).cloned().unwrap_or_default(),
            genre: genres.get(&number(row, "genre_id")?).cloned().unwrap_or_default(),
            label: labels.get(&number(row, "label_id")?).cloned().unwrap_or_default(),
            key: keys.get(&number(row, "key_id")?).cloned().unwrap_or_default(),
            path: text(row, "path")?,
            filename: text(row, "fileName")?,
            analysis: text(row, "analysisDataFilePath")?,
            artwork: images.get(&number(row, "image_id")?).cloned().unwrap_or_default(),
            comment: text(row, "djComment")?,
            bpm_x100: number(row, "bpmx100")?,
            duration: number(row, "length")?,
            bitrate: number(row, "bitrate")?,
            sample_rate: number(row, "samplingRate")?,
            bit_depth: number(row, "bitDepth")?,
            file_type: Some(number(row, "fileType")?).filter(|t| *t != 0).unwrap_or(1),
            file_size: number(row, "fileSize")?,
            rating: number(row, "rating")?,
            color: number(row, "color_id")?,
            year: number(row, "releaseYear")?,
            track_no: number(row, "trackNo")?,
            play_count: number(row, "djPlayCount")?,
            hot_cue_auto: number(row, "isHotCueAutoLoadOn")? != 0,
            date_added: text(row, "dateAdded")?,
            cues: cues.get(&id).cloned().unwrap_or_default(),
        })
    })?;
    for row in rows {
        tracks.push(row?);
    }

    let mut members: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT playlist_id, content_id FROM playlist_content ORDER BY playlist_id, sequenceNo")?;
    for row in stmt.query_map([], |row| Ok((number(row, "playlist_id")?, number(row, "content_id")?)))? {
        let (playlist, content) = row?;
        members.entry(playlist).or_default().push(content);
    }
    let mut playlists = Vec::new();
    let mut stmt = conn.prepare("SELECT * FROM playlist")?;
    for row in stmt.query_map([], |row| {
        let id = number(row, "playlist_id")?;
        Ok(Playlist {
            export_id: id,
            name: text(row, "name")?,
            parent_id: number(row, "playlist_id_parent")?,
            seq: number(row, "sequenceNo")?,
            folder: number(row, "attribute")? == 1,
            track_ids: members.get(&id).cloned().unwrap_or_default(),
        })
    })? {
        playlists.push(row?);
    }

    let mut played: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT history_id, content_id FROM history_content ORDER BY history_id, sequenceNo")?;
    for row in stmt.query_map([], |row| Ok((number(row, "history_id")?, number(row, "content_id")?)))? {
        let (history, content) = row?;
        played.entry(history).or_default().push(content);
    }
    let mut histories = Vec::new();
    let mut stmt = conn.prepare("SELECT * FROM history")?;
    for row in stmt.query_map([], |row| {
        let id = number(row, "history_id")?;
        Ok(History {
            export_id: id,
            name: text(row, "name")?,
            parent_id: number(row, "history_id_parent")?,
            seq: number(row, "sequenceNo")?,
            attribute: number(row, "attribute")?,
            track_ids: played.get(&id).cloned().unwrap_or_default(),
        })
    })? {
        histories.push(row?);
    }

    let device_name = conn
        .query_row("SELECT deviceName FROM property LIMIT 1", [], |row| text(row, "deviceName"))
        .unwrap_or_default();
    Ok(Device { kind: LibraryKind::OneLibrary, root_name: root_name.to_owned(), device_name, tracks, playlists, histories })
}

/// rekordbox's file type code from an extension.
fn file_type_of(name: &str) -> i64 {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("m4a") => 4,
        Some("flac") => 5,
        Some("wav") => 11,
        Some("aif" | "aiff") => 12,
        _ => 1,
    }
}

fn from_pdb(bytes: &[u8], root_name: &str) -> Result<Device> {
    use rbl_pdb::PageType;
    let pdb = rbl_pdb::Pdb::parse(bytes).map_err(db)?;
    let named = |kind: PageType| -> HashMap<u32, String> {
        pdb.table(kind).map(|t| pdb.named_rows(t).into_iter().map(|r| (r.id, r.name)).collect()).unwrap_or_default()
    };
    let (artists, albums, genres, labels, keys) =
        (named(PageType::Artists), named(PageType::Albums), named(PageType::Genres), named(PageType::Labels), named(PageType::Keys));
    let images = named(PageType::Artwork);
    let pick = |map: &HashMap<u32, String>, id: u32| map.get(&id).cloned().unwrap_or_default();
    let tracks = pdb
        .table(PageType::Tracks)
        .map(|t| pdb.track_rows(t))
        .unwrap_or_default()
        .into_iter()
        .map(|row| Track {
            export_id: i64::from(row.id),
            master_id: (row.master_content_id != 0).then_some(i64::from(row.master_content_id)),
            master_db_id: (row.master_db_id != 0).then_some(i64::from(row.master_db_id)),
            title: row.title.clone(),
            artist: pick(&artists, row.artist_id),
            album: pick(&albums, row.album_id),
            genre: pick(&genres, row.genre_id),
            label: pick(&labels, row.label_id),
            key: pick(&keys, row.key_id),
            file_type: file_type_of(&row.filename),
            path: row.file_path,
            filename: row.filename,
            analysis: row.analyze_path,
            artwork: pick(&images, row.artwork_id),
            comment: row.comment,
            bpm_x100: i64::from(row.tempo_x100),
            duration: i64::from(row.duration_sec),
            bitrate: i64::from(row.bitrate),
            sample_rate: i64::from(row.sample_rate),
            bit_depth: i64::from(row.sample_depth),
            file_size: i64::from(row.file_size),
            rating: i64::from(row.rating),
            color: i64::from(row.color_id),
            year: i64::from(row.year),
            track_no: i64::from(row.track_number),
            play_count: i64::from(row.play_count),
            hot_cue_auto: row.hot_cue_auto_load,
            date_added: row.date_added,
            cues: Vec::new(),
        })
        .collect();

    let mut entries: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    for entry in pdb.table(PageType::PlaylistEntries).map(|t| pdb.playlist_entries(t)).unwrap_or_default() {
        entries.entry(entry.playlist_id).or_default().push((entry.entry_index, entry.track_id));
    }
    let playlists = pdb
        .table(PageType::PlaylistTree)
        .map(|t| pdb.playlist_nodes(t))
        .unwrap_or_default()
        .into_iter()
        .map(|node| {
            let mut members = entries.remove(&node.id).unwrap_or_default();
            members.sort_unstable();
            Playlist {
                export_id: i64::from(node.id),
                name: node.name,
                parent_id: i64::from(node.parent_id),
                seq: i64::from(node.sort_order),
                folder: node.is_folder,
                track_ids: members.into_iter().map(|(_, track)| i64::from(track)).collect(),
            }
        })
        .collect();

    let mut played: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    for entry in pdb.played_history_entries() {
        played.entry(entry.playlist_id).or_default().push((entry.entry_index, entry.track_id));
    }
    let histories = pdb
        .played_histories()
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut members = played.remove(&row.id).unwrap_or_default();
            members.sort_unstable();
            History {
                export_id: i64::from(row.id),
                name: row.name,
                parent_id: 0,
                seq: i64::try_from(index).unwrap_or(0) + 1,
                attribute: 0,
                track_ids: members.into_iter().map(|(_, track)| i64::from(track)).collect(),
            }
        })
        .collect();
    let device_name = pdb.property().map(|p| p.device_name).unwrap_or_default();
    Ok(Device { kind: LibraryKind::DeviceLibrary, root_name: root_name.to_owned(), device_name, tracks, playlists, histories })
}
