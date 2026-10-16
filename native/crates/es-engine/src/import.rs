//! Adding audio files to the collection.

use std::collections::BTreeSet;
use std::path::Path;

use es_db::{FileFacts, Library, NewTrack, Stream, TrackEdit};
use uuid::Uuid;

use crate::{artwork, Result};

const SUFFIXES: [&str; 6] = ["mp3", "m4a", "flac", "wav", "aif", "aiff"];

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub artwork: bool,
    /// A normal playlist to put every imported track in.
    pub playlist: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Every track the files now are, in order, including ones already in the collection.
    pub ids: Vec<String>,
    /// Tracks created or brought back by this import.
    pub added: Vec<String>,
    pub skipped: usize,
    pub failed: Vec<Failure>,
}

/// rekordbox's file type code for an audio file extension.
#[must_use]
pub fn file_type_code(extension: &str) -> Option<i64> {
    match extension.to_ascii_lowercase().as_str() {
        "mp3" => Some(1),
        "m4a" => Some(4),
        "flac" => Some(5),
        "wav" => Some(11),
        "aif" | "aiff" => Some(12),
        _ => None,
    }
}

struct Wanted {
    stored: String,
    facts: FileFacts,
    info: es_tags::Info,
    /// A new row's id for artwork, when the file has no row yet.
    uuid: Option<String>,
    image_path: Option<String>,
}

fn nonzero(value: i64) -> Option<i64> {
    (value != 0).then_some(value)
}

fn stream_of(info: &es_tags::Info) -> Stream {
    Stream {
        length: nonzero(info.length),
        bit_rate: nonzero(info.bitrate),
        sample_rate: nonzero(info.sample_rate),
        bit_depth: nonzero(info.bit_depth),
    }
}

fn edit_of(info: &es_tags::Info, stem: &str) -> TrackEdit {
    let f = &info.fields;
    let some = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    let bounded = |n: u32| (1..=9999).contains(&n).then_some(i64::from(n));
    TrackEdit {
        title: Some(some(&f.title).unwrap_or_else(|| stem.to_owned())),
        comment: some(&f.comment),
        year: bounded(f.year),
        track_no: bounded(f.track_no),
        artist: some(&f.artist),
        remixer: some(&f.remixer),
        original_artist: some(&f.original_artist),
        composer: some(&f.composer),
        album: some(&f.album),
        genre: some(&f.genre),
        label: some(&f.label),
        ..TrackEdit::default()
    }
}

/// Add files to the collection, and optionally to the end of a playlist.
///
/// A file already in the collection is not added twice, but still goes in the playlist. A removed
/// track comes back as the same row. Files that cannot be imported are reported, not fatal.
pub fn import_files(lib: &mut Library, share: &Path, paths: &[String], options: &Options) -> Result<Outcome> {
    let mut outcome = Outcome::default();
    let mut seen = BTreeSet::new();
    let mut wanted = Vec::new();
    for raw in paths {
        if !seen.insert(raw.clone()) {
            continue;
        }
        let path = Path::new(raw);
        let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if !SUFFIXES.contains(&extension.as_str()) {
            let shown = if extension.is_empty() { "this".to_owned() } else { format!(".{extension}") };
            outcome.failed.push(Failure { path: raw.clone(), error: format!("rekordbox can't import {shown} files") });
            continue;
        }
        let Some(size) = path.is_absolute().then(|| std::fs::metadata(path).ok().filter(|m| m.is_file())).flatten() else {
            outcome.failed.push(Failure { path: raw.clone(), error: "File not found".into() });
            continue;
        };
        let stored = raw.replace('\\', "/");
        let rows = lib.tracks_at(&stored)?;
        let has_live = rows.iter().any(|(_, removed)| !removed);
        let info = if has_live {
            es_tags::Info::default()
        } else {
            es_tags::read(path).unwrap_or_else(|e| {
                eprintln!("[es-engine] could not read tags from {raw}: {e}");
                es_tags::Info::default()
            })
        };
        let is_new = rows.is_empty();
        let uuid = is_new.then(|| Uuid::new_v4().to_string());
        let image_path = match (&uuid, options.artwork, &info.artwork) {
            (Some(uuid), true, Some(cover)) => match artwork::save(cover, share, uuid) {
                Ok(relative) => Some(relative),
                Err(e) => {
                    eprintln!("[es-engine] could not save artwork for {raw}: {e}");
                    None
                }
            },
            _ => None,
        };
        wanted.push(Wanted {
            facts: FileFacts {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                size: i64::try_from(size.len()).unwrap_or(i64::MAX),
                file_type: file_type_code(&extension).unwrap_or(0),
            },
            stored,
            info,
            uuid,
            image_path,
        });
    }
    if wanted.is_empty() {
        return Ok(outcome);
    }

    let playlist = options.playlist.as_deref();
    let done = lib.write(|tx| {
        let mut ids = Vec::new();
        let mut added = Vec::new();
        let mut skipped = 0;
        for item in &wanted {
            let rows = tx.tracks_at(&item.stored)?;
            if let Some((id, _)) = rows.iter().find(|(_, removed)| !removed) {
                skipped += 1;
                ids.push(id.clone());
                continue;
            }
            let stem = Path::new(&item.facts.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let id = if let Some((id, _)) = rows.first() {
                tx.revive_track(id, &item.facts, &stream_of(&item.info))?;
                id.clone()
            } else {
                tx.create_track(&NewTrack {
                    uuid: item.uuid.clone().unwrap_or_else(|| Uuid::new_v4().to_string()),
                    path: item.stored.clone(),
                    file_name: item.facts.name.clone(),
                    file_size: item.facts.size,
                    file_type: item.facts.file_type,
                    stream: stream_of(&item.info),
                    edit: edit_of(&item.info, &stem),
                    image_path: item.image_path.clone(),
                })?
            };
            added.push(id.clone());
            ids.push(id);
        }
        if let Some(playlist) = playlist {
            let mut seen = BTreeSet::new();
            let ordered: Vec<String> = ids.iter().filter(|id| seen.insert((*id).clone())).cloned().collect();
            tx.add_to_playlist(playlist, &ordered)?;
        }
        Ok((ids, added, skipped))
    })?;
    let (ids, added, skipped) = done;
    let mut seen = BTreeSet::new();
    outcome.ids = ids.into_iter().filter(|id| seen.insert(id.clone())).collect();
    outcome.added = added;
    outcome.skipped = skipped;
    Ok(outcome)
}
