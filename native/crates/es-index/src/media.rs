//! Where a track's audio and artwork are on disk.

use std::path::{Component, Path, PathBuf};

use es_db::{Error, Library, Result};

/// The file for `kind` (`audio` or `art`) of track `id`, if it exists. Artwork has small (`s`) and
/// medium (`m`) versions beside the original.
pub fn resolve_media(lib: &Library, share: &Path, kind: &str, id: &str, size: &str) -> Result<Option<PathBuf>> {
    let files = lib.track_files(id)?.ok_or_else(|| Error::NotFound(format!("track {id}")))?;
    let found = match kind {
        "audio" => Some(PathBuf::from(&files.folder_path)).filter(|p| p.is_file()),
        "art" => files.image_path.as_deref().and_then(|relative| art(share, relative, size)),
        _ => None,
    };
    Ok(found)
}

fn art(share: &Path, relative: &str, size: &str) -> Option<PathBuf> {
    let trimmed = relative.trim_start_matches(['/', '\\']);
    if Path::new(trimmed).components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let path = share.join(trimmed);
    if matches!(size, "s" | "m") {
        let stem = path.file_stem()?.to_string_lossy();
        let name = match path.extension() {
            Some(ext) => format!("{stem}_{size}.{}", ext.to_string_lossy()),
            None => format!("{stem}_{size}"),
        };
        let sized = path.with_file_name(name);
        if sized.is_file() {
            return Some(sized);
        }
    }
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_sized_copy_and_stays_inside_the_share_folder() {
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path();
        std::fs::create_dir_all(share.join("img")).unwrap();
        std::fs::write(share.join("img/a.jpg"), b"x").unwrap();
        std::fs::write(share.join("img/a_s.jpg"), b"x").unwrap();
        assert_eq!(art(share, "/img/a.jpg", "s"), Some(share.join("img/a_s.jpg")));
        assert_eq!(art(share, "/img/a.jpg", "m"), Some(share.join("img/a.jpg")));
        assert_eq!(art(share, "/img/a.jpg", ""), Some(share.join("img/a.jpg")));
        assert_eq!(art(share, "../../etc/passwd", ""), None);
    }
}
