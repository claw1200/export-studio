import base64
import os
import struct
from pathlib import Path

import numpy as np
from pyrekordbox.anlz import structs

DETAIL_RATE = 150
TAG_HEADER_BYTES = 12

RGB_TAG = "PWV5"
DETAIL_BANDS_TAG = "PWV7"
PREVIEW_BANDS_TAG = "PWV6"
MONO_TAG = "PWV3"
BEATGRID_TAG = "PQTZ"
EXTENDED_BEATGRID_TAG = "PQT2"
HOT_CUE_TAG = "PCO2"
PATH_TAG = "PPTH"
PHRASE_TAG = "PSSI"
FILE_LENGTH_OFFSET = 8
BROWSER_WAVE_COLUMNS = 400
TAG_LENGTH_OFFSET = 8
PATH_LENGTH_OFFSET = 12
PQT2_ANCHORS_OFFSET = 24

RGB_BITS = 0x7
HEIGHT_BITS = 0x1F
RED_SHIFT = 13
GREEN_SHIFT = 10
BLUE_SHIFT = 7
HEIGHT_SHIFT = 2
WHITENESS_SHIFT = 5
BYTE_MAX = 255
RGB_MAX = 7
HEIGHT_MAX = 31
BAND_SCALE = 2

Tags = dict[str, list[memoryview]]


def read_tags(path: Path) -> Tags:
    data = memoryview(path.read_bytes())
    if bytes(data[:4]) != b"PMAI":
        raise ValueError(f"Not an ANLZ file: {path}")
    (file_header,) = struct.unpack_from(">I", data, 4)
    tags: Tags = {}
    pos = file_header
    while pos + TAG_HEADER_BYTES <= len(data):
        kind = bytes(data[pos : pos + 4]).decode("ascii", "replace")
        _len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < TAG_HEADER_BYTES:
            break
        tags.setdefault(kind, []).append(data[pos : pos + len_tag])
        pos += len_tag
    return tags


def _first(tags: Tags | None, kind: str) -> memoryview | None:
    if not tags or kind not in tags:
        return None
    return tags[kind][0]


def _body(tag: memoryview) -> tuple[memoryview, int]:
    len_header, _len_tag, _entry_bytes, len_entries = struct.unpack_from(">IIII", tag, 4)
    return tag[len_header:], len_entries


def _entries(tags: Tags | None, kind: str) -> tuple[memoryview, int] | None:
    tag = _first(tags, kind)
    if tag is None:
        return None
    body, count = _body(tag)
    if not count:
        return None
    return body, count


def _b64(arr: np.ndarray) -> str:
    return base64.b64encode(np.ascontiguousarray(arr, dtype=np.uint8).tobytes()).decode("ascii")


def _to_bytes(values: np.ndarray, stored_max: int) -> np.ndarray:
    return (values.astype(np.float32) * (BYTE_MAX / stored_max)).round()


def decode_rgb(ext: Tags | None) -> dict | None:
    found = _entries(ext, RGB_TAG)
    if found is None:
        return None
    body, count = found
    packed = np.frombuffer(body, dtype=">u2", count=count).astype(np.uint16)
    channels = np.stack(
        [
            (packed >> RED_SHIFT) & RGB_BITS,
            (packed >> GREEN_SHIFT) & RGB_BITS,
            (packed >> BLUE_SHIFT) & RGB_BITS,
        ],
        axis=1,
    )
    heights = (packed >> HEIGHT_SHIFT) & HEIGHT_BITS
    return {
        "length": int(count),
        "heights": _b64(_to_bytes(heights, HEIGHT_MAX)),
        "colors": _b64(_to_bytes(channels, RGB_MAX).reshape(-1)),
    }


