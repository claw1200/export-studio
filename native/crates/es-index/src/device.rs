//! A stick's tracks and playlists in the shapes the interface shows, and its tracks' detail.

use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rb_anlz::decode::{self, Tags};
use rb_device::paths::volume_path;
use rb_device::{Cue as DeviceCue, Device, LibraryKind, Playlist, Track};
use serde::Serialize;

use crate::detail::{Bands, Colour, Cue, Detail, Grid, Mono, Preview, Waveform};
use crate::{cue_counts, stored_file_type, Node, TrackRow};

/// The key the interface uses for a track on a stick: `usb:<volume>:<id>`, the volume percent-encoded.
#[must_use]
pub fn track_key(volume: &str, export_id: i64) -> String {
    let mut encoded = String::new();
    for byte in volume.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("usb:{encoded}:{export_id}")
}

fn decode_percent(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The volume and export id a track key names.
#[must_use]
pub fn parse_track_key(key: &str) -> Option<(String, i64)> {
    let rest = key.strip_prefix("usb:")?;
    let (volume, id) = rest.split_once(':')?;
    if volume.is_empty() {
        return None;
    }
    Some((decode_percent(volume)?, id.parse().ok()?))
}

fn audio_path(volume: &Path, track: &Track) -> Option<std::path::PathBuf> {
    volume_path(&track.path, volume).ok()
}

fn stem(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn track_row(volume: &Path, track: &Track) -> TrackRow {
    let audio = audio_path(volume, track);
    let path = audio.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let kinds: Vec<i64> = track.cues.iter().map(|c| c.kind).collect();
    let (memory, hot) = cue_counts(&kinds);
    let phrase = !track.analysis.is_empty()
        && volume_path(&track.analysis, volume)
            .ok()
            .is_some_and(|base| rb_anlz::has_tag(&base.with_extension("EXT"), rb_anlz::PHRASE_TAG));
    let name_source = if track.filename.is_empty() { &track.path } else { &track.filename };
    let stored = stored_file_type(track.file_type);
    TrackRow {
        id: track_key(&volume.to_string_lossy(), track.export_id),
        title: if track.title.is_empty() { Some(stem(name_source)).filter(|s| !s.is_empty()).unwrap_or_else(|| "Untitled".into()) } else { track.title.clone() },
        artist: track.artist.clone(),
        album: track.album.clone(),
        genre: track.genre.clone(),
        label: track.label.clone(),
        remixer: String::new(),
        original_artist: String::new(),
        composer: String::new(),
        key: track.key.clone(),
        bpm: track.bpm_x100 as f64 / 100.0,
        duration: track.duration,
        rating: track.rating,
        color: track.color,
        comment: track.comment.clone(),
        year: track.year,
        date_added: track.date_added.clone(),
        added_at: String::new(),
        date_created: String::new(),
        play_count: track.play_count,
        bit_rate: track.bitrate,
        sample_rate: track.sample_rate,
        bit_depth: track.bit_depth,
        file_type: stored.map_or_else(|| Path::new(name_source).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default(), str::to_owned),
        stored_type: stored,
        missing: audio.as_ref().is_none_or(|p| !p.is_file()),
        streaming: false,
        path,
        file_size: track.file_size,
        has_artwork: artwork_file(volume, track, "").is_some(),
        track_no: track.track_no,
        analysed: !track.analysis.is_empty(),
        my_tags: Vec::new(),
        cue_count: memory + hot,
        cue_stars: i64::from(memory > 0) + i64::from(hot > 0) + i64::from(phrase),
        phrase,
    }
}

fn playlist_node(row: &Playlist) -> Node {
    Node {
        id: row.export_id.to_string(),
        name: row.name.clone(),
        parent_id: if row.parent_id == 0 { "root".into() } else { row.parent_id.to_string() },
        seq: row.seq,
        kind: if row.folder { "folder" } else { "playlist" },
        count: Some((!row.folder).then_some(i64::try_from(row.track_ids.len()).unwrap_or(0))),
        date: None,
    }
}

pub fn playlists(device: &Device) -> Vec<Node> {
    device.playlists.iter().map(playlist_node).collect()
}

/// A library on a stick, with its own playlist tree.
#[derive(Debug, Serialize)]
pub struct LibraryView {
    pub id: &'static str,
    pub name: &'static str,
    pub playlists: Vec<Node>,
}

#[must_use]
pub fn library_view(kind: LibraryKind, device: &Device) -> LibraryView {
    match kind {
        LibraryKind::DeviceLibrary => LibraryView { id: "device", name: "Device Library", playlists: playlists(device) },
        LibraryKind::OneLibrary => LibraryView { id: "one", name: "OneLibrary", playlists: playlists(device) },
    }
}

/// Tracks of a stick, or of one of its playlists.
pub fn tracks(volume: &Path, device: &Device, playlist_id: Option<&str>) -> Result<Vec<TrackRow>, String> {
    let chosen: Vec<&Track> = match playlist_id.filter(|p| !p.is_empty() && *p != "root") {
        None => device.tracks.iter().collect(),
        Some(id) => {
            let playlist = device
                .playlists
                .iter()
                .find(|p| p.export_id.to_string() == id)
                .ok_or_else(|| format!("Unknown device playlist {id}"))?;
            playlist.track_ids.iter().filter_map(|t| device.track(*t)).collect()
        }
    };
    Ok(chosen.into_iter().map(|t| track_row(volume, t)).collect())
}

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn device_cues(track: &Track, ext: Option<&Tags>) -> Vec<Cue> {
    let colors = ext.map(decode::hot_cue_colors).unwrap_or_default();
    let mut out: Vec<Cue> = track
        .cues
        .iter()
        .enumerate()
        .map(|(index, c): (usize, &DeviceCue)| {
            let hot = c.kind > 0;
            let mut cue = Cue {
                id: format!("{}:{}", track.export_id, index + 1),
                kind: if hot { "hot" } else { "memory" },
                slot: c.kind,
                start: c.start_ms,
                end: c.end_ms,
                color: None,
                color_index: c.color_index,
                color_code: None,
                comment: c.comment.clone(),
                rgb: None,
            };
            cue.apply_analysis_color(&colors);
            cue
        })
        .collect();
    out.sort_by_key(|c| (c.kind != "hot", c.slot, c.start));
    out
}

/// A stick track's cues, beat grid and waveforms.
pub fn detail(volume: &Path, key: &str, track: &Track) -> Detail {
    let base = (!track.analysis.is_empty()).then(|| volume_path(&track.analysis, volume).ok()).flatten();
    let read = |suffix: &str| base.as_ref().and_then(|b| Tags::read(&b.with_extension(suffix)).ok());
    let (dat, ext, ex2) = (read("DAT"), read("EXT"), read("2EX"));
    let rgb = ext.as_ref().and_then(decode::rgb);
    let bands = |kind: &str| ex2.as_ref().and_then(|t| decode::bands(t, kind)).map(|b| Bands { length: b.length, bands: b64(&b.bands) });
    let audio = audio_path(volume, track);
    Detail {
        id: key.to_owned(),
        file_exists: audio.as_ref().is_some_and(|p| p.is_file()),
        cues: device_cues(track, ext.as_ref()),
        beatgrid: dat.as_ref().and_then(decode::beatgrid).map(|g| Grid { times: g.times, beats: g.beats, bpms: g.bpms }),
        waveform: Waveform {
            rate: 150,
            three_band: bands("PWV7"),
            three_band_preview: bands("PWV6"),
            mono: if rgb.is_some() { None } else { ext.as_ref().and_then(decode::mono).map(|m| Mono { length: m.length, heights: b64(&m.heights), whiteness: b64(&m.whiteness) }) },
            rgb: rgb.map(|r| Colour { length: r.length, heights: b64(&r.heights), colors: b64(&r.colors) }),
        },
    }
}

/// The audio file of a stick track, if it is there.
#[must_use]
pub fn audio_file(volume: &Path, track: &Track) -> Option<String> {
    audio_path(volume, track).filter(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}

pub fn artwork_file(volume: &Path, track: &Track, size: &str) -> Option<std::path::PathBuf> {
    let relative = track.artwork.replace('\\', "/");
    volume_path(&relative, volume).ok()?;
    let path = crate::media::art(volume, &relative, size)?;
    volume_path(path.strip_prefix(volume).ok()?.to_str()?, volume).ok()
}

/// Previews for stick tracks; each key's stick is read once.
pub fn previews(read: &mut dyn FnMut(&Path) -> Option<std::sync::Arc<Device>>, keys: &[String]) -> Vec<Preview> {
    let mut seen: Vec<&String> = Vec::new();
    let mut out = Vec::new();
    for key in keys {
        if seen.contains(&key) || seen.len() >= 80 {
            continue;
        }
        seen.push(key);
        let found = parse_track_key(key).and_then(|(volume, export_id)| {
            let volume = std::path::PathBuf::from(volume);
            let device = read(&volume)?;
            let track = device.track(export_id)?;
            let base = (!track.analysis.is_empty()).then(|| volume_path(&track.analysis, &volume).ok()).flatten();
            let tags = |suffix: &str| base.as_ref().and_then(|b| Tags::read(&b.with_extension(suffix)).ok());
            Some(Preview::build(key, device_cues(track, None), tags("EXT").as_ref(), tags("2EX").as_ref()))
        });
        out.push(found.unwrap_or_else(|| Preview::blank(key)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_artwork_is_available_to_the_interface() {
        let dir = tempfile::tempdir().unwrap();
        let relative = "/PIONEER/Artwork/cover.jpg";
        let original = dir.path().join("PIONEER/Artwork/cover.jpg");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::write(&original, b"jpeg").unwrap();
        let track = Track { artwork: relative.into(), ..Track::default() };
        assert!(track_row(dir.path(), &track).has_artwork);
        assert_eq!(artwork_file(dir.path(), &track, "m"), Some(original.clone()));
        let thumbnail = original.with_file_name("cover_m.jpg");
        std::fs::write(&thumbnail, b"thumbnail").unwrap();
        assert_eq!(artwork_file(dir.path(), &track, "m"), Some(thumbnail));
        assert_eq!(artwork_file(dir.path(), &track, ""), Some(original.clone()));
        let windows_path = Track { artwork: relative.replace('/', "\\"), ..Track::default() };
        assert_eq!(artwork_file(dir.path(), &windows_path, ""), Some(original.clone()));
        std::fs::remove_file(original).unwrap();
        assert!(!track_row(dir.path(), &track).has_artwork);
        assert_eq!(artwork_file(dir.path(), &track, ""), None);
    }

    #[test]
    fn device_artwork_stays_inside_the_volume() {
        let dir = tempfile::tempdir().unwrap();
        for relative in ["", "/../cover.jpg", "/PIONEER/../../cover.jpg"] {
            let track = Track { artwork: relative.into(), ..Track::default() };
            assert!(!track_row(dir.path(), &track).has_artwork);
            assert_eq!(artwork_file(dir.path(), &track, "m"), None);
        }
    }

    #[test]
    fn track_keys_round_trip_with_awkward_volume_names() {
        let key = track_key("/Volumes/MY STICK & more", 42);
        assert_eq!(key, "usb:%2FVolumes%2FMY%20STICK%20%26%20more:42");
        assert_eq!(parse_track_key(&key), Some(("/Volumes/MY STICK & more".to_owned(), 42)));
        assert_eq!(parse_track_key("tidal:track:1"), None);
        assert_eq!(parse_track_key("usb::5"), None);
    }
}
