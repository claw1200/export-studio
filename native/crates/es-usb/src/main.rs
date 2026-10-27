//! Versioned JSON transport for the application-independent USB export service.

mod convert;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use rb_model::source::{Playlist, Source, Tag, Track};
use serde::Deserialize;

const JOB_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, thiserror::Error)]
enum JobError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Export(#[from] rb_usb::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema_version: u32,
    #[serde(default = "default_operation")]
    operation: String,
    destination: PathBuf,
    #[serde(default)]
    db_id: u64,
    #[serde(default)]
    automatic: bool,
    #[serde(default)]
    settings_dir: Option<PathBuf>,
    #[serde(default)]
    my_tags: Vec<Tag>,
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    #[serde(default)]
    decisions: Option<rb_model::conflict::Decisions>,
    /// The format tracks not already in it are converted to on the stick.
    #[serde(default)]
    conversion: Option<serde_json::Value>,
}

fn default_operation() -> String { "export".into() }

fn emit(value: &serde_json::Value) -> io::Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, value).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn phase(stage: &str) -> &'static str {
    match stage {
        "copying" => "copy",
        "converting" => "convert",
        "database" => "writing",
        "verifying" => "verifying",
        "publishing" => "publishing",
        _ => "reading",
    }
}

fn parse_job(bytes: &[u8]) -> Result<Job, JobError> {
    let job: Job = serde_json::from_slice(bytes)
        .map_err(|error| JobError::Message(format!("Export job is not valid JSON: {error}")))?;
    if job.schema_version != JOB_SCHEMA_VERSION {
        return Err(JobError::Message(format!(
            "Unsupported export job schema version: {} (expected {})",
            job.schema_version, JOB_SCHEMA_VERSION
        )));
    }
    Ok(job)
}

fn converter(job: &Job) -> Result<Option<Arc<dyn rb_usb::AudioConverter + Send>>, JobError> {
    let Some(raw) = job.conversion.as_ref().filter(|v| !v.is_null()) else { return Ok(None) };
    let preset = es_convert::clean_preset(raw).map_err(|error| JobError::Message(error.to_string()))?;
    if job.operation == "export" {
        es_convert::ffmpeg_path().map_err(|error| JobError::Message(error.to_string()))?;
    }
    Ok(Some(Arc::new(convert::Ffmpeg::new(preset))))
}

fn run(path: &Path) -> Result<(), JobError> {
    let job = parse_job(&fs::read(path)?)?;
    let converter = converter(&job)?;
    let source = Source {
        db_id: job.db_id,
        automatic: job.automatic,
        tracks: job.tracks,
        playlists: job.playlists,
        my_tags: job.my_tags,
    };
    let options = rb_usb::Options {
        settings_dir: job.settings_dir,
        decisions: job.decisions,
        converter,
    };
    if job.operation == "inspect" {
        let inspection = rb_usb::inspect(&job.destination, &source, &options)?;
        emit(&serde_json::json!({ "event": "result", "inspection": inspection }))?;
        return Ok(());
    }
    if job.operation != "export" {
        return Err(JobError::Message(format!("Unsupported USB operation: {}", job.operation)));
    }
    let report = rb_usb::export(
        &job.destination,
        &source,
        &options,
        &mut |event| {
            let _ = emit(&serde_json::json!({
                "event": "progress", "phase": phase(event.stage),
                "done": event.done, "total": event.total, "name": event.title,
            }));
        },
        &|| false,
    )?;
    emit(&serde_json::json!({
        "event": "result", "tracks": report.tracks, "playlists": report.playlists,
        "reused": report.reused, "removed": report.removed, "skipped": report.skipped,
        "oneLibrary": report.one_library,
    }))?;
    Ok(())
}

fn kind_of(error: &JobError) -> &'static str {
    match error {
        JobError::Export(error) => error.kind(),
        _ => "export",
    }
}

fn review_of(error: &JobError) -> Option<&rb_model::conflict::Inspection> {
    match error {
        JobError::Export(rb_usb::Error::ReviewRequired(inspection)) => Some(inspection),
        _ => None,
    }
}

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1) else {
        let _ = writeln!(io::stderr(), "usage: es-usb <job.json>");
        return ExitCode::from(2);
    };
    match run(Path::new(&path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "{error}");
            let _ = emit(&serde_json::json!({
                "event": "error", "kind": kind_of(&error), "message": error.to_string(),
                "review": review_of(&error),
            }));
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_current_job_schema() {
        let job = serde_json::json!({ "schemaVersion": JOB_SCHEMA_VERSION, "destination": "/tmp", "tracks": [], "playlists": [] });
        assert!(parse_job(job.to_string().as_bytes()).is_ok());
    }

    #[test]
    fn a_conversion_is_checked_like_the_library_s() {
        let job = |conversion: serde_json::Value| {
            let raw = serde_json::json!({ "schemaVersion": JOB_SCHEMA_VERSION, "operation": "inspect", "destination": "/tmp", "tracks": [], "playlists": [], "conversion": conversion });
            parse_job(raw.to_string().as_bytes()).and_then(|job| converter(&job))
        };
        assert!(job(serde_json::Value::Null).is_ok_and(|c| c.is_none()));
        let wav = job(serde_json::json!({ "format": "wav", "bitDepth": 16, "sampleRate": 44100 })).ok().flatten();
        assert_eq!(wav.map(|c| (c.extension().to_owned(), c.profile().to_owned())), Some(("wav".into(), "es-ffmpeg-v1:wav-16-44100".into())));
        assert!(job(serde_json::json!({ "format": "ogg" })).is_err());
    }

    #[test]
    fn rejects_missing_or_unsupported_job_schema() {
        assert!(parse_job(br#"{"destination":"/tmp","tracks":[],"playlists":[]}"#).is_err());
        let job = serde_json::json!({ "schemaVersion": JOB_SCHEMA_VERSION + 1, "destination": "/tmp", "tracks": [], "playlists": [] });
        assert!(parse_job(job.to_string().as_bytes())
            .unwrap_err()
            .to_string()
            .contains("Unsupported export job schema version"));
    }

    #[test]
    fn rejects_legacy_jobs_and_obsolete_overwrite_option() {
        let mut job = serde_json::json!({ "schemaVersion": 1, "destination": "/tmp", "tracks": [], "playlists": [] });
        assert!(parse_job(job.to_string().as_bytes()).is_err());
        job["schemaVersion"] = serde_json::json!(JOB_SCHEMA_VERSION);
        job["overwrite"] = serde_json::json!(false);
        assert!(parse_job(job.to_string().as_bytes()).is_err());
    }
}
