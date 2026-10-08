import struct


def rewrite_ppth(data: bytes, usb_path: str) -> bytes:
    """Point every PPTH tag at the volume-relative audio path."""
    if data[:4] != b"PMAI":
        raise ValueError("Not an ANLZ file")
    header_len = struct.unpack_from(">I", data, 4)[0]
    if header_len < 12 or header_len > len(data):
        raise ValueError("ANLZ header is invalid")
    encoded = usb_path.encode("utf-16-be") + b"\x00\x00"
    out = bytearray(data[:header_len])
    pos = header_len
    while pos + 12 <= len(data):
        kind = data[pos : pos + 4]
        len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < 12 or pos + len_tag > len(data):
            break
        if kind == b"PPTH":
            len_tag_new = 16 + len(encoded)
            out += b"PPTH" + struct.pack(">III", 16, len_tag_new, len(encoded)) + encoded
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
