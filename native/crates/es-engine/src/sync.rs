//! Copy playlists onto a stick. The native `es-usb` program writes the device databases; this builds its job
//! from the library and relays its progress.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use es_db::export::{Bundle, ExportTrack};
use es_db::Library;
use serde_json::{json, Value};

use crate::{Error, Result};

/// What rekordbox's own analysis marks a fully analysed track with.
const ANALYSED: i64 = 105;
const JOB_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone)]
pub enum Progress {
    Step { phase: String, done: Option<i64>, total: Option<i64>, name: String },
    Done { tracks: i64, playlists: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub volume: String,
    pub tracks: i64,
    pub playlists: i64,
}

fn num(text: &str) -> i64 {
    text.trim().parse().unwrap_or(0)
}

/// The directory holding rekordbox My Settings, when this machine has them.
#[must_use]
pub fn find_settings() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?);
    let bases = [
        home.join("Library/Application Support/Pioneer"),
        home.join("Library/Pioneer"),
        home.join("AppData/Roaming/Pioneer"),
    ];
    fn search(dir: &Path, depth: u32) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_file() && entry.file_name() == "MYSETTING.DAT" {
                return Some(dir.to_path_buf());
            }
            if depth > 0 && path.is_dir() {
                if let Some(found) = search(&path, depth - 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    bases.iter().filter(|b| b.is_dir()).find_map(|b| search(b, 6))
}

/// Where the export program lives: the environment, next to the app, or the native build.
pub fn exporter() -> Result<PathBuf> {
    let name = if cfg!(windows) { "es-usb.exe" } else { "es-usb" };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(env) = std::env::var_os("EXPORTSTUDIO_USB_EXPORT") {
        candidates.push(PathBuf::from(env));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join(name));
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    candidates.push(repo.join("native/target/release").join(name));
    candidates.push(repo.join("native/target/debug").join(name));
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| Error::Other("The USB export program is not built. From the app directory, run npm run build:native.".into()))
}

fn analysis_files(share: &Path, track: &ExportTrack) -> Vec<String> {
    let Some(rel) = &track.analysis_path else { return Vec::new() };
    let dat = share.join(rel.trim_matches(['/', '\\'])).with_extension("DAT");
    ["DAT", "EXT", "2EX"]
        .iter()
        .map(|kind| dat.with_extension(kind))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

fn track_job(share: &Path, track: &ExportTrack) -> Value {
    let analysis = analysis_files(share, track);
    let analysed = if track.analysed <= 0 && !analysis.is_empty() { ANALYSED } else { track.analysed };
    let artwork = track
        .image_path
        .as_ref()
        .map(|i| share.join(i.trim_start_matches(['/', '\\'])))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned());
    let cues: Vec<Value> = track
        .cues
        .iter()
        .map(|c| {
            json!({
                "kind": c.kind, "timeMs": c.start_ms.max(0), "endMs": c.end_ms, "color": c.color,
                "colorCode": c.color_index.unwrap_or(0), "comment": c.comment,
                "activeLoop": c.active_loop, "beatLoopSize": c.beat_loop.max(0),
            })
        })
        .collect();
    let tags: Vec<i64> = track.my_tags.iter().map(|t| num(t)).filter(|t| *t != 0).collect();
    json!({
        "id": num(&track.id), "sourcePath": track.folder_path, "title": track.title, "artist": track.artist,
        "album": track.album, "genre": track.genre, "label": track.label, "key": track.key, "comment": track.comment,
        "dateAdded": track.date_added, "releaseDate": track.release_date, "bpmX100": track.bpm_x100.max(0),
        "durationSec": track.length.max(0), "rating": track.rating.max(0), "colorId": track.color.max(0),
        "year": track.year.max(0), "bitrate": track.bit_rate.max(0), "sampleRate": track.sample_rate.max(0),
        "fileSize": track.file_size.max(0), "analysis": analysis, "artwork": artwork, "myTags": tags,
        "metadata": {
            "trackNumber": track.track_no.max(0), "discNumber": track.disc_no.max(0), "bitDepth": track.bit_depth.max(0),
            "playCount": track.play_count.max(0), "analysed": analysed.max(0), "hotCueAutoLoad": track.hot_cue_auto,
            "dateCreated": track.date_created, "isrc": track.isrc,
        },
        "cues": cues,
    })
}

