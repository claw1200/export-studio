//! Where rekordbox's timeline starts for an MP3.
//!
//! Rekordbox places LAME-encoded MP3s one frame later than the raw decode: its beatgrid, cues and
//! waveforms all sit on that timeline. Measured against its own analysis waveforms, LAME files
//! are one frame late and files without a LAME tag (none at all, or ffmpeg's `Lavc`) line up.
//! Encoder delay is not trimmed either way.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes read from the first frame, enough to cover its Xing/Info and LAME tags.
const HEAD_BYTES: usize = 4096;

/// Source frames of silence to play before the file's audio so it lands where rekordbox has it.
pub fn lead_in(path: &Path) -> u64 {
    read_head(path).map_or(0, |head| lead_in_from(&head))
}

fn read_head(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut id3 = [0u8; 10];
    file.read_exact(&mut id3).ok()?;
    let start = if &id3[..3] == b"ID3" {
        let size = id3[6..10]
            .iter()
            .fold(0u64, |acc, &b| (acc << 7) | u64::from(b & 0x7f));
        let footer = if id3[5] & 0x10 != 0 { 10 } else { 0 };
        10 + size + footer
    } else {
        0
    };
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut head = Vec::with_capacity(HEAD_BYTES);
    file.take(HEAD_BYTES as u64).read_to_end(&mut head).ok()?;
    Some(head)
}

/// The lead-in for an MP3 whose first bytes (after any ID3v2 tag) are `head`.
fn lead_in_from(head: &[u8]) -> u64 {
    let Some(sync) = head
        .windows(2)
        .position(|w| w[0] == 0xff && w[1] & 0xe0 == 0xe0)
    else {
        return 0;
    };
    let header = &head[sync..];
    if header.len() < 4 {
        return 0;
    }
    let version = (header[1] >> 3) & 0b11;
    let layer = (header[1] >> 1) & 0b11;
    // Layer III only: 1152 samples a frame for MPEG-1, 576 for MPEG-2 and 2.5.
    if layer != 0b01 {
        return 0;
    }
    let samples = if version == 0b11 { 1152 } else { 576 };
    let frame = &header[..header.len().min(2048)];
    let tagged = ["Xing", "Info"]
        .iter()
        .find_map(|tag| find(frame, tag.as_bytes()));
    match tagged {
        Some(at) if find(&frame[at..], b"LAME").is_some() => samples,
        _ => 0,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(version_bits: u8, tag: &[u8], encoder: &[u8]) -> Vec<u8> {
        let mut head = vec![0xff, 0xe2 | (version_bits << 3), 0x90, 0x44];
        head.extend_from_slice(&[0; 32]);
        head.extend_from_slice(tag);
        head.extend_from_slice(&[0; 116]);
        head.extend_from_slice(encoder);
        head.resize(1024, 0);
        head
    }

    #[test]
    fn a_lame_tagged_mpeg1_file_starts_a_frame_late() {
        assert_eq!(lead_in_from(&frame(0b11, b"Info", b"LAME3.100")), 1152);
        assert_eq!(lead_in_from(&frame(0b11, b"Xing", b"LAME3.99r")), 1152);
    }

    #[test]
    fn mpeg2_frames_are_half_as_long() {
        assert_eq!(lead_in_from(&frame(0b10, b"Xing", b"LAME3.100")), 576);
    }

    #[test]
    fn untagged_and_ffmpeg_files_start_on_time() {
        assert_eq!(lead_in_from(&frame(0b11, b"\0\0\0\0", b"\0\0\0\0")), 0);
        assert_eq!(lead_in_from(&frame(0b11, b"Info", b"Lavc61.22")), 0);
    }
}
