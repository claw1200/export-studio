//! Zip backups of the rekordbox library: the database, playlist XML files and the share folder
//! (analysis files and artwork). Audio files are never included. The layout matches what earlier
//! versions wrote, so their backups keep working.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{Local, NaiveDateTime, TimeZone};
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, Error>;

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::Message(message.into()))
}

const DB_ARC: &str = "master.db";
const DB_SUFFIXES: [&str; 2] = ["", "-wal"];
const SIDE_FILES: [&str; 2] = ["masterPlaylists6.xml", "automixPlaylist6.xml"];
const MANIFEST: &str = "manifest.json";
const SETTINGS: &str = "settings.json";
const AUTO_REASONS: [&str; 2] = ["before-edit", "daily"];
const REASONS: [&str; 4] = ["before-edit", "daily", "manual", "pre-restore"];
const KEEP_CHOICES: [i64; 3] = [5, 10, 20];
const INTERVAL_CHOICES: [i64; 6] = [1, 3, 6, 12, 24, 48];
const STORED_SUFFIXES: [&str; 4] = ["jpg", "jpeg", "png", "webp"];
const COPY_ATTEMPTS: usize = 3;
const FORMAT_VERSION: i64 = 1;
const BLOCKED: &str = "Close rekordbox before restoring a backup";

/// What a database held when it was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub tracks: i64,
    pub playlists: i64,
}

pub type Progress<'a> = &'a dyn Fn(Value);

type Verify = Box<dyn Fn(&Path) -> Result<Counts> + Send + Sync>;

pub struct Backups {
    root: PathBuf,
    verify: Verify,
    running: Box<dyn Fn() -> bool + Send + Sync>,
}

fn share_root(db_dir: &Path) -> PathBuf {
    db_dir.join("share").join("PIONEER")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk(&path, out);
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

/// Every file under `share/PIONEER`, sorted.
fn share_files(db_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(&share_root(db_dir), &mut out);
    out.sort();
    out
}

fn mtime_ns(meta: &fs::Metadata) -> i128 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as i128)
}

/// Changes whenever the database, the side files or anything in the share folder changes.
#[must_use]
pub fn signature(src: &Path) -> Value {
    let dir = src.parent().unwrap_or_else(|| Path::new("."));
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut names: Vec<String> = DB_SUFFIXES.iter().map(|s| format!("{name}{s}")).collect();
    names.extend(SIDE_FILES.iter().map(|s| (*s).to_owned()));
    let mut sig: Vec<Value> = names
        .into_iter()
        .map(|n| match fs::metadata(dir.join(&n)) {
            Ok(m) => json!([n, mtime_ns(&m) as i64, m.len()]),
            Err(_) => json!([n, null, null]),
        })
        .collect();
    let (mut count, mut size, mut newest) = (0u64, 0u64, 0i128);
    for path in share_files(dir) {
        if let Ok(m) = fs::metadata(&path) {
            count += 1;
            size += m.len();
            newest = newest.max(mtime_ns(&m));
        }
    }
    sig.push(json!(["share", count, size, newest as i64]));
    Value::Array(sig)
}

fn safe_member(name: &str) -> bool {
    let path = Path::new(name);
    if path.is_absolute() || name.contains('\\') || path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return false;
    }
    let parts: Vec<_> = path.components().collect();
    name == MANIFEST
        || DB_SUFFIXES.iter().any(|s| name == format!("{DB_ARC}{s}"))
        || SIDE_FILES.contains(&name)
        || (parts.len() > 2 && path.starts_with("share/PIONEER"))
}

