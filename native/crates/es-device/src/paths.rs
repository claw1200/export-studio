//! Where things are on a stick.

use std::path::{Component, Path, PathBuf};

use crate::{fail, Result};

pub const ROOT_NAMES: [&str; 2] = ["PIONEER", ".PIONEER"];
const DB_FILES: [&str; 2] = ["export.pdb", "exportLibrary.db"];

fn has_database(root: &Path) -> bool {
    DB_FILES.iter().any(|db| root.join("rekordbox").join(db).is_file())
}

/// The device root and the spelling of its name that the stick already uses.
#[must_use]
pub fn pioneer_dir(volume: &Path) -> (PathBuf, &'static str) {
    let found: Vec<(PathBuf, &'static str)> =
        ROOT_NAMES.iter().map(|name| (volume.join(name), *name)).filter(|(root, _)| has_database(root) || root.is_dir()).collect();
    if let Some(with_db) = found.iter().find(|(root, _)| has_database(root)) {
        return with_db.clone();
    }
    found.into_iter().next().unwrap_or_else(|| (volume.join("PIONEER"), "PIONEER"))
}

#[must_use]
pub fn has_library(volume: &Path) -> bool {
    ROOT_NAMES.iter().any(|name| has_database(&volume.join(name)))
}

/// A path from a stick's database, turned into a path under `volume`. Anything that could leave the volume is refused.
pub fn volume_path(relative: &str, volume: &Path) -> Result<PathBuf> {
    let text = relative.replace('\\', "/");
    let text = text.strip_prefix('/').unwrap_or(&text);
    let unsafe_part = text.is_empty() || text.split('/').any(|part| matches!(part, "" | "." | ".."));
    if unsafe_part || Path::new(text).components().any(|c| !matches!(c, Component::Normal(_))) {
        return fail(format!("Unsafe device path: {relative}"));
    }
    let target = volume.join(text);
    // A symlink on the stick must not lead out of it.
    if let (Ok(root), Ok(real)) = (volume.canonicalize(), target.canonicalize()) {
        if !real.starts_with(&root) {
            return fail(format!("Path escapes the volume: {relative}"));
        }
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_paths_that_climb_out_of_the_volume() {
        let volume = Path::new("/Volumes/STICK");
        assert!(volume_path("/Contents/a/b.mp3", volume).is_ok());
        assert!(volume_path("/Contents/../../etc/passwd", volume).is_err());
        assert!(volume_path("", volume).is_err());
        assert!(volume_path("//double", volume).is_err());
    }

    #[test]
    fn prefers_the_root_that_holds_a_library() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("PIONEER")).unwrap();
        std::fs::create_dir_all(dir.path().join(".PIONEER/rekordbox")).unwrap();
        std::fs::write(dir.path().join(".PIONEER/rekordbox/export.pdb"), b"x").unwrap();
        assert_eq!(pioneer_dir(dir.path()).1, ".PIONEER");
        assert!(has_library(dir.path()));
    }

    #[test]
    fn an_empty_volume_gets_the_default_root() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(pioneer_dir(dir.path()), (dir.path().join("PIONEER"), "PIONEER"));
        assert!(!has_library(dir.path()));
    }
}
