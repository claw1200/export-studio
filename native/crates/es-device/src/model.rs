//! A stick's library as plain data, whichever database format it was read from.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryKind {
    /// `exportLibrary.db`, the SQLCipher database (OneLibrary / Device Library Plus).
    OneLibrary,
    /// `export.pdb`, the DeviceSQL database older players read.
    DeviceLibrary,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cue {
    /// 0 is a memory cue, 1 and up are hot cues and loops.
    pub kind: i64,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub color_index: Option<i64>,
    pub comment: String,
    pub active_loop: bool,
    /// `(numerator << 16) | denominator`.
    pub beat_loop: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    pub export_id: i64,
    /// The desktop library's id for this track, and the id of that library, when the stick records them.
    pub master_id: Option<i64>,
    pub master_db_id: Option<i64>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub key: String,
    /// Path on the stick, from its root.
    pub path: String,
    pub filename: String,
    pub analysis: String,
    pub comment: String,
    pub bpm_x100: i64,
    pub duration: i64,
    pub bitrate: i64,
    pub sample_rate: i64,
    pub bit_depth: i64,
    pub file_type: i64,
    pub file_size: i64,
    pub rating: i64,
    pub color: i64,
    pub year: i64,
    pub track_no: i64,
    pub play_count: i64,
    pub hot_cue_auto: bool,
    pub date_added: String,
    pub cues: Vec<Cue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Playlist {
    pub export_id: i64,
    pub name: String,
    pub parent_id: i64,
    pub seq: i64,
    pub folder: bool,
    pub track_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct History {
    pub export_id: i64,
    pub name: String,
    pub parent_id: i64,
    pub seq: i64,
    pub attribute: i64,
    pub track_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub kind: LibraryKind,
    pub root_name: String,
    pub device_name: String,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub histories: Vec<History>,
}

impl Device {
    #[must_use]
    pub fn track(&self, export_id: i64) -> Option<&Track> {
        self.tracks.iter().find(|t| t.export_id == export_id)
    }
}