def decode_three_band(ex2: Tags | None, kind: str = DETAIL_BANDS_TAG) -> dict | None:
    found = _entries(ex2, kind)
    if found is None:
        return None
    body, count = found
    raw = np.frombuffer(body, dtype=np.uint8, count=count * 3).reshape(-1, 3)
    bands = np.clip(raw.astype(np.float32) * BAND_SCALE, 0, BYTE_MAX).round()
    return {"length": int(count), "bands": _b64(bands.reshape(-1))}


def has_phrase(ext: Tags | None) -> bool:
    return bool(ext and ext.get(PHRASE_TAG))


def _hot_slots() -> set[int]:
    return {kind for kind in range(1, 10) if kind != 4}


def cue_marks(kinds) -> tuple[bool, bool]:
    """Whether any memory cue (kind 0) or hot cue is present. One of each, at most."""
    memory = hot = False
    hot_kinds = _hot_slots()
    for kind in kinds:
        slot = int(kind or 0)
        if slot in hot_kinds:
            hot = True
        elif slot == 0:
            memory = True
        if memory and hot:
            break
    return memory, hot


def cue_sort_facts(kinds, phrase: bool) -> dict:
    """Star count for the CUE column, and how many memory and hot cues are placed.

    A loop (kind 4) is neither. Phrase analysis adds a star and no cue.
    """
    memory = hot = 0
    hot_kinds = _hot_slots()
    for kind in kinds:
        slot = int(kind or 0)
        if slot in hot_kinds:
            hot += 1
        elif slot == 0:
            memory += 1
    return {
        "cueCount": memory + hot,
        "cueStars": int(memory > 0) + int(hot > 0) + int(phrase),
        "phrase": bool(phrase),
    }


def file_has_tag(path: Path, kind: str) -> bool:
    """Whether an ANLZ file contains `kind`, skipping each tag body."""
    try:
        with path.open("rb") as handle:
            head = handle.read(8)
            if len(head) < 8 or head[:4] != b"PMAI":
                return False
            (file_header,) = struct.unpack(">I", head[4:8])
            handle.seek(file_header)
            target = kind.encode("ascii")
            while True:
                tag = handle.read(TAG_HEADER_BYTES)
                if len(tag) < TAG_HEADER_BYTES:
                    return False
                if tag[:4] == target:
                    return True
                _len_header, len_tag = struct.unpack_from(">II", tag, 4)
                if len_tag < TAG_HEADER_BYTES:
                    return False
                handle.seek(len_tag - TAG_HEADER_BYTES, os.SEEK_CUR)
    except OSError:
        return False


def browser_cue(cue_id, kind, start, end, color, color_index, comment) -> dict:
    """One cue in the shape the browser markers draw, without an analysis colour yet."""
    slot = int(kind or 0)
    return {
        "id": str(cue_id),
        "kind": "hot" if slot > 0 else "memory",
        "slot": slot,
        "start": int(start or 0),
        "end": None if end is None or int(end) < 0 else int(end),
        "color": None if color is None or int(color) < 0 else int(color),
        "colorIndex": None if color_index is None else int(color_index),
        "comment": comment or "",
    }


def paint_browser_cues(cues: list[dict], colors: dict) -> list[dict]:
    painted = []
    for cue in cues:
        rgb = colors.get((int(cue["slot"]), int(cue["start"]))) if cue["kind"] == "hot" else None
        painted.append({**cue, "rgb": rgb})
    painted.sort(key=lambda cue: (cue["kind"] != "hot", cue["slot"], cue["start"]))
    return painted


def browser_waveform(ext: Tags | None, ex2: Tags | None) -> dict | None:
    """A short overview: 3-band data for that drawing, and a colour waveform for RGB drawing."""
    out: dict = {}
    for kind in (PREVIEW_BANDS_TAG, DETAIL_BANDS_TAG):
        rows = _band_rows(ex2, kind)
        if rows is not None:
            pooled = _pool_peak(rows, BROWSER_WAVE_COLUMNS)
            out["bandLength"] = int(len(pooled))
            out["bands"] = _b64(pooled.reshape(-1))
            break
    colour = _rgb_rows(ext) or _mono_rows(ext)
    if colour is not None:
        heights, colors = _pool_rgb(*colour, BROWSER_WAVE_COLUMNS)
        out["rgbLength"] = int(len(heights))
        out["heights"] = _b64(heights)
        out["colors"] = _b64(colors.reshape(-1))
    return out or None


