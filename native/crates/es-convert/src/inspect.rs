//! Header checks of audio files: what the container says about its format.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Audio {
    pub kind: &'static str,
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub channels: u32,
    pub bitrate: u32,
    pub encoding: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Issue {
    pub code: &'static str,
    pub detail: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Inspection {
    pub size: u64,
    /// Nanoseconds since the epoch, as text: they don't fit in a JSON number.
    pub mtime: Option<String>,
    pub issues: Vec<Issue>,
    pub audio: Option<Audio>,
}

const MISSING: Issue = Issue { code: "missing", detail: "File missing" };
const UNREADABLE: Issue = Issue { code: "unreadable", detail: "File can't be read" };

fn nanos(meta: &std::fs::Metadata) -> Option<u128> {
    meta.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_nanos())
}

/// The file's size and mtime when they still match a cached probe; a missing file matches size 0 with no mtime.
#[must_use]
pub fn same_fingerprint(path: &Path, size: Option<i64>, mtime: Option<&str>) -> Option<(u64, Option<String>)> {
    let hinted = size?;
    if !path.is_file() {
        return (hinted == 0 && mtime.is_none()).then_some((0, None));
    }
    let hinted_time: u128 = mtime?.trim().parse().ok()?;
    let meta = path.metadata().ok()?;
    (i64::try_from(meta.len()).ok()? == hinted && nanos(&meta)? == hinted_time).then(|| (meta.len(), Some(hinted_time.to_string())))
}

/// Header measurements for one local file. Missing and unreadable files have no audio.
#[must_use]
pub fn inspect_file(path: &Path) -> Inspection {
    let Some(meta) = path.metadata().ok().filter(|m| m.is_file()) else {
        return Inspection { size: 0, mtime: None, issues: vec![MISSING], audio: None };
    };
    let size = meta.len();
    let mtime = nanos(&meta).map(|n| n.to_string());
    let audio = if size == 0 { None } else { read_header(path, size).ok().flatten() };
    let issues = if audio.is_some() { Vec::new() } else { vec![UNREADABLE] };
    Inspection { size, mtime, issues, audio }
}

