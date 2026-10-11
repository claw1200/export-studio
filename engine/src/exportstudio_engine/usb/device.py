"""A USB library as playlists and tracks, read from either device database."""

import sqlite3
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path

from . import onelib
from .pdb import (
    EntryRow,
    NamedRow,
    PdbLibrary,
    PlaylistRow,
    TrackRow,
    build_pdb,
    read_pdb,
)
from .paths import pioneer_dir


@dataclass
class CueRow:
    kind: int
    start_ms: int
    end_ms: int | None = None
    color: int | None = None
    color_index: int | None = None
    comment: str = ""
    active_loop: bool = False
    beat_loop: int = 0


@dataclass
class DeviceTrack:
    export_id: int
    master_id: str | None
    title: str
    artist: str = ""
    album: str = ""
    genre: str = ""
    label: str = ""
    key: str = ""
    path: str = ""
    filename: str = ""
    analysis: str = ""
    comment: str = ""
    bpm_x100: int = 0
    duration: int = 0
    bitrate: int = 0
    sample_rate: int = 0
    bit_depth: int = 0
    file_type: int = 1
    file_size: int = 0
    rating: int = 0
    color: int = 0
    year: int = 0
    track_no: int = 0
    play_count: int = 0
    hot_cue_auto: bool = True
    date_added: str = ""
    cues: list[CueRow] = field(default_factory=list)
    owned: bool = False


@dataclass
class DevicePlaylist:
    export_id: int
    master_id: str | None
    name: str
    parent_id: int = 0
    seq: int = 1
    folder: bool = False
    track_ids: list[int] = field(default_factory=list)
    owned: bool = False


@dataclass
class DeviceHistory:
    export_id: int
    name: str
    parent_id: int = 0
    seq: int = 0
    attribute: int = 0
    track_ids: list[int] = field(default_factory=list)


@dataclass
class DeviceLibrary:
    root_name: str
    tracks: list[DeviceTrack] = field(default_factory=list)
    playlists: list[DevicePlaylist] = field(default_factory=list)
    histories: list[DeviceHistory] = field(default_factory=list)
    device_name: str = ""
    background: int = 0
    tag_dbid: int = 1
    categories: list[tuple] | None = None
    sorts: list[tuple] | None = None
    colors: list[tuple[int, str]] | None = None
    pdb_rows: dict[int, list[bytes]] = field(default_factory=dict)

    def track(self, export_id: int) -> DeviceTrack | None:
        return next((row for row in self.tracks if row.export_id == export_id), None)


def _name_map(rows: list[NamedRow]) -> dict[int, str]:
    return {row.export_id: row.name for row in rows}


def from_pdb(library: PdbLibrary, root_name: str) -> DeviceLibrary:
    artists = _name_map(library.artists)
    albums = {row.export_id: row.name for row in library.albums}
    genres = _name_map(library.genres)
    labels = _name_map(library.labels)
    keys = _name_map(library.keys)
    membership: dict[int, list[tuple[int, int]]] = {}
    for entry in sorted(library.entries, key=lambda row: (row.playlist_id, row.seq)):
        membership.setdefault(entry.playlist_id, []).append((entry.seq, entry.track_id))
    playlists = [
        DevicePlaylist(
            export_id=row.export_id,
            master_id=None,
            name=row.name,
            parent_id=row.parent_id,
            seq=row.seq,
            folder=row.folder,
            track_ids=[track for _seq, track in membership.get(row.export_id, [])],
        )
        for row in library.playlists
    ]
    tracks = [
        DeviceTrack(
            export_id=row.export_id,
            master_id=None,
            title=row.title,
            artist=artists.get(row.artist_id, ""),
            album=albums.get(row.album_id, ""),
            genre=genres.get(row.genre_id, ""),
            label=labels.get(row.label_id, ""),
            key=keys.get(row.key_id, ""),
            path=row.path,
            filename=row.filename,
            analysis=row.analysis,
            comment=row.comment,
            bpm_x100=row.bpm_x100,
            duration=row.duration,
            bitrate=row.bitrate,
            sample_rate=row.sample_rate,
            bit_depth=row.bit_depth,
            file_type=row.file_type,
            file_size=row.file_size,
            rating=row.rating,
            color=row.color_id,
            year=row.year,
            track_no=row.track_no,
            play_count=row.play_count,
            hot_cue_auto=row.hot_cue_auto,
            date_added=row.date_added,
        )
        for row in library.tracks
    ]
    return DeviceLibrary(root_name, tracks, playlists, pdb_rows=dict(library.preserved))


def _intern(names: dict[str, int], name: str) -> int:
    key = name or ""
    if not key:
        return 0
    if key not in names:
        names[key] = len(names) + 1
    return names[key]