def _band_rows(tags: Tags | None, kind: str) -> np.ndarray | None:
    found = _entries(tags, kind)
    if found is None:
        return None
    body, count = found
    raw = np.frombuffer(body, dtype=np.uint8, count=count * 3).reshape(-1, 3)
    return np.clip(raw.astype(np.float32) * BAND_SCALE, 0, BYTE_MAX).round().astype(np.uint8)


def _rgb_rows(ext: Tags | None) -> tuple[np.ndarray, np.ndarray] | None:
    found = _entries(ext, RGB_TAG)
    if found is None:
        return None
    body, count = found
    packed = np.frombuffer(body, dtype=">u2", count=count).astype(np.uint16)
    channels = np.stack(
        [
            (packed >> RED_SHIFT) & RGB_BITS,
            (packed >> GREEN_SHIFT) & RGB_BITS,
            (packed >> BLUE_SHIFT) & RGB_BITS,
        ],
        axis=1,
    )
    heights = (packed >> HEIGHT_SHIFT) & HEIGHT_BITS
    return _to_bytes(heights, HEIGHT_MAX).astype(np.uint8), _to_bytes(channels, RGB_MAX).astype(np.uint8)


def _mono_rows(ext: Tags | None) -> tuple[np.ndarray, np.ndarray] | None:
    found = _entries(ext, MONO_TAG)
    if found is None:
        return None
    body, count = found
    packed = np.frombuffer(body, dtype=np.uint8, count=count)
    heights = _to_bytes(packed & HEIGHT_BITS, HEIGHT_MAX).astype(np.uint8)
    white = ((packed >> WHITENESS_SHIFT) & RGB_BITS).astype(np.float32) / RGB_MAX
    colors = np.empty((count, 3), dtype=np.uint8)
    colors[:, 0] = np.rint(40 + 180 * white)
    colors[:, 1] = np.rint(110 + 130 * white)
    colors[:, 2] = 255
    return heights, colors


def _pool_peak(rows: np.ndarray, columns: int) -> np.ndarray:
    count = len(rows)
    if count <= columns:
        return np.ascontiguousarray(rows)
    out = np.empty((columns, rows.shape[1]), dtype=rows.dtype)
    for index in range(columns):
        start = (index * count) // columns
        end = ((index + 1) * count) // columns
        out[index] = rows[start:end].max(axis=0)
    return out


def _pool_rgb(heights: np.ndarray, colors: np.ndarray, columns: int) -> tuple[np.ndarray, np.ndarray]:
    count = len(heights)
    if count <= columns:
        return heights, colors
    out_h = np.empty(columns, dtype=np.uint8)
    out_c = np.empty((columns, 3), dtype=np.uint8)
    for index in range(columns):
        start = (index * count) // columns
        end = ((index + 1) * count) // columns
        peak = start + int(heights[start:end].argmax())
        out_h[index] = heights[peak]
        out_c[index] = colors[peak]
    return out_h, out_c


def decode_mono(ext: Tags | None) -> dict | None:
    found = _entries(ext, MONO_TAG)
    if found is None:
        return None
    body, count = found
    packed = np.frombuffer(body, dtype=np.uint8, count=count)
    return {
        "length": int(count),
        "heights": _b64(_to_bytes(packed & HEIGHT_BITS, HEIGHT_MAX)),
        "whiteness": _b64(_to_bytes(packed >> WHITENESS_SHIFT, RGB_MAX)),
    }


_BEAT = np.dtype([("beat", ">u2"), ("tempo", ">u2"), ("time", ">u4")])
BEAT_COUNT_OFFSET = 20