fn pcm(format: u16) -> &'static str {
    match format {
        3 => "float",
        1 => "pcm",
        _ => "other",
    }
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn read_exact_or_less(file: &mut File, len: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

fn read_header(path: &Path, size: u64) -> std::io::Result<Option<Audio>> {
    let mut file = File::open(path)?;
    let lead = read_exact_or_less(&mut file, 12)?;
    if lead.starts_with(b"RIFF") && lead.get(8..12) == Some(b"WAVE") {
        return wav(&mut file, size);
    }
    if lead.starts_with(b"FORM") && matches!(lead.get(8..12), Some(b"AIFF" | b"AIFC")) {
        return aiff(&mut file, size, lead.get(8..12) == Some(b"AIFC"));
    }
    if lead.starts_with(b"fLaC") {
        return flac(&mut file);
    }
    if lead.len() >= 8 && &lead[4..8] == b"ftyp" {
        return Ok(Some(mp4(&mut file, size)));
    }
    if lead.starts_with(b"ID3") || mp3_frame(&lead) {
        return mp3(&mut file, size);
    }
    Ok(None)
}

fn wav(file: &mut File, size: u64) -> std::io::Result<Option<Audio>> {
    let mut found = None;
    file.seek(SeekFrom::Start(12))?;
    for _ in 0..64 {
        let at = file.stream_position()?;
        if at + 8 > size {
            break;
        }
        let head = read_exact_or_less(file, 8)?;
        if head.len() < 8 {
            break;
        }
        let chunk = u64::from(le32(&head[4..8]));
        let pos = at + 8;
        if &head[..4] == b"fmt " && chunk >= 16 {
            let raw = read_exact_or_less(file, chunk.min(64) as usize)?;
            if raw.len() >= 16 {
                let mut format = u16::from_le_bytes([raw[0], raw[1]]);
                if format == 0xFFFE && raw.len() >= 26 {
                    format = u16::from_le_bytes([raw[24], raw[25]]);
                }
                found = Some((format, u32::from(u16::from_le_bytes([raw[2], raw[3]])), le32(&raw[4..8]), u32::from(u16::from_le_bytes([raw[14], raw[15]]))));
            }
        } else if &head[..4] == b"data" {
            break;
        }
        let next = pos + chunk + (chunk & 1);
        if chunk == 0 || next <= pos {
            break;
        }
        file.seek(SeekFrom::Start(next))?;
    }
    Ok(found.map(|(format, channels, rate, bits)| Audio { kind: "WAV", sample_rate: rate, bit_depth: bits, channels, bitrate: 0, encoding: pcm(format) }))
}

fn extended(raw: &[u8]) -> f64 {
    if raw.len() < 10 {
        return 0.0;
    }
    let exponent = (i32::from(raw[0] & 0x7F) << 8) | i32::from(raw[1]);
    let mantissa = u64::from_be_bytes([raw[2], raw[3], raw[4], raw[5], raw[6], raw[7], raw[8], raw[9]]);
    if exponent == 0 || mantissa == 0 {
        return 0.0;
    }
    let value = mantissa as f64 / (1u64 << 63) as f64 * 2f64.powi(exponent - 16383);
    if raw[0] & 0x80 != 0 { -value } else { value }
}

fn aiff(file: &mut File, size: u64, compressed: bool) -> std::io::Result<Option<Audio>> {
    file.seek(SeekFrom::Start(12))?;
    for _ in 0..64 {
        let at = file.stream_position()?;
        if at + 8 > size {
            break;
        }
        let head = read_exact_or_less(file, 8)?;
        if head.len() < 8 {
            break;
        }
        let chunk = u64::from(be32(&head[4..8]));
        let pos = at + 8;
        if &head[..4] == b"COMM" && chunk >= 18 {
            let raw = read_exact_or_less(file, chunk.min(64) as usize)?;
            if raw.len() < 18 {
                return Ok(None);
            }
            let channels = u32::from(u16::from_be_bytes([raw[0], raw[1]]));
            let bits = u32::from(u16::from_be_bytes([raw[6], raw[7]]));
            let rate = extended(&raw[8..18]);
            let compression: Vec<u8> = if compressed && raw.len() >= 22 { raw[18..22].to_ascii_uppercase() } else { b"NONE".to_vec() };
            let format = match compression.as_slice() {
                b"NONE" | b"TWOS" | b"SOWT" | b"RAW " => 1,
                b"FL32" | b"FL64" => 3,
                _ => 0,
            };
            let rate = if rate == 0.0 { 0 } else { rate.round_ties_even() as u32 };
            return Ok(Some(Audio { kind: "AIFF", sample_rate: rate, bit_depth: bits, channels, bitrate: 0, encoding: pcm(format) }));
        }
        let next = pos + chunk + (chunk & 1);
        if chunk == 0 || next <= pos {
            break;
        }
        file.seek(SeekFrom::Start(next))?;
    }
    Ok(None)
}

fn flac(file: &mut File) -> std::io::Result<Option<Audio>> {
    file.seek(SeekFrom::Start(4))?;
    let block = read_exact_or_less(file, 4)?;
    if block.len() < 4 {
        return Ok(None);
    }
    let len = (u32::from(block[1]) << 16) | (u32::from(block[2]) << 8) | u32::from(block[3]);
    if block[0] & 0x7F != 0 || len < 18 {
        return Ok(None);
    }
    let info = read_exact_or_less(file, 18)?;
    if info.len() < 18 {
        return Ok(None);
    }
    let packed = u64::from_be_bytes([info[10], info[11], info[12], info[13], info[14], info[15], info[16], info[17]]);
    Ok(Some(Audio {
        kind: "FLAC",
        sample_rate: (packed >> 44) as u32,
        bit_depth: ((packed >> 36) & 0x1F) as u32 + 1,
        channels: ((packed >> 41) & 0x7) as u32 + 1,
        bitrate: 0,
        encoding: "flac",
    }))
}

#[derive(Default)]
struct Mp4 {
    rate: u32,
    channels: u32,
    bits: u32,
    codec: [u8; 4],
}

fn mp4_walk(file: &mut File, end: u64, depth: u32, size: u64, out: &mut Mp4) -> std::io::Result<()> {
    while file.stream_position()? + 8 <= end {
        let start = file.stream_position()?;
        let head = read_exact_or_less(file, 8)?;
        if head.len() < 8 {
            return Ok(());
        }
        let mut box_size = u64::from(be32(&head[..4]));
        let kind = [head[4], head[5], head[6], head[7]];
        let mut header = 8;
        if box_size == 1 {
            let big = read_exact_or_less(file, 8)?;
            if big.len() < 8 {
                return Ok(());
            }
            box_size = u64::from_be_bytes([big[0], big[1], big[2], big[3], big[4], big[5], big[6], big[7]]);
            header = 16;
        } else if box_size == 0 {
            box_size = end - start;
        }
        if box_size < header {
            return Ok(());
        }
        let box_end = start + box_size;
        if box_end > size {
            return Ok(());
        }
        match &kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" if depth < 6 => mp4_walk(file, box_end, depth + 1, size, out)?,
            b"mdhd" => {
                let version = read_exact_or_less(file, 1)?;
                let skip = if version.first() == Some(&0) { 12 } else { 20 };
                file.seek(SeekFrom::Start(start + header + skip))?;
                let raw = read_exact_or_less(file, 4)?;
                if raw.len() == 4 && be32(&raw) != 0 {
                    out.rate = be32(&raw);
                }
            }
            b"stsd" if box_end.saturating_sub(start + header) >= 28 => {
                file.seek(SeekFrom::Start(start + header + 8))?;
                let entry = read_exact_or_less(file, 8)?;
                if entry.len() >= 8 {
                    out.codec = [entry[4], entry[5], entry[6], entry[7]];
                }
                read_exact_or_less(file, 16)?;
                if file.stream_position()? + 4 <= box_end {
                    let raw = read_exact_or_less(file, 4)?;
                    if raw.len() == 4 {
                        out.channels = u32::from(u16::from_be_bytes([raw[0], raw[1]]));
                        out.bits = u32::from(u16::from_be_bytes([raw[2], raw[3]]));
                    }
                }
            }
            _ => {}
        }
        file.seek(SeekFrom::Start(box_end))?;
    }
    Ok(())
}

