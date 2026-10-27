//! Converting tracks' audio files with ffmpeg and pointing the library at the results.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rb_anlz::decode::{self, Tags};
use rb_anlz::write::{build_beatgrid, clear_extended_beatgrid, patch_tags, path_tag};
use rb_library::{ConversionTarget, Library, Stream};

use crate::import::file_type_code;
use crate::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(60 * 30);
const BIT_DEPTHS: [i64; 2] = [16, 24];
const BITRATES: [i64; 7] = [96, 128, 160, 192, 224, 256, 320];
const SAMPLE_RATES: [i64; 4] = [44100, 48000, 88200, 96000];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A new track beside the original.
    Copy,
    /// The track points at the new file; the old one stays.
    Update,
    /// Like `Update`, and the old file is listed for the caller to remove.
    UpdateDelete,
}

impl Mode {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "copy" => Ok(Self::Copy),
            "update" => Ok(Self::Update),
            "update_delete" => Ok(Self::UpdateDelete),
            other => Err(Error::Other(format!("Unknown conversion mode: {other}"))),
        }
    }
}

/// A validated target format: lossless formats carry a bit depth and sample rate, lossy ones a bitrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub format: &'static str,
    pub extension: &'static str,
    pub bit_depth: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bitrate: Option<i64>,
}

fn whole(value: &serde_json::Value, key: &str, default: i64) -> i64 {
    match value.get(key) {
        Some(v) => v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).filter(|n| *n != 0).unwrap_or(default),
        None => default,
    }
}

pub fn clean_preset(raw: &serde_json::Value) -> Result<Preset> {
    let format = raw.get("format").and_then(serde_json::Value::as_str).unwrap_or("").to_lowercase();
    let (name, extension, lossless): (&'static str, &'static str, bool) = match format.as_str() {
        "wav" => ("wav", ".wav", true),
        "aiff" => ("aiff", ".aiff", true),
        "flac" => ("flac", ".flac", true),
        "alac" => ("alac", ".m4a", true),
        "mp3" => ("mp3", ".mp3", false),
        "aac" => ("aac", ".m4a", false),
        other => return Err(Error::Other(format!("Unsupported format: {}", if other.is_empty() { "none" } else { other }))),
    };
    if lossless {
        let depth = whole(raw, "bitDepth", 16);
        let rate = whole(raw, "sampleRate", 48000);
        if !BIT_DEPTHS.contains(&depth) {
            return Err(Error::Other(format!("Bit depth must be one of {BIT_DEPTHS:?}")));
        }
        if !SAMPLE_RATES.contains(&rate) {
            return Err(Error::Other(format!("Sample rate must be one of {SAMPLE_RATES:?}")));
        }
        Ok(Preset { format: name, extension, bit_depth: Some(depth), sample_rate: Some(rate), bitrate: None })
    } else {
        let bitrate = whole(raw, "bitrate", 320);
        if !BITRATES.contains(&bitrate) {
            return Err(Error::Other(format!("Bitrate must be one of {BITRATES:?}")));
        }
        Ok(Preset { format: name, extension, bit_depth: None, sample_rate: None, bitrate: Some(bitrate) })
    }
}

/// The ffmpeg named by the environment (the bundled one when packaged), else the one on `PATH`.
pub fn ffmpeg_path() -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(env) = std::env::var_os("EXPORTSTUDIO_FFMPEG") {
        candidates.push(PathBuf::from(env));
    }
    if let Some(path) = std::env::var_os("PATH") {
        let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(name)));
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| Error::Other("ffmpeg was not found".into()))
}

fn codec_args(preset: &Preset) -> Vec<String> {
    let depth = preset.bit_depth.unwrap_or(16);
    let raw = |extra: bool| if extra { vec!["-bits_per_raw_sample".to_owned(), "24".to_owned()] } else { Vec::new() };
    let strings = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    match preset.format {
        "wav" => vec!["-c:a".into(), format!("pcm_s{depth}le")],
        "aiff" => vec!["-c:a".into(), format!("pcm_s{depth}be")],
        "flac" => [strings(&["-c:a", "flac", "-sample_fmt", if depth == 16 { "s16" } else { "s32" }]), raw(depth != 16)].concat(),
        "alac" => [strings(&["-c:a", "alac", "-sample_fmt", if depth == 16 { "s16p" } else { "s32p" }]), raw(depth != 16)].concat(),
        "mp3" => vec!["-c:a".into(), "libmp3lame".into(), "-b:a".into(), format!("{}k", preset.bitrate.unwrap_or(320))],
        _ => vec!["-c:a".into(), "aac".into(), "-b:a".into(), format!("{}k", preset.bitrate.unwrap_or(320))],
    }
}

