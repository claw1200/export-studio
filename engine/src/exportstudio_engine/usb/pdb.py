"""DeviceSQL export.pdb reader and writer.

Page layout follows the rekordbox device library: a file header, an index page
before every table, then heap rows with a backwards offset index.
"""

import struct
from dataclasses import dataclass, field
from datetime import date

PAGE = 4096
HEADER = 0x28
TABLES = 20
SENTINEL = 0x03FFFFFF
TRACK_STRINGS = 21

# Captured rekordbox browse rows. These are device-library constants, not a
# re-encoding, so a player still has its column, category, and sort tables.
_COLUMNS = (
    "0100800090120000FAFF470045004E0052004500FBFF0000",
    "0200810090140000FAFF410052005400490053005400FBFF",
    "0300820090120000FAFF41004C00420055004D00FBFF0000",
    "0400830090120000FAFF54005200410043004B00FBFF0000",
    "05008500900E0000FAFF420050004D00FBFF0000",
    "0600860090140000FAFF52004100540049004E004700FBFF",
    "0700870090100000FAFF5900450041005200FBFF",
    "0800880090160000FAFF520045004D004900580045005200FBFF0000",
    "0900890090120000FAFF4C004100420045004C00FBFF0000",
    "0A008A0090260000FAFF4F0052004900470049004E0041004C002000410052005400490053005400FBFF0000",
    "0B008B00900E0000FAFF4B0045005900FBFF0000",
    "0C008D00900E0000FAFF430055004500FBFF0000",
    "0D008E0090120000FAFF43004F004C004F005200FBFF0000",
    "0E00920090100000FAFF540049004D004500FBFF",
    "0F00930090160000FAFF4200490054005200410054004500FBFF0000",
    "10009400901A0000FAFF460049004C00450020004E0041004D004500FBFF0000",
    "1100840090180000FAFF50004C00410059004C00490053005400FBFF",
    "1200980090200000FAFF48004F00540020004300550045002000420041004E004B00FBFF",
    "1300950090160000FAFF48004900530054004F0052005900FBFF0000",
    "1400910090140000FAFF530045004100520043004800FBFF",
    "1500960090180000FAFF43004F004D004D0045004E0054005300FBFF",
    "16008C00901C0000FAFF4400410054004500200041004400440045004400FBFF",
    "1700970090220000FAFF44004A00200050004C0041005900200043004F0055004E005400FBFF0000",
    "1800900090140000FAFF46004F004C00440045005200FBFF",
    "1900A10090160000FAFF440045004600410055004C005400FBFF0000",
    "1A00A20090180000FAFF41004C00500048004100420045005400FBFF",
    "1B00AA0090180000FAFF4D00410054004300480049004E004700FBFF",
)
_CATEGORIES = (
    "0100010063010000",
    "0500060005010000",
    "0600070063010000",
    "0700080063010000",
    "0800090063010000",
    "09000A0063010000",
    "0A000B0063010000",
    "0D000F0063010000",
    "0E00130004010000",
    "0F00140006010000",
    "1000150063010000",
    "1200170063010000",
    "0200020002000100",
    "0300030003000200",
    "0400040001000300",
    "0B000C0063000400",
    "1100050063000500",
    "1300160063000600",
    "1400120063000700",
    "1B001A0063020800",
    "1800110063000900",
    "16001B0063000A00",
)
_SORTS = (
    "0100060001000000",
    "1500070003000000",
    "0E00080001000000",
    "0800090001000000",
    "09000A0001000000",
    "0A000B0001000000",
    "0F000D0001000000",
    "0D000F0001000000",
    "1700100001000000",
    "1600110001000000",
    "1900000000010000",
    "1A00010000020000",
    "0200020000030000",
    "0300030000040000",
    "0500040000050000",
    "0600050000060000",
    "0B000C0000070000",
)
COLORS = ("Pink", "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple")
FORMATS = {"MP3": 1, "M4A": 4, "AAC": 4, "FLAC": 5, "WAV": 11, "AIFF": 12, "AIF": 12}