fn move_path(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, to)?;
        fs::remove_dir_all(from)?;
    } else {
        fs::copy(from, to)?;
        fs::remove_file(from)?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn read_manifest(path: &Path) -> Result<Value> {
    let mut zip = ZipArchive::new(File::open(path)?)?;
    let mut text = String::new();
    zip.by_name(MANIFEST)?.read_to_string(&mut text)?;
    serde_json::from_str(&text).map_err(|error| Error::Message(error.to_string()))
}

fn is_automatic_archive_name(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(timestamped_reason) = stem.strip_prefix("exportstudio-") else {
        return false;
    };
    let mut parts = timestamped_reason.splitn(3, '-');
    let Some(date) = parts.next() else { return false };
    let Some(time) = parts.next() else { return false };
    let Some(reason) = parts.next() else { return false };
    if date.len() != 8 || !date.bytes().all(|byte| byte.is_ascii_digit()) || time.len() != 6 || !time.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    AUTO_REASONS.iter().any(|automatic| {
        reason == *automatic
            || reason
                .strip_prefix(&format!("{automatic}-"))
                .is_some_and(|suffix| suffix.parse::<u32>().is_ok())
    })
}

fn automatic_archive_created(path: &Path) -> Option<f64> {
    let stem = path.file_stem()?.to_str()?.strip_prefix("exportstudio-")?;
    let mut parts = stem.splitn(3, '-');
    let date = parts.next()?;
    let time = parts.next()?;
    let _reason = parts.next()?;
    if !is_automatic_archive_name(path) {
        return None;
    }
    let local = NaiveDateTime::parse_from_str(&format!("{date}-{time}"), "%Y%m%d-%H%M%S").ok()?;
    Local
        .from_local_datetime(&local)
        .single()
        .map(|created| created.timestamp_millis() as f64 / 1000.0)
}

fn temp_dir(prefix: &str) -> Result<PathBuf> {
    let base = std::env::temp_dir();
    for n in 0..1000u32 {
        let path = base.join(format!("{prefix}{}-{n}", std::process::id()));
        if fs::create_dir(&path).is_ok() {
            return Ok(path);
        }
    }
    fail("could not make a temporary folder")
}

impl Backups {
    pub fn new(
        root: impl Into<PathBuf>,
        verify: impl Fn(&Path) -> Result<Counts> + Send + Sync + 'static,
        running: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        Self { root: root.into(), verify: Box::new(verify), running: Box::new(running) }
    }

    #[must_use]
    pub fn settings(&self) -> Value {
        let stored: Value = fs::read_to_string(self.root.join(SETTINGS))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let mut out = json!({ "auto": true, "keep": 10, "interval": 12 });
        if let Some(auto) = stored.get("auto").and_then(Value::as_bool) {
            out["auto"] = json!(auto);
        }
        if let Some(keep) = json_i64(&stored, "keep").filter(|k| KEEP_CHOICES.contains(k)) {
            out["keep"] = json!(keep);
        }
        if let Some(interval) = json_i64(&stored, "interval").filter(|i| INTERVAL_CHOICES.contains(i)) {
            out["interval"] = json!(interval);
        }
        out
    }

    pub fn set_settings(&self, auto: Option<bool>, keep: Option<i64>, interval: Option<i64>) -> Result<Value> {
        let mut out = self.settings();
        if let Some(auto) = auto {
            out["auto"] = json!(auto);
        }
        if let Some(keep) = keep {
            if !KEEP_CHOICES.contains(&keep) {
                return fail("Keep must be one of 5, 10, 20");
            }
            out["keep"] = json!(keep);
        }
        if let Some(interval) = interval {
            if !INTERVAL_CHOICES.contains(&interval) {
                return fail("Interval must be one of 1, 3, 6, 12, 24, 48 hours");
            }
            out["interval"] = json!(interval);
        }
        fs::create_dir_all(&self.root)?;
        fs::write(self.root.join(SETTINGS), out.to_string())?;
        self.prune(json_i64(&out, "keep").unwrap_or(10) as usize);
        Ok(out)
    }

    /// Every readable backup, newest first.
    #[must_use]
    pub fn list(&self, src: Option<&Path>) -> Vec<Value> {
        let Ok(entries) = fs::read_dir(&self.root) else { return Vec::new() };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "zip") {
                continue;
            }
            let manifest = read_manifest(&path);
            let Ok(mut manifest) = manifest else { continue };
            let Some(map) = manifest.as_object_mut() else { continue };
            map.insert("id".into(), json!(path.file_stem().map(|s| s.to_string_lossy().into_owned())));
            map.insert("path".into(), json!(path.to_string_lossy()));
            map.insert("size".into(), json!(fs::metadata(&path).map_or(0, |m| m.len())));
            let other = src.is_some_and(|s| map.get("dbPath").and_then(Value::as_str) != Some(&s.to_string_lossy()));
            map.insert("otherLibrary".into(), json!(other));
            out.push(manifest);
        }
        out.sort_by(|a, b| {
            let created = |v: &Value| v.get("created").and_then(Value::as_f64).unwrap_or(0.0);
            created(b).partial_cmp(&created(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    fn is_automatic(manifest: &Value) -> bool {
        manifest.get("reason").and_then(Value::as_str).is_some_and(|r| AUTO_REASONS.contains(&r))
    }

    #[must_use]
    pub fn latest_automatic(&self) -> Option<Value> {
        let Ok(entries) = fs::read_dir(&self.root) else { return None };
        let mut candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "zip") && is_automatic_archive_name(path))
            .collect();
        candidates.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));

        for path in candidates.into_iter().rev() {
            let Ok(mut manifest) = read_manifest(&path) else { continue };
            if !Self::is_automatic(&manifest) {
                continue;
            }
            let Some(map) = manifest.as_object_mut() else { continue };
            map.insert("id".into(), json!(path.file_stem().map(|name| name.to_string_lossy().into_owned())));
            map.insert("path".into(), json!(path.to_string_lossy()));
            map.insert("size".into(), json!(fs::metadata(&path).map_or(0, |metadata| metadata.len())));
            map.insert("otherLibrary".into(), json!(false));
            return Some(manifest);
        }

        self.list(None).into_iter().find(Self::is_automatic)
    }

    /// Creation time from the timestamped filename, avoiding a scan of ZIP manifests.
    #[must_use]
    pub fn latest_automatic_created(&self) -> Option<f64> {
        let Ok(entries) = fs::read_dir(&self.root) else { return None };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter_map(|path| automatic_archive_created(&path))
            .max_by(f64::total_cmp)
    }

    /// Back up the library at `src`. An automatic backup is skipped (`None`) when nothing changed.
    pub fn create(&self, src: &Path, reason: &str, progress: Progress<'_>) -> Result<Option<Value>> {
        if !REASONS.contains(&reason) {
            return fail(format!("Unknown backup reason: {reason}"));
        }
        if !src.is_file() {
            return fail(format!("Can't find the rekordbox library at {}", src.display()));
        }
        let sig = signature(src);
        if AUTO_REASONS.contains(&reason) {
            if let Some(latest) = self.list(None).first() {
                if latest.get("signature") == Some(&sig) {
                    return Ok(None);
                }
            }
        }
        progress(json!({ "stage": "checking", "reason": reason, "done": 0, "total": 0 }));
        let snap = temp_dir("exportstudio-backup-")?;
        let result = (|| -> Result<Value> {
            let counts = self.copy_verified(src, &snap)?;
            let name = src.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let dir = src.parent().unwrap_or_else(|| Path::new("."));
            let mut entries: Vec<(PathBuf, String)> = Vec::new();
            for suffix in DB_SUFFIXES {
                let copy = snap.join(format!("{name}{suffix}"));
                if copy.exists() {
                    entries.push((copy, format!("{DB_ARC}{suffix}")));
                }
            }
            for side in SIDE_FILES {
                if dir.join(side).is_file() {
                    entries.push((dir.join(side), side.to_owned()));
                }
            }
            for path in share_files(dir) {
                let arc = path.strip_prefix(dir).unwrap_or(&path).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                entries.push((path, arc));
            }
            let manifest = json!({
                "version": FORMAT_VERSION, "reason": reason, "dbPath": src.to_string_lossy(), "verified": true,
                "signature": sig, "tracks": counts.tracks, "playlists": counts.playlists,
            });
            self.write(&entries, manifest, reason, progress)
        })();
        let _ = fs::remove_dir_all(&snap);
        let result = result?;
        if AUTO_REASONS.contains(&reason) {
            self.prune(json_i64(&self.settings(), "keep").unwrap_or(10) as usize);
        }
        Ok(Some(result))
    }

    fn copy_verified(&self, src: &Path, snap: &Path) -> Result<Counts> {
        let name = src.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut last = String::new();
        for attempt in 0..COPY_ATTEMPTS {
            for suffix in ["", "-wal", "-shm"] {
                let _ = fs::remove_file(snap.join(format!("{name}{suffix}")));
            }
            for suffix in DB_SUFFIXES {
                let from = src.with_file_name(format!("{name}{suffix}"));
                if from.exists() {
                    fs::copy(&from, snap.join(format!("{name}{suffix}")))?;
                }
            }
            match (self.verify)(&snap.join(&name)) {
                Ok(counts) => return Ok(counts),
                Err(e) => {
                    last = e.to_string();
                    if attempt + 1 < COPY_ATTEMPTS {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                }
            }
        }
        fail(format!("The library could not be backed up because it failed its check: {last}"))
    }

    fn write(&self, entries: &[(PathBuf, String)], manifest: Value, reason: &str, progress: Progress<'_>) -> Result<Value> {
        fs::create_dir_all(&self.root)?;
        let now = chrono::Local::now();
        let base = format!("exportstudio-{}-{reason}", now.format("%Y%m%d-%H%M%S"));
        let mut stem = base.clone();
        let mut n = 1;
        while self.root.join(format!("{stem}.zip")).exists() {
            n += 1;
            stem = format!("{base}-{n}");
        }
        let final_path = self.root.join(format!("{stem}.zip"));
        let partial = self.root.join(format!("{stem}.zip.partial"));
        let bytes: u64 = entries.iter().map(|(p, _)| fs::metadata(p).map_or(0, |m| m.len())).sum();
        let mut manifest = manifest;
        if let Some(map) = manifest.as_object_mut() {
            map.insert("created".into(), json!(now.timestamp_millis() as f64 / 1000.0));
            map.insert("createdAt".into(), json!(now.format("%Y-%m-%dT%H:%M:%S").to_string()));
            map.insert("files".into(), json!(entries.len()));
            map.insert("bytes".into(), json!(bytes));
        }
        let total = entries.len();
        let written = (|| -> Result<()> {
            let mut zip = ZipWriter::new(File::create(&partial)?);
            let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).compression_level(Some(1)).large_file(true);
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored).large_file(true);
            for (done, (path, arc)) in entries.iter().enumerate() {
                let is_image = path.extension().and_then(|e| e.to_str()).is_some_and(|e| STORED_SUFFIXES.contains(&e.to_ascii_lowercase().as_str()));
                zip.start_file(arc, if is_image { stored } else { deflated })?;
                io::copy(&mut File::open(path)?, &mut zip)?;
                let done = done + 1;
                if done % 50 == 0 || done == total {
                    progress(json!({ "stage": "saving", "reason": reason, "done": done, "total": total }));
                }
            }
            zip.start_file(MANIFEST, deflated)?;
            zip.write_all(serde_json::to_string_pretty(&manifest).map_err(|e| Error::Message(e.to_string()))?.as_bytes())?;
            zip.finish()?;
            fs::rename(&partial, &final_path)?;
            Ok(())
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&partial);
            return Err(e);
        }
        if let Some(map) = manifest.as_object_mut() {
            map.insert("id".into(), json!(stem));
            map.insert("path".into(), json!(final_path.to_string_lossy()));
            map.insert("size".into(), json!(fs::metadata(&final_path).map_or(0, |m| m.len())));
            map.insert("otherLibrary".into(), json!(false));
        }
        Ok(manifest)
    }

    fn prune(&self, keep: usize) {
        let automatic: Vec<Value> = self.list(None).into_iter().filter(Self::is_automatic).collect();
        for manifest in automatic.into_iter().skip(keep) {
            if let Some(path) = manifest.get("path").and_then(Value::as_str) {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn path_of(&self, id: &str) -> Result<PathBuf> {
        let path = self.root.join(format!("{id}.zip"));
        if Path::new(id).file_name().is_none_or(|n| n != id) || !path.is_file() {
            return fail(format!("Unknown backup {id}"));
        }
        Ok(path)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        Ok(fs::remove_file(self.path_of(id)?)?)
    }

    /// Put a backup in place of the library at `src`. The files it replaces are kept as a
    /// pre-restore backup, or in a `replaced-*` folder if that cannot be written.
    pub fn restore(&self, id: &str, src: &Path, progress: Progress<'_>) -> Result<Value> {
        if (self.running)() {
            return fail(BLOCKED);
        }
        let path = self.path_of(id)?;
        let db_dir = src.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(db_dir)?;
        progress(json!({ "stage": "checking", "reason": "restore", "done": 0, "total": 0 }));

        let mut zip = ZipArchive::new(File::open(&path)?)?;
        let names: Vec<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().filter(|f| !f.is_dir()).and_then(|f| f.name().ok().map(|n| n.into_owned()))).collect();
        for name in &names {
            let mut sink = io::sink();
            io::copy(&mut zip.by_name(name)?, &mut sink).map_err(|_| Error::Message(format!("The backup is damaged ({name})")))?;
        }
        if let Some(bad) = names.iter().find(|n| !safe_member(n)) {
            return fail(format!("The backup contains an unexpected file: {bad}"));
        }
        if !names.iter().any(|n| n == DB_ARC) {
            return fail("The backup has no database in it");
        }
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let staging = db_dir.join(format!(".exportstudio-restore-{stamp}"));
        fs::create_dir_all(&staging)?;
        let outcome = (|| -> Result<(PathBuf, Counts)> {
            let members: Vec<&String> = names.iter().filter(|n| *n != MANIFEST).collect();
            for (done, name) in members.iter().enumerate() {
                let target = staging.join(name);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                io::copy(&mut zip.by_name(name)?, &mut File::create(&target)?)?;
                let done = done + 1;
                if done % 50 == 0 || done == members.len() {
                    progress(json!({ "stage": "extracting", "reason": "restore", "done": done, "total": members.len() }));
                }
            }
            let name = src.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let check = temp_dir("exportstudio-check-")?;
            let counts = (|| -> Result<Counts> {
                for suffix in DB_SUFFIXES {
                    let from = staging.join(format!("{DB_ARC}{suffix}"));
                    if from.exists() {
                        fs::copy(&from, check.join(format!("{name}{suffix}")))?;
                    }
                }
                (self.verify)(&check.join(&name))
            })();
            let _ = fs::remove_dir_all(&check);
            let counts = counts?;
            Ok((self.swap(&staging, src, &stamp)?, counts))
        })();
        let _ = fs::remove_dir_all(&staging);
        let (replaced, counts) = outcome?;
        let kept = self.keep_replaced(&replaced, src);
        Ok(json!({ "restored": id, "kept": kept, "tracks": counts.tracks, "playlists": counts.playlists }))
    }

    fn swap(&self, staging: &Path, src: &Path, stamp: &str) -> Result<PathBuf> {
        let db_dir = src.parent().unwrap_or_else(|| Path::new("."));
        let name = src.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let replaced = self.root.join(format!("replaced-{stamp}"));
        let mut targets: Vec<(String, String)> = DB_SUFFIXES.iter().map(|s| (format!("{name}{s}"), format!("{DB_ARC}{s}"))).collect();
        targets.extend(SIDE_FILES.iter().map(|n| ((*n).to_owned(), (*n).to_owned())));
        targets.push(("share/PIONEER".into(), "share/PIONEER".into()));
        let mut moved_out = Vec::new();
        let mut moved_in = Vec::new();
        let attempt = (|| -> Result<()> {
            let shm = db_dir.join(format!("{name}-shm"));
            if shm.exists() {
                move_path(&shm, &replaced.join(format!("{name}-shm")))?;
                moved_out.push(format!("{name}-shm"));
            }
            for (live, _) in &targets {
                if db_dir.join(live).exists() {
                    move_path(&db_dir.join(live), &replaced.join(live))?;
                    moved_out.push(live.clone());
                }
            }
            for (live, arc) in &targets {
                if staging.join(arc).exists() {
                    move_path(&staging.join(arc), &db_dir.join(live))?;
                    moved_in.push(live.clone());
                }
            }
            Ok(())
        })();
        if let Err(e) = attempt {
            for live in &moved_in {
                let target = db_dir.join(live);
                if target.is_dir() {
                    let _ = fs::remove_dir_all(&target);
                } else {
                    let _ = fs::remove_file(&target);
                }
            }
            for live in &moved_out {
                let _ = move_path(&replaced.join(live), &db_dir.join(live));
            }
            let _ = fs::remove_dir_all(&replaced);
            return Err(e);
        }
        Ok(replaced)
    }

    fn keep_replaced(&self, replaced: &Path, src: &Path) -> Option<String> {
        if !replaced.is_dir() {
            return None;
        }
        let name = src.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut entries: Vec<(PathBuf, String)> = Vec::new();
        for suffix in DB_SUFFIXES {
            let p = replaced.join(format!("{name}{suffix}"));
            if p.exists() {
                entries.push((p, format!("{DB_ARC}{suffix}")));
            }
        }
        for side in SIDE_FILES {
            if replaced.join(side).is_file() {
                entries.push((replaced.join(side), side.to_owned()));
            }
        }
        let mut share = Vec::new();
        walk(&replaced.join("share").join("PIONEER"), &mut share);
        share.sort();
        for p in share {
            let arc = p.strip_prefix(replaced).unwrap_or(&p).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            entries.push((p, arc));
        }
        if !replaced.join(&name).exists() || entries.is_empty() {
            let _ = fs::remove_dir_all(replaced);
            return None;
        }
        let manifest = json!({ "version": FORMAT_VERSION, "reason": "pre-restore", "dbPath": src.to_string_lossy(), "verified": false });
        match self.write(&entries, manifest, "pre-restore", &|_| {}) {
            Ok(done) => {
                let _ = fs::remove_dir_all(replaced);
                done.get("path").and_then(Value::as_str).map(str::to_owned)
            }
            Err(e) => {
                eprintln!("[es-backup] could not zip the replaced library, leaving it in {}: {e}", replaced.display());
                Some(replaced.to_string_lossy().into_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests;
