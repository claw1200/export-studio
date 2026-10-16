//! Rows for the interface, built from `es-db` reads plus what is on disk. Field names and shapes
//! follow what the app already consumes.

use std::path::Path;

use es_db::{Library, PlaylistKind, Result};
use serde::Serialize;

pub mod detail;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRow {
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
    pub bpm: f64,
    pub duration: i64,
    pub rating: i64,
    pub color: i64,
    pub comment: String,
    pub year: i64,
    pub date_added: String,
    pub added_at: String,
    pub date_created: String,
    pub play_count: i64,
    #[serde(rename = "bitrate")]
    pub bit_rate: i64,
    pub sample_rate: i64,
    pub bit_depth: i64,
    pub file_type: String,
    pub stored_type: Option<&'static str>,
    pub path: String,
    pub missing: bool,
    pub streaming: bool,
    pub file_size: i64,
    pub has_artwork: bool,
    pub track_no: i64,
    pub analysed: bool,
    pub my_tags: Vec<String>,
    pub cue_count: i64,
    pub cue_stars: i64,
    pub phrase: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub name: String,
    pub parent_id: String,
    pub seq: i64,
    pub kind: &'static str,
    /// Present on playlists and histories; `null` for folders and smart playlists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[must_use]
pub fn stored_file_type(code: i64) -> Option<&'static str> {
    match code {
        1 => Some("MP3"),
        4 => Some("M4A"),
        5 => Some("FLAC"),
        11 => Some("WAV"),
        12 => Some("AIFF"),
        _ => None,
    }
}

/// A path that names a file on this computer, not a streaming service URI such as `tidal:track:1`.
#[must_use]
pub fn is_local(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let Some(colon) = path.find(':') else { return true };
    let scheme = &path[..colon];
    let looks_like_scheme = colon >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    !looks_like_scheme
}

/// Memory and hot cue counts. A loop (kind 4) is neither.
#[must_use]
pub fn cue_counts(kinds: &[i64]) -> (i64, i64) {
    let mut memory = 0;
    let mut hot = 0;
    for &kind in kinds {
        match kind {
            0 => memory += 1,
            1..=9 if kind != 4 => hot += 1,
            _ => {}
        }
    }
    (memory, hot)
}

fn stem(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn extension_upper(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default()
}

/// Live tracks as rows. `share` is the library's `share` folder, where analysis files live.
pub fn track_rows(lib: &Library, share: &Path, ids: Option<&[String]>) -> Result<Vec<TrackRow>> {
    let tracks = lib.tracks(ids)?;
    let kinds = lib.cue_kinds(ids)?;
    let tags = lib.my_tag_ids(ids)?;
    Ok(tracks
        .into_iter()
        .map(|t| {
            let phrase = t
                .analysis_path
                .as_deref()
                .and_then(|relative| es_anlz::file(share, relative, "EXT"))
                .is_some_and(|ext| es_anlz::has_tag(&ext, es_anlz::PHRASE_TAG));
            let (memory, hot) = cue_counts(kinds.get(&t.id).map_or(&[][..], Vec::as_slice));
            let local = is_local(&t.folder_path);
            let stored = stored_file_type(t.file_type);
            TrackRow {
                title: if t.title.is_empty() { stem(&t.folder_path) } else { t.title },
                color: t.color_id.parse().unwrap_or(0),
                bpm: t.bpm_x100 as f64 / 100.0,
                file_type: stored.map_or_else(|| extension_upper(&t.folder_path), str::to_owned),
                stored_type: stored,
                missing: local && !Path::new(&t.folder_path).is_file(),
                streaming: !t.folder_path.is_empty() && !local,
                has_artwork: !t.image_path.is_empty(),
                analysed: t.analysis_path.is_some(),
                my_tags: tags.get(&t.id).cloned().unwrap_or_default(),
                cue_count: memory + hot,
                cue_stars: i64::from(memory > 0) + i64::from(hot > 0) + i64::from(phrase),
                phrase,
                path: t.folder_path,
                id: t.id,
                artist: t.artist,
                album: t.album,
                genre: t.genre,
                label: t.label,
                remixer: t.remixer,
                original_artist: t.original_artist,
                composer: t.composer,
                key: t.key,
                duration: t.length,
                rating: t.rating,
                comment: t.comment,
                year: t.year,
                date_added: t.stock_date,
                added_at: t.created_at,
                date_created: t.date_created,
                play_count: t.play_count,
                bit_rate: t.bit_rate,
                sample_rate: t.sample_rate,
                bit_depth: t.bit_depth,
                file_size: t.file_size,
                track_no: t.track_no,
            }
        })
        .collect())
}

fn root_if_empty(parent: String) -> String {
    if parent.is_empty() { "root".into() } else { parent }
}

pub fn playlist_nodes(lib: &Library) -> Result<Vec<Node>> {
    Ok(lib
        .playlists()?
        .into_iter()
        .map(|p| Node {
            kind: match p.kind {
                PlaylistKind::Folder => "folder",
                PlaylistKind::Playlist => "playlist",
                PlaylistKind::Smart => "smart",
            },
            id: p.id,
            name: p.name,
            parent_id: root_if_empty(p.parent_id),
            seq: p.seq,
            count: Some(p.count),
            date: None,
        })
        .collect())
}

pub fn history_nodes(lib: &Library) -> Result<Vec<Node>> {
    Ok(lib
        .histories()?
        .into_iter()
        .map(|h| Node {
            kind: if h.is_folder { "folder" } else { "playlist" },
            id: h.id,
            name: h.name,
            parent_id: root_if_empty(h.parent_id),
            seq: h.seq,
            count: Some(h.count),
            date: Some(h.date),
        })
        .collect())
}

pub fn tag_nodes(lib: &Library) -> Result<Vec<Node>> {
    Ok(lib
        .my_tags()?
        .into_iter()
        .map(|t| Node {
            kind: if t.is_folder { "folder" } else { "tag" },
            id: t.id,
            name: t.name,
            parent_id: root_if_empty(t.parent_id),
            seq: t.seq,
            count: None,
            date: None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_uris_are_not_local_files() {
        assert!(is_local("/Users/a/b.mp3"));
        assert!(is_local("C:\\Music\\b.mp3"));
        assert!(!is_local("tidal:track:123"));
        assert!(!is_local(""));
    }

    #[test]
    fn loops_are_neither_memory_nor_hot_cues() {
        assert_eq!(cue_counts(&[0, 0, 1, 4, 5, 9, 12]), (2, 3));
    }
}
