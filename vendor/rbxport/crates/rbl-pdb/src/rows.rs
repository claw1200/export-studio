//! Row encoders for `export.pdb`.
//!
//! Field offsets mirror `crate::TrackRow` and friends exactly, because the
//! reader is what verifies the writer: anything written here is read back with
//! the same layout, and the layout itself was validated against a real
//! rekordbox-authored export.

use crate::build::device_sql_string;

/// Fixed portion of a track row, before the 21 string offsets.
const TRACK_FIXED_LEN: usize = 0x5e;
/// Number of string slots at the end of a track row.
pub const TRACK_STRINGS: usize = 21;

/// Slot numbers within a track row's string block.
pub mod slot {
    pub const ISRC: usize = 0;
    pub const HOT_CUE_AUTO_LOAD: usize = 7;
    pub const DATE_ADDED: usize = 10;
    pub const RELEASE_DATE: usize = 11;
    pub const MIX_NAME: usize = 12;
    pub const ANALYZE_PATH: usize = 14;
    pub const ANALYZE_DATE: usize = 15;
    pub const COMMENT: usize = 16;
    pub const TITLE: usize = 17;
    pub const FILENAME: usize = 19;
    pub const FILE_PATH: usize = 20;
}

/// Everything needed to write one track row.
#[derive(Debug, Clone, Default)]
pub struct TrackInput {
    pub hot_cue_auto_load: bool,
    pub id: u32,
    pub artist_id: u32,
    pub album_id: u32,
    pub genre_id: u32,
    pub key_id: u32,
    pub label_id: u32,
    pub artwork_id: u32,
    pub color_id: u8,
    pub rating: u8,
    pub tempo_x100: u32,
    pub duration_sec: u16,
    pub year: u16,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub file_size: u32,
    pub track_number: u32,
    pub play_count: u16,
    pub disc_number: u16,
    pub sample_depth: u16,
    pub title: String,
    pub filename: String,
    /// Media-relative, e.g. `/Contents/Artist/Album/Track.mp3`.
    pub file_path: String,
    /// Media-relative, e.g. `/PIONEER/USBANLZ/P001/00000001/ANLZ0000.DAT`.
    pub analyze_path: String,
    pub comment: String,
    pub date_added: String,
    pub release_date: String,
    pub mix_name: String,
    pub isrc: String,
}

fn put_u2(row: &mut [u8], at: usize, v: u16) {
    if let Some(slice) = row.get_mut(at..at + 2) {
        slice.copy_from_slice(&v.to_le_bytes());
    }
}
fn put_u4(row: &mut [u8], at: usize, v: u32) {
    if let Some(slice) = row.get_mut(at..at + 4) {
        slice.copy_from_slice(&v.to_le_bytes());
    }
}

