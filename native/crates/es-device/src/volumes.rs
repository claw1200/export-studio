//! Mounted volumes, and ejecting or formatting a removable one (macOS `diskutil`).

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::paths::{has_library, ROOT_NAMES};
use crate::{fail, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub path: String,
    pub has_library: bool,
    pub ejectable: bool,
    pub root_name: Option<String>,
    pub file_system: String,
    pub bytes_total: u64,
    pub bytes_used: u64,
    pub bytes_free: u64,
}

/// Run a command, killing it when it takes longer than `limit`.
fn run(program: &str, args: &[&str], limit: Duration) -> std::io::Result<Option<Output>> {
    let mut child = Command::new(program).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output().map(Some);
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn disk_info(path: &Path) -> plist::Dictionary {
    let Ok(Some(out)) = run("diskutil", &["info", "-plist", &path.to_string_lossy()], Duration::from_secs(3)) else {
        return plist::Dictionary::new();
    };
    plist::Value::from_reader(std::io::Cursor::new(out.stdout)).ok().and_then(plist::Value::into_dictionary).unwrap_or_default()
}

fn flag(info: &plist::Dictionary, key: &str) -> bool {
    info.get(key).and_then(plist::Value::as_boolean).unwrap_or(false)
}

fn text(info: &plist::Dictionary, key: &str) -> String {
    info.get(key).and_then(plist::Value::as_string).unwrap_or_default().to_owned()
}

fn ejectable(info: &plist::Dictionary) -> bool {
    (flag(info, "RemovableMedia") || flag(info, "Ejectable")) && !flag(info, "Internal")
}

fn usage(path: &Path) -> (u64, u64, u64) {
    let (Ok(total), Ok(free)) = (fs2::total_space(path), fs2::free_space(path)) else { return (0, 0, 0) };
    (total, total.saturating_sub(free), free)
}

fn root_name(volume: &Path) -> Option<String> {
    ROOT_NAMES.iter().find(|name| volume.join(name).join("rekordbox").is_dir()).map(|n| (*n).to_owned())
}

#[cfg(unix)]
fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.dev())
}

#[cfg(not(unix))]
fn device_of(_path: &Path) -> Option<u64> {
    None
}

/// Mounted volumes. Scanning the real `/Volumes` skips the boot volume and anything that is neither
/// removable nor holds a library; a test folder passed as `root` lists every directory.
#[must_use]
pub fn list(root: Option<&Path>) -> Vec<Volume> {
    let real = root.is_none();
    let base = root.unwrap_or_else(|| Path::new("/Volumes"));
    let Ok(entries) = std::fs::read_dir(base) else { return Vec::new() };
    let boot = if real { device_of(Path::new("/")) } else { None };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    paths.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    let mut out = Vec::new();
    for path in paths {
        let Some(dev) = device_of(&path).or(Some(0)) else { continue };
        if real && boot == Some(dev) {
            continue;
        }
        let library = has_library(&path);
        let info = if real { disk_info(&path) } else { plist::Dictionary::new() };
        let can_eject = real && ejectable(&info);
        if real && !library && !can_eject {
            continue;
        }
        let (bytes_total, bytes_used, bytes_free) = usage(&path);
        let file_system = if real {
            Some(text(&info, "FilesystemName")).filter(|s| !s.is_empty()).unwrap_or_else(|| text(&info, "FilesystemType"))
        } else {
            String::new()
        };
        out.push(Volume {
            id: path.to_string_lossy().into_owned(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            path: path.to_string_lossy().into_owned(),
            has_library: library,
            ejectable: can_eject,
            root_name: root_name(&path),
            file_system,
            bytes_total,
            bytes_used,
            bytes_free,
        });
    }
    out
}

fn listed<'a>(volume: &Path, volumes: &'a [Volume]) -> Option<&'a Volume> {
    let wanted = volume.canonicalize().ok()?;
    volumes.iter().find(|v| Path::new(&v.path).canonicalize().ok().as_ref() == Some(&wanted))
}

