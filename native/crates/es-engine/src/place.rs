//! Where copies of a stick's audio go, and copying them safely. Names follow how rekordbox lays out a
//! stick: `Contents/<Artist>/<Album>/<file>`, made safe for FAT, and a file already at a path is never
//! replaced.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use crate::{Error, Result};

const DIR_NAME_MAX: usize = 48;
const FILE_NAME_MAX: usize = 120;
const NUMBERED_MAX: usize = 48;
const IMPORTED_FROM_DEVICE: [&str; 2] = ["PioneerDJ", "Imported from Device"];
const TRIES: u32 = 10_000;

/// Where rekordbox puts copies, under the user's music folder.
#[must_use]
pub fn destination(music: &Path) -> PathBuf {
    IMPORTED_FROM_DEVICE.iter().fold(music.to_path_buf(), |path, part| path.join(part))
}

fn trim_end(mut text: String) -> String {
    while text.ends_with('.') || text.ends_with(' ') {
        text.pop();
    }
    text
}

#[must_use]
pub fn fat_safe(name: &str) -> String {
    let replaced: String = name
        .nfc()
        .map(|c| if "/\\:*?\"<>|".contains(c) || u32::from(c) < 0x20 { '_' } else { c })
        .collect();
    let cleaned = trim_end(replaced);
    if cleaned.is_empty() { "Unknown".into() } else { cleaned }
}

#[must_use]
pub fn dir_name(name: &str) -> String {
    let safe = fat_safe(name);
    if safe.chars().count() <= DIR_NAME_MAX {
        return safe;
    }
    let cut = trim_end(safe.chars().take(DIR_NAME_MAX).collect());
    if cut.is_empty() { "Unknown".into() } else { cut }
}

#[must_use]
pub fn fat_file_name(name: &str) -> String {
    let safe = fat_safe(name);
    if safe.len() <= FILE_NAME_MAX {
        return safe;
    }
    let (stem, extension) = match safe.rfind('.') {
        Some(dot) => (&safe[..dot], &safe[dot + 1..]),
        None => ("", safe.as_str()),
    };
    let keep_extension = !stem.is_empty() && extension.len() < FILE_NAME_MAX;
    let suffix = if keep_extension { format!(".{extension}") } else { String::new() };
    let body = if keep_extension { stem } else { safe.as_str() };
    let end = FILE_NAME_MAX - suffix.len();
    let mut cut = end.min(body.len());
    while !body.is_char_boundary(cut) {
        cut -= 1;
    }
    let raw = trim_end(body[..cut].to_owned());
    format!("{}{suffix}", if raw.is_empty() { "Unknown" } else { raw.as_str() })
}

/// The name rekordbox tries for the `n`th clash of `file_name`: `<stem>-<n><.ext>`, the stem cut so the whole fits 48 characters.
#[must_use]
pub fn numbered(file_name: &str, n: u32) -> String {
    let (stem, extension) = match file_name.rfind('.') {
        Some(dot) => (&file_name[..dot], &file_name[dot..]),
        None => (file_name, ""),
    };
    let suffix = format!("-{n}");
    let length = file_name.chars().count() + suffix.chars().count();
    let stem: String = if length > NUMBERED_MAX {
        let keep = NUMBERED_MAX.saturating_sub(suffix.chars().count() + extension.chars().count());
        stem.chars().take(keep).collect()
    } else {
        stem.to_owned()
    };
    format!("{stem}{suffix}{extension}")
}

/// Where a track's copy goes under `root`, before any clash. `contents` adds the `Contents` level rekordbox uses.
#[must_use]
pub fn candidate(root: &Path, artist: &str, album: &str, file_name: &str, contents: bool) -> PathBuf {
    let pick = |value: &str, blank: &str| dir_name(if value.trim().is_empty() { blank } else { value.trim() });
    let base = if contents { root.join("Contents") } else { root.to_path_buf() };
    base.join(pick(artist, "UnknownArtist")).join(pick(album, "UnknownAlbum")).join(fat_file_name(file_name))
}

/// Read until the buffer is full or the file ends; returns how many bytes were read.
fn fill(file: &mut File, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = file.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

fn same_bytes(left: &Path, right: &Path) -> bool {
    let (Ok(a), Ok(b)) = (fs::metadata(left), fs::metadata(right)) else { return false };
    if a.len() != b.len() {
        return false;
    }
    let (Ok(mut x), Ok(mut y)) = (File::open(left), File::open(right)) else { return false };
    let (mut one, mut two) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let (Ok(n), Ok(m)) = (fill(&mut x, &mut one), fill(&mut y, &mut two)) else { return false };
        if n != m || one[..n] != two[..n] {
            return false;
        }
        if n == 0 {
            return true;
        }
    }
}

