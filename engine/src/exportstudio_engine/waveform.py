import base64
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
HOT_CUE_TAG = "PCO2"

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
