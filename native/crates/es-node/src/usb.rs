//! USB sticks: listing and browsing them, importing from one, and syncing playlists to one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rb_device::{paths::pioneer_dir, volumes, Device, LibraryKind, Volume};
use es_engine::device_import::{self, PlaylistPolicy, TrackStatus};
use serde_json::{json, Value};

use crate::library::{failure, json_of, Core};
use es_engine::library::LibraryService;

pub(crate) enum UsbOp {
    List,
    Eject(String),
    Format(String),
    Settings(String),
    SetSettings(String, Value),
    Playlists(String),
    Libraries(String),
    Selection(String),
    Tracks(String, Option<String>),
    Detail(String),
    Media(String),
    ImportPreview { volume: String, folder: Option<String>, policy: Option<String> },
    Import { volume: String, folder: Option<String>, policy: Option<String> },
    SyncReview { volume: String, playlists: Vec<String>, convert: Option<serde_json::Value> },
    Sync { volume: String, playlists: Vec<String>, decisions: Option<serde_json::Value>, convert: Option<serde_json::Value> },
    RecoveryStatus(String),
    Recover(String),
}

type Signature = Vec<Option<(SystemTime, u64)>>;

/// Sticks already read, until their database files change.
#[derive(Default)]
pub(crate) struct Devices(Mutex<HashMap<String, Cached>>);

type Cached = (Signature, Option<Arc<Device>>);

fn signature(volume: &Path) -> Signature {
    let (root, _) = pioneer_dir(volume);
    ["rekordbox/export.pdb", "rekordbox/exportLibrary.db", "exportstudio/manifest.json"]
        .iter()
        .map(|rel| std::fs::metadata(root.join(rel)).ok().and_then(|m| Some((m.modified().ok()?, m.len()))))
        .collect()
}

impl Devices {
    pub(crate) fn load(&self, volume: &Path) -> napi::Result<Option<Arc<Device>>> {
        let key = volume.to_string_lossy().into_owned();
        let now = signature(volume);
        let mut cache = self.0.lock().map_err(|_| failure("the device cache was poisoned"))?;
        if let Some((seen, device)) = cache.get(&key) {
            if *seen == now {
                return Ok(device.clone());
            }
        }
        let device = rb_device::read::read(volume).map_err(failure)?.map(Arc::new);
        cache.insert(key, (now, device.clone()));
        Ok(device)
    }

    fn forget(&self, volume: &Path) {
        if let Ok(mut cache) = self.0.lock() {
            cache.remove(&volume.to_string_lossy().into_owned());
        }
    }
}

fn mounted(volume: &str) -> napi::Result<PathBuf> {
    let path = Path::new(volume).canonicalize().map_err(|_| failure(format!("Volume is not mounted: {volume}")))?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err(failure(format!("Volume is not mounted: {volume}")))
    }
}

/// A volume the device list offers; anything else is refused.
fn listed(volume: &str) -> napi::Result<PathBuf> {
    let path = mounted(volume)?;
    let allowed = volumes::list(None).iter().filter_map(|v| Path::new(&v.path).canonicalize().ok()).any(|p| p == path);
    if allowed {
        Ok(path)
    } else {
        Err(failure("Choose a volume from the device list"))
    }
}

fn publish_volumes(core: &Core) {
    core.emit("usb.volumes", &json!({ "volumes": volumes::list(None) }));
}

fn sync_review_value(review: &impl serde::Serialize) -> napi::Result<serde_json::Value> {
    let mut value = json_of(review)?;
    if let Some(bytes) = value["plan"]["estimatedPeakBytes"].as_u64() {
        value["plan"]["estimatedPeakBytes"] = json!(bytes as f64);
    }
    Ok(value)
}

/// The format a sync converts to; none when the caller sent none.
fn preset(raw: Option<&Value>) -> napi::Result<Option<es_engine::convert::Preset>> {
    raw.filter(|v| !v.is_null()).map(es_engine::convert::clean_preset).transpose().map_err(failure)
}

fn volume_value(volume: Volume) -> serde_json::Value {
    json!({
        "id": volume.id,
        "name": volume.name,
        "path": volume.path,
        "hasLibrary": volume.has_library,
        "ejectable": volume.ejectable,
        "rootName": volume.root_name,
        "fileSystem": volume.file_system,
        "bytesTotal": volume.bytes_total as f64,
        "bytesUsed": volume.bytes_used as f64,
        "bytesFree": volume.bytes_free as f64,
    })
}

