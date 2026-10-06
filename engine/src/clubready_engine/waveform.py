"""Fast, read-only decoding of the rekordbox ANLZ tags the UI displays.

pyrekordbox's ``AnlzFile`` parses every tag with ``construct``, which takes ~40 ms
per track. Here files are only split by tag header and the tags we need are decoded
directly with numpy. Layouts follow https://djl-analysis.deepsymmetry.org.

All detail waveforms are sampled at 150 entries per second of audio.
"""

import base64
import struct
from pathlib import Path

import numpy as np
from pyrekordbox.anlz import structs

DETAIL_RATE = 150

Tags = dict[str, list[memoryview]]


def read_tags(path: Path) -> Tags:
    """Split an ANLZ file into raw tags (each view includes its own 12-byte header)."""
    data = memoryview(path.read_bytes())
    if bytes(data[:4]) != b"PMAI":
        raise ValueError(f"Not an ANLZ file: {path}")
    (file_header,) = struct.unpack_from(">I", data, 4)
    tags: Tags = {}
    pos = file_header
    while pos + 12 <= len(data):
        kind = bytes(data[pos : pos + 4]).decode("ascii", "replace")
        _len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < 12:
            break
        tags.setdefault(kind, []).append(data[pos : pos + len_tag])
        pos += len_tag
    return tags


def _first(tags: Tags | None, kind: str) -> memoryview | None:
    if not tags or kind not in tags:
        return None
    return tags[kind][0]


def _body(tag: memoryview) -> tuple[memoryview, int]:
    """Returns (entries, len_entries) for the common 24-byte waveform tag header."""
    len_header, _len_tag, _entry_bytes, len_entries = struct.unpack_from(">IIII", tag, 4)
    return tag[len_header:], len_entries


def _b64(arr: np.ndarray) -> str:
    return base64.b64encode(np.ascontiguousarray(arr, dtype=np.uint8).tobytes()).decode("ascii")


def decode_rgb(ext: Tags | None) -> dict | None:
    """PWV5: one big-endian uint16 per entry, bits `rrrgggbbbhhhhh00`."""
    tag = _first(ext, "PWV5")
    if tag is None:
        return None
    body, n = _body(tag)
    if not n:
        return None
    x = np.frombuffer(body, dtype=">u2", count=n).astype(np.uint16)
    rgb = np.stack([(x >> 13) & 0x7, (x >> 10) & 0x7, (x >> 7) & 0x7], axis=1).astype(np.float32)
    h = ((x >> 2) & 0x1F).astype(np.float32)
    return {
        "length": int(n),
        "heights": _b64((h * (255 / 31)).round()),
        "colors": _b64((rgb * (255 / 7)).round().reshape(-1)),
    }


def decode_three_band(ex2: Tags | None) -> dict | None:
    """PWV7: three bytes per entry, ordered mid, high, low. Returned as low, mid, high."""
    tag = _first(ex2, "PWV7")
    if tag is None:
        return None
    body, n = _body(tag)
    if not n:
        return None
    raw = np.frombuffer(body, dtype=np.uint8, count=n * 3).reshape(-1, 3)
    bands = raw[:, [2, 0, 1]].astype(np.float32) * 2
    return {"length": int(n), "bands": _b64(np.clip(bands, 0, 255).round().reshape(-1))}


def decode_mono(ext: Tags | None) -> dict | None:
    """PWV3: one byte per entry, 3 bits whiteness + 5 bits height."""
    tag = _first(ext, "PWV3")
    if tag is None:
        return None
    body, n = _body(tag)
    if not n:
        return None
    x = np.frombuffer(body, dtype=np.uint8, count=n)
    h = (x & 0x1F).astype(np.float32) * (255 / 31)
    w = (x >> 5).astype(np.float32) * (255 / 7)
    return {"length": int(n), "heights": _b64(h.round()), "whiteness": _b64(w.round())}


_BEAT = np.dtype([("beat", ">u2"), ("tempo", ">u2"), ("time", ">u4")])


def decode_beatgrid(dat: Tags | None) -> dict | None:
    """PQTZ: 24-byte header, then (beat u16, tempo*100 u16, time_ms u32) entries."""
    tag = _first(dat, "PQTZ")
    if tag is None:
        return None
    len_header = struct.unpack_from(">I", tag, 4)[0]
    (count,) = struct.unpack_from(">I", tag, 20)
    if not count:
        return None
    entries = np.frombuffer(tag[len_header:], dtype=_BEAT, count=count)
    return {
        "times": entries["time"].astype(int).tolist(),
        "beats": entries["beat"].astype(int).tolist(),
        "bpms": (entries["tempo"] / 100).round(2).tolist(),
    }


def hot_cue_colors(ext: Tags | None) -> dict[tuple[int, int], str]:
    """PCO2 hot cue RGB colours keyed by (hot cue number, time in ms)."""
    colors: dict[tuple[int, int], str] = {}
    for tag in (ext or {}).get("PCO2", []):
        try:
            content = structs.AnlzTag.parse(bytes(tag)).content
        except Exception:
            continue
        for e in content.entries:
            if e.hot_cue:
                colors[(int(e.hot_cue), int(e.time))] = f"#{e.color_red:02x}{e.color_green:02x}{e.color_blue:02x}"
    return colors
