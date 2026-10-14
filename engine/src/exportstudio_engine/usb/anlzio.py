import struct


def _hot_slot(cue) -> int:
    return cue.kind if cue.kind and cue.kind > 0 else 0


def _is_loop(cue) -> bool:
    return cue.end_ms is not None and cue.end_ms >= 0


def _pcpt(cue, index: int, count: int) -> bytes:
    entry = bytearray(56)
    entry[0:4] = b"PCPT"
    struct.pack_into(">II", entry, 4, 28, 56)
    hot = _hot_slot(cue)
    struct.pack_into(">I", entry, 0x0C, hot)
    struct.pack_into(">I", entry, 0x10, 4 if cue.active_loop else 0)
    struct.pack_into(">I", entry, 0x14, 0x00010000)
    if hot:
        previous = following = 0xFFFF
    else:
        previous = 0xFFFF if index == 0 else index - 1
        following = 0xFFFF if index + 1 == count else index + 1
    struct.pack_into(">HH", entry, 0x18, previous, following)
    entry[0x1C] = 2 if _is_loop(cue) else 1
    entry[0x1D:0x20] = b"\x00\x03\xe8"
    struct.pack_into(">I", entry, 0x20, cue.start_ms & 0xFFFFFFFF)
    struct.pack_into(">I", entry, 0x24, (cue.end_ms if _is_loop(cue) else 0xFFFFFFFF) & 0xFFFFFFFF)
    return bytes(entry)


def _pcob(cue_type: int, cues: list) -> bytes:
    tail = 0xFFFFFFFF if cue_type == 1 or not cues else len(cues) - 1
    entries = b"".join(_pcpt(cue, index, len(cues)) for index, cue in enumerate(cues))
    body = struct.pack(">IHHI", cue_type, 0, len(cues), tail) + entries
    return b"PCOB" + struct.pack(">II", 24, 12 + len(body)) + body


def _comment_bytes(cue) -> bytes:
    text = cue.comment or ""
    if not text:
        return b""
    return text.encode("utf-16-be") + b"\x00\x00"


def _pcp2(cue) -> bytes:
    comment = _comment_bytes(cue)
    length = 88 + len(comment)
    entry = bytearray(length)
    entry[0:4] = b"PCP2"
    struct.pack_into(">II", entry, 4, 16, length)
    hot = _hot_slot(cue)
    struct.pack_into(">I", entry, 0x0C, hot)
    entry[0x10] = 2 if _is_loop(cue) else 1
    entry[0x11:0x14] = b"\x00\x03\xe8"
    struct.pack_into(">I", entry, 0x14, cue.start_ms & 0xFFFFFFFF)
    struct.pack_into(">I", entry, 0x18, (cue.end_ms if _is_loop(cue) else 0xFFFFFFFF) & 0xFFFFFFFF)
    color_index = cue.color_index
    entry[0x1C] = 0 if hot or color_index is None or color_index < 0 else color_index & 0xFF
    entry[0x1D] = 0x01
    beat = cue.beat_loop or 0
    struct.pack_into(">HH", entry, 0x24, (beat >> 16) & 0xFFFF, beat & 0xFFFF)
    struct.pack_into(">I", entry, 0x28, len(comment))
    if comment:
        entry[0x2C : 0x2C + len(comment)] = comment
    color = cue.color if isinstance(cue.color, int) and cue.color >= 0 else 0
    at = 0x2C + len(comment)
    entry[at] = 0 if color_index is None or color_index < 0 else color_index & 0xFF
    entry[at + 1] = (color >> 16) & 0xFF
    entry[at + 2] = (color >> 8) & 0xFF
    entry[at + 3] = color & 0xFF
    return bytes(entry)


def _pco2(cue_type: int, cues: list) -> bytes:
    entries = b"".join(_pcp2(cue) for cue in cues)
    body = struct.pack(">IHH", cue_type, len(cues), 0) + entries
    return b"PCO2" + struct.pack(">II", 20, 12 + len(body)) + body


def apply_cues(data: bytes, cues) -> bytes:
    """Replace empty analysis cue lists with the library's cues.

    A track with no cues keeps the lists already in the file.
    """
    if not cues or data[:4] != b"PMAI":
        return data
    memory = [cue for cue in cues if not _hot_slot(cue)]
    hot = [cue for cue in cues if _hot_slot(cue)]
    header_len = struct.unpack_from(">I", data, 4)[0]
    if header_len < 12 or header_len > len(data):
        return data
    out = bytearray(data[:header_len])
    pos = header_len
    while pos + 12 <= len(data):
        kind = data[pos : pos + 4]
        _len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < 12 or pos + len_tag > len(data):
            break
        if kind in (b"PCOB", b"PCO2") and len_tag >= 16:
            cue_type = struct.unpack_from(">I", data, pos + 12)[0]
            chosen = hot if cue_type == 1 else memory
            out += _pcob(1 if cue_type == 1 else 0, chosen) if kind == b"PCOB" else _pco2(1 if cue_type == 1 else 0, chosen)
        else:
            out += data[pos : pos + len_tag]
        pos += len_tag
    struct.pack_into(">I", out, 8, len(out))
    return bytes(out)


def ppth_path(data: bytes) -> str | None:
    if data[:4] != b"PMAI":
        return None
    header_len = struct.unpack_from(">I", data, 4)[0]
    pos = header_len
    while pos + 12 <= len(data):
        kind = data[pos : pos + 4]
        _len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < 12 or pos + len_tag > len(data):
            return None
        if kind == b"PPTH" and len_tag >= 18:
            length = struct.unpack_from(">I", data, pos + 12)[0]
            raw = data[pos + 16 : pos + 16 + max(0, length - 2)]
            return raw.decode("utf-16-be", "replace")
        pos += len_tag
    return None