#[must_use]
pub fn build_command(ffmpeg: &Path, source: &Path, target: &Path, preset: &Preset) -> Vec<String> {
    let mut args: Vec<String> = ["-nostdin", "-v", "error", "-y", "-i"].iter().map(|s| (*s).to_owned()).collect();
    args.push(source.to_string_lossy().into_owned());
    args.extend(["-map", "0:a:0", "-map_metadata", "0"].map(str::to_owned));
    if matches!(preset.format, "mp3" | "flac" | "alac" | "aac") {
        args.extend(["-map", "0:v:0?", "-c:v", "copy", "-disposition:v", "attached_pic"].map(str::to_owned));
    } else {
        args.push("-vn".into());
    }
    if let Some(rate) = preset.sample_rate {
        args.extend(["-ar".to_owned(), rate.to_string()]);
    }
    args.extend(codec_args(preset));
    if preset.format == "mp3" {
        args.extend(["-id3v2_version", "3"].map(str::to_owned));
    }
    match preset.format {
        "alac" | "aac" => args.extend(["-f", "ipod"].map(str::to_owned)),
        "wav" => args.extend(["-f", "wav"].map(str::to_owned)),
        "aiff" => args.extend(["-f", "aiff"].map(str::to_owned)),
        _ => {}
    }
    args.push(target.to_string_lossy().into_owned());
    let mut out = vec![ffmpeg.to_string_lossy().into_owned()];
    out.extend(args);
    out
}

/// Next to the source, same name, with a numeric suffix when the name is taken.
#[must_use]
pub fn output_path(source: &Path, extension: &str) -> PathBuf {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = |s: &str| source.with_file_name(format!("{s}{extension}"));
    let mut target = name(&stem);
    let mut number = 1;
    while target.exists() || target == source {
        target = name(&format!("{stem} ({number})"));
        number += 1;
    }
    target
}

/// Encode `source` to a new file beside it. Nothing is left behind on failure.
pub fn convert_file(source: &Path, preset: &Preset) -> Result<PathBuf> {
    if !source.is_file() {
        return Err(Error::Other("The audio file is missing".into()));
    }
    let target = output_path(source, preset.extension);
    let folder = target.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let partial = folder.join(format!(".convert-{}{}", uuid::Uuid::new_v4().simple(), preset.extension));
    let result = run_ffmpeg(&build_command(&ffmpeg_path()?, source, &partial, preset), &partial);
    match result {
        Ok(()) => std::fs::rename(&partial, &target).map_err(|e| Error::Other(e.to_string())).map(|()| target),
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn run_ffmpeg(command: &[String], partial: &Path) -> Result<()> {
    let mut ffmpeg = Command::new(&command[0]);
    ffmpeg.args(&command[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    // CREATE_NO_WINDOW: ffmpeg is a console program and the app has no console to share.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut ffmpeg, 0x0800_0000);
    let mut child = ffmpeg
        .spawn()
        .map_err(|e| Error::Other(format!("Could not start ffmpeg: {e}")))?;
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut text);
        }
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| Error::Other(e.to_string()))? {
            Some(status) => break status,
            None if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Other("Conversion timed out".into()));
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let text = reader.join().unwrap_or_default();
    let size = partial.metadata().map_or(0, |m| m.len());
    if !status.success() || size == 0 {
        return Err(Error::Other(text.trim().lines().last().unwrap_or("ffmpeg failed").to_owned()));
    }
    Ok(())
}

/// Milliseconds that cues and the beat grid move after converting `source` to `target`, from what each file says
/// about its own delay. `None` when either does not say, and nothing is moved.
#[must_use]
pub fn delay_offset_ms(source: &Path, target: &Path) -> Option<i64> {
    rb_audio::conversion_shift_ms(source, target)
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub ids: Vec<String>,
    pub originals: Vec<String>,
    pub failed: Vec<(String, String)>,
}

fn grid_patches(path: &Path, offset_ms: i64) -> Vec<(&'static str, Vec<u8>)> {
    let Ok(tags) = Tags::read(path) else { return Vec::new() };
    let mut out = Vec::new();
    if let (Some(grid), Some(pqtz)) = (decode::beatgrid(&tags), tags.first("PQTZ")) {
        let keep: Vec<usize> = (0..grid.times.len()).filter(|i| i64::from(grid.times[*i]) + offset_ms >= 0).collect();
        if !keep.is_empty() {
            let times: Vec<u32> = keep.iter().map(|i| u32::try_from(i64::from(grid.times[*i]) + offset_ms).unwrap_or(0)).collect();
            let beats: Vec<u16> = keep.iter().map(|i| grid.beats[*i]).collect();
            let bpms: Vec<f64> = keep.iter().map(|i| grid.bpms[*i]).collect();
            out.push(("PQTZ", build_beatgrid(pqtz, &times, &beats, &bpms)));
        }
    }
    if let Some(pqt2) = tags.first("PQT2") {
        out.push(("PQT2", clear_extended_beatgrid(pqt2)));
    }
    out
}

fn stored_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn convert_one(lib: &mut Library, share: &Path, id: &str, preset: &Preset, mode: Mode) -> Result<(String, PathBuf)> {
    let files = lib.track_files(id)?.ok_or_else(|| Error::Other(format!("Unknown track {id}")))?;
    let source = PathBuf::from(&files.folder_path);
    let target = convert_file(&source, preset)?;
    let done = (|| -> Result<String> {
        let info = es_tags::read(&target).map_err(|e| Error::Other(e.to_string()))?;
        let offset = delay_offset_ms(&source, &target).unwrap_or(0);
        let nonzero = |v: i64| (v != 0).then_some(v);
        let old_files: Vec<PathBuf> = files.analysis_path.as_deref().map(|rel| rb_anlz::files(share, rel)).unwrap_or_default().into_iter().filter(|p| p.is_file()).collect();
        let carries = old_files.iter().any(|p| p.extension().is_some_and(|e| e == "DAT"));
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let extension = target.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let change = ConversionTarget {
            stored: stored_path(&target),
            file_name: name.clone(),
            file_size: target.metadata().ok().and_then(|m| i64::try_from(m.len()).ok()),
            file_type: file_type_code(&extension),
            stream: Stream {
                length: nonzero(info.length),
                bit_rate: nonzero(info.bitrate),
                sample_rate: nonzero(info.sample_rate),
                bit_depth: nonzero(info.bit_depth),
            },
            offset_ms: offset,
            carry_analysis: carries,
        };
        let converted = lib.write(|tx| tx.convert_track(id, mode == Mode::Copy, &change))?;
        if let Some(relative) = &converted.analysis {
            let dat = share.join(relative.trim_matches(['/', '\\']));
            for source_file in &old_files {
                let Some(suffix) = source_file.extension() else { continue };
                let path = dat.with_extension(suffix);
                let bytes = std::fs::read(source_file).map_err(|e| Error::Other(e.to_string()))?;
                let mut replace = vec![("PPTH", path_tag(&format!("?/{name}")))];
                if offset != 0 {
                    replace.extend(grid_patches(source_file, offset));
                }
                let patched = patch_tags(&bytes, &replace, &[]).map_err(|e| Error::Other(e.to_string()))?;
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| Error::Other(e.to_string()))?;
                }
                std::fs::write(&path, patched).map_err(|e| Error::Other(e.to_string()))?;
            }
        }
        Ok(converted.id)
    })();
    match done {
        Ok(new_id) => Ok((new_id, source)),
        Err(e) => {
            let _ = std::fs::remove_file(&target);
            Err(e)
        }
    }
}

