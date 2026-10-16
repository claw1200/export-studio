//! Reading what an analysis file holds: waveforms, the beat grid and cue colours.

use std::collections::HashMap;
use std::path::Path;

pub(crate) const TAG_HEADER: usize = 12;
const BEAT_COUNT_AT: usize = 20;

/// The tags of one analysis file, in file order.
pub struct Tags {
    data: Vec<u8>,
    spans: Vec<([u8; 4], usize, usize)>,
}

impl Tags {
    pub fn read(path: &Path) -> std::io::Result<Self> {
        Self::parse(std::fs::read(path)?)
    }

    pub fn parse(data: Vec<u8>) -> std::io::Result<Self> {
        let bad = |why: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, why.to_owned());
        if data.len() < 8 || &data[..4] != b"PMAI" {
            return Err(bad("not an analysis file"));
        }
        let mut pos = be32(&data, 4).ok_or_else(|| bad("short header"))? as usize;
        let mut spans = Vec::new();
        while pos + TAG_HEADER <= data.len() {
            let len = be32(&data, pos + 8).unwrap_or(0) as usize;
            if len < TAG_HEADER || pos + len > data.len() {
                break;
            }
            let mut kind = [0u8; 4];
            kind.copy_from_slice(&data[pos..pos + 4]);
            spans.push((kind, pos, len));
            pos += len;
        }
        Ok(Self { data, spans })
    }

    pub fn all<'a>(&'a self, kind: &str) -> impl Iterator<Item = &'a [u8]> + 'a {
        let wanted = kind.as_bytes().to_vec();
        self.spans
            .iter()
            .filter(move |(k, _, _)| k.as_slice() == wanted.as_slice())
            .map(|&(_, at, len)| &self.data[at..at + len])
    }

    #[must_use]
    pub fn first(&self, kind: &str) -> Option<&[u8]> {
        self.all(kind).next()
    }

    /// The entries of a fixed-width tag: its body and the entry count. Empty tags are none.
    fn entries(&self, kind: &str) -> Option<(&[u8], usize)> {
        let tag = self.first(kind)?;
        let header = be32(tag, 4)? as usize;
        let count = be32(tag, 16)? as usize;
        (count > 0 && header <= tag.len()).then(|| (&tag[header..], count))
    }
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be16(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

/// A stored value scaled to 0..=255 the way the interface expects, rounding halves to even.
fn scale(value: u32, stored_max: u32) -> u8 {
    (value as f32 * (255.0_f64 / f64::from(stored_max)) as f32).round_ties_even().clamp(0.0, 255.0) as u8
}

pub struct Rgb {
    pub length: usize,
    pub heights: Vec<u8>,
    pub colors: Vec<u8>,
}

/// The colour waveform (`PWV5`): a height and three colour channels for each column.
#[must_use]
pub fn rgb(ext: &Tags) -> Option<Rgb> {
    let (body, count) = ext.entries("PWV5")?;
    let count = count.min(body.len() / 2);
    let mut heights = Vec::with_capacity(count);
    let mut colors = Vec::with_capacity(count * 3);
    for i in 0..count {
        let packed = u32::from(be16(body, i * 2)?);
        colors.extend([13, 10, 7].map(|shift| scale((packed >> shift) & 7, 7)));
        heights.push(scale((packed >> 2) & 31, 31));
    }
    Some(Rgb { length: count, heights, colors })
}

pub struct Bands {
    pub length: usize,
    pub bands: Vec<u8>,
}

/// Three-band data (`PWV7` detail, `PWV6` preview): low, mid and high for each column.
#[must_use]
pub fn bands(tags: &Tags, kind: &str) -> Option<Bands> {
    let (body, count) = tags.entries(kind)?;
    let count = count.min(body.len() / 3);
    let bands = body[..count * 3].iter().map(|&b| (f32::from(b) * 2.0).round_ties_even().clamp(0.0, 255.0) as u8).collect();
    Some(Bands { length: count, bands })
}

pub struct Mono {
    pub length: usize,
    pub heights: Vec<u8>,
    pub whiteness: Vec<u8>,
}

/// The older monochrome waveform (`PWV3`).
#[must_use]
pub fn mono(ext: &Tags) -> Option<Mono> {
    let (body, count) = ext.entries("PWV3")?;
    let count = count.min(body.len());
    let heights = body[..count].iter().map(|&b| scale(u32::from(b & 31), 31)).collect();
    let whiteness = body[..count].iter().map(|&b| scale(u32::from(b >> 5), 7)).collect();
    Some(Mono { length: count, heights, whiteness })
}

pub struct Grid {
    pub times: Vec<u32>,
    pub beats: Vec<u16>,
    pub bpms: Vec<f64>,
}

/// The beat grid (`PQTZ`), or none when the file has no beats.
#[must_use]
pub fn beatgrid(dat: &Tags) -> Option<Grid> {
    let tag = dat.first("PQTZ")?;
    let header = be32(tag, 4)? as usize;
    let count = be32(tag, BEAT_COUNT_AT)? as usize;
    let body = tag.get(header..)?;
    let count = count.min(body.len() / 8);
    if count == 0 {
        return None;
    }
    let mut grid = Grid { times: Vec::new(), beats: Vec::new(), bpms: Vec::new() };
    for i in 0..count {
        let at = i * 8;
        grid.beats.push(be16(body, at)?);
        grid.bpms.push(f64::from(be16(body, at + 2)?) / 100.0);
        grid.times.push(be32(body, at + 4)?);
    }
    Some(grid)
}

/// One entry of the extended cue list (`PCO2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedCue {
    /// 0 for a memory cue, otherwise the hot cue slot (1 is A).
    pub slot: u32,
    pub is_loop: bool,
    pub start_ms: u32,
    pub loop_end_ms: Option<u32>,
    pub color_id: u8,
    pub comment: String,
    pub rgb: [u8; 3],
}