@dataclass
class TrackRow:
    export_id: int
    title: str
    artist_id: int = 0
    album_id: int = 0
    genre_id: int = 0
    key_id: int = 0
    label_id: int = 0
    composer_id: int = 0
    remixer_id: int = 0
    original_artist_id: int = 0
    artwork_id: int = 0
    path: str = ""
    filename: str = ""
    analysis: str = ""
    comment: str = ""
    bpm_x100: int = 0
    duration: int = 0
    bitrate: int = 0
    sample_rate: int = 0
    bit_depth: int = 0
    file_size: int = 0
    file_type: int = 1
    rating: int = 0
    color_id: int = 0
    year: int = 0
    track_no: int = 0
    disc: int = 0
    play_count: int = 0
    hot_cue_auto: bool = True
    date_added: str = ""


@dataclass
class NamedRow:
    export_id: int
    name: str
    artist_id: int = 0


@dataclass
class PlaylistRow:
    export_id: int
    name: str
    parent_id: int = 0
    seq: int = 1
    folder: bool = False


@dataclass
class EntryRow:
    seq: int
    track_id: int
    playlist_id: int


@dataclass
class PdbLibrary:
    tracks: list[TrackRow] = field(default_factory=list)
    artists: list[NamedRow] = field(default_factory=list)
    albums: list[NamedRow] = field(default_factory=list)
    genres: list[NamedRow] = field(default_factory=list)
    labels: list[NamedRow] = field(default_factory=list)
    keys: list[NamedRow] = field(default_factory=list)
    playlists: list[PlaylistRow] = field(default_factory=list)
    entries: list[EntryRow] = field(default_factory=list)
    preserved: dict[int, list[bytes]] = field(default_factory=dict)
    device_name: str = ""
    background: int = 0


def _u16(buf: bytes, offset: int) -> int:
    return struct.unpack_from("<H", buf, offset)[0]


def _u32(buf: bytes, offset: int) -> int:
    return struct.unpack_from("<I", buf, offset)[0]


def encode_string(text: str) -> bytes:
    if text.isascii():
        raw = text.encode("ascii")
        marker = (len(raw) + 1) * 2 + 1
        if marker < 0x100:
            return bytes([marker]) + raw
        return bytes([0x40]) + struct.pack("<H", 4 + len(raw)) + b"\x00" + raw
    raw = text.encode("utf-16-le")
    return bytes([0x90]) + struct.pack("<H", 4 + len(raw)) + b"\x00" + raw


def decode_string(buf: bytes, offset: int) -> str:
    if offset < 0 or offset >= len(buf):
        return ""
    marker = buf[offset]
    if marker == 0x40 and offset + 4 <= len(buf):
        length = _u16(buf, offset + 1)
        return buf[offset + 4 : offset + length].decode("ascii", "replace")
    if marker == 0x90 and offset + 4 <= len(buf):
        length = _u16(buf, offset + 1)
        raw = buf[offset + 4 : offset + length]
        if raw.endswith(b"\x00\x00"):
            raw = raw[:-2]
        return raw.decode("utf-16-le", "replace")
    if marker < 0x100 and (marker - 1) % 2 == 0:
        count = (marker - 1) // 2 - 1
        if count < 0:
            return ""
        return buf[offset + 1 : offset + 1 + count].decode("ascii", "replace")
    return ""


def _id_name(export_id: int, name: str) -> bytes:
    return struct.pack("<I", export_id) + encode_string(name)


def _key_row(export_id: int, name: str) -> bytes:
    return struct.pack("<II", export_id, export_id) + encode_string(name)


def _aligned_name(encoded: bytes, start: int) -> int:
    if encoded[:1] in (b"\x40", b"\x90"):
        return (start + 3) & ~3
    return start