/// Encodes a track row.
///
/// The string block holds offsets relative to the start of the row, and the
/// strings themselves follow it. Every slot must point somewhere valid, so
/// unused slots point at a single shared empty string rather than at zero —
/// a zero offset reads back as an empty string too, but real files always point
/// at something and matching that keeps players on the path they exercise.
pub fn track_row(input: &TrackInput) -> Vec<u8> {
    let mut strings: Vec<String> = vec![String::new(); TRACK_STRINGS];
    let set = |strings: &mut Vec<String>, slot: usize, value: &str| {
        if let Some(entry) = strings.get_mut(slot) {
            entry.clear();
            entry.push_str(value);
        }
    };
    set(&mut strings, slot::ISRC, &input.isrc);
    set(&mut strings, slot::HOT_CUE_AUTO_LOAD, if input.hot_cue_auto_load { "ON" } else { "" });
    set(&mut strings, slot::DATE_ADDED, &input.date_added);
    set(&mut strings, slot::RELEASE_DATE, &input.release_date);
    set(&mut strings, slot::MIX_NAME, &input.mix_name);
    set(&mut strings, slot::ANALYZE_PATH, &input.analyze_path);
    set(&mut strings, slot::ANALYZE_DATE, "");
    set(&mut strings, slot::COMMENT, &input.comment);
    set(&mut strings, slot::TITLE, &input.title);
    set(&mut strings, slot::FILENAME, &input.filename);
    set(&mut strings, slot::FILE_PATH, &input.file_path);

    let block_len = TRACK_STRINGS * 2;
    let mut row = vec![0_u8; TRACK_FIXED_LEN + block_len];

    // These are part of the record layout, not optional metadata. A zero
    // subtype makes the CDJ-3000 ignore the tracks; zero trailer words leave
    // its string columns misread. Pinned against rekordbox's MP3 export and
    // CDJ-3000 firmware browse/load tests (../rbxport-private/docs/audits/usb-track-records.md).
    put_u2(&mut row, 0x00, 0x24); // track record with 16-bit string offsets
    put_u2(&mut row, 0x02, 0); // index_shift
    put_u4(&mut row, 0x04, 0); // bitmask
    put_u4(&mut row, 0x08, input.sample_rate);
    put_u4(&mut row, 0x0c, 0); // composer_id
    put_u4(&mut row, 0x10, input.file_size);
    put_u4(&mut row, 0x1c, input.artwork_id);
    put_u4(&mut row, 0x20, input.key_id);
    put_u4(&mut row, 0x24, 0); // original_artist_id
    put_u4(&mut row, 0x28, input.label_id);
    put_u4(&mut row, 0x2c, 0); // remixer_id
    put_u4(&mut row, 0x30, input.bitrate);
    put_u4(&mut row, 0x34, input.track_number);
    put_u4(&mut row, 0x38, input.tempo_x100);
    put_u4(&mut row, 0x3c, input.genre_id);
    put_u4(&mut row, 0x40, input.album_id);
    put_u4(&mut row, 0x44, input.artist_id);
    put_u4(&mut row, 0x48, input.id);
    put_u2(&mut row, 0x4c, input.disc_number);
    put_u2(&mut row, 0x4e, input.play_count);
    put_u2(&mut row, 0x50, input.year);
    put_u2(&mut row, 0x52, input.sample_depth);
    put_u2(&mut row, 0x54, input.duration_sec);
    put_u2(&mut row, 0x56, 0x29);
    if let Some(b) = row.get_mut(0x58) {
        *b = input.color_id;
    }
    if let Some(b) = row.get_mut(0x59) {
        *b = input.rating;
    }
    put_u2(&mut row, 0x5a, audio_file_type(&input.filename));
    put_u2(&mut row, 0x5c, 3);

    // Append each string, recording where it landed.
    let mut offsets = [0_u16; TRACK_STRINGS];
    for (slot, text) in strings.iter().enumerate() {
        let at = u16::try_from(row.len()).unwrap_or(0);
        if let Some(entry) = offsets.get_mut(slot) {
            *entry = at;
        }
        row.extend_from_slice(&device_sql_string(text));
    }
    for (slot, offset) in offsets.iter().enumerate() {
        put_u2(&mut row, TRACK_FIXED_LEN + slot * 2, *offset);
    }

    row
}

/// `DeviceSQL`'s audio format code, from the exported filename (which can
/// differ from the source after compatibility conversion).
pub fn audio_file_type(filename: &str) -> u16 {
    let extension = std::path::Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    match extension.to_ascii_lowercase().as_str() {
        "mp3" => 1,
        "m4a" | "mp4" | "aac" => 4,
        "flac" => 5,
        "wav" => 11,
        "aif" | "aiff" => 12,
        _ => 0,
    }
}

/// `genres` and `labels`: u4 id then an inline string.
pub fn simple_named_row(id: u32, name: &str) -> Vec<u8> {
    let mut row = id.to_le_bytes().to_vec();
    row.extend_from_slice(&device_sql_string(name));
    row
}

/// `artwork`: u4 id then the media-relative path of the image, e.g.
/// `/PIONEER/Artwork/00001/a1.jpg` — the same shape as a genre row.
pub fn artwork_row(id: u32, path: &str) -> Vec<u8> {
    simple_named_row(id, path)
}

/// `keys`: the id appears twice.
pub fn key_row(id: u32, name: &str) -> Vec<u8> {
    let mut row = id.to_le_bytes().to_vec();
    row.extend_from_slice(&id.to_le_bytes());
    row.extend_from_slice(&device_sql_string(name));
    row
}

