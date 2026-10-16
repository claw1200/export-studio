//! rekordbox analysis files. A file is a `PMAI` header followed by tags; each tag starts with its
//! four-letter kind, its header length and its total length, all big-endian.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

pub mod decode;

pub const PHRASE_TAG: &str = "PSSI";

const MAGIC: &[u8; 4] = b"PMAI";
const TAG_HEADER: usize = 12;
const SUFFIXES: [&str; 3] = ["DAT", "EXT", "2EX"];

/// The `.DAT`, `.EXT` and `.2EX` files for a stored analysis path, under the library's share folder.
/// A path that climbs out of the share folder yields nothing.
#[must_use]
pub fn files(share: &Path, relative: &str) -> Vec<PathBuf> {
    let trimmed = relative.trim().trim_matches(['/', '\\']);
    if trimmed.is_empty() {
        return Vec::new();
    }
    let path = share.join(trimmed);
    let escapes = Path::new(trimmed).components().any(|part| !matches!(part, Component::Normal(_)));
    if escapes {
        return Vec::new();
    }
    SUFFIXES.iter().map(|suffix| path.with_extension(suffix)).collect()
}

/// One analysis file by suffix (`DAT`, `EXT` or `2EX`).
#[must_use]
pub fn file(share: &Path, relative: &str, suffix: &str) -> Option<PathBuf> {
    files(share, relative).into_iter().find(|p| p.extension().is_some_and(|e| e == suffix))
}

/// Whether the file holds a tag of `kind`. Unreadable and malformed files hold none.
#[must_use]
pub fn has_tag(path: &Path, kind: &str) -> bool {
    scan(path, kind).unwrap_or(false)
}

fn scan(path: &Path, kind: &str) -> std::io::Result<bool> {
    let mut handle = File::open(path)?;
    let mut head = [0u8; 8];
    handle.read_exact(&mut head)?;
    if &head[..4] != MAGIC {
        return Ok(false);
    }
    handle.seek(SeekFrom::Start(u64::from(u32::from_be_bytes([head[4], head[5], head[6], head[7]]))))?;
    loop {
        let mut tag = [0u8; TAG_HEADER];
        if handle.read_exact(&mut tag).is_err() {
            return Ok(false);
        }
        if tag[..4] == *kind.as_bytes() {
            return Ok(true);
        }
        let length = u32::from_be_bytes([tag[8], tag[9], tag[10], tag[11]]) as usize;
        if length < TAG_HEADER {
            return Ok(false);
        }
        handle.seek(SeekFrom::Current(i64::try_from(length - TAG_HEADER).unwrap_or(i64::MAX)))?;
    }
}

/// Delete analysis files, ignoring ones already gone. Returns the first failure.
pub fn discard(paths: &[PathBuf]) -> std::io::Result<()> {
    let mut first = None;
    for path in paths {
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound && first.is_none() {
                first = Some(e);
            }
        }
    }
    first.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(kind: &str, body: &[u8]) -> Vec<u8> {
        let mut out = kind.as_bytes().to_vec();
        out.extend(12u32.to_be_bytes());
        out.extend((12 + u32::try_from(body.len()).unwrap()).to_be_bytes());
        out.extend(body);
        out
    }

    fn anlz(tags: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"PMAI".to_vec();
        out.extend(28u32.to_be_bytes());
        out.extend([0u8; 20]);
        for t in tags {
            out.extend(t);
        }
        out
    }

    #[test]
    fn lists_the_three_files_inside_the_share_folder() {
        let found = files(Path::new("/share"), "/PIONEER/USBANLZ/abc/def012/ANLZ0000.DAT");
        let names: Vec<_> = found.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["ANLZ0000.DAT", "ANLZ0000.EXT", "ANLZ0000.2EX"]);
        assert!(found[0].starts_with("/share/PIONEER"));
    }

    #[test]
    fn refuses_empty_and_escaping_paths() {
        assert!(files(Path::new("/share"), "").is_empty());
        assert!(files(Path::new("/share"), "../../secrets.DAT").is_empty());
    }

    #[test]
    fn finds_a_tag_after_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.EXT");
        std::fs::write(&path, anlz(&[tag("PPTH", b"abcd"), tag("PSSI", b"")])).unwrap();
        assert!(has_tag(&path, PHRASE_TAG));
        assert!(!has_tag(&path, "PWV5"));
    }

    #[test]
    fn missing_and_malformed_files_hold_no_tags() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_tag(&dir.path().join("none.EXT"), PHRASE_TAG));
        let bad = dir.path().join("bad.EXT");
        std::fs::write(&bad, b"not an analysis file").unwrap();
        assert!(!has_tag(&bad, PHRASE_TAG));
    }

    #[test]
    fn discard_ignores_files_already_gone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.DAT");
        std::fs::write(&path, b"x").unwrap();
        discard(&[path.clone(), dir.path().join("b.DAT")]).unwrap();
        assert!(!path.exists());
    }
}