fn policy(name: Option<&str>) -> napi::Result<PlaylistPolicy> {
    match name {
        None | Some("" | "skipExisting") => Ok(PlaylistPolicy::SkipExisting),
        Some("merge") => Ok(PlaylistPolicy::Merge),
        Some("createNew") => Ok(PlaylistPolicy::CreateNew),
        Some(other) => Err(failure(format!("Unknown playlist policy: {other}"))),
    }
}

fn import_options(folder: &Option<String>, policy_name: &Option<String>) -> napi::Result<device_import::Options> {
    Ok(device_import::Options {
        folder: folder.as_deref().filter(|f| !f.is_empty()).map(PathBuf::from),
        music: None,
        policy: policy(policy_name.as_deref())?,
    })
}

fn require(device: Option<Arc<Device>>) -> napi::Result<Arc<Device>> {
    device.ok_or_else(|| failure("This volume has no rekordbox library"))
}

pub(crate) fn perform(core: &Core, service: &mut LibraryService, op: &UsbOp) -> napi::Result<Value> {
    match op {
        UsbOp::List => {
            let found: Vec<_> = volumes::list(None).into_iter().map(volume_value).collect();
            json_of(&found)
        }
        UsbOp::Eject(id) => {
            volumes::eject(id, None).map_err(failure)?;
            publish_volumes(core);
            Ok(Value::Null)
        }
        UsbOp::Format(id) => {
            volumes::format(id, None).map_err(failure)?;
            core.devices.forget(Path::new(id));
            publish_volumes(core);
            Ok(Value::Null)
        }
        UsbOp::Settings(id) => {
            let volume = listed(id)?;
            let settings = rb_device::settings::read(&volume).map_err(failure)?;
            Ok(crate::usb_settings::view(&settings))
        }
        UsbOp::SetSettings(id, value) => {
            let volume = listed(id)?;
            if rb_library::rekordbox_running() {
                return Err(failure("Close rekordbox before changing device settings"));
            }
            let current = rb_device::settings::read(&volume).map_err(failure)?;
            let next = crate::usb_settings::edited(&current, value)?;
            rb_device::settings::write_changes(&volume, &current, &next).map_err(failure)?;
            core.devices.forget(&volume);
            let saved = rb_device::settings::read(&volume).map_err(failure)?;
            Ok(crate::usb_settings::view(&saved))
        }
        UsbOp::Playlists(volume) => {
            let volume = mounted(volume)?;
            let nodes = core.devices.load(&volume)?.map(|d| es_index::device::playlists(&d)).unwrap_or_default();
            json_of(&nodes)
        }
        UsbOp::Libraries(volume) => {
            let volume = mounted(volume)?;
            let mut found = Vec::new();
            for kind in [LibraryKind::DeviceLibrary, LibraryKind::OneLibrary] {
                if let Some(device) = rb_device::read::read_kind(&volume, kind).map_err(failure)? {
                    found.push(es_index::device::library_view(kind, &device));
                }
            }
            json_of(&found)
        }
        UsbOp::Selection(volume) => {
            let (ids, db_id) = service.playlist_selection().map_err(failure)?;
            json_of(&json!({ "playlistIds": rb_device::selection::recorded_playlist_ids(&mounted(volume)?, &ids, db_id) }))
        }
        UsbOp::Tracks(volume, playlist) => {
            let volume = mounted(volume)?;
            match core.devices.load(&volume)? {
                None => Ok(json!([])),
                Some(device) => json_of(&es_index::device::tracks(&volume, &device, playlist.as_deref()).map_err(failure)?),
            }
        }
        UsbOp::Detail(key) => {
            let (volume, export_id) = es_index::device::parse_track_key(key).ok_or_else(|| failure("Not a device track"))?;
            let volume = mounted(&volume)?;
            let device = require(core.devices.load(&volume)?)?;
            let track = device.track(export_id).ok_or_else(|| failure(format!("Unknown device track {key}")))?;
            json_of(&es_index::device::detail(&volume, key, track))
        }
        UsbOp::Media(key) => {
            let found = es_index::device::parse_track_key(key).and_then(|(volume, export_id)| {
                let volume = mounted(&volume).ok()?;
                let device = core.devices.load(&volume).ok()??;
                es_index::device::audio_file(&volume, device.track(export_id)?)
            });
            json_of(&found)
        }
        UsbOp::ImportPreview { volume, folder, policy } => {
            let state = service.session().ok_or_else(|| failure("Library not open"))?;
            let volume = listed(volume)?;
            let device = require(core.devices.load(&volume)?)?;
            let plan = device_import::preview(state.database(), &volume, &device, &import_options(folder, policy)?).map_err(failure)?;
            let status = |s: TrackStatus| match s {
                TrackStatus::Reuse => "reuse",
                TrackStatus::Copy => "copy",
                TrackStatus::Missing => "missing",
            };
            let count = |wanted: TrackStatus| plan.tracks.iter().filter(|t| t.status == wanted).count();
            json_of(&json!({
                "destination": plan.destination,
                "layout": plan.layout,
                "playlists": plan.playlists.iter().map(|p| json!({
                    "name": p.name, "folder": p.folder, "depth": p.depth, "tracks": p.tracks, "exists": p.exists,
                })).collect::<Vec<_>>(),
                "tracks": plan.tracks.iter().map(|t| json!({
                    "title": t.title, "artist": t.artist, "status": status(t.status),
                })).collect::<Vec<_>>(),
                "counts": {
                    "reuse": count(TrackStatus::Reuse),
                    "copy": count(TrackStatus::Copy),
                    "missing": count(TrackStatus::Missing),
                },
            }))
        }
        UsbOp::Import { volume, folder, policy } => {
            let volume = listed(volume)?;
            let device = require(core.devices.load(&volume)?)?;
            let options = import_options(folder, policy)?;
            let emit = |event| core.emit_service_event(event);
            let outcome = service.device_import(&volume, &device, &options, &emit).map_err(failure)?;
            core.emit("usb.progress", &json!({ "phase": "done" }));
            json_of(&json!({
                "playlists": outcome.playlists,
                "tracks": outcome.tracks,
                "copied": outcome.copied,
                "reused": outcome.reused,
                "failed": outcome.failed.iter().map(|(title, error)| json!({ "title": title, "error": error })).collect::<Vec<_>>(),
            }))
        }
        UsbOp::SyncReview { volume, playlists, convert } => {
            let volume = listed(volume)?;
            let convert = preset(convert.as_ref())?;
            let review = service.usb_sync_review(&volume, playlists, convert.as_ref()).map_err(failure)?;
            sync_review_value(&review)
        }
        UsbOp::RecoveryStatus(volume) => {
            let volume = listed(volume)?;
            json_of(&json!({ "required": service.usb_recovery_required(&volume) }))
        }
        UsbOp::Recover(volume) => {
            let volume = listed(volume)?;
            service.usb_recover(&volume).map_err(failure)?;
            core.devices.forget(&volume);
            publish_volumes(core);
            Ok(json!({ "recovered": true }))
        }
        UsbOp::Sync { volume, playlists, decisions, convert } => {
            let volume = listed(volume)?;
            let emit = |event| core.emit_service_event(event);
            let decisions = decisions.as_ref().map(|value| serde_json::from_value(value.clone())).transpose().map_err(failure)?;
            let convert = preset(convert.as_ref())?;
            let result = service.usb_sync(&volume, playlists, decisions.as_ref(), convert.as_ref(), &emit);
            core.devices.forget(&volume);
            let result = result.map_err(failure)?;
            publish_volumes(core);
            json_of(&json!({ "volumeId": result.volume, "tracks": result.tracks, "playlists": result.playlists }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_review_estimates_above_four_gib_are_serialized_as_numbers() {
        let review = json!({
            "token": "review-token",
            "diagnostics": [],
            "plan": { "tracks": 602, "playlists": 20, "estimatedPeakBytes": 20_231_004_590_u64, "approximate": true },
        });
        let value = sync_review_value(&review).unwrap();
        assert!(value["plan"]["estimatedPeakBytes"].is_f64());
        assert_eq!(value["plan"]["estimatedPeakBytes"].as_f64(), Some(20_231_004_590.0));
        assert_eq!(value["token"], review["token"]);
        assert_eq!(value["diagnostics"], review["diagnostics"]);
        assert_eq!(value["plan"]["tracks"], 602);
    }

    #[test]
    fn sync_review_preserves_absent_estimates() {
        for plan in [serde_json::Value::Null, json!({ "estimatedPeakBytes": null })] {
            let review = json!({ "token": "review-token", "diagnostics": [], "plan": plan });
            assert_eq!(sync_review_value(&review).unwrap(), review);
        }
    }

    #[test]
    fn volume_capacities_are_serialized_as_numbers() {
        let value = volume_value(Volume {
            id: "/Volumes/USB".into(),
            name: "USB".into(),
            path: "/Volumes/USB".into(),
            has_library: true,
            ejectable: true,
            root_name: None,
            file_system: "APFS".into(),
            bytes_total: 4096,
            bytes_used: 1024,
            bytes_free: 3072,
        });

        assert_eq!(value["bytesTotal"].as_f64(), Some(4096.0));
        assert_eq!(value["bytesUsed"].as_f64(), Some(1024.0));
        assert_eq!(value["bytesFree"].as_f64(), Some(3072.0));
    }
}
