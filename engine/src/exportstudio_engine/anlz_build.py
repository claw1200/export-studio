"""Builds rekordbox analysis files (ANLZ0000.DAT/.EXT/.2EX) from our own analysis.

The native analyser supplies the beatgrid and, per 1/150 s, the peak and RMS of the whole signal
and of its low, mid and high bands. Each waveform tag's values are fitted to those features: the
coefficients below come from least squares against a few hundred files rekordbox analysed.
"""

import struct

import numpy as np

from . import waveform

FILE_HEADER = struct.pack(">4sII4I", b"PMAI", 0x1C, 0, 1, 0x10000, 0x10000, 0)
FEATURES = 8
PREVIEW = 400
TINY_PREVIEW = 100
COLOR_PREVIEW = 1200
VBR_ENTRIES = 400
MEMORY_CUES, HOT_CUES = 0, 1

# lane: (transform, weights for [peak, rms, low peak, low rms, mid peak, mid rms, high peak,
# high rms], bias, largest value)
LANES: dict[str, tuple[str, tuple[float, ...], float, int]] = {
    "PWAV.h": ("sqrt", (-14.02, 15.97, 13.10, 7.54, 4.05, 3.72, 3.85, -2.83), -1.95, 31),
    "PWAV.w": ("lin", (-1.48, 3.86, -2.58, -2.09, -0.60, -4.74, 1.43, 2.78), 5.17, 7),
    "PWV2.h": ("sqrt", (1.37, -21.80, 3.54, 23.21, 3.45, 9.53, 0.72, 1.51), 0.0, 15),
    "PWV3.h": ("lin", (25.29, 5.01, 2.72, 1.14, 0.95, 1.05, 4.15, 2.47), -6.28, 31),
    "PWV3.w": ("sqrt", (4.92, -1.08, -11.27, 0.19, -0.26, 3.46, 0.43, 0.15), 5.11, 7),
    "PWV4.0": ("lin", (112.16, 25.17, 7.06, -17.46, 3.21, -9.88, 3.70, 0.18), -0.74, 127),
    "PWV4.1": ("lin", (-63.49, 8.18, -14.55, -13.52, -7.43, 6.34, -8.79, -18.18), 221.95, 255),
    "PWV4.2": ("lin", (22.86, -5.73, 105.72, -3.62, 4.98, 41.16, -3.21, -20.09), -0.76, 127),
    "PWV4.3": ("lin", (-7.30, -20.70, 122.10, 34.91, 3.05, -22.41, 2.26, 14.73), -0.47, 127),
    "PWV4.4": ("lin", (-29.73, 14.26, 11.20, -16.15, 83.41, 86.89, 11.10, -56.97), -0.62, 127),
    "PWV4.5": ("lin", (-33.40, -7.57, 9.58, 19.14, 9.77, -18.40, 66.78, 174.44), 0.71, 127),
    "PWV5.h": ("lin", (21.02, 0.47, 5.93, 4.91, 0.58, 6.49, 1.72, 14.28), -5.70, 31),
    "PWV5.r": ("db", (-3.89, -0.45, 8.66, 6.83, -1.62, -7.16, 0.98, -1.05), 2.87, 7),
    "PWV5.g": ("db", (-5.36, -10.93, -3.27, 2.59, 11.23, 6.14, -1.29, 1.98), 4.40, 7),
    "PWV5.b": ("db", (-0.47, -14.73, -5.18, 4.91, -2.06, 4.87, 7.76, 5.57), 6.87, 7),
    "PWV6.0": ("lin", (9.36, -49.95, -6.82, 102.51, -8.26, 47.31, -0.59, 37.32), 2.11, 127),
    "PWV6.1": ("sqrt", (10.97, -24.28, 1.17, 10.32, -18.08, 87.55, -0.60, 9.58), -3.75, 127),
    "PWV6.2": ("sqrt", (7.69, -29.15, 3.82, 15.02, -2.71, 5.17, -6.46, 99.37), -3.85, 127),
    "PWV7.0": ("sqrt", (-11.08, 21.93, 105.01, -14.33, -10.01, 3.71, 11.19, -8.91), -3.34, 127),
    "PWV7.1": ("lin", (6.07, -1.04, 1.67, 2.93, 38.22, 85.53, -6.94, 9.27), 6.48, 127),
    "PWV7.2": ("lin", (-15.90, 10.01, 13.31, -5.97, -18.48, 23.04, 61.39, 123.84), -4.69, 127),
}
DB_FLOOR = -60.0
SILENCE = 1 / 255