/// Every cue of the extended cue lists (`PCO2`), hot and memory.
#[must_use]
pub fn extended_cues(ext: &Tags) -> Vec<ExtendedCue> {
    let mut out = Vec::new();
    for tag in ext.all("PCO2") {
        let (Some(header), Some(count)) = (be32(tag, 4), be16(tag, 16)) else { continue };
        let mut at = header as usize;
        for _ in 0..count {
            if tag.get(at..at + 4) != Some(b"PCP2") {
                break;
            }
            let (Some(entry), Some(slot), Some(start), Some(end), Some(comment_len)) =
                (be32(tag, at + 8), be32(tag, at + 12), be32(tag, at + 20), be32(tag, at + 24), be32(tag, at + 40))
            else {
                break;
            };
            let comment_at = at + 44;
            let comment_end = comment_at + comment_len as usize;
            let units: Vec<u16> = tag
                .get(comment_at..comment_end)
                .unwrap_or_default()
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .take_while(|u| *u != 0)
                .collect();
            let rgb = tag.get(comment_end + 1..comment_end + 4).map_or([0; 3], |s| [s[0], s[1], s[2]]);
            out.push(ExtendedCue {
                slot,
                is_loop: tag.get(at + 16) == Some(&2),
                start_ms: start,
                loop_end_ms: (end != u32::MAX).then_some(end),
                color_id: tag.get(at + 28).copied().unwrap_or(0),
                comment: String::from_utf16_lossy(&units),
                rgb,
            });
            if entry == 0 {
                break;
            }
            at += entry as usize;
        }
    }
    out
}

