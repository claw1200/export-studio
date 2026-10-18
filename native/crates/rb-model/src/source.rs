//! Caller-supplied library records, separate from export preparation and device state.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub db_id: u64,
    pub automatic: bool,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub my_tags: Vec<Tag>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: u64,
    pub source_path: PathBuf,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub genre: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub date_added: String,
    #[serde(default)]
    pub release_date: String,
    #[serde(default)]
    pub bpm_x100: u32,
    #[serde(default = "one_second")]
    pub duration_sec: u32,
    #[serde(default)]
    pub rating: u32,
    #[serde(default)]
    pub color_id: u32,
    #[serde(default)]
    pub year: u32,
    #[serde(default)]
    pub bitrate: u32,
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default)]
    pub file_size: u64,
    #[serde(default)]
    pub analysis: Vec<PathBuf>,
    #[serde(default)]
    pub artwork: Option<PathBuf>,
    #[serde(default)]
    pub my_tags: Vec<u64>,
    #[serde(default)]
    pub metadata: Metadata,
    pub cues: Option<Vec<Cue>>,
}

fn one_second() -> u32 {
    1
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub track_number: u32,
    #[serde(default)]
    pub disc_number: u16,
    #[serde(default)]
    pub bit_depth: u16,
    #[serde(default)]
    pub play_count: u32,
    #[serde(default)]
    pub analysed: u32,
    #[serde(default = "yes")]
    pub hot_cue_auto_load: bool,
    #[serde(default)]
    pub date_created: String,
    #[serde(default)]
    pub isrc: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    #[serde(default)]
    pub kind: u8,
    #[serde(default)]
    pub time_ms: u32,
    pub end_ms: Option<i64>,
    pub color: Option<i64>,
    #[serde(default)]
    pub color_code: u8,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub active_loop: bool,
    #[serde(default)]
    pub beat_loop_size: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub parent_id: u64,
    #[serde(default)]
    pub folder: bool,
    #[serde(default)]
    pub track_ids: Vec<u64>,
    #[serde(default)]
    pub entries_changed_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: u64,
    #[serde(default)]
    pub seq: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub attribute: u8,
    #[serde(default)]
    pub parent: u64,
}
