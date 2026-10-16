//! Reading the tags inside audio files, as the fields rekordbox tracks carry. Writing is still done by
//! the Python engine: lofty does not yet write the year and original artist everywhere, and its WAV
//! handling duplicates an existing `id3 ` chunk.

use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use lofty::tag::{ItemValue, Tag, TagItem, TagType};

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

/// Fields to write into a file's own tags. `None` leaves a field alone; empty text or 0 removes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub remixer: Option<String>,
    pub original_artist: Option<String>,
    pub composer: Option<String>,
    pub comment: Option<String>,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
}

/// Name rekordbox and other tools use for a field the format has no standard place for.
fn set_text(tag: &mut Tag, key: ItemKey, unknown: &[&str], text: &str) {
    let text = text.trim();
    tag.remove_key(&key);
    for name in unknown {
        tag.remove_key(&ItemKey::Unknown((*name).to_owned()));
    }
    if text.is_empty() {
        return;
    }
    if !tag.insert_text(key, text.to_owned()) {
        for name in unknown {
            tag.insert_unchecked(TagItem::new(ItemKey::Unknown((*name).to_owned()), ItemValue::Text(text.to_owned())));
        }
    }
}

/// Write the given fields into the audio file's own tags: ID3v2.3 for MP3, WAV and AIFF, Vorbis comments for
/// FLAC and iTunes atoms for M4A.
pub fn write(path: &Path, edit: &Edit) -> Result<()> {
    use lofty::config::WriteOptions;
    if !path.is_file() {
        return Err(Error::Missing(path.display().to_string()));
    }
    let probe = Probe::open(path)?;
    if probe.file_type().is_none() {
        return Err(Error::Unsupported(path.extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy()))));
    }
    let mut file = probe.read()?;
    let kind = if file.tag(TagType::Id3v2).is_some() { TagType::Id3v2 } else { file.primary_tag_type() };
    if file.tag(kind).is_none() {
        file.insert_tag(Tag::new(kind));
    }
    let tag = file.tag_mut(kind).ok_or_else(|| Error::Unsupported("tags".into()))?;
    let vorbis = kind == TagType::VorbisComments;
    let mp4 = kind == TagType::Mp4Ilst;
    let freeform = |name: &str| format!("----:com.apple.iTunes:{name}");
    let fields: [(&Option<String>, ItemKey, Vec<String>); 9] = [
        (&edit.title, ItemKey::TrackTitle, vec![]),
        (&edit.artist, ItemKey::TrackArtist, vec![]),
        (&edit.album, ItemKey::AlbumTitle, vec![]),
        (&edit.genre, ItemKey::Genre, vec![]),
        (&edit.composer, ItemKey::Composer, vec![]),
        (&edit.comment, ItemKey::Comment, vec![]),
        (&edit.label, ItemKey::Label, if mp4 { vec![freeform("LABEL")] } else if vorbis { vec!["LABEL".into()] } else { vec![] }),
        (&edit.remixer, ItemKey::Remixer, if mp4 { vec![freeform("REMIXER")] } else if vorbis { vec!["REMIXER".into()] } else { vec![] }),
        (
            &edit.original_artist,
            ItemKey::OriginalArtist,
            if mp4 { vec![freeform("ORIGINAL ARTIST")] } else if vorbis { vec!["ORIGINALARTIST".into()] } else { vec![] },
        ),
    ];
    for (value, key, unknown) in fields {
        if let Some(text) = value {
            let names: Vec<&str> = unknown.iter().map(String::as_str).collect();
            set_text(tag, key, &names, text);
        }
    }
    if let Some(year) = edit.year {
        tag.remove_key(&ItemKey::Year);
        tag.remove_key(&ItemKey::RecordingDate);
        if year > 0 {
            let text = year.to_string();
            let unchecked = |tag: &mut Tag, name: &str| {
                tag.insert_unchecked(TagItem::new(ItemKey::Unknown(name.to_owned()), ItemValue::Text(text.clone())));
            };
            match tag.tag_type() {
                TagType::Id3v2 => unchecked(tag, "TYER"),
                TagType::VorbisComments => unchecked(tag, "DATE"),
                _ => {
                    tag.insert_text(ItemKey::RecordingDate, text.clone());
                }
            }
        }
    }
    if let Some(number) = edit.track_no {
        if number > 0 {
            tag.set_track(number);
        } else {
            tag.remove_track();
        }
    }
    file.save_to_path(path, WriteOptions::default().use_id3v23(true))?;
    Ok(())
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
