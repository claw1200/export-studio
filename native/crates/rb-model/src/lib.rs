//! Rekordbox domain values without database, filesystem or application dependencies.

use serde::{Deserialize, Serialize};

pub mod source;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlaylistId(pub u64);

pub type RowIndex = u32;

pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f +00:00";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCc(pub [u8; 4]);

impl FourCc {
    pub const fn new(tag: &[u8; 4]) -> Self {
        Self(*tag)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).unwrap_or("????")
    }
}

impl core::fmt::Display for FourCc {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportMetadata {
    pub track_number: u32,
    pub disc_number: u16,
    pub bit_depth: u16,
    pub play_count: u32,
    pub analysed: u32,
    pub hot_cue_auto_load: bool,
    pub date_created: String,
    pub isrc: String,
}