def _named(subtype: int, export_id: int, name: str, *, second_id: int | None = None) -> bytes:
    encoded = encode_string(name)
    long_string = encoded[:1] in (b"\x40", b"\x90")
    if second_id is None and not long_string and len(encoded) + 10 < 256:
        row = bytearray(10)
        struct.pack_into("<H", row, 0, subtype)
        struct.pack_into("<I", row, 4, export_id)
        row[8] = 0x03
        row[9] = 10
        return bytes(row) + encoded
    row = bytearray(12)
    struct.pack_into("<H", row, 0, 0x64 if subtype == 0x60 else subtype)
    struct.pack_into("<I", row, 4, export_id)
    if second_id is not None:
        struct.pack_into("<I", row, 8, second_id)
        row += b"\x03"
        start = len(row) + 2
        row += struct.pack("<H", start)
        return bytes(row) + encoded
    struct.pack_into("<H", row, 0x0A, 12)
    return bytes(row) + encoded


def _album(row: NamedRow) -> bytes:
    encoded = encode_string(row.name)
    start = _aligned_name(encoded, 22)
    body = bytearray(start)
    struct.pack_into("<H", body, 0, 0x80)
    struct.pack_into("<I", body, 8, row.artist_id)
    struct.pack_into("<I", body, 0x0C, row.export_id)
    body[0x14] = 0x03
    body[0x15] = start
    return bytes(body) + encoded


def _color(index: int, name: str) -> bytes:
    encoded = encode_string(name)
    body = bytearray(8)
    body[4] = index
    struct.pack_into("<H", body, 5, index)
    return bytes(body) + encoded


def _playlist(row: PlaylistRow) -> bytes:
    body = bytearray(20)
    struct.pack_into("<I", body, 0, row.parent_id)
    struct.pack_into("<I", body, 8, row.seq)
    struct.pack_into("<I", body, 0x0C, row.export_id)
    struct.pack_into("<I", body, 0x10, 1 if row.folder else 0)
    return bytes(body) + encode_string(row.name)


def _entry(row: EntryRow) -> bytes:
    return struct.pack("<III", row.seq, row.track_id, row.playlist_id)


def _track(row: TrackRow, shift: int) -> bytes:
    strings = [""] * TRACK_STRINGS
    strings[7] = "ON" if row.hot_cue_auto else ""
    strings[10] = row.date_added
    strings[14] = row.analysis
    strings[16] = row.comment
    strings[17] = row.title
    strings[19] = row.filename
    strings[20] = row.path
    encoded = [encode_string(text) for text in strings]
    body = bytearray(0x5E + TRACK_STRINGS * 2)
    struct.pack_into("<H", body, 0, 0x24)
    struct.pack_into("<H", body, 2, shift)
    struct.pack_into("<I", body, 0x04, 0x000C0700)
    struct.pack_into("<I", body, 0x08, row.sample_rate)
    struct.pack_into("<I", body, 0x0C, row.composer_id)
    struct.pack_into("<I", body, 0x10, min(row.file_size, 0xFFFFFFFF))
    struct.pack_into("<I", body, 0x14, 0x00100000 + (row.export_id & 0xFFFFFFFF))
    struct.pack_into("<H", body, 0x18, 0xAE49)
    struct.pack_into("<H", body, 0x1A, 0x03DD)
    struct.pack_into("<I", body, 0x1C, row.artwork_id)
    struct.pack_into("<I", body, 0x20, row.key_id)
    struct.pack_into("<I", body, 0x24, row.original_artist_id)
    struct.pack_into("<I", body, 0x28, row.label_id)
    struct.pack_into("<I", body, 0x2C, row.remixer_id)
    struct.pack_into("<I", body, 0x30, row.bitrate)
    struct.pack_into("<I", body, 0x34, row.track_no)
    struct.pack_into("<I", body, 0x38, row.bpm_x100)
    struct.pack_into("<I", body, 0x3C, row.genre_id)
    struct.pack_into("<I", body, 0x40, row.album_id)
    struct.pack_into("<I", body, 0x44, row.artist_id)
    struct.pack_into("<I", body, 0x48, row.export_id)
    struct.pack_into("<H", body, 0x4C, row.disc)
    struct.pack_into("<H", body, 0x4E, row.play_count)
    struct.pack_into("<H", body, 0x50, row.year)
    struct.pack_into("<H", body, 0x52, row.bit_depth)
    struct.pack_into("<H", body, 0x54, row.duration)
    struct.pack_into("<H", body, 0x56, 0x29)
    body[0x58] = row.color_id & 0xFF
    body[0x59] = row.rating & 0xFF
    struct.pack_into("<H", body, 0x5A, row.file_type)
    struct.pack_into("<H", body, 0x5C, 3)
    cursor = len(body)
    blob = bytearray(body)
    for index, piece in enumerate(encoded):
        cursor = _aligned_name(piece, cursor)
        if cursor > len(blob):
            blob += b"\x00" * (cursor - len(blob))
        struct.pack_into("<H", blob, 0x5E + index * 2, cursor)
        blob += piece
        cursor += len(piece)
    return bytes(blob)


