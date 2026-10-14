import struct


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
