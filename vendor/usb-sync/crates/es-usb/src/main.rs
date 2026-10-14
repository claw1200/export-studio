//! Runs one USB export from a JSON job.
//!
//! Progress is one JSON object per line on stdout. The last line is either
//! `{"event":"result",...}` or `{"event":"error","kind":"...","message":"..."}`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rbl_anlz::cues::ExportCue;
use rbl_export::{
    export_cancellable, copy_my_settings, ExportError, ExportOptions, ExportProgress, SourceMyTag,
    SourcePlaylist, SourceTrack, SyncNode, SyncSource,
};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
enum JobError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    destination: PathBuf,
    #[serde(default)]
    db_id: u64,
    #[serde(default)]
    automatic: bool,
    #[serde(default)]
    settings_dir: Option<PathBuf>,
    #[serde(default)]
    my_tags: Vec<JobTag>,
    tracks: Vec<JobTrack>,
    playlists: Vec<JobPlaylist>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobTag {
    id: u64,
    #[serde(default)]
    seq: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    attribute: u8,
    #[serde(default)]
    parent: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobTrack {
    id: u64,
    source_path: PathBuf,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
    #[serde(default)]
    genre: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    date_added: String,
    #[serde(default)]
    release_date: String,
    #[serde(default)]
    bpm_x100: u32,
    #[serde(default = "one_second")]
    duration_sec: u32,
    #[serde(default)]
    rating: u32,
    #[serde(default)]
    color_id: u32,
    #[serde(default)]
    year: u32,
    #[serde(default)]
    bitrate: u32,
    #[serde(default)]
    sample_rate: u32,
    #[serde(default)]
    file_size: u64,
    #[serde(default)]
    analysis: Vec<PathBuf>,
    #[serde(default)]
    artwork: Option<PathBuf>,
    #[serde(default)]
    my_tags: Vec<u64>,
    #[serde(default)]
    metadata: JobMetadata,
    /// Present, including an empty list, replaces the analysis cue tags.
    cues: Option<Vec<JobCue>>,
}

fn one_second() -> u32 {
    1
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobMetadata {
    #[serde(default)]
    track_number: u32,
    #[serde(default)]
    disc_number: u16,
    #[serde(default)]
    bit_depth: u16,
    #[serde(default)]
    play_count: u32,
    #[serde(default)]
    analysed: u32,
    #[serde(default = "yes")]
    hot_cue_auto_load: bool,
    #[serde(default)]
    date_created: String,
    #[serde(default)]
    isrc: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobCue {
    #[serde(default)]
    kind: u8,
    #[serde(default)]
    time_ms: u32,
    end_ms: Option<i64>,
    color: Option<i64>,
    #[serde(default)]
    color_code: u8,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    active_loop: bool,
    #[serde(default)]
    beat_loop_size: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobPlaylist {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    parent_id: u64,
    #[serde(default)]
    folder: bool,
    #[serde(default)]
    track_ids: Vec<u64>,
}

fn emit(value: &serde_json::Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, value).map_err(io::Error::other)?;
    out.write_all(b"\n")?;
    out.flush()
}

fn phase(stage: &str) -> &'static str {
    match stage {
        "checking" => "reading",
        "copying" => "copy",
        "database" => "writing",
        "verifying" => "verifying",
        "publishing" => "publishing",
        _ => "reading",
    }
}

fn narrow_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn narrow_u8(value: u32) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// A measured loop end, or one derived from `BeatLoopSize` and tempo.
///
/// `BeatLoopSize` is `(numerator << 16) | denominator`. This is the same
/// derivation used when reading `djmdCue`.
fn loop_end(start_ms: u32, stored_end: Option<i64>, beat_loop_size: u32, bpm_x100: u32) -> Option<u32> {
    if let Some(end) = stored_end.and_then(|end| u32::try_from(end).ok()) {
        if end > start_ms {
            return Some(end);
        }
    }
    let numerator = u64::from(beat_loop_size >> 16);
    let denominator = u64::from(beat_loop_size & 0xffff);
    if numerator == 0 || denominator == 0 || bpm_x100 == 0 {
        return None;
    }
    let divisor = denominator.saturating_mul(u64::from(bpm_x100));
    let duration_ms = numerator.saturating_mul(6_000_000).saturating_add(divisor / 2) / divisor;
    u32::try_from(u64::from(start_ms).saturating_add(duration_ms)).ok()
}

fn export_cue(cue: &JobCue, bpm_x100: u32) -> ExportCue {
    let color = cue.color.unwrap_or(255);
    ExportCue {
        kind: cue.kind,
        time_ms: cue.time_ms,
        loop_time_ms: loop_end(cue.time_ms, cue.end_ms, cue.beat_loop_size, bpm_x100),
        color_id: if cue.kind == 0 {
            u8::try_from(color).ok().filter(|c| *c != 255).unwrap_or(0)
        } else {
            0
        },
        color_code: cue.color_code,
        comment: cue.comment.clone(),
        active_loop: cue.active_loop,
        loop_numerator: u16::try_from(cue.beat_loop_size >> 16).unwrap_or(0),
        loop_denominator: u16::try_from(cue.beat_loop_size & 0xffff).unwrap_or(0),
    }
}

fn analysis_bytes(paths: &[PathBuf]) -> Result<Vec<(String, Vec<u8>)>, JobError> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("DAT")
            .to_ascii_uppercase();
        let bytes = fs::read(path).map_err(|err| {
            JobError::Message(format!("Analysis file is unreadable ({}): {err}", path.display()))
        })?;
        files.push((extension, bytes));
    }
    Ok(files)
}