fn detail(output: &Output, fallback: &str) -> Error {
    let text = String::from_utf8_lossy(if output.stderr.is_empty() { &output.stdout } else { &output.stderr }).trim().to_owned();
    Error::Message(if text.is_empty() { fallback.to_owned() } else { text })
}

/// Eject a removable volume that appears in the device list.
pub fn eject(volume_id: &str, volumes: Option<&[Volume]>) -> Result<()> {
    let current;
    let volumes = match volumes {
        Some(v) => v,
        None => {
            current = list(None);
            &current
        }
    };
    let volume = Path::new(volume_id);
    if !listed(volume, volumes).is_some_and(|v| v.ejectable) {
        return fail("That device can’t be ejected");
    }
    match run("diskutil", &["eject", &volume.canonicalize()?.to_string_lossy()], Duration::from_secs(30))? {
        None => fail("Ejecting the device took too long"),
        Some(out) if out.status.success() => Ok(()),
        Some(out) => Err(detail(&out, "Couldn’t eject the device")),
    }
}

/// A FAT32 volume label: capitals and digits, at most 11 characters.
#[must_use]
pub fn fat_label(name: &str) -> String {
    let cleaned: String = name.to_uppercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let trimmed = cleaned.trim_matches('_');
    let label = if trimmed.is_empty() { "USB" } else { trimmed };
    label.chars().take(11).collect()
}

/// Erase a removable volume and format it FAT32.
pub fn format(volume_id: &str, volumes: Option<&[Volume]>) -> Result<()> {
    let current;
    let volumes = match volumes {
        Some(v) => v,
        None => {
            current = list(None);
            &current
        }
    };
    let volume = Path::new(volume_id);
    let Some(found) = listed(volume, volumes).filter(|v| v.ejectable) else {
        return fail("That device can’t be formatted");
    };
    let info = disk_info(volume);
    if !ejectable(&info) {
        return fail("That device can’t be formatted");
    }
    let device = text(&info, "DeviceIdentifier");
    if device.is_empty() {
        return fail("Couldn’t identify the device");
    }
    match run("diskutil", &["eraseVolume", "FAT32", &fat_label(&found.name), &device], Duration::from_secs(120))? {
        None => fail("Formatting the device took too long"),
        Some(out) if out.status.success() => Ok(()),
        Some(out) => Err(detail(&out, "Couldn’t format the device")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_fat_safe() {
        assert_eq!(fat_label("my stick!"), "MY_STICK");
        assert_eq!(fat_label("___"), "USB");
        assert_eq!(fat_label("A very long volume name"), "A_VERY_LONG");
    }

    #[test]
    fn a_test_folder_lists_every_directory_with_its_library_flag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Plain")).unwrap();
        std::fs::create_dir_all(dir.path().join("Stick/PIONEER/rekordbox")).unwrap();
        std::fs::write(dir.path().join("Stick/PIONEER/rekordbox/export.pdb"), b"x").unwrap();
        let found = list(Some(dir.path()));
        let summary: Vec<_> = found.iter().map(|v| (v.name.as_str(), v.has_library, v.root_name.as_deref())).collect();
        assert_eq!(summary, [("Plain", false, None), ("Stick", true, Some("PIONEER"))]);
    }

    #[test]
    fn only_listed_ejectable_volumes_can_be_ejected() {
        let dir = tempfile::tempdir().unwrap();
        let volume = Volume {
            id: dir.path().to_string_lossy().into_owned(),
            name: "x".into(),
            path: dir.path().to_string_lossy().into_owned(),
            has_library: false,
            ejectable: false,
            root_name: None,
            file_system: String::new(),
            bytes_total: 0,
            bytes_used: 0,
            bytes_free: 0,
        };
        assert!(eject(&volume.id, Some(std::slice::from_ref(&volume))).is_err());
        assert!(eject("/nowhere", Some(&[volume])).is_err());
    }
}