/// An earlier copy of `source` at `wanted` or one of its numbered neighbours, if there is one.
fn existing_copy(source: &Path, wanted: &Path) -> Option<PathBuf> {
    let parent = wanted.parent()?;
    let name = wanted.file_name()?.to_string_lossy().into_owned();
    for n in 0..TRIES {
        let at = parent.join(if n == 0 { name.clone() } else { numbered(&name, n) });
        if !at.exists() {
            return None;
        }
        if at.is_file() && same_bytes(source, &at) {
            return Some(at);
        }
    }
    None
}

/// Copy `source` beside `wanted` under a free name, never replacing a file. The copy is checked byte for byte.
fn copy_new(source: &Path, wanted: &Path) -> Result<PathBuf> {
    let parent = wanted.parent().ok_or_else(|| Error::Other("a copy needs a folder".into()))?;
    let name = wanted.file_name().ok_or_else(|| Error::Other("a copy needs a name".into()))?.to_string_lossy().into_owned();
    fs::create_dir_all(parent).map_err(|e| Error::Other(e.to_string()))?;
    for n in 0..TRIES {
        let target = parent.join(if n == 0 { name.clone() } else { numbered(&name, n) });
        let mut out = match OpenOptions::new().write(true).create_new(true).open(&target) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(Error::Other(e.to_string())),
        };
        let copied = (|| -> std::io::Result<()> {
            let mut input = File::open(source)?;
            std::io::copy(&mut input, &mut out)?;
            out.flush()?;
            out.sync_all()
        })();
        let verified = copied.map_err(|e| e.to_string()).and_then(|()| {
            if same_bytes(source, &target) { Ok(()) } else { Err(format!("the copy of {} did not match the device's file", source.display())) }
        });
        if let Err(message) = verified {
            let _ = fs::remove_file(&target);
            return Err(Error::Other(message));
        }
        return Ok(target);
    }
    Err(Error::Other(format!("no free name for {}", wanted.display())))
}

/// `(path, created)`. An identical earlier copy is reused rather than copied again.
pub fn place_copy(source: &Path, wanted: &Path) -> Result<(PathBuf, bool)> {
    match existing_copy(source, wanted) {
        Some(found) => Ok((found, false)),
        None => Ok((copy_new(source, wanted)?, true)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_safe_for_fat() {
        assert_eq!(fat_safe("AC/DC: Live?"), "AC_DC_ Live_");
        assert_eq!(fat_safe("trailing. "), "trailing");
        assert_eq!(fat_safe("..."), "Unknown");
    }

    #[test]
    fn long_folder_and_file_names_are_cut() {
        assert_eq!(dir_name(&"a".repeat(60)).len(), 48);
        let long = format!("{}.mp3", "b".repeat(200));
        let cut = fat_file_name(&long);
        assert_eq!(cut.len(), 120);
        assert!(cut.ends_with(".mp3"));
    }

    #[test]
    fn clashes_get_numbered_within_48_characters() {
        assert_eq!(numbered("song.mp3", 1), "song-1.mp3");
        let long = format!("{}.mp3", "x".repeat(60));
        let name = numbered(&long, 12);
        assert_eq!(name.chars().count(), 48);
        assert!(name.ends_with("-12.mp3"));
    }

    #[test]
    fn candidates_follow_the_rekordbox_layout() {
        let root = Path::new("/music/PioneerDJ/Imported from Device");
        assert_eq!(candidate(root, "DJ", "", "song.mp3", true), root.join("Contents/DJ/UnknownAlbum/song.mp3"));
        assert_eq!(candidate(Path::new("/x"), " ", "Album", "a.wav", false), Path::new("/x/UnknownArtist/Album/a.wav"));
    }

    #[test]
    fn an_identical_copy_is_reused_and_a_different_file_gets_a_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src.wav");
        fs::write(&source, b"audio").unwrap();
        let wanted = dir.path().join("out/Artist/Album/song.wav");
        let (first, created) = place_copy(&source, &wanted).unwrap();
        assert!(created && first == wanted);
        assert_eq!(place_copy(&source, &wanted).unwrap(), (wanted.clone(), false));

        let other = dir.path().join("other.wav");
        fs::write(&other, b"different").unwrap();
        let (second, created) = place_copy(&other, &wanted).unwrap();
        assert!(created);
        assert_eq!(second, dir.path().join("out/Artist/Album/song-1.wav"));
        assert_eq!(fs::read(&wanted).unwrap(), b"audio", "an existing file is never replaced");
    }
}
