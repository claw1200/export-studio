//! Prints what `read` finds in each file named on stdin, one JSON object per line.

use std::io::BufRead;
use std::path::Path;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let path = line.unwrap();
        let out = match es_tags::read(Path::new(&path)) {
            Ok(info) => serde_json::json!({
                "path": path,
                "title": info.fields.title, "artist": info.fields.artist, "album": info.fields.album,
                "genre": info.fields.genre, "label": info.fields.label, "remixer": info.fields.remixer,
                "originalArtist": info.fields.original_artist, "composer": info.fields.composer,
                "comment": info.fields.comment, "year": info.fields.year, "trackNo": info.fields.track_no,
                "length": info.length, "bitrate": info.bitrate, "sampleRate": info.sample_rate,
                "bitDepth": info.bit_depth, "bpm": info.bpm, "key": info.key,
                "artwork": info.artwork.map(|a| a.len()),
            }),
            Err(e) => serde_json::json!({ "path": path, "error": e.to_string() }),
        };
        println!("{out}");
    }
}