fn mp4(file: &mut File, size: u64) -> Audio {
    let mut out = Mp4::default();
    let _ = file.seek(SeekFrom::Start(0)).and_then(|_| mp4_walk(file, size, 0, size, &mut out));
    let (kind, encoding) = match &out.codec {
        b"alac" => ("ALAC", "alac"),
        b"mp4a" => ("AAC", "aac"),
        _ => ("MP4", "other"),
    };
    Audio { kind, sample_rate: out.rate, bit_depth: out.bits, channels: out.channels, bitrate: 0, encoding }
}

fn mp3_frame(data: &[u8]) -> bool {
    if data.len() < 4 || data[0] != 0xFF || data[1] & 0xE0 != 0xE0 {
        return false;
    }
    let version = (data[1] >> 3) & 3;
    let layer = (data[1] >> 1) & 3;
    let bitrate = (data[2] >> 4) & 0xF;
    let rate = (data[2] >> 2) & 3;
    version != 1 && layer == 1 && bitrate != 0 && bitrate != 15 && rate != 3
}

fn mp3(file: &mut File, size: u64) -> std::io::Result<Option<Audio>> {
    file.seek(SeekFrom::Start(0))?;
    let lead = read_exact_or_less(file, 10)?;
    let mut offset = 0u64;
    if lead.starts_with(b"ID3") && lead.len() >= 10 {
        let tag = (u64::from(lead[6] & 0x7F) << 21) | (u64::from(lead[7] & 0x7F) << 14) | (u64::from(lead[8] & 0x7F) << 7) | u64::from(lead[9] & 0x7F);
        offset = 10 + tag;
        if lead[5] & 0x10 != 0 {
            offset += 10;
        }
    }
    if offset >= size {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(offset))?;
    let frame = read_exact_or_less(file, 4)?;
    if !mp3_frame(&frame) {
        return Ok(None);
    }
    let version = usize::from((frame[1] >> 3) & 3);
    let rate_index = usize::from((frame[2] >> 2) & 3);
    let bitrate_index = usize::from((frame[2] >> 4) & 0xF);
    const RATES: [[u32; 3]; 4] = [[11025, 12000, 8000], [0; 3], [22050, 24000, 16000], [44100, 48000, 32000]];
    const V1: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
    const V2: [u32; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];
    let bitrate = if version == 3 { V1[bitrate_index] } else { V2[bitrate_index] };
    let channels = if (frame[3] >> 6) & 3 == 3 { 1 } else { 2 };
    Ok(Some(Audio { kind: "MP3", sample_rate: RATES[version][rate_index], bit_depth: 0, channels, bitrate, encoding: "mp3" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_bytes(format: u16, bits: u16) -> Vec<u8> {
        let mut b = b"RIFF".to_vec();
        b.extend(44u32.to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(format.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(44100u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        b.extend(bits.to_le_bytes());
        b.extend(b"data");
        b.extend(8u32.to_le_bytes());
        b.extend([0u8; 8]);
        b
    }

    #[test]
    fn a_wav_header_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        std::fs::write(&path, wav_bytes(3, 32)).unwrap();
        let found = inspect_file(&path);
        let audio = found.audio.unwrap();
        assert_eq!((audio.kind, audio.sample_rate, audio.bit_depth, audio.channels, audio.encoding), ("WAV", 44100, 32, 2, "float"));
        assert!(found.issues.is_empty());
    }

    #[test]
    fn missing_empty_and_unknown_files_have_issues() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(inspect_file(&dir.path().join("none.wav")).issues, vec![MISSING]);
        let empty = dir.path().join("e.wav");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(inspect_file(&empty).issues, vec![UNREADABLE]);
        let junk = dir.path().join("j.mp3");
        std::fs::write(&junk, b"not audio at all").unwrap();
        assert_eq!(inspect_file(&junk).issues, vec![UNREADABLE]);
    }

    #[test]
    fn a_flac_and_an_mp3_frame_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let mut flac = b"fLaC".to_vec();
        flac.extend([0, 0, 0, 34]);
        let packed: u64 = (44100u64 << 44) | (1u64 << 41) | (15u64 << 36);
        flac.extend([0u8; 10]);
        flac.extend(packed.to_be_bytes());
        flac.extend([0u8; 16]);
        let path = dir.path().join("a.flac");
        std::fs::write(&path, &flac).unwrap();
        let audio = inspect_file(&path).audio.unwrap();
        assert_eq!((audio.kind, audio.sample_rate, audio.channels, audio.bit_depth), ("FLAC", 44100, 2, 16));

        let path = dir.path().join("a.mp3");
        std::fs::write(&path, [0xFF, 0xFB, 0x90, 0x00, 0, 0, 0, 0]).unwrap();
        let audio = inspect_file(&path).audio.unwrap();
        assert_eq!((audio.kind, audio.sample_rate, audio.bitrate, audio.channels), ("MP3", 44100, 128, 2));
    }

    #[test]
    fn fingerprints_match_only_an_unchanged_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        std::fs::write(&path, wav_bytes(1, 16)).unwrap();
        let seen = inspect_file(&path);
        let size = i64::try_from(seen.size).ok();
        assert!(same_fingerprint(&path, size, seen.mtime.as_deref()).is_some());
        assert!(same_fingerprint(&path, size, Some("1")).is_none());
        assert!(same_fingerprint(&path, size, None).is_none());
        assert_eq!(same_fingerprint(&dir.path().join("gone"), Some(0), None), Some((0, None)));
    }
}