/// Encode each track's audio file to `preset` and point the library at the result. A track that fails is left
/// as it was.
pub fn convert_tracks(lib: &mut Library, share: &Path, ids: &[String], preset: &Preset, mode: Mode, progress: &dyn Fn(usize, usize, Option<&str>)) -> Outcome {
    let mut ordered: Vec<&String> = Vec::new();
    for id in ids {
        if !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    let mut outcome = Outcome::default();
    for (done, id) in ordered.iter().enumerate() {
        progress(done, ordered.len(), Some(id));
        match convert_one(lib, share, id, preset, mode) {
            Ok((new_id, source)) => {
                outcome.ids.push(new_id);
                if mode == Mode::UpdateDelete {
                    outcome.originals.push(source.to_string_lossy().into_owned());
                }
            }
            Err(e) => {
                let message = e.to_string();
                outcome.failed.push(((*id).clone(), if message.is_empty() { "Error".into() } else { message }));
            }
        }
    }
    progress(ordered.len(), ordered.len(), None);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn presets_are_checked_and_filled() {
        let wav = clean_preset(&json!({"format": "WAV"})).unwrap();
        assert_eq!((wav.bit_depth, wav.sample_rate, wav.bitrate), (Some(16), Some(48000), None));
        let mp3 = clean_preset(&json!({"format": "mp3", "bitrate": 192})).unwrap();
        assert_eq!((mp3.bitrate, mp3.bit_depth), (Some(192), None));
        assert!(clean_preset(&json!({"format": "ogg"})).is_err());
        assert!(clean_preset(&json!({"format": "flac", "bitDepth": 20})).is_err());
        assert!(clean_preset(&json!({"format": "mp3", "bitrate": 100})).is_err());
    }

    #[test]
    fn commands_match_the_formats() {
        let flac = clean_preset(&json!({"format": "flac", "bitDepth": 24, "sampleRate": 44100})).unwrap();
        let args = build_command(Path::new("ffmpeg"), Path::new("a.wav"), Path::new("b.flac"), &flac).join(" ");
        assert!(args.contains("-c:a flac -sample_fmt s32 -bits_per_raw_sample 24"), "{args}");
        assert!(args.contains("-ar 44100") && args.contains("-map 0:v:0?"), "{args}");
        let wav = clean_preset(&json!({"format": "wav"})).unwrap();
        let args = build_command(Path::new("ffmpeg"), Path::new("a.flac"), Path::new("b.wav"), &wav).join(" ");
        assert!(args.contains("pcm_s16le") && args.contains("-vn") && args.ends_with("-f wav b.wav"), "{args}");
    }

    #[test]
    fn outputs_do_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.wav");
        std::fs::write(&source, b"x").unwrap();
        assert_eq!(output_path(&source, ".flac"), dir.path().join("a.flac"));
        std::fs::write(dir.path().join("a.flac"), b"x").unwrap();
        assert_eq!(output_path(&source, ".flac"), dir.path().join("a (1).flac"));
        assert_eq!(output_path(&source, ".wav"), dir.path().join("a (1).wav"));
    }
}
