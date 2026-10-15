import os
import shutil
import subprocess
import tempfile
from pathlib import Path

FFMPEG_ENV = "EXPORTSTUDIO_FFMPEG"
FORMATS = {
    # id: (extension, lossless)
    "wav": (".wav", True),
    "aiff": (".aiff", True),
    "flac": (".flac", True),
    "alac": (".m4a", True),
    "mp3": (".mp3", False),
    "aac": (".m4a", False),
}
BIT_DEPTHS = (16, 24)
BITRATES = (96, 128, 160, 192, 224, 256, 320)
SAMPLE_RATES = (44100, 48000, 88200, 96000)
MODES = ("copy", "update", "update_delete")
CONVERT_TIMEOUT = 60 * 30


class ConvertError(RuntimeError):
    pass


def ffmpeg_path() -> str:
    """The ffmpeg named by the environment (the bundled one when packaged), else the one on PATH."""
    for candidate in (os.environ.get(FFMPEG_ENV), shutil.which("ffmpeg")):
        if candidate and Path(candidate).is_file():
            return candidate
    raise ConvertError("ffmpeg was not found")


def clean_preset(raw: dict) -> dict:
    """A validated copy of a preset: a format plus sample rate and bit depth, or a bitrate."""
    fmt = str(raw.get("format") or "").lower()
    if fmt not in FORMATS:
        raise ValueError(f"Unsupported format: {fmt or 'none'}")
    preset: dict = {"format": fmt}
    if FORMATS[fmt][1]:
        depth = int(raw.get("bitDepth") or 16)
        rate = int(raw.get("sampleRate") or 48000)
        if depth not in BIT_DEPTHS:
            raise ValueError(f"Bit depth must be one of {BIT_DEPTHS}")
        if rate not in SAMPLE_RATES:
            raise ValueError(f"Sample rate must be one of {SAMPLE_RATES}")
        preset["bitDepth"] = depth
        preset["sampleRate"] = rate
    else:
        bitrate = int(raw.get("bitrate") or 320)
        if bitrate not in BITRATES:
            raise ValueError(f"Bitrate must be one of {BITRATES}")
        preset["bitrate"] = bitrate
    return preset


def delay_offset_ms(source: Path, target: Path) -> int:
    """Shift in milliseconds that cues need when the encoder adds or removes padding. None yet."""
    return 0


def output_path(source: Path, extension: str) -> Path:
    """Next to the source, same name, with a numeric suffix when the name is taken."""
    target = source.with_suffix(extension)
    number = 1
    while target.exists() or target == source:
        target = source.with_name(f"{source.stem} ({number}){extension}")
        number += 1
    return target


def _codec_args(preset: dict) -> list[str]:
    fmt = preset["format"]
    depth = preset.get("bitDepth")
    if fmt == "wav":
        return ["-c:a", f"pcm_s{depth}le"]
    if fmt == "aiff":
        return ["-c:a", f"pcm_s{depth}be"]
    if fmt == "flac":
        return ["-c:a", "flac", "-sample_fmt", "s16" if depth == 16 else "s32"] + (
            [] if depth == 16 else ["-bits_per_raw_sample", "24"]
        )
    if fmt == "alac":
        return ["-c:a", "alac", "-sample_fmt", "s16p" if depth == 16 else "s32p"] + (
            [] if depth == 16 else ["-bits_per_raw_sample", "24"]
        )
    if fmt == "mp3":
        return ["-c:a", "libmp3lame", "-b:a", f"{preset['bitrate']}k"]
    return ["-c:a", "aac", "-b:a", f"{preset['bitrate']}k"]


def build_command(ffmpeg: str, source: Path, target: Path, preset: dict) -> list[str]:
    args = [ffmpeg, "-nostdin", "-v", "error", "-y", "-i", str(source), "-map", "0:a:0", "-map_metadata", "0"]
    if preset["format"] in ("mp3", "flac", "alac", "aac"):
        args += ["-map", "0:v:0?", "-c:v", "copy", "-disposition:v", "attached_pic"]
    else:
        args += ["-vn"]
    if preset.get("sampleRate"):
        args += ["-ar", str(preset["sampleRate"])]
    args += _codec_args(preset)
    if preset["format"] == "mp3":
        args += ["-id3v2_version", "3"]
    if preset["format"] in ("alac", "aac"):
        args += ["-f", "ipod"]
    elif preset["format"] == "wav":
        args += ["-f", "wav"]
    elif preset["format"] == "aiff":
        args += ["-f", "aiff"]
    return args + [str(target)]


def convert_file(source: Path, preset: dict) -> Path:
    """Encode `source` to a new file beside it and return its path. Nothing is left behind on failure."""
    if not source.is_file():
        raise ConvertError("The audio file is missing")
    extension = FORMATS[preset["format"]][0]
    target = output_path(source, extension)
    handle, partial_name = tempfile.mkstemp(prefix=".convert-", suffix=extension, dir=target.parent)
    os.close(handle)
    partial = Path(partial_name)
    try:
        command = build_command(ffmpeg_path(), source, partial, preset)
        result = subprocess.run(command, capture_output=True, text=True, timeout=CONVERT_TIMEOUT)
        if result.returncode != 0 or not partial.stat().st_size:
            lines = (result.stderr or "").strip().splitlines()
            raise ConvertError(lines[-1] if lines else "ffmpeg failed")
        os.replace(partial, target)
    except subprocess.TimeoutExpired:
        raise ConvertError("Conversion timed out") from None
    finally:
        partial.unlink(missing_ok=True)
    return target
