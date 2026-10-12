"""Compare a collection file's header with the type, size, and length rekordbox stored.

Reads the container header only. MP3 length comes from a Xing or VBRI tag in the first
frame, not from walking the file.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from pathlib import Path

LENGTH_SLACK = 2
CDJ_RATES = {44_100, 48_000}

SUFFIXES = {
    ".mp3": "MP3",
    ".m4a": "M4A",
    ".mp4": "M4A",
    ".flac": "FLAC",
    ".wav": "WAV",
    ".aif": "AIFF",
    ".aiff": "AIFF",
}

_SEVERITY = {"missing": 0, "unreadable": 1, "type": 2, "incompatible": 3, "size": 4, "duration": 5}


@dataclass
class Header:
    kind: str
    length: int | None
    incompatible: str | None


def extension_type(path: Path) -> str | None:
    return SUFFIXES.get(path.suffix.lower())


def same_fingerprint(path: Path, size: object, mtime: object) -> tuple[int, int | None] | None:
    """Return the file's size and mtime when they still match a cached probe.

    A missing file matches a cache entry of size 0 and no mtime. Anything else returns None
    so the caller reads the header again.
    """
    try:
        hinted_size = int(size)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None
    if not path.is_file():
        if hinted_size == 0 and mtime is None:
            return 0, None
        return None
    if mtime is None:
        return None
    try:
        hinted_mtime = int(mtime)  # type: ignore[arg-type]
        stat = path.stat()
    except (TypeError, ValueError, OSError):
        return None
    if stat.st_size == hinted_size and stat.st_mtime_ns == hinted_mtime:
        return stat.st_size, stat.st_mtime_ns
    return None


def inspect_file(path: Path, stored_type: str | None, stored_size: int, stored_length: int) -> dict:
    """Issues for one local file. `stored_type` is MP3, M4A, FLAC, WAV, AIFF, or None."""
    if not path.is_file():
        issues = [{"code": "missing", "detail": "File missing"}]
        _type_issue(issues, stored_type, extension_type(path), None)
        return {"size": 0, "mtime": None, "issues": _ordered(issues)}

    try:
        stat = path.stat()
    except OSError:
        return {"size": 0, "mtime": None, "issues": [{"code": "missing", "detail": "File missing"}]}

    size = stat.st_size
    # Nanoseconds don't fit in a JSON number, so the client keeps this as text.
    mtime = str(stat.st_mtime_ns)
    issues: list[dict] = []
    if size <= 0:
        issues.append({"code": "unreadable", "detail": "File can't be read"})
    else:
        try:
            header = _read_header(path, size)
        except OSError:
            header = None
        if header is None:
            issues.append({"code": "unreadable", "detail": "File can't be read"})
            _type_issue(issues, stored_type, extension_type(path), None)
        else:
            _type_issue(issues, stored_type, extension_type(path), header.kind)
            if header.incompatible:
                issues.append({"code": "incompatible", "detail": header.incompatible})
            if (
                header.length is not None
                and stored_length > 0
                and abs(header.length - stored_length) > LENGTH_SLACK
            ):
                issues.append({"code": "duration", "detail": "Length doesn't match the library"})
    if stored_size > 0 and size != stored_size:
        issues.append({"code": "size", "detail": "File size doesn't match the library"})
    return {"size": size, "mtime": mtime, "issues": _ordered(issues)}


def _ordered(issues: list[dict]) -> list[dict]:
    return sorted(issues, key=lambda item: _SEVERITY.get(item["code"], 9))


def _type_issue(issues: list[dict], stored: str | None, ext: str | None, kind: str | None) -> None:
    if stored and kind and stored != kind:
        detail = f"Rekordbox says {stored}, the file is {kind}"
    elif stored and ext and stored != ext:
        detail = f"Rekordbox says {stored}, the file is {ext}"
    elif ext and kind and ext != kind:
        detail = f"Extension is {ext}, the file is {kind}"
    else:
        return
    issues.append({"code": "type", "detail": detail})


def _khz(rate: int) -> str:
    if rate % 1000 == 0:
        text = str(rate // 1000)
    elif rate % 100 == 0:
        text = f"{rate / 1000:.1f}"
    else:
        text = f"{rate / 1000:.2f}"
    return f"{text} kHz"


def _playback_detail(kind: str, reasons: list[str]) -> str | None:
    if not reasons:
        return None
    head = reasons[0]
    text = f"{kind} has {head}" if head.endswith("channels") else f"{kind} is {head}"
    if len(reasons) > 1:
        text += ", " + ", ".join(reasons[1:])
    return text


def _pcm_detail(kind: str, audio_format: int, bits: int, rate: int, channels: int) -> str | None:
    reasons: list[str] = []
    if audio_format == 3:
        reasons.append(f"{bits}-bit float" if bits else "float")
    elif audio_format == 2:
        reasons.append("ADPCM")
    elif audio_format != 1:
        reasons.append("not integer PCM")
    elif bits not in (16, 24):
        reasons.append(f"{bits}-bit PCM")
    if rate and rate not in CDJ_RATES:
        reasons.append(_khz(rate))
    if channels and channels not in (1, 2):
        reasons.append(f"{channels} channels")
    return _playback_detail(kind, reasons)


def _rate_detail(kind: str, rate: int | None, channels: int | None) -> str | None:
    reasons: list[str] = []
    if rate and rate not in CDJ_RATES:
        reasons.append(_khz(rate))
    if channels and channels not in (1, 2):
        reasons.append(f"{channels} channels")
    return _playback_detail(kind, reasons)


def _read_header(path: Path, size: int) -> Header | None:
    with path.open("rb") as handle:
        lead = handle.read(12)
        if lead.startswith(b"RIFF") and lead[8:12] == b"WAVE":
            return _wav(handle, size)
        if lead.startswith(b"FORM") and lead[8:12] in (b"AIFF", b"AIFC"):
            return _aiff(handle, size, lead[8:12] == b"AIFC")
        if lead.startswith(b"fLaC"):
            return _flac(handle)
        if len(lead) >= 8 and lead[4:8] == b"ftyp":
            return _mp4(handle, size)
        if lead.startswith(b"ID3") or _mp3_frame(lead):
            return _mp3(handle, size)
    return None


def _wav(handle, size: int) -> Header | None:
    fmt = None
    data_size = None
    handle.seek(12)
    for _ in range(64):
        if handle.tell() + 8 > size:
            break
        ident = handle.read(4)
        raw_size = handle.read(4)
        if len(ident) < 4 or len(raw_size) < 4:
            break
        chunk_size = struct.unpack("<I", raw_size)[0]
        pos = handle.tell()
        if ident == b"fmt " and chunk_size >= 16:
            raw = handle.read(min(chunk_size, 64))
            audio_format, channels, rate, _, _, bits = struct.unpack("<HHIIHH", raw[:16])
            if audio_format == 0xFFFE and len(raw) >= 26:
                audio_format = struct.unpack("<H", raw[24:26])[0]
            fmt = (audio_format, channels, rate, bits)
        elif ident == b"data":
            data_size = chunk_size
            break
        nxt = pos + chunk_size + (chunk_size & 1)
        if chunk_size <= 0 or nxt <= pos:
            break
        handle.seek(nxt)
    if fmt is None:
        return None
    audio_format, channels, rate, bits = fmt
    length = None
    frame = channels * (bits // 8)
    if data_size is not None and frame and rate:
        length = int(round(data_size / frame / rate))
    return Header("WAV", length, _pcm_detail("WAV", audio_format, bits, rate, channels))


def _extended(raw: bytes) -> float:
    """80-bit AIFF sample rate."""
    if len(raw) < 10:
        return 0.0
    exponent = ((raw[0] & 0x7F) << 8) | raw[1]
    mantissa = int.from_bytes(raw[2:10], "big")
    if exponent == 0 or mantissa == 0:
        return 0.0
    value = mantissa / (1 << 63) * (2 ** (exponent - 16383))
    if raw[0] & 0x80:
        value = -value
    return value


def _aiff(handle, size: int, compressed: bool) -> Header | None:
    handle.seek(12)
    found = None
    for _ in range(64):
        if handle.tell() + 8 > size:
            break
        ident = handle.read(4)
        raw_size = handle.read(4)
        if len(ident) < 4 or len(raw_size) < 4:
            break
        chunk_size = struct.unpack(">I", raw_size)[0]
        pos = handle.tell()
        if ident == b"COMM" and chunk_size >= 18:
            raw = handle.read(min(chunk_size, 64))
            channels, frames, bits = struct.unpack(">HIH", raw[:8])
            rate = _extended(raw[8:18])
            compression = raw[18:22] if compressed and len(raw) >= 22 else b"NONE"
            found = (channels, frames, bits, rate, compression)
            break
        nxt = pos + chunk_size + (chunk_size & 1)
        if chunk_size <= 0 or nxt <= pos:
            break
        handle.seek(nxt)
    if found is None:
        return None
    channels, frames, bits, rate, compression = found
    rate_i = int(round(rate)) if rate else 0
    comp = compression.upper()
    if comp in (b"NONE", b"TWOS", b"SOWT", b"RAW "):
        audio_format = 1
    elif comp in (b"FL32", b"FL64"):
        audio_format = 3
    else:
        audio_format = 0
    length = int(round(frames / rate)) if rate else None
    return Header("AIFF", length, _pcm_detail("AIFF", audio_format, bits, rate_i, channels))


def _flac(handle) -> Header | None:
    handle.seek(4)
    block = handle.read(4)
    if len(block) < 4:
        return None
    block_type = block[0] & 0x7F
    block_len = int.from_bytes(block[1:4], "big")
    if block_type != 0 or block_len < 18:
        return None
    info = handle.read(18)
    if len(info) < 18:
        return None
    packed = int.from_bytes(info[10:18], "big")
    rate = packed >> 44
    channels = ((packed >> 41) & 0x7) + 1
    samples = packed & 0xFFFFFFFFF
    length = int(round(samples / rate)) if rate and samples else None
    return Header("FLAC", length, _rate_detail("FLAC", rate, channels))


def _mp4(handle, size: int) -> Header:
    duration = None
    rate = None
    channels = None
    try:
        handle.seek(0)

        def walk(end: int, depth: int) -> None:
            nonlocal duration, rate, channels
            while handle.tell() + 8 <= end:
                start = handle.tell()
                raw_size = handle.read(4)
                kind = handle.read(4)
                if len(raw_size) < 4 or len(kind) < 4:
                    return
                box_size = struct.unpack(">I", raw_size)[0]
                header = 8
                if box_size == 1:
                    (box_size,) = struct.unpack(">Q", handle.read(8))
                    header = 16
                elif box_size == 0:
                    box_size = end - start
                if box_size < header:
                    return
                box_end = start + box_size
                if box_end > size:
                    return
                if kind in (b"moov", b"trak", b"mdia", b"minf", b"stbl") and depth < 6:
                    walk(box_end, depth + 1)
                elif kind == b"mvhd":
                    version = handle.read(1)
                    if version == b"\x00":
                        handle.seek(start + header + 12)
                        timescale, dur = struct.unpack(">II", handle.read(8))
                    else:
                        handle.seek(start + header + 20)
                        timescale = struct.unpack(">I", handle.read(4))[0]
                        dur = struct.unpack(">Q", handle.read(8))[0]
                    if timescale:
                        duration = int(round(dur / timescale))
                elif kind == b"mdhd":
                    version = handle.read(1)
                    if version == b"\x00":
                        handle.seek(start + header + 12)
                    else:
                        handle.seek(start + header + 20)
                    (timescale,) = struct.unpack(">I", handle.read(4))
                    if timescale:
                        rate = timescale
                elif kind == b"stsd" and box_end - (start + header) >= 28:
                    handle.seek(start + header + 8)
                    handle.read(8)
                    handle.read(16)
                    if handle.tell() + 2 <= box_end:
                        (channels,) = struct.unpack(">H", handle.read(2))
                handle.seek(box_end)

        walk(size, 0)
    except (OSError, struct.error):
        pass
    return Header("M4A", duration, _rate_detail("M4A", rate, channels))


def _mp3_frame(data: bytes) -> bool:
    if len(data) < 4 or data[0] != 0xFF or (data[1] & 0xE0) != 0xE0:
        return False
    version = (data[1] >> 3) & 0x03
    layer = (data[1] >> 1) & 0x03
    bitrate = (data[2] >> 4) & 0x0F
    sample_rate = (data[2] >> 2) & 0x03
    return version != 1 and layer == 1 and bitrate not in (0, 15) and sample_rate != 3


def _mp3(handle, size: int) -> Header | None:
    handle.seek(0)
    lead = handle.read(10)
    offset = 0
    if lead.startswith(b"ID3") and len(lead) >= 10:
        tag_size = ((lead[6] & 0x7F) << 21) | ((lead[7] & 0x7F) << 14) | ((lead[8] & 0x7F) << 7) | (lead[9] & 0x7F)
        offset = 10 + tag_size
        if lead[5] & 0x10:
            offset += 10
    if offset >= size:
        return None
    handle.seek(offset)
    frame = handle.read(4)
    if not _mp3_frame(frame):
        return None
    version = (frame[1] >> 3) & 0x03
    sample_index = (frame[2] >> 2) & 0x03
    rates = {3: (44100, 48000, 32000), 2: (22050, 24000, 16000), 0: (11025, 12000, 8000)}
    rate = rates[version][sample_index]
    samples = 1152 if version == 3 else 576
    mode = (frame[3] >> 6) & 0x03
    side = (17 if mode == 3 else 32) if version == 3 else (9 if mode == 3 else 17)
    length = _mp3_length(handle, offset, side, samples, rate)
    detail = None if version == 3 and rate in CDJ_RATES else f"MP3 is {_khz(rate)}"
    return Header("MP3", length, detail)


def _mp3_length(handle, offset: int, side: int, samples: int, rate: int) -> int | None:
    handle.seek(offset + 4 + side)
    tag = handle.read(4)
    frames = None
    if tag in (b"Xing", b"Info") and len(tag) == 4:
        flags = handle.read(4)
        if len(flags) == 4 and struct.unpack(">I", flags)[0] & 1:
            raw = handle.read(4)
            if len(raw) == 4:
                frames = struct.unpack(">I", raw)[0]
    if frames is None:
        handle.seek(offset + 36)
        if handle.read(4) == b"VBRI":
            handle.read(6)
            raw = handle.read(4)
            if len(raw) == 4:
                frames = struct.unpack(">I", raw)[0]
    if not frames or not rate:
        return None
    return int(round(frames * samples / rate))