WAVE_TAGS = {
    "DAT": ("PWAV", "PWV2"),
    "EXT": ("PWV3", "PWV5", "PWV4"),
    "2EX": ("PWV7", "PWV6"),
}


def _transform(x: np.ndarray, kind: str) -> np.ndarray:
    if kind == "sqrt":
        return np.sqrt(x)
    if kind == "db":
        return np.maximum(20 * np.log10(x + 1e-4), DB_FLOOR) / -DB_FLOOR + 1
    return x


def _lane(name: str, features: np.ndarray) -> np.ndarray:
    kind, weights, bias, top = LANES[name]
    values = _transform(features, kind) @ np.asarray(weights) + bias
    values = np.clip(np.round(values), 0, top).astype(np.uint16)
    values[features[:, 0] < SILENCE] = 0
    return values


def _pool(features: np.ndarray, columns: int) -> np.ndarray:
    """Detail features pooled into `columns` preview columns: the loudest peak, and the RMS."""
    n = len(features)
    if n == 0:
        return np.zeros((columns, FEATURES))
    starts = np.minimum((np.arange(columns) * n) // columns, n - 1)
    out = np.empty((columns, FEATURES))
    out[:, 0::2] = np.maximum.reduceat(features[:, 0::2], starts, axis=0)
    sums = np.add.reduceat(features[:, 1::2] ** 2, starts, axis=0)
    counts = np.diff(np.append(starts, n)).clip(min=1)[:, None]
    out[:, 1::2] = np.sqrt(sums / counts)
    return out


def _tag(kind: str, header: bytes, body: bytes) -> bytes:
    len_header = 12 + len(header)
    return struct.pack(">4sII", kind.encode(), len_header, len_header + len(body)) + header + body


def path_tag(name: str) -> bytes:
    body = name.encode("utf-16-be") + b"\0\0"
    return _tag("PPTH", struct.pack(">I", len(body)), body)


def _vbr_tag() -> bytes:
    return _tag("PVBR", bytes(4), bytes(4 * (VBR_ENTRIES + 1)))


def beatgrid_tag(beats: list[tuple[int, int, int]]) -> bytes:
    body = b"".join(struct.pack(">HHI", number, tempo, ms) for number, tempo, ms in beats)
    return _tag("PQTZ", struct.pack(">III", 0, 0x80000, len(beats)), body)


def _cue_tags() -> list[bytes]:
    return [
        _tag("PCOB", struct.pack(">IHHi", kind, 0, 0, -1), b"") for kind in (HOT_CUES, MEMORY_CUES)
    ]


def _extended_cue_tags() -> list[bytes]:
    return [_tag("PCO2", struct.pack(">IHH", kind, 0, 0), b"") for kind in (HOT_CUES, MEMORY_CUES)]


def wave_tags(features: np.ndarray) -> dict[str, bytes]:
    """Every waveform tag we write, keyed by tag kind. `features` is n x 8, 0 to 1."""
    preview = _pool(features, PREVIEW)
    tiny = _pool(features, TINY_PREVIEW)
    color = _pool(features, COLOR_PREVIEW)
    n = len(features)

    pwav = (_lane("PWAV.h", preview) | (_lane("PWAV.w", preview) << 5)).astype(np.uint8)
    pwv2 = _lane("PWV2.h", tiny).astype(np.uint8)
    pwv3 = (_lane("PWV3.h", features) | (_lane("PWV3.w", features) << 5)).astype(np.uint8)
    pwv5 = (
        (_lane("PWV5.r", features) << 13)
        | (_lane("PWV5.g", features) << 10)
        | (_lane("PWV5.b", features) << 7)
        | (_lane("PWV5.h", features) << 2)
    ).astype(">u2")
    pwv4 = np.stack([_lane(f"PWV4.{k}", color) for k in range(6)], axis=1).astype(np.uint8)
    # Silence is bright in rekordbox's second colour-preview byte, not dark.
    pwv4[color[:, 0] < SILENCE, 1] = 255
    pwv6 = np.stack([_lane(f"PWV6.{k}", color) for k in range(3)], axis=1).astype(np.uint8)
    pwv7 = np.stack([_lane(f"PWV7.{k}", features) for k in range(3)], axis=1).astype(np.uint8)

    rate = int(waveform.DETAIL_RATE)
    return {
        "PWAV": _tag("PWAV", struct.pack(">II", PREVIEW, 0x10000), pwav.tobytes()),
        "PWV2": _tag("PWV2", struct.pack(">II", TINY_PREVIEW, 0x10000), pwv2.tobytes()),
        "PWV3": _tag("PWV3", struct.pack(">III", 1, n, rate << 16), pwv3.tobytes()),
        "PWV5": _tag("PWV5", struct.pack(">III", 2, n, (rate << 16) | 0x305), pwv5.tobytes()),
        "PWV4": _tag("PWV4", struct.pack(">III", 6, COLOR_PREVIEW, 0), pwv4.tobytes()),
        "PWV6": _tag("PWV6", struct.pack(">II", 3, COLOR_PREVIEW), pwv6.tobytes()),
        "PWV7": _tag("PWV7", struct.pack(">III", 3, n, rate << 16), pwv7.tobytes()),
    }


def _file(tags: list[bytes]) -> bytes:
    out = bytearray(FILE_HEADER)
    for tag in tags:
        out += tag
    struct.pack_into(">I", out, waveform.FILE_LENGTH_OFFSET, len(out))
    return bytes(out)


def new_files(audio_name: str, beats: list[tuple[int, int, int]], features: np.ndarray) -> dict[str, bytes]:
    """DAT, EXT and 2EX for a track with no analysis yet, laid out as rekordbox writes them."""
    path = path_tag("?/" + audio_name)
    waves = wave_tags(features)
    return {
        "DAT": _file([path, _vbr_tag(), beatgrid_tag(beats), waves["PWAV"], waves["PWV2"], *_cue_tags()]),
        "EXT": _file([path, waves["PWV3"], *_cue_tags(), *_extended_cue_tags(), waves["PWV5"], waves["PWV4"]]),
        "2EX": _file([path, waves["PWV7"], waves["PWV6"]]),
    }


def patch_file(data: bytes, replace: dict[str, bytes], drop: set[str] = frozenset()) -> bytes:
    """`data` with tags of the kinds in `replace` swapped in place (or added at the end when the
    file has none), and tags in `drop` removed. Everything else is kept byte for byte."""
    if data[:4] != b"PMAI":
        raise ValueError("Not an ANLZ file")
    (file_header,) = struct.unpack_from(">I", data, 4)
    out = bytearray(data[:file_header])
    pending = dict(replace)
    pos = file_header
    while pos + waveform.TAG_HEADER_BYTES <= len(data):
        kind = data[pos : pos + 4].decode("ascii", "replace")
        _len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < waveform.TAG_HEADER_BYTES:
            break
        if kind in pending:
            out += pending.pop(kind)
        elif kind not in drop and kind not in replace:
            out += data[pos : pos + len_tag]
        pos += len_tag
    for tag in pending.values():
        out += tag
    struct.pack_into(">I", out, waveform.FILE_LENGTH_OFFSET, len(out))
    return bytes(out)


def features_from_bytes(raw: bytes) -> np.ndarray:
    usable = len(raw) - len(raw) % FEATURES
    return np.frombuffer(raw[:usable], dtype=np.uint8).reshape(-1, FEATURES).astype(np.float64) / 255