fn job(share: &Path, volume: &Path, bundle: &Bundle) -> Result<Value> {
    let tracks: Vec<Value> = bundle.tracks.iter().map(|t| track_job(share, t)).collect();
    let known: std::collections::HashSet<i64> = bundle.tracks.iter().map(|t| num(&t.id)).collect();
    let mut playlists = Vec::new();
    for row in &bundle.playlists {
        let members: Vec<i64> = row.track_ids.iter().map(|t| num(t)).collect();
        if members.iter().any(|m| !known.contains(m)) {
            return Err(Error::Other(format!("Playlist '{}' refers to a track that is not in this export", row.name)));
        }
        playlists.push(json!({
            "id": num(&row.id), "name": row.name, "parentId": row.parent_id.as_deref().map_or(0, num),
            "folder": row.folder, "trackIds": members,
            "entriesChangedMs": row.entries_changed_ms,
        }));
    }
    let tags: Vec<Value> = bundle
        .tags
        .iter()
        .filter(|t| num(&t.id) != 0)
        .map(|t| json!({"id": num(&t.id), "seq": t.seq.max(0), "name": t.name, "attribute": t.attribute, "parent": t.parent_id.as_deref().map_or(0, num)}))
        .collect();
    Ok(json!({
        "schemaVersion": JOB_SCHEMA_VERSION,
        "destination": volume.to_string_lossy(), "dbId": num(&bundle.db_id), "automatic": false,
        "settingsDir": find_settings().map(|p| p.to_string_lossy().into_owned()),
        "myTags": tags, "tracks": tracks, "playlists": playlists,
    }))
}

fn export_error(kind: &str, message: String) -> Error {
    if kind == "conflict" {
        Error::Conflict(message)
    } else if message.to_lowercase().contains("verif") || message.to_lowercase().contains("disagree") {
        Error::Verify(message)
    } else {
        Error::Publication(message)
    }
}

/// Publishes a prepared, versioned USB export job and reports progress from the publisher.
pub trait UsbPublisher {
    fn publish(&self, job: &Value, on_progress: &dyn Fn(Progress)) -> Result<Value>;
}

/// The isolated `es-usb` process that writes the device databases.
pub struct CommandPublisher {
    binary: PathBuf,
}

impl CommandPublisher {
    pub fn discover() -> Result<Self> {
        Ok(Self { binary: exporter()? })
    }
}

