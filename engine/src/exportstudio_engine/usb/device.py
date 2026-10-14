"""A USB library as playlists and tracks, read from either device database."""

import sqlite3
from dataclasses import dataclass, field
from pathlib import Path

from . import onelib
from .pdb import NamedRow, PdbLibrary, read_pdb
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