def to_pdb(library: DeviceLibrary, *, today: date | None = None) -> bytes:
    artists: dict[str, int] = {}
    albums: dict[tuple[str, int], int] = {}
    genres: dict[str, int] = {}
    labels: dict[str, int] = {}
    keys: dict[str, int] = {}
    tracks = []
    for row in library.tracks:
        artist_id = _intern(artists, row.artist)
        album_key = (row.album, artist_id)
        album_id = 0
        if row.album:
            if album_key not in albums:
                albums[album_key] = len(albums) + 1
            album_id = albums[album_key]
        tracks.append(
            TrackRow(
                export_id=row.export_id,
                title=row.title,
                artist_id=artist_id,
                album_id=album_id,
                genre_id=_intern(genres, row.genre),
                label_id=_intern(labels, row.label),
                key_id=_intern(keys, row.key),
                path=row.path,
                filename=row.filename or Path(row.path).name,
                analysis=row.analysis,
                comment=row.comment,
                bpm_x100=row.bpm_x100,
                duration=row.duration,
                bitrate=row.bitrate,
                sample_rate=row.sample_rate,
                bit_depth=row.bit_depth,
                file_size=row.file_size,
                file_type=row.file_type,
                rating=row.rating,
                color_id=row.color,
                year=row.year,
                track_no=row.track_no,
                play_count=row.play_count,
                hot_cue_auto=row.hot_cue_auto,
                date_added=row.date_added,
            )
        )
    entries = []
    for playlist in library.playlists:
        for seq, track_id in enumerate(playlist.track_ids, 1):
            entries.append(EntryRow(seq, track_id, playlist.export_id))
    return build_pdb(
        PdbLibrary(
            tracks=tracks,
            artists=[NamedRow(i, name) for name, i in artists.items()],
            albums=[NamedRow(i, name, artist_id) for (name, artist_id), i in albums.items()],
            genres=[NamedRow(i, name) for name, i in genres.items()],
            labels=[NamedRow(i, name) for name, i in labels.items()],
            keys=[NamedRow(i, name) for name, i in keys.items()],
            playlists=[
                PlaylistRow(row.export_id, row.name, row.parent_id, row.seq, row.folder) for row in library.playlists
            ],
            entries=entries,
            preserved=library.pdb_rows,
            device_name=library.device_name or "USB",
            background=library.background,
        ),
        today=today,
    )


