"""Read a collection file's container header.

Returns measurements the client compares with player limits. MP3 length is not
read; a Xing or VBRI tag is not required.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Header:
    kind: str
    sample_rate: int
    bit_depth: int
    channels: int
    encoding: str
    bitrate: int = 0


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


def inspect_file(path: Path) -> dict:
    """Header measurements for one local file. Missing and unreadable files have no audio."""
    if not path.is_file():
        return {"size": 0, "mtime": None, "issues": [{"code": "missing", "detail": "File missing"}], "audio": None}

    try:
        stat = path.stat()
    except OSError:
        return {"size": 0, "mtime": None, "issues": [{"code": "missing", "detail": "File missing"}], "audio": None}

    size = stat.st_size
    # Nanoseconds don't fit in a JSON number, so the client keeps this as text.
    mtime = str(stat.st_mtime_ns)
    unreadable = {"code": "unreadable", "detail": "File can't be read"}
    if size <= 0:
        return {"size": size, "mtime": mtime, "issues": [unreadable], "audio": None}
    try:
        header = _read_header(path, size)
    except OSError:
        header = None
    if header is None:
        return {"size": size, "mtime": mtime, "issues": [unreadable], "audio": None}
    return {"size": size, "mtime": mtime, "issues": [], "audio": _audio(header)}


def _audio(header: Header) -> dict:
    return {
        "kind": header.kind,
        "sampleRate": header.sample_rate,
        "bitDepth": header.bit_depth,
        "channels": header.channels,
        "bitrate": header.bitrate,
        "encoding": header.encoding,
    }


def _pcm_encoding(audio_format: int) -> str:
    if audio_format == 3:
        return "float"
    if audio_format == 1:
        return "pcm"
    return "other"


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
            break
        nxt = pos + chunk_size + (chunk_size & 1)
        if chunk_size <= 0 or nxt <= pos:
            break
        handle.seek(nxt)
    if fmt is None:
        return None
    audio_format, channels, rate, bits = fmt
    return Header("WAV", rate, bits, channels, _pcm_encoding(audio_format))


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
            channels, _frames, bits = struct.unpack(">HIH", raw[:8])
            rate = _extended(raw[8:18])
            compression = raw[18:22] if compressed and len(raw) >= 22 else b"NONE"
            found = (channels, bits, rate, compression)
            break
        nxt = pos + chunk_size + (chunk_size & 1)
        if chunk_size <= 0 or nxt <= pos:
            break
        handle.seek(nxt)
    if found is None:
        return None
    channels, bits, rate, compression = found
    rate_i = int(round(rate)) if rate else 0
    comp = compression.upper()
    if comp in (b"NONE", b"TWOS", b"SOWT", b"RAW "):
        audio_format = 1
    elif comp in (b"FL32", b"FL64"):
        audio_format = 3
    else:
        audio_format = 0
    return Header("AIFF", rate_i, bits, channels, _pcm_encoding(audio_format))


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
    bits = ((packed >> 36) & 0x1F) + 1
    return Header("FLAC", rate, bits, channels, "flac")


def _mp4(handle, size: int) -> Header:
    rate = 0
    channels = 0
    bits = 0
    codec = b""
    try:
        handle.seek(0)

        def walk(end: int, depth: int) -> None:
            nonlocal rate, channels, bits, codec
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
                    entry = handle.read(8)
                    if len(entry) >= 8:
                        codec = entry[4:8]
                    handle.read(16)
                    if handle.tell() + 4 <= box_end:
                        channels, bits = struct.unpack(">HH", handle.read(4))
                handle.seek(box_end)

        walk(size, 0)
    except (OSError, struct.error):
        pass
    if codec == b"alac":
        return Header("ALAC", rate, bits, channels, "alac")
    return Header("AAC", rate, bits, channels, "aac")


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
    bitrate_index = (frame[2] >> 4) & 0x0F
    rates = {3: (44100, 48000, 32000), 2: (22050, 24000, 16000), 0: (11025, 12000, 8000)}
    rate = rates[version][sample_index]
    bitrates = {
        3: (0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0),
        2: (0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0),
        0: (0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0),
    }
    mode = (frame[3] >> 6) & 0x03
    channels = 1 if mode == 3 else 2
    return Header("MP3", rate, 0, channels, "mp3", bitrates[version][bitrate_index])