def decode_beatgrid(dat: Tags | None) -> dict | None:
    tag = _first(dat, BEATGRID_TAG)
    if tag is None:
        return None
    len_header = struct.unpack_from(">I", tag, 4)[0]
    (count,) = struct.unpack_from(">I", tag, BEAT_COUNT_OFFSET)
    if not count:
        return None
    entries = np.frombuffer(tag[len_header:], dtype=_BEAT, count=count)
    return {
        "times": entries["time"].astype(int).tolist(),
        "beats": entries["beat"].astype(int).tolist(),
        "bpms": (entries["tempo"] / 100).round(2).tolist(),
    }


def build_beatgrid(tag: memoryview, times: list[int], beats: list[int], bpms: list[float]) -> bytes:
    len_header = struct.unpack_from(">I", tag, 4)[0]
    entries = np.empty(len(times), dtype=_BEAT)
    entries["time"] = times
    entries["beat"] = beats
    entries["tempo"] = np.round(np.asarray(bpms, dtype=np.float64) * 100)
    body = entries.tobytes()
    header = bytearray(tag[:len_header])
    struct.pack_into(">I", header, TAG_LENGTH_OFFSET, len_header + len(body))
    struct.pack_into(">I", header, BEAT_COUNT_OFFSET, len(times))
    return bytes(header) + body


def clear_extended_beatgrid(tag: memoryview) -> bytes:
    """Empty PQT2, as rekordbox writes for many tracks. Its per-beat data can't be rebuilt, so players fall back to PQTZ."""
    len_header = struct.unpack_from(">I", tag, 4)[0]
    header = bytearray(tag[:len_header])
    header[PQT2_ANCHORS_OFFSET:] = bytes(len_header - PQT2_ANCHORS_OFFSET)
    struct.pack_into(">I", header, TAG_LENGTH_OFFSET, len_header)
    return bytes(header)


def build_path(tag: memoryview, audio_path: str) -> bytes:
    len_header = struct.unpack_from(">I", tag, 4)[0]
    body = audio_path.encode("utf-16-be") + b"\0\0"
    header = bytearray(tag[:len_header])
    struct.pack_into(">I", header, TAG_LENGTH_OFFSET, len_header + len(body))
    struct.pack_into(">I", header, PATH_LENGTH_OFFSET, len(body))
    return bytes(header) + body


def write_tags(path: Path, replace: dict[str, bytes]) -> None:
    data = path.read_bytes()
    if data[:4] != b"PMAI":
        raise ValueError(f"Not an ANLZ file: {path}")
    (file_header,) = struct.unpack_from(">I", data, 4)
    out = bytearray(data[:file_header])
    pos = file_header
    pending = dict(replace)
    while pos + TAG_HEADER_BYTES <= len(data):
        kind = data[pos : pos + 4].decode("ascii", "replace")
        _len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < TAG_HEADER_BYTES:
            break
        out += pending.pop(kind) if kind in pending else data[pos : pos + len_tag]
        pos += len_tag
    if pending:
        raise ValueError(f"{path.name} has no {', '.join(pending)} tag")
    out += data[pos:]
    struct.pack_into(">I", out, FILE_LENGTH_OFFSET, len(out))
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_bytes(out)
    os.replace(tmp, path)


def hot_cue_colors(ext: Tags | None) -> dict[tuple[int, int], str]:
    colors: dict[tuple[int, int], str] = {}
    for tag in (ext or {}).get(HOT_CUE_TAG, []):
        try:
            content = structs.AnlzTag.parse(bytes(tag)).content
        except Exception:
            continue
        for entry in content.entries:
            if not entry.hot_cue:
                continue
            red, green, blue = int(entry.color_red), int(entry.color_green), int(entry.color_blue)
            colors[(int(entry.hot_cue), int(entry.time))] = f"#{red:02x}{green:02x}{blue:02x}"
    return colors