def from_onelibrary(conn: sqlite3.Connection, root_name: str) -> DeviceLibrary:
    artists = {row["artist_id"]: row["name"] or "" for row in onelib.rows(conn, "SELECT artist_id, name FROM artist")}
    albums = {row["album_id"]: row["name"] or "" for row in onelib.rows(conn, "SELECT album_id, name FROM album")}
    genres = {row["genre_id"]: row["name"] or "" for row in onelib.rows(conn, "SELECT genre_id, name FROM genre")}
    labels = {row["label_id"]: row["name"] or "" for row in onelib.rows(conn, "SELECT label_id, name FROM label")}
    keys = {row["key_id"]: row["name"] or "" for row in onelib.rows(conn, 'SELECT key_id, name FROM "key"')}
    cues: dict[int, list[CueRow]] = {}
    for row in onelib.rows(conn, "SELECT * FROM cue ORDER BY cue_id"):
        end = row["outUsec"]
        cues.setdefault(row["content_id"], []).append(
            CueRow(
                kind=row["kind"] or 0,
                start_ms=(row["inUsec"] or 0) // 1000,
                end_ms=None if end is None or end < 0 else end // 1000,
                color_index=row["colorTableIndex"],
                comment=row["cueComment"] or "",
                active_loop=bool(row["isActiveLoop"]),
                beat_loop=((row["beatLoopNumerator"] or 0) << 16) | (row["beatLoopDenominator"] or 0),
            )
        )
    tracks = []
    for row in onelib.rows(conn, "SELECT * FROM content ORDER BY content_id"):
        master = row["masterContentId"]
        tracks.append(
            DeviceTrack(
                export_id=row["content_id"],
                master_id=str(master) if master else None,
                title=row["title"] or "",
                artist=artists.get(row["artist_id_artist"], ""),
                album=albums.get(row["album_id"], ""),
                genre=genres.get(row["genre_id"], ""),
                label=labels.get(row["label_id"], ""),
                key=keys.get(row["key_id"], ""),
                path=row["path"] or "",
                filename=row["fileName"] or "",
                analysis=row["analysisDataFilePath"] or "",
                comment=row["djComment"] or "",
                bpm_x100=row["bpmx100"] or 0,
                duration=row["length"] or 0,
                bitrate=row["bitrate"] or 0,
                sample_rate=row["samplingRate"] or 0,
                bit_depth=row["bitDepth"] or 0,
                file_type=row["fileType"] or 1,
                file_size=row["fileSize"] or 0,
                rating=row["rating"] or 0,
                color=row["color_id"] or 0,
                year=row["releaseYear"] or 0,
                track_no=row["trackNo"] or 0,
                play_count=row["djPlayCount"] or 0,
                hot_cue_auto=bool(row["isHotCueAutoLoadOn"]),
                date_added=row["dateAdded"] or "",
                cues=cues.get(row["content_id"], []),
            )
        )
    members: dict[int, list[int]] = {}
    for row in onelib.rows(conn, "SELECT playlist_id, content_id FROM playlist_content ORDER BY playlist_id, sequenceNo"):
        members.setdefault(row["playlist_id"], []).append(row["content_id"])
    playlists = [
        DevicePlaylist(
            export_id=row["playlist_id"],
            master_id=None,
            name=row["name"] or "",
            parent_id=row["playlist_id_parent"] or 0,
            seq=row["sequenceNo"] or 0,
            folder=(row["attribute"] or 0) == 1,
            track_ids=members.get(row["playlist_id"], []),
        )
        for row in onelib.rows(conn, "SELECT * FROM playlist")
    ]
    history_members: dict[int, list[int]] = {}
    for row in onelib.rows(conn, "SELECT history_id, content_id FROM history_content ORDER BY history_id, sequenceNo"):
        history_members.setdefault(row["history_id"], []).append(row["content_id"])
    histories = [
        DeviceHistory(
            export_id=row["history_id"],
            name=row["name"] or "",
            parent_id=row["history_id_parent"] or 0,
            seq=row["sequenceNo"] or 0,
            attribute=row["attribute"] or 0,
            track_ids=history_members.get(row["history_id"], []),
        )
        for row in onelib.rows(conn, "SELECT * FROM history")
    ]
    prop = onelib.rows(conn, "SELECT * FROM property LIMIT 1")
    categories = [tuple(row.values()) for row in onelib.rows(conn, "SELECT category_id, menuItem_id, sequenceNo, isVisible FROM category")]
    sorts = [
        tuple(row.values())
        for row in onelib.rows(conn, "SELECT sort_id, menuItem_id, sequenceNo, isVisible, isSelectedAsSubColumn FROM sort")
    ]
    colors = [(row["color_id"], row["name"] or "") for row in onelib.rows(conn, "SELECT color_id, name FROM color")]
    device = DeviceLibrary(root_name, tracks, playlists, histories)
    if prop:
        device.device_name = prop[0]["deviceName"] or ""
        device.background = prop[0]["backGroundColorType"] or 0
        device.tag_dbid = prop[0]["myTagMasterDBID"] or 1
    if categories:
        device.categories = categories
    if sorts:
        device.sorts = sorts
    if colors:
        device.colors = colors
    return device