fn prepare(job: &Job) -> Result<(Vec<SourceTrack>, Vec<SourcePlaylist>, Vec<SourceMyTag>, SyncSource), JobError> {
    let mut tracks = Vec::with_capacity(job.tracks.len());
    let mut index_of: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for track in &job.tracks {
        if index_of.insert(track.id, tracks.len()).is_some() && track.id != 0 {
            return Err(JobError::Message(format!("Duplicate track id {}", track.id)));
        }
        let cues = track.cues.as_ref().map(|cues| {
            cues.iter().map(|cue| export_cue(cue, track.bpm_x100)).collect::<Vec<_>>()
        });
        tracks.push(SourceTrack {
            metadata: rbl_core::ExportMetadata {
                track_number: track.metadata.track_number,
                disc_number: track.metadata.disc_number,
                bit_depth: track.metadata.bit_depth,
                play_count: track.metadata.play_count,
                analysed: track.metadata.analysed,
                hot_cue_auto_load: track.metadata.hot_cue_auto_load,
                date_created: track.metadata.date_created.clone(),
                isrc: track.metadata.isrc.clone(),
            },
            cues,
            device: None,
            id: track.id,
            source_path: track.source_path.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            genre: track.genre.clone(),
            label: track.label.clone(),
            key: track.key.clone(),
            comment: track.comment.clone(),
            date_added: track.date_added.clone(),
            release_date: track.release_date.clone(),
            bpm_x100: track.bpm_x100,
            duration_sec: narrow_u16(track.duration_sec),
            rating: narrow_u8(track.rating),
            color_id: narrow_u8(track.color_id),
            year: narrow_u16(track.year),
            bitrate: track.bitrate,
            sample_rate: track.sample_rate,
            file_size: track.file_size,
            analysis: analysis_bytes(&track.analysis)?,
            artwork: track.artwork.clone(),
            my_tags: track.my_tags.clone(),
        });
    }

    let mut playlists = Vec::with_capacity(job.playlists.len());
    for playlist in &job.playlists {
        let mut track_indices = Vec::with_capacity(playlist.track_ids.len());
        for id in &playlist.track_ids {
            let Some(index) = index_of.get(id).copied() else {
                return Err(JobError::Message(format!(
                    "Playlist '{}' refers to track {id}, which is not in this export",
                    playlist.name
                )));
            };
            track_indices.push(index);
        }
        playlists.push(SourcePlaylist {
            device_id: 0,
            device_only: false,
            id: playlist.id,
            name: playlist.name.clone(),
            parent_id: playlist.parent_id,
            folder: playlist.folder,
            track_indices,
        });
    }

    let tags = job
        .my_tags
        .iter()
        .map(|tag| SourceMyTag {
            id: tag.id,
            seq: tag.seq,
            name: tag.name.clone(),
            attribute: tag.attribute,
            parent: tag.parent,
        })
        .collect();
    let sync = SyncSource {
        db_id: job.db_id,
        automatic: job.automatic,
        tree: job
            .playlists
            .iter()
            .map(|playlist| SyncNode {
                id: playlist.id,
                parent: playlist.parent_id,
                attribute: u8::from(playlist.folder),
            })
            .collect(),
    };
    Ok((tracks, playlists, tags, sync))
}

fn run(job_path: &Path) -> Result<(), JobError> {
    let job: Job = serde_json::from_slice(&fs::read(job_path)?).map_err(|err| {
        JobError::Message(format!("Export job is not valid JSON: {err}"))
    })?;
    let (tracks, playlists, tags, sync) = prepare(&job)?;
    let options = ExportOptions {
        defaults: None,
        sync: Some(&sync),
        compatibility: None,
        root: None,
    };
    let report = export_cancellable(
        &job.destination,
        &tracks,
        &playlists,
        &tags,
        &options,
        &mut |progress: &ExportProgress| {
            let _ = emit(&serde_json::json!({
                "event": "progress",
                "phase": phase(progress.stage),
                "done": progress.done,
                "total": progress.total,
                "name": progress.title,
            }));
        },
        &|| false,
    )?;
    if let Some(settings) = job.settings_dir.as_deref() {
        copy_my_settings(&job.destination, settings)?;
    }
    emit(&serde_json::json!({
        "event": "result",
        "tracks": report.tracks,
        "playlists": report.playlists,
        "reused": report.reused,
        "removed": report.removed,
        "skipped": report.skipped,
        "oneLibrary": report.one_library,
    }))?;
    Ok(())
}

fn kind_of(err: &JobError) -> &'static str {
    match err {
        JobError::Export(ExportError::Conflict(_)) => "conflict",
        JobError::Export(ExportError::Cancelled) => "cancelled",
        JobError::Export(ExportError::DeviceGone) => "device",
        JobError::Message(_) | JobError::Export(_) | JobError::Io(_) => "export",
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(path) = args.next() else {
        let _ = writeln!(io::stderr(), "usage: es-usb <job.json>");
        return ExitCode::from(2);
    };
    match run(Path::new(&path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(io::stderr(), "{err}");
            let _ = emit(&serde_json::json!({
                "event": "error",
                "kind": kind_of(&err),
                "message": err.to_string(),
            }));
            ExitCode::from(1)
        }
    }
}