def _property(count: int, day: date, device_name: str = "", background: int = 0) -> bytes:
    """Device Library property row. `count` is numberOfContents, shared with OneLibrary."""
    row = bytearray(12)
    row[0:2] = b"\x80\x02"
    struct.pack_into("<I", row, 4, count & 0xFFFFFFFF)
    row[9] = background & 0xFF
    row += encode_string(day.isoformat())
    row += b"\x19\x1e"
    row += encode_string("1000")
    row += encode_string(device_name)
    row += b"\x00" * 8
    if len(row) % 4:
        row += b"\x00" * (4 - len(row) % 4)
    return bytes(row)


def _chunks(rows: list[bytes]) -> list[list[bytes]]:
    if not rows:
        return []
    pages: list[list[bytes]] = []
    current: list[bytes] = []
    used = 0

    def budget(count: int) -> int:
        groups = (count + 15) // 16 if count else 0
        return PAGE - HEADER - groups * 0x24

    for row in rows:
        aligned = (len(row) + 3) & ~3
        if current and used + aligned > budget(len(current) + 1):
            pages.append(current)
            current = []
            used = 0
        current.append(row)
        used += aligned
    if current:
        pages.append(current)
    return pages


def _write_offsets(page: bytearray, offsets: list[int]) -> None:
    for index, offset in enumerate(offsets):
        group, slot = divmod(index, 16)
        base = PAGE - group * 0x24
        struct.pack_into("<H", page, base - (6 + 2 * slot), offset - HEADER)
        mask = _u16(page, base - 4) | (1 << slot)
        struct.pack_into("<H", page, base - 4, mask)
        struct.pack_into("<H", page, base - 2, mask)


