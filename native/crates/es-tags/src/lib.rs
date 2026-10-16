//! Reading the tags inside audio files, as the fields rekordbox tracks carry. Writing is still done by
//! the Python engine: lofty does not yet write the year and original artist everywhere, and its WAV
//! handling duplicates an existing `id3 ` chunk.

use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use lofty::tag::{Tag, TagType};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Tag(#[from] lofty::error::LoftyError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("File not found: {0}")]
    Missing(String),
    #[error("Unsupported file type: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The text and number fields a track has. Empty text and 0 mean not set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub original_artist: String,
    pub composer: String,
    pub comment: String,
    pub year: u32,
    pub track_no: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub fields: Fields,
    /// The front cover, or the first picture.
    pub artwork: Option<Vec<u8>>,
    /// Whole seconds.
    pub length: i64,
    /// Kilobits per second.
    pub bitrate: i64,
    pub sample_rate: i64,
    pub bit_depth: i64,
    pub bpm: f64,
    pub key: String,
}

fn open(path: &Path) -> Result<TaggedFile> {
    if !path.is_file() {
        return Err(Error::Missing(path.display().to_string()));
    }
    let probe = Probe::open(path)?.options(ParseOptions::new().read_cover_art(true));
    if probe.file_type().is_none() {
        return Err(Error::Unsupported(path.extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy()))));
    }
    Ok(probe.read()?)
}

/// The tag rekordbox reads: ID3v2 when the file has one (including in WAV and AIFF), else the file's own.
fn pick(file: &TaggedFile) -> Option<&Tag> {
    file.tag(TagType::Id3v2).or_else(|| file.primary_tag()).or_else(|| file.first_tag())
}

fn text(tag: &Tag, key: &ItemKey) -> String {
    tag.get_string(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}

const VORBIS_ORIGINAL_ARTIST: &str = "ORIGINALARTIST";
const MP4_ORIGINAL_ARTIST: &str = "----:com.apple.iTunes:ORIGINAL ARTIST";

fn original_artist(tag: &Tag) -> String {
    let standard = text(tag, &ItemKey::OriginalArtist);
    if !standard.is_empty() {
        return standard;
    }
    tag.items()
        .find_map(|item| match item.key() {
            ItemKey::Unknown(name)
                if name.eq_ignore_ascii_case(VORBIS_ORIGINAL_ARTIST) || name.eq_ignore_ascii_case(MP4_ORIGINAL_ARTIST) =>
            {
                item.value().text().map(|s| s.trim().to_owned())
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn year(tag: &Tag) -> u32 {
    tag.year().filter(|y| *y > 0).unwrap_or_else(|| {
        let value = text(tag, &ItemKey::Year);
        let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().unwrap_or(0)
    })
}

fn leading_float(value: &str) -> f64 {
    let end = value.trim_start().find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(value.trim_start().len());
    value.trim_start()[..end].parse::<f64>().ok().filter(|v| *v > 0.0 && *v < 1000.0).unwrap_or(0.0)
}

/// A FLAC file's bitrate in kilobits per second, rounded: lofty truncates it.
fn flac_bitrate(path: &Path, duration_secs: f64) -> Option<i64> {
    use std::io::{Read, Seek, SeekFrom};
    if duration_secs <= 0.0 {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).ok()?;
    if &magic != b"fLaC" {
        return None;
    }
    loop {
        let mut head = [0u8; 4];
        file.read_exact(&mut head).ok()?;
        let len = (u64::from(head[1]) << 16) | (u64::from(head[2]) << 8) | u64::from(head[3]);
        file.seek(SeekFrom::Current(i64::try_from(len).ok()?)).ok()?;
        if head[0] & 0x80 != 0 {
            break;
        }
    }
    let audio = file.metadata().ok()?.len().checked_sub(file.stream_position().ok()?)?;
    Some((audio as f64 * 8.0 / duration_secs / 1000.0).round_ties_even() as i64)
}

pub fn read(path: &Path) -> Result<Info> {
    let file = open(path)?;
    let props = file.properties();
    let mut info = Info {
        length: props.duration().as_secs_f64().round_ties_even() as i64,
        bitrate: i64::from(props.audio_bitrate().unwrap_or(0)),
        sample_rate: i64::from(props.sample_rate().unwrap_or(0)),
        bit_depth: i64::from(props.bit_depth().unwrap_or(0)),
        ..Info::default()
    };
    if file.file_type() == lofty::file::FileType::Flac {
        if let Some(rounded) = flac_bitrate(path, props.duration().as_secs_f64()) {
            info.bitrate = rounded;
        }
    }
    if let Some(tag) = pick(&file) {
        info.fields = Fields {
            title: text(tag, &ItemKey::TrackTitle),
            artist: text(tag, &ItemKey::TrackArtist),
            album: text(tag, &ItemKey::AlbumTitle),
            genre: text(tag, &ItemKey::Genre),
            label: text(tag, &ItemKey::Label),
            remixer: text(tag, &ItemKey::Remixer),
            original_artist: original_artist(tag),
            composer: text(tag, &ItemKey::Composer),
            comment: text(tag, &ItemKey::Comment),
            year: year(tag),
            track_no: tag.track().unwrap_or(0),
        };
        let pictures: Vec<_> = tag.pictures().iter().filter(|p| !p.data().is_empty()).collect();
        info.artwork = pictures
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront)
            .or_else(|| pictures.first())
            .map(|p| p.data().to_vec());
        info.bpm = leading_float(&text(tag, &ItemKey::Bpm)).max(leading_float(&text(tag, &ItemKey::IntegerBpm)));
        info.key = text(tag, &ItemKey::InitialKey);
    }
    Ok(info)
}