/// The colour of each hot cue, keyed by slot and start time, from the extended cue list (`PCO2`).
#[must_use]
pub fn hot_cue_colors(ext: &Tags) -> HashMap<(u32, u32), String> {
    extended_cues(ext)
        .into_iter()
        .filter(|cue| cue.slot != 0)
        .map(|cue| ((cue.slot, cue.start_ms), format!("#{:02x}{:02x}{:02x}", cue.rgb[0], cue.rgb[1], cue.rgb[2])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(kind: &str, header_extra: &[u8], body: &[u8]) -> Vec<u8> {
        let header = 12 + header_extra.len();
        let mut out = kind.as_bytes().to_vec();
        out.extend(u32::try_from(header).unwrap().to_be_bytes());
        out.extend(u32::try_from(header + body.len()).unwrap().to_be_bytes());
        out.extend(header_extra);
        out.extend(body);
        out
    }

    fn file(tags: &[Vec<u8>]) -> Tags {
        let mut data = b"PMAI".to_vec();
        data.extend(28u32.to_be_bytes());
        data.extend([0u8; 20]);
        tags.iter().for_each(|t| data.extend(t));
        Tags::parse(data).unwrap()
    }

    fn entries_header(entry: u32, count: u32) -> Vec<u8> {
        // After the tag length: bytes per entry, then the entry count.
        let mut h = entry.to_be_bytes().to_vec();
        h.extend(count.to_be_bytes());
        h.extend(0u32.to_be_bytes());
        h
    }

    #[test]
    fn colour_waveform_columns_are_scaled_to_bytes() {
        // height 31, red 7, green 0, blue 7
        let packed: u16 = (7 << 13) | (7 << 7) | (31 << 2);
        let ext = file(&[tag("PWV5", &entries_header(2, 1), &packed.to_be_bytes())]);
        let wave = rgb(&ext).unwrap();
        assert_eq!((wave.length, wave.heights, wave.colors), (1, vec![255], vec![255, 0, 255]));
    }

    #[test]
    fn bands_double_and_clip() {
        let ex2 = file(&[tag("PWV7", &entries_header(3, 1), &[10, 100, 200])]);
        assert_eq!(bands(&ex2, "PWV7").unwrap().bands, vec![20, 200, 255]);
    }

    #[test]
    fn mono_splits_height_and_whiteness() {
        let ext = file(&[tag("PWV3", &entries_header(1, 1), &[0b1111_1111])]);
        let m = mono(&ext).unwrap();
        assert_eq!((m.heights, m.whiteness), (vec![255], vec![255]));
    }

    #[test]
    fn beat_grid_reads_beat_tempo_and_time() {
        let mut header = vec![0u8; 4];
        header.extend(0x80000u32.to_be_bytes());
        header.extend(2u32.to_be_bytes());
        let mut body = Vec::new();
        body.extend(1u16.to_be_bytes());
        body.extend(17450u16.to_be_bytes());
        body.extend(250u32.to_be_bytes());
        body.extend(2u16.to_be_bytes());
        body.extend(17450u16.to_be_bytes());
        body.extend(595u32.to_be_bytes());
        let dat = file(&[tag("PQTZ", &header, &body)]);
        let grid = beatgrid(&dat).unwrap();
        assert_eq!((grid.times, grid.beats, grid.bpms), (vec![250, 595], vec![1, 2], vec![174.5, 174.5]));
    }

    #[test]
    fn hot_cue_colours_are_keyed_by_slot_and_time() {
        let mut entry = b"PCP2".to_vec();
        entry.extend(16u32.to_be_bytes());
        entry.extend(52u32.to_be_bytes());
        entry.extend(2u32.to_be_bytes()); // slot B
        entry.extend([1, 0, 0, 0]);
        entry.extend(1500u32.to_be_bytes());
        entry.extend(u32::MAX.to_be_bytes());
        entry.extend([0u8; 8]);
        entry.extend([0u8; 4]);
        entry.extend(0u32.to_be_bytes());
        entry.extend([46, 0x10, 0x20, 0x30]);
        entry.resize(52, 0);
        let mut header = vec![0u8, 0, 0, 1];
        header.extend(1u16.to_be_bytes());
        header.extend(0u16.to_be_bytes());
        header.extend(0u32.to_be_bytes());
        let ext = file(&[tag("PCO2", &[header[..4].to_vec(), 1u16.to_be_bytes().to_vec(), 0u16.to_be_bytes().to_vec()].concat(), &entry)]);
        assert_eq!(hot_cue_colors(&ext).get(&(2, 1500)).map(String::as_str), Some("#102030"));
    }
}