impl UsbPublisher for CommandPublisher {
    fn publish(&self, job: &Value, on_progress: &dyn Fn(Progress)) -> Result<Value> {
        let job_path = std::env::temp_dir().join(format!("exportstudio-usb-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&job_path, job.to_string()).map_err(|error| Error::Other(error.to_string()))?;
        let outcome = run(&self.binary, &job_path, on_progress);
        let _ = std::fs::remove_file(&job_path);
        outcome
    }
}

/// Copy the bundle's playlists onto `volume` and publish both device databases.
pub fn sync_volume(
    lib: &Library,
    volume: &Path,
    playlist_ids: &[String],
    on_progress: &dyn Fn(Progress),
) -> Result<Outcome> {
    let publisher = CommandPublisher::discover()?;
    sync_volume_with_publisher(lib, volume, playlist_ids, &publisher, on_progress)
}

/// Build and validate the library export, then publish it through the selected device boundary.
pub fn sync_volume_with_publisher(
    lib: &Library,
    volume: &Path,
    playlist_ids: &[String],
    publisher: &impl UsbPublisher,
    on_progress: &dyn Fn(Progress),
) -> Result<Outcome> {
    let volume = volume
        .canonicalize()
        .map_err(|_| Error::Other(format!("Volume is not mounted: {}", volume.display())))?;
    if !volume.is_dir() {
        return Err(Error::Other(format!("Volume is not mounted: {}", volume.display())));
    }
    let bundle = lib.export_bundle(playlist_ids)?;
    for track in &bundle.tracks {
        if !Path::new(&track.folder_path).is_file() {
            return Err(Error::Other(format!("Audio file is missing: {}", track.folder_path)));
        }
    }
    let body = job(&lib.share_dir(), &volume, &bundle)?;
    let result = publisher.publish(&body, on_progress)?;
    let tracks = result.get("tracks").and_then(Value::as_i64).unwrap_or(0);
    let playlists = result.get("playlists").and_then(Value::as_i64).unwrap_or(0);
    on_progress(Progress::Done { tracks, playlists });
    Ok(Outcome { volume: volume.to_string_lossy().into_owned(), tracks, playlists })
}

fn run(binary: &Path, job: &Path, on_progress: &dyn Fn(Progress)) -> Result<Value> {
    let mut child = Command::new(binary)
        .arg(job)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Other(format!("Could not start the USB export program: {e}")))?;
    let stdout = child.stdout.take().ok_or_else(|| Error::Other("no output from the USB export program".into()))?;
    let mut stderr = child.stderr.take();
    let drain = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut text);
        }
        text
    });
    let (mut result, mut error) = (None, None);
    for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
        let Ok(event) = serde_json::from_str::<Value>(line.trim()) else { continue };
        match event.get("event").and_then(Value::as_str) {
            Some("progress") => on_progress(Progress::Step {
                phase: event.get("phase").and_then(Value::as_str).filter(|p| !p.is_empty()).unwrap_or("reading").to_owned(),
                done: event.get("done").and_then(Value::as_i64),
                total: event.get("total").and_then(Value::as_i64),
                name: event.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
            }),
            Some("result") => result = Some(event),
            Some("error") => error = Some(event),
            _ => {}
        }
    }
    let status = child.wait().map_err(|e| Error::Other(e.to_string()))?;
    let stderr = drain.join().unwrap_or_default();
    match (result, error) {
        (Some(result), None) if status.success() => Ok(result),
        (_, error) => {
            let kind = error.as_ref().and_then(|e| e.get("kind")).and_then(Value::as_str).unwrap_or("export").to_owned();
            let mut message = error.as_ref().and_then(|e| e.get("message")).and_then(Value::as_str).unwrap_or("").to_owned();
            if message.is_empty() {
                message = stderr.trim().to_owned();
            }
            if message.is_empty() {
                message = format!("USB export failed ({})", status.code().unwrap_or(-1));
            }
            Err(export_error(&kind, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use es_db::fixture::{self, Rows};
    use es_db::Guard;

    struct FakePublisher;

    impl UsbPublisher for FakePublisher {
        fn publish(&self, job: &Value, _on_progress: &dyn Fn(Progress)) -> Result<Value> {
            assert_eq!(job["schemaVersion"], JOB_SCHEMA_VERSION);
            Ok(json!({ "tracks": 0, "playlists": 1 }))
        }
    }

    #[test]
    fn usb_job_declares_its_schema_version() {
        let directory = tempfile::tempdir().unwrap();
        let body = job(directory.path(), directory.path(), &Bundle::default()).unwrap();

        assert_eq!(body["schemaVersion"], JOB_SCHEMA_VERSION);
        assert!(body.get("overwrite").is_none());
    }

    #[test]
    fn sync_use_case_publishes_through_the_injected_port() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = fixture::build(directory.path(), 1).unwrap();
        {
            let connection = fixture::connect(&database_path).unwrap();
            Rows(&connection).playlist("playlist-1", "Test playlist", 0).unwrap();
        }
        let library = Library::open_live(&database_path, Guard::Off).unwrap();
        let volume = directory.path().join("STICK");
        std::fs::create_dir_all(&volume).unwrap();

        let result = sync_volume_with_publisher(
            &library,
            &volume,
            &["playlist-1".into()],
            &FakePublisher,
            &|_| {},
        )
        .unwrap();

        assert_eq!((result.tracks, result.playlists), (0, 1));
    }
}
