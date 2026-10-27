//! Encoding an audio file to a chosen format with ffmpeg, and how far the result moves rekordbox's timeline.

pub mod inspect;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(60 * 30);
const BIT_DEPTHS: [i64; 2] = [16, 24];
const BITRATES: [i64; 7] = [96, 128, 160, 192, 224, 256, 320];
const SAMPLE_RATES: [i64; 4] = [44100, 48000, 88200, 96000];

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);

pub type Result<T> = std::result::Result<T, Error>;

fn fail(message: impl Into<String>) -> Error {
    Error(message.into())
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
        other => return Err(fail(format!("Unsupported format: {}", if other.is_empty() { "none" } else { other }))),
    };
    if lossless {
        let depth = whole(raw, "bitDepth", 16);
        let rate = whole(raw, "sampleRate", 48000);
        if !BIT_DEPTHS.contains(&depth) {
            return Err(fail(format!("Bit depth must be one of {BIT_DEPTHS:?}")));
        }
        if !SAMPLE_RATES.contains(&rate) {
            return Err(fail(format!("Sample rate must be one of {SAMPLE_RATES:?}")));
        }
        Ok(Preset { format: name, extension, bit_depth: Some(depth), sample_rate: Some(rate), bitrate: None })
    } else {
        let bitrate = whole(raw, "bitrate", 320);
        if !BITRATES.contains(&bitrate) {
            return Err(fail(format!("Bitrate must be one of {BITRATES:?}")));
        }
        Ok(Preset { format: name, extension, bit_depth: None, sample_rate: None, bitrate: Some(bitrate) })
    }
}

/// Whether `path` already holds `preset`'s format, whatever its quality. Unreadable files don't.
#[must_use]
pub fn matches_format(path: &Path, preset: &Preset) -> bool {
    let Some(audio) = inspect::inspect_file(path).audio else { return false };
    let kind = match preset.format {
        "wav" => "WAV",
        "aiff" => "AIFF",
        "flac" => "FLAC",
        "alac" => "ALAC",
        "mp3" => "MP3",
        _ => "AAC",
    };
    // A WAVE or AIFF container can hold compressed audio, which isn't the format asked for.
    audio.kind == kind && audio.encoding != "other"
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
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| fail("ffmpeg was not found"))
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
        return Err(fail("The audio file is missing"));
    }
    let target = output_path(source, preset.extension);
    encode(source, &target, preset)?;
    Ok(target)
}

/// Encode `source` to `target`, replacing it. The file appears only once it is complete.
pub fn encode(source: &Path, target: &Path, preset: &Preset) -> Result<()> {
    let folder = target.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let partial = folder.join(format!(".convert-{}{}", uuid::Uuid::new_v4().simple(), preset.extension));
    let result = run_ffmpeg(&build_command(&ffmpeg_path()?, source, &partial, preset), &partial)
        .and_then(|()| std::fs::rename(&partial, target).map_err(|e| fail(e.to_string())));
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

fn run_ffmpeg(command: &[String], partial: &Path) -> Result<()> {
    let mut ffmpeg = Command::new(&command[0]);
    ffmpeg.args(&command[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    // CREATE_NO_WINDOW: ffmpeg is a console program and the app has no console to share.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut ffmpeg, 0x0800_0000);
    let mut child = ffmpeg
        .spawn()
        .map_err(|e| fail(format!("Could not start ffmpeg: {e}")))?;
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
        match child.try_wait().map_err(|e| fail(e.to_string()))? {
            Some(status) => break status,
            None if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(fail("Conversion timed out"));
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let text = reader.join().unwrap_or_default();
    let size = partial.metadata().map_or(0, |m| m.len());
    if !status.success() || size == 0 {
        return Err(fail(text.trim().lines().last().unwrap_or("ffmpeg failed")));
    }
    Ok(())
}

/// Milliseconds that cues, the beat grid and the detail waveforms move after converting `source` to `target`, from
/// what each file says about its own delay. `None` when either does not say, and nothing is moved.
#[must_use]
pub fn delay_offset_ms(source: &Path, target: &Path) -> Option<i64> {
    rb_audio::conversion_shift_ms(source, target)
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

    fn pcm_wav(path: &Path) {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(40u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend([1, 0, 2, 0]);
        bytes.extend(44100u32.to_le_bytes());
        bytes.extend((44100u32 * 4).to_le_bytes());
        bytes.extend([4, 0, 16, 0]);
        bytes.extend(b"data");
        bytes.extend(4u32.to_le_bytes());
        bytes.extend([0; 4]);
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_format_matches_whatever_its_quality() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.wav");
        pcm_wav(&file);
        let wav = |depth: i64, rate: i64| clean_preset(&json!({"format": "wav", "bitDepth": depth, "sampleRate": rate})).unwrap();
        assert!(matches_format(&file, &wav(16, 44100)));
        assert!(matches_format(&file, &wav(24, 96000)));
        assert!(!matches_format(&file, &clean_preset(&json!({"format": "aiff"})).unwrap()));
        assert!(!matches_format(&dir.path().join("missing.wav"), &wav(16, 44100)));
        let bare_mp4 = dir.path().join("a.m4a");
        std::fs::write(&bare_mp4, [&[0, 0, 0, 16][..], b"ftypM4A ", &[0; 4]].concat()).unwrap();
        assert!(!matches_format(&bare_mp4, &clean_preset(&json!({"format": "aac"})).unwrap()), "no codec is not AAC");
    }

    #[test]
    fn encoding_writes_the_target_and_nothing_else() {
        if ffmpeg_path().is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.wav");
        pcm_wav(&source);
        let out = dir.path().join("elsewhere");
        std::fs::create_dir(&out).unwrap();
        let target = out.join("b.flac");
        encode(&source, &target, &clean_preset(&json!({"format": "flac"})).unwrap()).unwrap();
        assert!(matches_format(&target, &clean_preset(&json!({"format": "flac"})).unwrap()));
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
    }
}