def _finish_data(page: bytearray, index: int, table: int, nxt: int, count: int, heap_end: int) -> None:
    struct.pack_into("<I", page, 0x04, index)
    struct.pack_into("<I", page, 0x08, table)
    struct.pack_into("<I", page, 0x0C, nxt)
    struct.pack_into("<I", page, 0x10, 1)
    page[0x18] = count & 0xFF
    page[0x19] = (count * 32) & 0xFF
    page[0x1A] = (count // 8) & 0xFF
    page[0x1B] = 0x24
    used = (heap_end - HEADER + 3) & ~3
    groups = (count + 15) // 16 if count else 0
    free = PAGE - HEADER - used - (2 * count + 4 * groups)
    struct.pack_into("<H", page, 0x1C, max(0, free) & 0xFFFF)
    struct.pack_into("<H", page, 0x1E, used & 0xFFFF)
    struct.pack_into("<H", page, 0x20, count & 0xFFFF)
    if count > 0xFF:
        struct.pack_into("<H", page, 0x22, count)


def _finish_index(page: bytearray, index: int, table: int, first_data: int | None, empty_page: int) -> None:
    struct.pack_into("<I", page, 0x04, index)
    struct.pack_into("<I", page, 0x08, table)
    struct.pack_into("<I", page, 0x0C, first_data if first_data is not None else empty_page)
    struct.pack_into("<I", page, 0x10, 1)
    page[0x1B] = 0x64
    struct.pack_into("<H", page, 0x20, 0x1FFF)
    struct.pack_into("<H", page, 0x22, 0x1FFF)
    struct.pack_into("<H", page, 0x24, 0x03EC)
    struct.pack_into("<I", page, 0x28, index)
    struct.pack_into("<I", page, 0x2C, first_data if first_data is not None else SENTINEL)
    struct.pack_into("<I", page, 0x30, SENTINEL)
    pattern = bytes.fromhex("ff1ff8ff")
    pos = 0x34
    for _ in range(1004):
        if pos + 4 > PAGE:
            break
        page[pos : pos + 4] = pattern
        pos += 4
    if pos + 2 <= PAGE:
        page[pos : pos + 2] = bytes.fromhex("ff1f")


def _place(page: bytearray, rows: list[bytes]) -> int:
    cursor = HEADER
    offsets = []
    for row in rows:
        cursor = (cursor + 3) & ~3
        offsets.append(cursor)
        page[cursor : cursor + len(row)] = row
        cursor += len(row)
    _write_offsets(page, offsets)
    return cursor


def _row_starts(page: bytes) -> list[int]:
    if page[0x1B] == 0x64 or page[0x1B] & 0x40:
        return []
    count = _u16(page, 0x20)
    if count == 0x1FFF:
        return []
    starts = []
    for index in range(count):
        group, slot = divmod(index, 16)
        base = PAGE - group * 0x24
        if not (_u16(page, base - 4) & (1 << slot)):
            continue
        starts.append(HEADER + _u16(page, base - (6 + 2 * slot)))
    return starts


def _parse_track(page: bytes, start: int) -> TrackRow | None:
    if start + 0x5E + TRACK_STRINGS * 2 > len(page) or _u16(page, start) != 0x24:
        return None
    strings = []
    for index in range(TRACK_STRINGS):
        offset = _u16(page, start + 0x5E + index * 2)
        strings.append(decode_string(page, start + offset) if offset else "")
    return TrackRow(
        export_id=_u32(page, start + 0x48),
        title=strings[17],
        artist_id=_u32(page, start + 0x44),
        album_id=_u32(page, start + 0x40),
        genre_id=_u32(page, start + 0x3C),
        key_id=_u32(page, start + 0x20),
        label_id=_u32(page, start + 0x28),
        composer_id=_u32(page, start + 0x0C),
        remixer_id=_u32(page, start + 0x2C),
        original_artist_id=_u32(page, start + 0x24),
        artwork_id=_u32(page, start + 0x1C),
        path=strings[20],
        filename=strings[19],
        analysis=strings[14],
        comment=strings[16],
        bpm_x100=_u32(page, start + 0x38),
        duration=_u16(page, start + 0x54),
        bitrate=_u32(page, start + 0x30),
        sample_rate=_u32(page, start + 0x08),
        bit_depth=_u16(page, start + 0x52),
        file_size=_u32(page, start + 0x10),
        file_type=_u16(page, start + 0x5A),
        rating=page[start + 0x59],
        color_id=page[start + 0x58],
        year=_u16(page, start + 0x50),
        track_no=_u32(page, start + 0x34),
        disc=_u16(page, start + 0x4C),
        play_count=_u16(page, start + 0x4E),
        hot_cue_auto=strings[7] == "ON",
        date_added=strings[10],
    )


def _parse_id_name(page: bytes, start: int) -> NamedRow | None:
    if start + 5 > len(page):
        return None
    export_id = _u32(page, start)
    name = decode_string(page, start + 4)
    if not export_id:
        return None
    return NamedRow(export_id, name)


def _parse_key(page: bytes, start: int) -> NamedRow | None:
    if start + 9 > len(page):
        return None
    export_id = _u32(page, start)
    if not export_id:
        return None
    return NamedRow(export_id, decode_string(page, start + 8))


def _parse_named(page: bytes, start: int) -> NamedRow | None:
    if start + 10 > len(page):
        return None
    subtype = _u16(page, start)
    export_id = _u32(page, start + 4)
    if subtype == 0x64 and start + 12 <= len(page):
        offset = _u16(page, start + 0x0A)
    elif subtype in (0x60, 0x80) or page[start + 8] == 0x03:
        offset = page[start + 9] if subtype != 0x80 else page[start + 0x15]
        if subtype == 0x80 and start + 0x16 <= len(page):
            return NamedRow(export_id=_u32(page, start + 0x0C), name=decode_string(page, start + offset), artist_id=_u32(page, start + 8))
    else:
        return None
    return NamedRow(export_id, decode_string(page, start + offset))


def _parse_playlist(page: bytes, start: int) -> PlaylistRow | None:
    if start + 21 > len(page):
        return None
    return PlaylistRow(
        export_id=_u32(page, start + 0x0C),
        name=decode_string(page, start + 20),
        parent_id=_u32(page, start),
        seq=_u32(page, start + 8),
        folder=_u32(page, start + 0x10) == 1,
    )


def _parse_entry(page: bytes, start: int) -> EntryRow | None:
    if start + 12 > len(page):
        return None
    seq, track, playlist = struct.unpack_from("<III", page, start)
    return EntryRow(seq, track, playlist)


def build_pdb(library: PdbLibrary, *, today: date | None = None) -> bytes:
    today = today or date.today()
    table_rows: list[list[bytes]] = [[] for _ in range(TABLES)]
    for index, row in enumerate(library.tracks):
        table_rows[0].append(_track(row, (index % 16) * 32))
    for row in library.genres:
        table_rows[1].append(_id_name(row.export_id, row.name))
    for row in library.artists:
        table_rows[2].append(_named(0x60, row.export_id, row.name))
    for row in library.albums:
        table_rows[3].append(_album(row))
    for row in library.labels:
        table_rows[4].append(_id_name(row.export_id, row.name))
    for row in library.keys:
        table_rows[5].append(_key_row(row.export_id, row.name))
    for index, name in enumerate(COLORS, 1):
        table_rows[6].append(_color(index, name))
    for row in library.playlists:
        table_rows[7].append(_playlist(row))
    for row in library.entries:
        table_rows[8].append(_entry(row))
    table_rows[16] = [bytes.fromhex(item) for item in _COLUMNS]
    table_rows[17] = [bytes.fromhex(item) for item in _CATEGORIES]
    table_rows[18] = [bytes.fromhex(item) for item in _SORTS]
    table_rows[19] = [_property(len(library.tracks), today, library.device_name, library.background)]
    for table in (11, 12, 13):
        if library.preserved.get(table):
            table_rows[table] = list(library.preserved[table])

    plans = []
    next_page = 1
    for table, rows in enumerate(table_rows):
        chunks = _chunks(rows)
        index_at = next_page
        next_page += 1
        data_at = list(range(next_page, next_page + len(chunks)))
        next_page += len(chunks)
        plans.append((table, index_at, data_at, chunks))
    candidate_base = next_page
    pages: dict[int, bytearray] = {}
    meta = []
    for table, index_at, data_at, chunks in plans:
        pages[index_at] = bytearray(PAGE)
        _finish_index(
            pages[index_at],
            index_at,
            table,
            data_at[0] if data_at else None,
            candidate_base + table,
        )
        for chunk_index, chunk in enumerate(chunks):
            number = data_at[chunk_index]
            page = bytearray(PAGE)
            heap_end = _place(page, chunk)
            nxt = data_at[chunk_index + 1] if chunk_index + 1 < len(data_at) else candidate_base + table
            _finish_data(page, number, table, nxt, len(chunk), heap_end)
            pages[number] = page
        last = data_at[-1] if data_at else index_at
        meta.append((table, candidate_base + table, index_at, last))

    header = bytearray(PAGE)
    struct.pack_into("<I", header, 0x04, PAGE)
    struct.pack_into("<I", header, 0x08, TABLES)
    struct.pack_into("<I", header, 0x0C, candidate_base + TABLES)
    struct.pack_into("<I", header, 0x10, 1)
    struct.pack_into("<I", header, 0x14, 1)
    for table, _candidate, first, last in meta:
        struct.pack_into("<4I", header, 0x1C + table * 16, table, candidate_base + table, first, last)
    parts = [header]
    for number in range(1, next_page):
        parts.append(pages[number])
    return b"".join(parts)


def _page(data: bytes, number: int) -> bytes | None:
    start = number * PAGE
    if number < 0 or start + PAGE > len(data):
        return None
    return data[start : start + PAGE]


def _walk(data: bytes, first: int, last: int, parse) -> list:
    page = _page(data, first)
    if page is None:
        return []
    if page[0x1B] == 0x64:
        data_at = _u32(page, 0x2C)
        if data_at in (0, SENTINEL):
            return []
    else:
        data_at = first
    rows = []
    seen: set[int] = set()
    while data_at and data_at not in seen:
        seen.add(data_at)
        current = _page(data, data_at)
        if current is None or current[0x1B] == 0x64:
            break
        for start in _row_starts(current):
            row = parse(current, start)
            if row is not None:
                rows.append(row)
        if data_at == last:
            break
        nxt = _u32(current, 0x0C)
        if nxt in (0, data_at, SENTINEL):
            break
        data_at = nxt
    return rows


def _raw_rows(page: bytes) -> list[bytes]:
    starts = _row_starts(page)
    if not starts:
        return []
    used = HEADER + _u16(page, 0x1E)
    rows = []
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else min(len(page), max(used, start))
        rows.append(bytes(page[start:end]))
    return rows


def _preserved(data: bytes, first: int, last: int) -> list[bytes]:
    page = _page(data, first)
    if page is None:
        return []
    if page[0x1B] == 0x64:
        data_at = _u32(page, 0x2C)
        if data_at in (0, SENTINEL):
            return []
    else:
        data_at = first
    rows = []
    seen: set[int] = set()
    while data_at and data_at not in seen:
        seen.add(data_at)
        current = _page(data, data_at)
        if current is None or current[0x1B] == 0x64:
            break
        rows.extend(_raw_rows(current))
        if data_at == last:
            break
        nxt = _u32(current, 0x0C)
        if nxt in (0, data_at, SENTINEL):
            break
        data_at = nxt
    return rows


def read_pdb(data: bytes) -> PdbLibrary:
    if len(data) < PAGE or _u32(data, 0x04) != PAGE:
        raise ValueError("Not a DeviceSQL export")
    count = _u32(data, 0x08)
    tables = []
    for index in range(count):
        table, _candidate, first, last = struct.unpack_from("<4I", data, 0x1C + index * 16)
        tables.append((table, first, last))
    found = {table: (first, last) for table, first, last in tables}

    def take(table: int, parse) -> list:
        if table not in found:
            return []
        first, last = found[table]
        return _walk(data, first, last, parse)

    preserved = {}
    for table in (11, 12, 13, 19):
        if table in found:
            rows = _preserved(data, *found[table])
            if rows:
                preserved[table] = rows
    return PdbLibrary(
        tracks=[row for row in take(0, _parse_track) if row.export_id],
        genres=take(1, _parse_id_name),
        artists=take(2, _parse_named),
        albums=take(3, _parse_named),
        labels=take(4, _parse_id_name),
        keys=take(5, _parse_key),
        playlists=[row for row in take(7, _parse_playlist) if row.export_id],
        entries=[row for row in take(8, _parse_entry) if row.track_id and row.playlist_id],
        preserved=preserved,
    )


def table_page(data: bytes, table: int) -> bytes:
    _type, _candidate, first, _last = struct.unpack_from("<4I", data, 0x1C + table * 16)
    page = _page(data, first)
    if page is None:
        raise ValueError(f"Table {table} is missing")
    return page