/// `colors`: four zero bytes, the id as one byte, the id again as two, one
/// pad byte, then the name [OBS 7.2.11: `00 00 00 00 01 01 00 00 0b "Pink"`,
/// `00 00 00 00 02 02 00 00 09 "Red"`].
pub fn color_row(id: u16, name: &str) -> Vec<u8> {
    let mut row = vec![0_u8; 8];
    row[4] = u8::try_from(id).unwrap_or(0);
    put_u2(&mut row, 5, id);
    row.extend_from_slice(&device_sql_string(name));
    row
}

/// `artists`: the name is located by a one-byte offset from the row start.
pub fn artist_row(id: u32, name: &str) -> Vec<u8> {
    let mut row = vec![0_u8; 10];
    put_u2(&mut row, 0x00, 0x60); // subtype: near offset
    put_u4(&mut row, 0x04, id);
    row[0x08] = 0x03; // constant rekordbox writes
    row[0x09] = 0x0a; // the name follows immediately
    row.extend_from_slice(&device_sql_string(name));
    row
}

/// `albums`: like artists, with an artist reference and a wider prefix.
///
/// The row rekordbox 7.2.11 writes for album 1, "Freak EP", by no artist
/// [OBS 2026-09-17]:
///
/// ```text
///   80 00  00 00  00 00 00 00  00 00 00 00  01 00 00 00  00 00 00 00  03  16  13 "Freak EP"
///   type   shift  (zero)       artist       id           (zero)       03  ofs name
/// ```
///
/// The id is the fourth word and the artist the third; an earlier version
/// of this writer had them the other way round, which a player reads as
/// every album having id 0.
pub fn album_row(id: u32, artist_id: u32, name: &str) -> Vec<u8> {
    let mut row = vec![0_u8; ALBUM_NAME_AT];
    put_u2(&mut row, 0x00, 0x80);
    put_u4(&mut row, 0x08, artist_id);
    put_u4(&mut row, 0x0c, id);
    row[0x14] = 0x03; // constant rekordbox writes
    row[0x15] = u8::try_from(ALBUM_NAME_AT).unwrap_or(0x16); // the name follows immediately
    row.extend_from_slice(&device_sql_string(name));
    row
}

/// Where an `albums` row's id sits, and where its name starts.
pub const ALBUM_ID_AT: usize = 0x0c;
pub const ALBUM_NAME_AT: usize = 0x16;

/// `playlist_tree`: parent, sort order, id, folder flag, then the name.
/// `playlist_tree`: parent, a word rekordbox leaves at zero, sort order, id,
/// folder flag, then the name at offset 20.
///
/// The row rekordbox 7.2.11 writes for a playlist called NP3-TEST-MP3 with
/// id 1 at the root [OBS 2026-09-17]:
///
/// ```text
///   00 00 00 00  00 00 00 00  00 00 00 00  01 00 00 00  00 00 00 00  1b "NP3-TEST-MP3"
///   parent       (zero)       sort order   id           folder       name
/// ```
///
/// Five words, not four: the zero word between the parent and the sort
/// order is in every rekordbox row, and a player reads the name from
/// offset 20. rekordcrate calls it `unknown`.
pub fn playlist_row(id: u32, parent_id: u32, sort_order: u32, is_folder: bool, name: &str) -> Vec<u8> {
    let mut row = Vec::with_capacity(PLAYLIST_NAME_AT + name.len() + 2);
    row.extend_from_slice(&parent_id.to_le_bytes());
    row.extend_from_slice(&0_u32.to_le_bytes());
    row.extend_from_slice(&sort_order.to_le_bytes());
    row.extend_from_slice(&id.to_le_bytes());
    row.extend_from_slice(&u32::from(is_folder).to_le_bytes());
    row.extend_from_slice(&device_sql_string(name));
    row
}

/// Where the name starts in a `playlist_tree` row.
pub const PLAYLIST_NAME_AT: usize = 20;

/// `playlist_entries`: position, track, playlist.
pub fn playlist_entry_row(entry_index: u32, track_id: u32, playlist_id: u32) -> Vec<u8> {
    let mut row = Vec::with_capacity(12);
    row.extend_from_slice(&entry_index.to_le_bytes());
    row.extend_from_slice(&track_id.to_le_bytes());
    row.extend_from_slice(&playlist_id.to_le_bytes());
    row
}
