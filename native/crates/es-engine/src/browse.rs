//! Track-shaped rows for files that are not in the collection.

use std::path::Path;

use serde_json::{json, Value};

/// A row for one file: its own tags when they read, and its name when they don't.
#[must_use]
pub fn file_row(path: &Path) -> Value {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut suffix = path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
    if suffix == "AIF" {
        suffix = "AIFF".into();
    }
    let is_file = path.is_file();
    let size = if is_file { path.metadata().map_or(0, |m| m.len()) } else { 0 };
    let mut row = json!({
        "title": stem, "artist": "", "album": "", "genre": "", "label": "", "remixer": "", "originalArtist": "",
        "composer": "", "key": "", "bpm": 0, "duration": 0, "rating": 0, "color": 0, "comment": "", "year": 0,
        "dateAdded": "", "dateCreated": "", "playCount": 0, "bitrate": 0, "sampleRate": 0, "bitDepth": 0,
        "fileType": suffix, "storedType": null, "path": path.to_string_lossy().replace('\\', "/"),
        "missing": !is_file, "streaming": false, "fileSize": size, "hasArtwork": false, "trackNo": 0,
        "analysed": false, "myTags": [], "cueStars": 0, "cueCount": 0, "phrase": false, "collected": false,
    });
    let Ok(info) = es_tags::read(path) else { return row };
    let f = &info.fields;
    let set = |row: &mut Value, key: &str, value: Value| row[key] = value;
    if !f.title.is_empty() {
        set(&mut row, "title", json!(f.title));
    }
    for (key, value) in [
        ("artist", &f.artist), ("album", &f.album), ("genre", &f.genre), ("label", &f.label), ("remixer", &f.remixer),
        ("originalArtist", &f.original_artist), ("composer", &f.composer), ("comment", &f.comment), ("key", &info.key),
    ] {
        set(&mut row, key, json!(value));
    }
    set(&mut row, "year", json!(f.year));
    set(&mut row, "trackNo", json!(f.track_no));
    set(&mut row, "bpm", if info.bpm == 0.0 { json!(0) } else { json!(info.bpm) });
    set(&mut row, "duration", json!(info.length));
    set(&mut row, "bitrate", json!(info.bitrate));
    set(&mut row, "sampleRate", json!(info.sample_rate));
    set(&mut row, "bitDepth", json!(info.bit_depth));
    row
}

/// Rows for up to 80 files.
#[must_use]
pub fn file_rows(paths: &[String]) -> Vec<Value> {
    paths.iter().take(80).map(|p| file_row(Path::new(p))).collect()
}
