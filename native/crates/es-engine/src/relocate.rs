//! Pointing tracks at moved files, and looking for the files of missing ones.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use es_anlz::decode::Tags;
use es_db::Library;

use crate::{Error, Result};

/// Streaming tracks are stored as a URI such as `tidal:track:1`.
#[must_use]
pub fn is_local(stored: &str) -> bool {
    let Some(colon) = stored.find(':') else { return !stored.is_empty() };
    let scheme = &stored[..colon];
    let streaming = scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    !stored.is_empty() && !streaming
}

fn anlz_backup_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".exportstudio").join("anlz-backup"))
}

pub(crate) fn back_up_once(content_id: &str, paths: &[PathBuf]) -> std::io::Result<()> {
    let Some(base) = anlz_backup_dir() else { return Ok(()) };
    let folder = base.join(content_id);
    std::fs::create_dir_all(&folder)?;
    for path in paths {
        let Some(name) = path.file_name() else { continue };
        let target = folder.join(name);
        if !target.exists() {
            std::fs::copy(path, target)?;
        }
    }
    Ok(())
}

/// Move each track to the file given (content id -> absolute path), updating the analysis files' recorded path.
/// Analysis files are restored when the library edit fails.
pub fn relocate(lib: &mut Library, share: &Path, moves: &[(String, String)]) -> Result<Vec<String>> {
    struct Move {
        id: String,
        stored: String,
        name: String,
        size: i64,
    }
    let mut plan = Vec::new();
    for (id, raw) in moves {
        let target = Path::new(raw);
        if !target.is_absolute() || !target.is_file() {
            let name = target.file_name().map_or_else(|| raw.clone(), |n| n.to_string_lossy().into_owned());
            return Err(Error::Other(format!("Can't find {name}")));
        }
        let size = i64::try_from(target.metadata().map_err(|e| Error::Other(e.to_string()))?.len()).unwrap_or(0);
        plan.push(Move {
            id: id.clone(),
            stored: raw.replace('\\', "/"),
            name: target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            size,
        });
    }

    let mut writes: Vec<(PathBuf, Vec<u8>, Vec<u8>)> = Vec::new();
    for item in &plan {
        let files = lib.track_files(&item.id)?.ok_or_else(|| Error::Other(format!("Unknown track {}", item.id)))?;
        let Some(relative) = files.analysis_path.as_deref() else { continue };
        let mut touched = Vec::new();
        for path in es_anlz::files(share, relative).into_iter().filter(|p| p.is_file()) {
            let old = std::fs::read(&path).map_err(|e| Error::Other(e.to_string()))?;
            let Ok(tags) = Tags::parse(old.clone()) else { continue };
            if tags.first("PPTH").is_none() {
                continue;
            }
            let new = es_anlz::write::replace_tags(&old, &[("PPTH", es_anlz::write::path_tag(&item.stored))]).map_err(|e| Error::Other(e.to_string()))?;
            touched.push(path.clone());
            writes.push((path, old, new));
        }
        if !touched.is_empty() {
            back_up_once(&item.id, &touched).map_err(|e| Error::Other(e.to_string()))?;
        }
    }

    let result = (|| -> Result<()> {
        for (path, _, new) in &writes {
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, new).and_then(|()| std::fs::rename(&tmp, path)).map_err(|e| Error::Other(e.to_string()))?;
        }
        lib.write(|tx| {
            for item in &plan {
                tx.relocate_track(&item.id, &item.stored, &item.name, item.size)?;
            }
            Ok(())
        })?;
        Ok(())
    })();
    if let Err(e) = result {
        for (path, old, _) in writes.iter() {
            if let Err(restore) = std::fs::write(path, old) {
                eprintln!("[es-engine] could not restore {}: {restore}", path.display());
            }
        }
        return Err(e);
    }
    Ok(plan.into_iter().map(|m| m.id).collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub id: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub matches: Vec<Match>,
    pub scanned: usize,
}

fn pick(candidates: &[PathBuf], size: Option<i64>) -> Option<&PathBuf> {
    let wanted = size.filter(|s| *s != 0);
    if candidates.len() <= 1 || wanted.is_none() {
        return candidates.first();
    }
    candidates
        .iter()
        .find(|p| p.metadata().ok().and_then(|m| i64::try_from(m.len()).ok()) == wanted)
        .or_else(|| candidates.first())
}

/// Look under `directory` for files named like each missing local track.
pub fn find_missing(lib: &Library, ids: &[String], directory: &Path) -> Result<Found> {
    if !directory.is_dir() {
        return Err(Error::Other(format!("Can't open the folder {}", directory.display())));
    }
    let mut order: Vec<String> = Vec::new();
    let mut wanted: HashMap<String, Vec<(String, Option<i64>)>> = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for id in ids.iter().filter(|i| seen.insert(i.as_str())) {
        let files = lib.track_files(id)?.ok_or_else(|| Error::Other(format!("Unknown track {id}")))?;
        let stored = files.folder_path;
        if !is_local(&stored) || Path::new(&stored).is_file() {
            continue;
        }
        let name = Path::new(&stored).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or(files.file_name);
        if !name.is_empty() {
            let key = name.to_lowercase();
            if !wanted.contains_key(&key) {
                order.push(key.clone());
            }
            wanted.entry(key).or_default().push((id.clone(), files.file_size));
        }
    }

    let mut found: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut scanned = 0;
    if !wanted.is_empty() {
        let mut stack = vec![directory.to_path_buf()];
        while let Some(folder) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else { continue };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                let name = entry.file_name().to_string_lossy().into_owned();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    if !name.starts_with('.') {
                        stack.push(entry.path());
                    }
                } else {
                    scanned += 1;
                    let key = name.to_lowercase();
                    if wanted.contains_key(&key) {
                        found.entry(key).or_default().push(entry.path());
                    }
                }
            }
        }
    }

    let mut matches = Vec::new();
    for key in order {
        for (id, size) in &wanted[&key] {
            let candidates = found.get(&key).map(Vec::as_slice).unwrap_or_default();
            matches.push(Match { id: id.clone(), path: pick(candidates, *size).map(|p| p.to_string_lossy().into_owned()) });
        }
    }
    Ok(Found { matches, scanned })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_uris_are_not_local_files() {
        assert!(!is_local("tidal:track:1"));
        assert!(!is_local(""));
        assert!(is_local("/Users/a/b.mp3"));
        assert!(is_local("C:\\Music\\a.mp3"));
    }
}