def write_onelibrary(path: Path, library: DeviceLibrary, *, encrypt: bool, today: date | None = None, dbid: int = 0) -> None:
    today = today or date.today()
    if path.exists():
        path.unlink()
    for suffix in ("-wal", "-shm"):
        extra = path.with_name(path.name + suffix)
        if extra.exists():
            extra.unlink()
    conn = onelib.connect(path, encrypt=encrypt)
    try:
        onelib.create_empty(conn)
        if library.categories:
            conn.execute("DELETE FROM category")
            conn.executemany(
                "INSERT INTO category(category_id, menuItem_id, sequenceNo, isVisible) VALUES (?, ?, ?, ?)",
                library.categories,
            )
        if library.sorts:
            conn.execute("DELETE FROM sort")
            conn.executemany(
                "INSERT INTO sort(sort_id, menuItem_id, sequenceNo, isVisible, isSelectedAsSubColumn) VALUES (?, ?, ?, ?, ?)",
                library.sorts,
            )
        if library.colors:
            conn.execute("DELETE FROM color")
            conn.executemany("INSERT INTO color(color_id, name) VALUES (?, ?)", library.colors)
        artists: dict[str, int] = {}
        albums: dict[tuple[str, int], int] = {}
        genres: dict[str, int] = {}
        labels: dict[str, int] = {}
        keys: dict[str, int] = {}
        for row in library.tracks:
            artist_id = _intern(artists, row.artist)
            album_id = 0
            if row.album:
                key = (row.album, artist_id)
                if key not in albums:
                    albums[key] = len(albums) + 1
                album_id = albums[key]
            genre_id = _intern(genres, row.genre)
            label_id = _intern(labels, row.label)
            key_id = _intern(keys, row.key)
            master = int(row.master_id) if row.master_id and row.master_id.isdigit() else 0
            conn.execute(
                """INSERT INTO content(
                    content_id, title, titleForSearch, bpmx100, length, trackNo, artist_id_artist, album_id,
                    genre_id, label_id, key_id, color_id, djComment, rating, releaseYear, dateAdded, path,
                    fileName, fileSize, fileType, bitrate, bitDepth, samplingRate, djPlayCount,
                    isHotCueAutoLoadOn, masterDbId, masterContentId, analysisDataFilePath, analysedBits,
                    cueUpdateCount
                ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
                (
                    row.export_id,
                    row.title,
                    row.title.casefold(),
                    row.bpm_x100,
                    row.duration,
                    row.track_no,
                    artist_id or None,
                    album_id or None,
                    genre_id or None,
                    label_id or None,
                    key_id or None,
                    row.color or None,
                    row.comment,
                    row.rating,
                    row.year,
                    row.date_added,
                    row.path,
                    row.filename,
                    row.file_size,
                    row.file_type,
                    row.bitrate,
                    row.bit_depth,
                    row.sample_rate,
                    row.play_count,
                    1 if row.hot_cue_auto else 0,
                    dbid,
                    master,
                    row.analysis,
                    105 if row.analysis else 0,
                    len(row.cues),
                ),
            )
            for index, cue in enumerate(row.cues, 1):
                end = -1 if cue.end_ms is None else cue.end_ms * 1000
                conn.execute(
                    """INSERT INTO cue(
                        cue_id, content_id, kind, colorTableIndex, cueComment, isActiveLoop,
                        beatLoopNumerator, beatLoopDenominator, inUsec, outUsec, in150FramePerSec, out150FramePerSec
                    ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)""",
                    (
                        row.export_id * 1000 + index,
                        row.export_id,
                        cue.kind,
                        cue.color_index if cue.color_index is not None else -1,
                        cue.comment,
                        1 if cue.active_loop else 0,
                        (cue.beat_loop >> 16) & 0xFFFF,
                        cue.beat_loop & 0xFFFF,
                        cue.start_ms * 1000,
                        end,
                        cue.start_ms * 150 // 1000,
                        -1 if cue.end_ms is None else cue.end_ms * 150 // 1000,
                    ),
                )
        conn.executemany("INSERT INTO artist(artist_id, name, nameForSearch) VALUES (?, ?, ?)", [(i, n, n.casefold()) for n, i in artists.items()])
        conn.executemany(
            "INSERT INTO album(album_id, name, artist_id, nameForSearch) VALUES (?, ?, ?, ?)",
            [(i, name, artist, name.casefold()) for (name, artist), i in albums.items()],
        )
        conn.executemany("INSERT INTO genre(genre_id, name) VALUES (?, ?)", [(i, n) for n, i in genres.items()])
        conn.executemany("INSERT INTO label(label_id, name) VALUES (?, ?)", [(i, n) for n, i in labels.items()])
        conn.executemany('INSERT INTO "key"(key_id, name) VALUES (?, ?)', [(i, n) for n, i in keys.items()])
        for row in library.playlists:
            conn.execute(
                "INSERT INTO playlist(playlist_id, sequenceNo, name, attribute, playlist_id_parent) VALUES (?, ?, ?, ?, ?)",
                (row.export_id, row.seq, row.name, 1 if row.folder else 0, row.parent_id or None),
            )
            conn.executemany(
                "INSERT INTO playlist_content(playlist_id, content_id, sequenceNo) VALUES (?, ?, ?)",
                [(row.export_id, track_id, seq) for seq, track_id in enumerate(row.track_ids, 1)],
            )
        for row in library.histories:
            conn.execute(
                "INSERT INTO history(history_id, sequenceNo, name, attribute, history_id_parent) VALUES (?, ?, ?, ?, ?)",
                (row.export_id, row.seq, row.name, row.attribute, row.parent_id or None),
            )
            conn.executemany(
                "INSERT INTO history_content(history_id, content_id, sequenceNo) VALUES (?, ?, ?)",
                [(row.export_id, track_id, seq) for seq, track_id in enumerate(row.track_ids, 1)],
            )
        conn.execute(
            "INSERT INTO property(deviceName, dbVersion, numberOfContents, createdDate, backGroundColorType, myTagMasterDBID) VALUES (?, ?, ?, ?, ?, ?)",
            (
                library.device_name or "USB",
                "1000",
                len(library.tracks),
                today.isoformat(),
                library.background,
                library.tag_dbid,
            ),
        )
        conn.commit()
    finally:
        conn.close()


def read_device(volume: Path) -> DeviceLibrary | None:
    root, root_name = pioneer_dir(volume)
    plus = root / "rekordbox" / "exportLibrary.db"
    pdb_path = root / "rekordbox" / "export.pdb"
    parsed = read_pdb(pdb_path.read_bytes()) if pdb_path.is_file() else None
    if plus.is_file():
        conn = onelib.connect(plus)
        try:
            library = from_onelibrary(conn, root_name)
        finally:
            conn.close()
        if parsed is not None:
            library.pdb_rows = dict(parsed.preserved)
        return library
    if parsed is not None:
        return from_pdb(parsed, root_name)
    return None
