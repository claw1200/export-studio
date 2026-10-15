import base64
import logging
import os
import re
import shutil
import tempfile
import time
from collections import defaultdict
from datetime import datetime
from pathlib import Path
from uuid import uuid4

from pyrekordbox import Rekordbox6Database
from pyrekordbox.config import get_config
from pyrekordbox.db6 import tables as t
from pyrekordbox.utils import get_rekordbox_pid
from sqlalchemy import event, select
from sqlalchemy.orm import aliased

from . import anlz_build, artwork
from . import convert as converter
from .analysed import ANALYSED
from . import tags as file_tags
from . import waveform
from .inspect import inspect_file, same_fingerprint

log = logging.getLogger(__name__)

DB_SUFFIXES = ("", "-wal", "-shm")
FILE_TYPES = {1: "MP3", 4: "M4A", 5: "FLAC", 11: "WAV", 12: "AIFF"}


def stored_file_type(value) -> str | None:
    """Rekordbox file-type code as MP3, M4A, FLAC, WAV, or AIFF. None when the code is unknown."""
    try:
        return FILE_TYPES.get(int(value))
    except (TypeError, ValueError):
        return None


def file_type_code(path: Path) -> int | None:
    """Rekordbox file-type code for a path. AIF and AIFF are the same code."""
    suffix = path.suffix.lstrip(".").upper()
    if suffix == "AIF":
        suffix = "AIFF"
    for code, name in FILE_TYPES.items():
        if name == suffix:
            return code
    return None


# Stream columns that describe the audio file itself. A converted MP3 keeps these after the
# path is pointed back at a WAV, which is what a CDJ trusts.
STREAM_COLUMNS = ("FileType", "BitRate", "SampleRate", "BitDepth")


def stream_fields(path: Path, info: dict | None = None) -> dict:
    """File type and stream numbers read from the audio file, for a revived collection row."""
    info = info or {}
    fields: dict = {"FileNameL": path.name}
    code = file_type_code(path)
    if code is not None:
        fields["FileType"] = code
    for column, key in (
        ("BitRate", "bitrate"),
        ("SampleRate", "sampleRate"),
        ("BitDepth", "bitDepth"),
        ("Length", "length"),
    ):
        value = info.get(key) or 0
        if value:
            fields[column] = int(value)
    try:
        fields["FileSize"] = path.stat().st_size
    except OSError:
        pass
    return fields


# Track columns a converted copy inherits; file, analysis and sync columns are rebuilt.
COPIED_COLUMNS = (
    "Title", "ArtistID", "AlbumID", "GenreID", "BPM", "TrackNo", "Commnt", "Rating", "ReleaseYear",
    "RemixerID", "LabelID", "OrgArtistID", "KeyID", "ColorID", "ImagePath", "DiscNo", "ComposerID",
    "Subtitle", "ReleaseDate", "DateCreated", "Lyricist", "ISRC", "DisableQuantize", "HotCueAutoLoad",
)
CLOUD_LIBRARY_SYNC = "100000"
HIDDEN_PLAYLISTS = {CLOUD_LIBRARY_SYNC}
WRITE_BLOCKED = "Close rekordbox before editing the library"
HOT_CUE_KINDS = tuple(sorted(waveform.HOT_SLOTS))
HOT_CUE_PALETTE_SIZE = 62
CDJ_FRAMES_PER_SECOND = 150
FOLDER = 1
MIN_GRID_BPM = 40
MAX_GRID_BPM = 300
BEATS_PER_BAR = 4
ANLZ_BACKUP_DIR = Path.home() / ".exportstudio" / "anlz-backup"
NORMAL_PLAYLIST = 0
STREAMING_URI = re.compile(r"^[a-z][a-z0-9+.-]+:", re.IGNORECASE)
IMPORT_SUFFIXES = {".mp3", ".m4a", ".flac", ".wav", ".aif", ".aiff"}
PHRASE_TAG = waveform.PHRASE_TAG
CAMELOT = re.compile(r"(1[0-2]|[1-9])[AB]")
TONICS = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"]
ENHARMONIC = {"Db": "C#", "Eb": "D#", "F#": "Gb", "Ab": "G#", "Bb": "A#"}

TEXT_COLUMNS = {"title": "Title", "comment": "Commnt"}
NUMBER_COLUMNS = {
    "year": ("ReleaseYear", 9999, "Year"),
    "trackNo": ("TrackNo", 9999, "Track number"),
    "rating": ("Rating", 5, "Rating"),
    "color": ("ColorID", 8, "Colour"),
}
NAMED_COLUMNS = {
    "artist": (t.DjmdArtist, "ArtistID", "add_artist"),
    "remixer": (t.DjmdArtist, "RemixerID", "add_artist"),
    "originalArtist": (t.DjmdArtist, "OrgArtistID", "add_artist"),
    "composer": (t.DjmdArtist, "ComposerID", "add_artist"),
    "album": (t.DjmdAlbum, "AlbumID", "add_album"),
    "genre": (t.DjmdGenre, "GenreID", "add_genre"),
    "label": (t.DjmdLabel, "LabelID", "add_label"),
}
EDITABLE_FIELDS = set(TEXT_COLUMNS) | set(NUMBER_COLUMNS) | set(NAMED_COLUMNS)


def _read_anlz(path: Path) -> waveform.Tags | None:
    if not path.is_file():
        return None
    try:
        return waveform.read_tags(path)
    except Exception as e:
        log.warning("Failed to read %s: %s", path, e)
        return None


def _stored_cues(rows) -> dict[str, list[dict]]:
    grouped: dict[str, list[dict]] = defaultdict(list)
    for content_id, cue_id, kind, start, end, color, color_index, comment, deleted in rows:
        if deleted:
            continue
        grouped[str(content_id)].append(
            waveform.browser_cue(cue_id, kind, start, end, color, color_index, comment)
        )
    return grouped


def _rekordbox_db_path() -> Path:
    for version in ("rekordbox7", "rekordbox6"):
        cfg = get_config(version)
        if cfg and cfg.get("db_path"):
            return Path(cfg["db_path"])
    raise FileNotFoundError("No rekordbox 6/7 library found on this computer")


def _source_signature(src: Path) -> tuple:
    sig = []
    for suf in DB_SUFFIXES:
        p = src.with_name(src.name + suf)
        if suf == "-shm":
            continue
        try:
            st = p.stat()
            sig.append((suf, st.st_mtime_ns, st.st_size))
        except FileNotFoundError:
            sig.append((suf, None, None))
    return tuple(sig)


def _dispose(db: Rekordbox6Database) -> None:
    try:
        db.close()
        db.engine.dispose()
    except Exception:
        pass


def _node(item_id, name, parent, seq, kind, **fields):
    node = {
        "id": item_id,
        "name": name or "",
        "parentId": parent or "root",
        "seq": seq or 0,
        "kind": kind,
    }
    node.update(fields)
    return node


def _leaf_or_folder(attr, leaf: str) -> str:
    return "folder" if attr == FOLDER else leaf


def _commit(db: Rekordbox6Database, change) -> None:
    try:
        change()
        db.commit()
    except Exception:
        db.rollback()
        raise


def _clamp_msec(content: t.DjmdContent, msec: int) -> int:
    msec = max(0, int(round(msec)))
    return min(msec, content.Length * 1000) if content.Length else msec


def _new_cue(db: Rekordbox6Database, content: t.DjmdContent, kind: int, msec: int) -> t.DjmdCue:
    return t.DjmdCue.create(
        ID=str(db.generate_unused_id(t.DjmdCue, is_28_bit=False)),
        UUID=str(uuid4()),
        ContentID=content.ID,
        ContentUUID=content.UUID,
        Kind=kind,
        InMsec=msec,
        InFrame=msec * CDJ_FRAMES_PER_SECOND // 1000,
        InMpegFrame=0,
        InMpegAbs=0,
        OutMsec=-1,
        OutFrame=0,
        OutMpegFrame=0,
        OutMpegAbs=0,
        Color=-1,
    )


def _check_grid(times: list[int], beats: list[int], bpms: list[float]) -> None:
    if not times or not len(times) == len(beats) == len(bpms):
        raise ValueError("A beatgrid needs matching times, beats and tempos")
    if times[0] < 0 or any(b <= a for a, b in zip(times, times[1:])):
        raise ValueError("Beatgrid times must start at 0 or later and keep increasing")
    if any(not 1 <= b <= BEATS_PER_BAR for b in beats):
        raise ValueError("Beat numbers must be between 1 and 4")
    if any(not MIN_GRID_BPM <= b <= MAX_GRID_BPM for b in bpms):
        raise ValueError(f"Tempo must be between {MIN_GRID_BPM} and {MAX_GRID_BPM} BPM")


def _is_local(path: str) -> bool:
    return bool(path) and not STREAMING_URI.match(path)


def analysis_files(share_dir: Path, relative: str | None) -> list[Path]:
    """The DAT, EXT and 2EX files for one analysis path, limited to the library share folder."""
    if not relative or not str(relative).strip():
        return []
    root = share_dir.resolve()
    dat = (share_dir / str(relative).strip("/\\")).resolve()
    if not dat.is_relative_to(root):
        return []
    return [dat.with_suffix(f".{suffix}") for suffix in ("DAT", "EXT", "2EX")]


def discard_analysis_files(paths: list[Path]) -> None:
    for path in paths:
        try:
            path.unlink(missing_ok=True)
        except OSError:
            log.warning("Could not delete %s", path)


def _pick_candidate(candidates: list[Path], size: int | None) -> Path | None:
    if len(candidates) <= 1 or not size:
        return candidates[0] if candidates else None
    for path in candidates:
        try:
            if path.stat().st_size == size:
                return path
        except OSError:
            continue
    return candidates[0]


def _back_up_once(content_id: str, paths: list[Path]) -> None:
    folder = ANLZ_BACKUP_DIR / content_id
    folder.mkdir(parents=True, exist_ok=True)
    for path in paths:
        target = folder / path.name
        if not target.exists():
            shutil.copy2(path, target)


def _move_cue(content: t.DjmdContent, cue: t.DjmdCue, move: dict) -> None:
    msec = _clamp_msec(content, move["start"])
    cue.InMsec = msec
    cue.InFrame = msec * CDJ_FRAMES_PER_SECOND // 1000
    cue.InMpegFrame = 0
    cue.InMpegAbs = 0
    end = move.get("end")
    if end is not None and cue.OutMsec is not None and cue.OutMsec >= 0:
        out = max(msec, _clamp_msec(content, end))
        cue.OutMsec = out
        cue.OutFrame = out * CDJ_FRAMES_PER_SECOND // 1000
        cue.OutMpegFrame = 0
        cue.OutMpegAbs = 0


def _clean_fields(fields: dict) -> dict:
    unknown = set(fields) - EDITABLE_FIELDS
    if unknown:
        raise ValueError(f"These fields can't be edited: {', '.join(sorted(unknown))}")
    out = {}
    for name, value in fields.items():
        if name in NUMBER_COLUMNS:
            _, limit, label = NUMBER_COLUMNS[name]
            try:
                number = int(value or 0)
            except (TypeError, ValueError):
                raise ValueError(f"{label} must be a whole number") from None
            if not 0 <= number <= limit:
                raise ValueError(f"{label} must be between 0 and {limit}")
            out[name] = number
        else:
            text = str(value or "").strip()
            if name == "title" and not text:
                raise ValueError("A track needs a title")
            out[name] = text
    return out


def _set(row, column: str, value) -> None:
    if getattr(row, column) != value:
        setattr(row, column, value)


def _renumber(db: Rekordbox6Database, songs: list[t.DjmdSongPlaylist]) -> None:
    now = datetime.now()
    moved = []
    with db.registry.disabled():
        for no, song in enumerate(songs, 1):
            if song.TrackNo != no:
                song.TrackNo = no
                song.updated_at = now
                moved.append(song)
    if moved:
        db.registry.on_move(moved)


def _ordered_songs(playlist: t.DjmdPlaylist) -> list[t.DjmdSongPlaylist]:
    return sorted(playlist.Songs, key=lambda s: s.TrackNo or 0)


def _name_resolver(db: Rekordbox6Database):
    """Look up artist, album, genre and label rows by name, adding any that are missing."""
    names: dict[tuple, str] = {}

    def named_id(table, add: str, name: str) -> str | None:
        if not name:
            return None
        key = (table, name)
        if key not in names:
            row = db.query(table).filter_by(Name=name).first()
            names[key] = (row or getattr(db, add)(name)).ID
        return names[key]

    return named_id


def _key_names(tonic: int, minor: bool) -> list[str]:
    """Names a key may be stored under: Camelot first, then the classic name and its enharmonic."""
    root = (tonic + 3) % 12 if minor else tonic
    camelot = f"{(7 * root + 7) % 12 + 1}{'A' if minor else 'B'}"
    suffix = "m" if minor else ""
    names = [camelot, TONICS[tonic] + suffix]
    if TONICS[tonic] in ENHARMONIC:
        names.append(ENHARMONIC[TONICS[tonic]] + suffix)
    return names


def _stored_path(path: Path) -> str:
    return str(path).replace("\\", "/")


def _import_fields(info: dict, named_id) -> dict:
    fields = info["fields"]
    out = {
        "Length": info["length"] or None,
        "BitRate": info["bitrate"] or None,
        "SampleRate": info["sampleRate"] or None,
        "BitDepth": info["bitDepth"] or None,
        "ReleaseYear": fields.get("year") if 0 < fields.get("year", 0) <= 9999 else None,
        "TrackNo": fields.get("trackNo") if 0 < fields.get("trackNo", 0) <= 9999 else None,
    }
    for name, column in TEXT_COLUMNS.items():
        if fields.get(name):
            out[column] = fields[name]
    for name, (table, column, add) in NAMED_COLUMNS.items():
        if fields.get(name):
            out[column] = named_id(table, add, fields[name])
    return {k: v for k, v in out.items() if v is not None}


class Library:
    def __init__(self, db_path: Path | None = None):
        self.src = Path(db_path) if db_path else _rekordbox_db_path()
        self.db_dir = self.src.parent
        self.share_dir = self.db_dir / "share"
        self.db: Rekordbox6Database | None = None
        self.writable = False
        self._snapshot_dir: Path | None = None
        self.snapshot_signature: tuple | None = None
        self.snapshot_time: float = 0
        self._content_cache: dict[str, dict] = {}

    def open(self) -> None:
        self._open_read_only()

    def reopen(self) -> None:
        if self.writable and not get_rekordbox_pid():
            try:
                self._open_live()
            except Exception:
                if self.db is None or self.writable:
                    raise
            return
        self._open_read_only()

    def set_writable(self, writable: bool) -> None:
        if writable:
            self._open_live()
        elif self.writable or self.db is None:
            self._open_read_only()

    def _open_read_only(self) -> None:
        last_err: Exception | None = None
        for _ in range(3):
            try:
                self._open_snapshot()
                return
            except Exception as e:
                last_err = e
                log.warning("Snapshot open failed, retrying: %s", e)
                time.sleep(0.3)
        raise RuntimeError(f"Could not open rekordbox library: {last_err}")

    def _open_live(self) -> None:
        if get_rekordbox_pid():
            raise RuntimeError(WRITE_BLOCKED)
        self.close()
        db: Rekordbox6Database | None = None
        try:
            db = Rekordbox6Database(path=self.src, db_dir=self.db_dir)
            db.query(t.DjmdProperty).first()
            if get_rekordbox_pid():
                raise RuntimeError(WRITE_BLOCKED)
        except Exception as e:
            if db is not None:
                _dispose(db)
            self.writable = False
            try:
                self._open_read_only()
            except Exception:
                log.exception("Could not return to the read-only snapshot")
            raise e
        self.db = db
        self.writable = True
        self.snapshot_signature = None
        self.snapshot_time = 0
        self._content_cache.clear()

    def _open_snapshot(self) -> None:
        self.close()
        signature = _source_signature(self.src)
        snap = Path(tempfile.mkdtemp(prefix="exportstudio-"))
        for suf in DB_SUFFIXES:
            p = self.src.with_name(self.src.name + suf)
            if p.exists():
                shutil.copy2(p, snap / p.name)
        db = Rekordbox6Database(path=snap / self.src.name, db_dir=self.db_dir)

        @event.listens_for(db.engine, "connect")
        def _read_only(dbapi_conn, _record):
            dbapi_conn.execute("PRAGMA query_only = ON")

        db.engine.dispose()
        db.query(t.DjmdProperty).first()

        self.db = db
        self.writable = False
        self._snapshot_dir = snap
        self.snapshot_signature = signature
        self.snapshot_time = time.time()
        self._content_cache.clear()

    def close(self) -> None:
        if self.db is not None:
            _dispose(self.db)
            self.db = None
        if self._snapshot_dir is not None:
            shutil.rmtree(self._snapshot_dir, ignore_errors=True)
            self._snapshot_dir = None

    def rekordbox_running(self) -> bool:
        return bool(get_rekordbox_pid())

    def source_changed(self) -> bool:
        if self.writable or self.snapshot_signature is None:
            return False
        return _source_signature(self.src) != self.snapshot_signature

    def _q(self, *entities):
        assert self.db is not None, "Library not open"
        return self.db.session.execute(select(*entities))

    def _counts(self, column) -> dict[str, int]:
        counts: dict[str, int] = defaultdict(int)
        for (key,) in self._q(column):
            counts[key] += 1
        return counts

    def info(self) -> dict:
        return {
            "dbPath": str(self.src),
            "snapshotTime": self.snapshot_time,
            "rekordboxRunning": self.rekordbox_running(),
            "sourceChanged": self.source_changed(),
            "writable": self.writable,
        }

    def tracks(self, ids: list[str] | None = None) -> list[dict]:
        C = t.DjmdContent
        artist, remixer, org_artist, composer = (aliased(t.DjmdArtist) for _ in range(4))
        album, genre, label, key = t.DjmdAlbum, t.DjmdGenre, t.DjmdLabel, t.DjmdKey
        stmt = (
            select(
                C.ID, C.Title, artist.Name, album.Name, genre.Name, label.Name, remixer.Name,
                org_artist.Name, composer.Name, key.ScaleName, C.BPM, C.Length, C.Rating,
                C.ColorID, C.Commnt, C.ReleaseYear, C.StockDate, C.DateCreated, C.DJPlayCount,
                C.BitRate, C.SampleRate, C.BitDepth, C.FileType, C.FolderPath, C.FileSize,
                C.ImagePath, C.TrackNo, C.AnalysisDataPath, C.created_at,
            )
            .outerjoin(artist, C.ArtistID == artist.ID)
            .outerjoin(remixer, C.RemixerID == remixer.ID)
            .outerjoin(org_artist, C.OrgArtistID == org_artist.ID)
            .outerjoin(composer, C.ComposerID == composer.ID)
            .outerjoin(album, C.AlbumID == album.ID)
            .outerjoin(genre, C.GenreID == genre.ID)
            .outerjoin(label, C.LabelID == label.ID)
            .outerjoin(key, C.KeyID == key.ID)
            .where(C.rb_local_deleted == 0)
        )
        SMT = t.DjmdSongMyTag
        tag_stmt = select(SMT.ContentID, SMT.MyTagID)
        if ids is not None:
            stmt = stmt.where(C.ID.in_(ids))
            tag_stmt = tag_stmt.where(SMT.ContentID.in_(ids))
        tags_by_track: dict[str, list[str]] = defaultdict(list)
        for content_id, tag_id in self.db.session.execute(tag_stmt):
            tags_by_track[content_id].append(tag_id)
        Q = t.DjmdCue
        cue_stmt = select(Q.ContentID, Q.Kind, Q.rb_local_deleted)
        if ids is not None:
            cue_stmt = cue_stmt.where(Q.ContentID.in_([str(i) for i in ids]))
        kinds_by_track: dict[str, list[int]] = defaultdict(list)
        for content_id, kind, deleted in self.db.session.execute(cue_stmt):
            if not deleted:
                kinds_by_track[str(content_id)].append(int(kind or 0))

        rows = []
        for r in self.db.session.execute(stmt):
            (cid, title, artist_n, album_n, genre_n, label_n, remixer_n, org_n, composer_n,
             key_n, bpm, length, rating, color_id, comment, year, stock, created, plays,
             bitrate, rate, depth, ftype, path, size, image, track_no, anlz, created_at) = r
            path = path or ""
            phrase = False
            if anlz:
                ext = (self.share_dir / str(anlz).strip("/\\")).with_suffix(".EXT")
                phrase = waveform.file_has_tag(ext, PHRASE_TAG)
            facts = waveform.cue_sort_facts(kinds_by_track.get(str(cid), []), phrase)
            rows.append({
                "id": cid,
                "title": title or Path(path).stem,
                "artist": artist_n or "",
                "album": album_n or "",
                "genre": genre_n or "",
                "label": label_n or "",
                "remixer": remixer_n or "",
                "originalArtist": org_n or "",
                "composer": composer_n or "",
                "key": key_n or "",
                "bpm": (bpm or 0) / 100,
                "duration": length or 0,
                "rating": rating or 0,
                "color": int(color_id) if color_id and str(color_id).isdigit() else 0,
                "comment": comment or "",
                "year": year or 0,
                "dateAdded": stock or "",
                "addedAt": str(created_at) if created_at else "",
                "dateCreated": created or "",
                "playCount": int(plays) if plays and str(plays).isdigit() else 0,
                "bitrate": bitrate or 0,
                "sampleRate": rate or 0,
                "bitDepth": depth or 0,
                "fileType": stored_file_type(ftype) or Path(path).suffix.lstrip(".").upper(),
                "storedType": stored_file_type(ftype),
                "path": path,
                "missing": _is_local(path) and not Path(path).is_file(),
                "streaming": bool(path) and not _is_local(path),
                "fileSize": size or 0,
                "hasArtwork": bool(image),
                "trackNo": track_no or 0,
                "analysed": bool(anlz),
                "myTags": tags_by_track.get(cid, []),
                **facts,
            })
        return rows

    def playlists(self) -> list[dict]:
        P, SP = t.DjmdPlaylist, t.DjmdSongPlaylist
        counts = self._counts(SP.PlaylistID)
        out = []
        for pid, name, parent, seq, attr in self._q(P.ID, P.Name, P.ParentID, P.Seq, P.Attribute).all():
            if pid in HIDDEN_PLAYLISTS:
                continue
            kind = {0: "playlist", FOLDER: "folder", 4: "smart"}.get(attr, "playlist")
            out.append(_node(
                pid, name, parent, seq, kind,
                count=counts.get(pid, 0) if kind == "playlist" else None,
            ))
        return out

    def playlist_tracks(self, playlist_id: str) -> list[str]:
        pl = self.db.get_playlist(ID=playlist_id)
        if pl is None or pl.is_folder:
            return []
        C = t.DjmdContent
        if pl.is_smart_playlist:
            return [cid for (cid,) in self.db.get_playlist_contents(pl, C.ID).filter(C.rb_local_deleted == 0).all()]
        SP = t.DjmdSongPlaylist
        stmt = (
            select(SP.ContentID)
            .join(C, C.ID == SP.ContentID)
            .where(SP.PlaylistID == playlist_id, C.rb_local_deleted == 0)
            .order_by(SP.TrackNo)
        )
        return [cid for (cid,) in self.db.session.execute(stmt)]

    def playlist_member_ids(self) -> list[str]:
        """Content ids that sit in at least one normal playlist. Smart playlists are not counted."""
        SP, P, C = t.DjmdSongPlaylist, t.DjmdPlaylist, t.DjmdContent
        stmt = (
            select(SP.ContentID)
            .join(P, P.ID == SP.PlaylistID)
            .join(C, C.ID == SP.ContentID)
            .where(
                P.Attribute == NORMAL_PLAYLIST,
                P.ID.notin_(HIDDEN_PLAYLISTS),
                C.rb_local_deleted == 0,
                SP.rb_local_deleted == 0,
            )
            .distinct()
        )
        return [str(cid) for (cid,) in self.db.session.execute(stmt)]

    def histories(self) -> list[dict]:
        H, SH = t.DjmdHistory, t.DjmdSongHistory
        counts = self._counts(SH.HistoryID)
        return [
            _node(
                hid, name, parent, seq, _leaf_or_folder(attr, "playlist"),
                date=created or "",
                count=None if attr == FOLDER else counts.get(hid, 0),
            )
            for hid, name, parent, seq, attr, created
            in self._q(H.ID, H.Name, H.ParentID, H.Seq, H.Attribute, H.DateCreated).all()
        ]

    def history_tracks(self, history_id: str) -> list[str]:
        SH, C = t.DjmdSongHistory, t.DjmdContent
        stmt = (
            select(SH.ContentID)
            .join(C, C.ID == SH.ContentID)
            .where(SH.HistoryID == history_id, C.rb_local_deleted == 0)
            .order_by(SH.TrackNo)
        )
        return [cid for (cid,) in self.db.session.execute(stmt)]

    def my_tags(self) -> list[dict]:
        M = t.DjmdMyTag
        return [
            _node(mid, name, parent, seq, _leaf_or_folder(attr, "tag"))
            for mid, name, parent, seq, attr in self._q(M.ID, M.Name, M.ParentID, M.Seq, M.Attribute).all()
        ]


    def _content(self, content_id: str) -> t.DjmdContent:
        content = self.db.get_content(ID=content_id)
        if content is None:
            raise KeyError(f"Unknown track {content_id}")
        return content

    def _anlz_path(self, content: t.DjmdContent) -> Path | None:
        if not content.AnalysisDataPath:
            return None
        return self.share_dir / content.AnalysisDataPath.strip("/\\")

    def _anlz(self, content: t.DjmdContent) -> dict[str, waveform.Tags | None]:
        out: dict[str, waveform.Tags | None] = {"DAT": None, "EXT": None, "2EX": None}
        dat = self._anlz_path(content)
        if dat is None:
            return out
        for kind in out:
            path = dat.with_suffix(f".{kind}")
            if path.is_file():
                try:
                    out[kind] = waveform.read_tags(path)
                except Exception as e:
                    log.warning("Failed to read %s: %s", path, e)
        return out

    def track_detail(self, content_id: str) -> dict:
        content = self._content(content_id)
        anlz = self._anlz(content)
        path = Path(content.FolderPath or "")
        rgb = waveform.decode_rgb(anlz["EXT"])
        return {
            "id": content.ID,
            "fileExists": path.is_file(),
            "cues": self._cues(content, anlz["EXT"]),
            "beatgrid": waveform.decode_beatgrid(anlz["DAT"]),
            "waveform": {
                "rate": waveform.DETAIL_RATE,
                "rgb": rgb,
                "threeBand": waveform.decode_three_band(anlz["2EX"]),
                "threeBandPreview": waveform.decode_three_band(anlz["2EX"], waveform.PREVIEW_BANDS_TAG),
                "mono": None if rgb else waveform.decode_mono(anlz["EXT"]),
            },
        }

    def track_previews(self, ids: list[str]) -> list[dict]:
        """Cue stars and a short waveform for the browser rows on screen."""
        wanted = list(dict.fromkeys(str(i) for i in ids))[:80]
        if not wanted:
            return []
        Q = t.DjmdCue
        stored = _stored_cues(self.db.session.execute(
            select(
                Q.ContentID, Q.ID, Q.Kind, Q.InMsec, Q.OutMsec, Q.Color, Q.ColorTableIndex,
                Q.Comment, Q.rb_local_deleted,
            ).where(Q.ContentID.in_(wanted))
        ))
        paths = {
            str(cid): anlz
            for cid, anlz in self.db.session.execute(
                select(t.DjmdContent.ID, t.DjmdContent.AnalysisDataPath).where(t.DjmdContent.ID.in_(wanted))
            )
        }
        rows = []
        for cid in wanted:
            phrase = False
            wave = None
            ext = None
            relative = paths.get(cid)
            if relative:
                dat = self.share_dir / str(relative).strip("/\\")
                ext = _read_anlz(dat.with_suffix(".EXT"))
                ex2 = _read_anlz(dat.with_suffix(".2EX"))
                phrase = waveform.has_phrase(ext)
                wave = waveform.browser_waveform(ext, ex2)
            cues = waveform.paint_browser_cues(stored.get(cid, []), waveform.hot_cue_colors(ext))
            memory, hot = waveform.cue_marks(cue["slot"] for cue in cues)
            rows.append({
                "id": cid,
                "memory": memory,
                "hot": hot,
                "phrase": phrase,
                "cues": cues,
                "waveform": wave,
            })
        return rows

    def _cues(self, content: t.DjmdContent, ext: waveform.Tags | None) -> list[dict]:
        stored = [
            waveform.browser_cue(q.ID, q.Kind, q.InMsec, q.OutMsec, q.Color, q.ColorTableIndex, q.Comment)
            for q in content.Cues
            if not q.rb_local_deleted
        ]
        return waveform.paint_browser_cues(stored, waveform.hot_cue_colors(ext))

    def set_hot_cue(self, content_id: str, kind: int, msec: int) -> list[dict]:
        if kind not in HOT_CUE_KINDS:
            raise ValueError(f"Not a hot cue slot: {kind}")
        db = self._writable_db()
        content = self._content(content_id)
        msec = _clamp_msec(content, msec)

        def place() -> None:
            for cue in list(content.Cues):
                if cue.Kind == kind and not cue.rb_local_deleted:
                    db.delete(cue)
            db.add(_new_cue(db, content, kind, msec))

        _commit(db, place)
        return self.cues(content_id)

    def add_memory_cue(self, content_id: str, msec: int) -> list[dict]:
        db = self._writable_db()
        content = self._content(content_id)
        msec = _clamp_msec(content, msec)
        duplicate = any(
            not cue.Kind
            and not cue.rb_local_deleted
            and cue.InMsec == msec
            and (cue.OutMsec is None or cue.OutMsec < 0)
            for cue in content.Cues
        )
        if not duplicate:
            _commit(db, lambda: db.add(_new_cue(db, content, 0, msec)))
        return self.cues(content_id)

    def delete_cue(self, content_id: str, cue_id: str) -> list[dict]:
        db = self._writable_db()
        cue = db.query(t.DjmdCue).filter_by(ID=cue_id, ContentID=content_id).one_or_none()
        if cue is not None:
            _commit(db, lambda: db.delete(cue))
        return self.cues(content_id)

    def set_cue_color(self, content_id: str, cue_id: str, color_index: int | None) -> list[dict]:
        if color_index is not None and not 1 <= color_index <= HOT_CUE_PALETTE_SIZE:
            raise ValueError(f"Not a hot cue colour: {color_index}")
        db = self._writable_db()
        cue = db.query(t.DjmdCue).filter_by(ID=cue_id, ContentID=content_id).one_or_none()
        if cue is None or not cue.Kind:
            raise KeyError(f"Unknown hot cue {cue_id}")
        if cue.ColorTableIndex != color_index:
            def paint() -> None:
                cue.ColorTableIndex = color_index

            _commit(db, paint)
        return self.cues(content_id)

    def set_beatgrid(
        self,
        content_id: str,
        times: list[int],
        beats: list[int],
        bpms: list[float],
        cues: list[dict] | None = None,
    ) -> dict:
        times = [int(round(x)) for x in times]
        beats = [int(b) for b in beats]
        bpms = [round(float(b), 2) for b in bpms]
        _check_grid(times, beats, bpms)
        db = self._writable_db()
        content = self._content(content_id)
        dat = self._anlz_path(content)
        if dat is None or not dat.is_file():
            raise FileNotFoundError("This track has no analysis file to hold a beatgrid")
        dat_tags = waveform.read_tags(dat)
        if waveform.BEATGRID_TAG not in dat_tags:
            raise ValueError("This track has no beatgrid to edit")

        writes = {dat: {waveform.BEATGRID_TAG: waveform.build_beatgrid(dat_tags[waveform.BEATGRID_TAG][0], times, beats, bpms)}}
        ext = dat.with_suffix(".EXT")
        if ext.is_file():
            ext_tags = waveform.read_tags(ext)
            if waveform.EXTENDED_BEATGRID_TAG in ext_tags:
                tag = ext_tags[waveform.EXTENDED_BEATGRID_TAG][0]
                writes[ext] = {waveform.EXTENDED_BEATGRID_TAG: waveform.clear_extended_beatgrid(tag)}

        _back_up_once(content_id, list(writes))
        previous = {path: path.read_bytes() for path in writes}
        constant_bpm = bpms[0] if len(set(bpms)) == 1 else None
        moves = {str(m["id"]): m for m in cues or []}

        def update() -> None:
            if constant_bpm is not None:
                content.BPM = int(round(constant_bpm * 100))
            for cue in content.Cues:
                move = moves.get(str(cue.ID))
                if move is None or cue.rb_local_deleted:
                    continue
                _move_cue(content, cue, move)

        try:
            for path, replace in writes.items():
                waveform.write_tags(path, replace)
            _commit(db, update)
        except Exception:
            for path, data in previous.items():
                try:
                    path.write_bytes(data)
                except OSError:
                    log.exception("Could not restore %s", path)
            raise
        self._content_cache.clear()
        return {
            "beatgrid": waveform.decode_beatgrid(waveform.read_tags(dat)),
            "cues": self.cues(content_id),
            "bpm": constant_bpm,
        }

    def cues(self, content_id: str) -> list[dict]:
        content = self._content(content_id)
        return self._cues(content, self._anlz(content)["EXT"])

    def update_tracks(self, ids: list[str], fields: dict, write_tags: bool = False) -> dict:
        fields = _clean_fields(fields)
        db = self._writable_db()
        contents = [self._content(cid) for cid in dict.fromkeys(ids)]
        file_fields = {k: v for k, v in fields.items() if k in file_tags.FILE_FIELDS}
        errors: list[str] = []
        named_id = _name_resolver(db)

        def update() -> None:
            for content in contents:
                for name, value in fields.items():
                    if name in TEXT_COLUMNS:
                        _set(content, TEXT_COLUMNS[name], value)
                    elif name == "color":
                        _set(content, "ColorID", str(value) if value else None)
                    elif name in NUMBER_COLUMNS:
                        _set(content, NUMBER_COLUMNS[name][0], value)
                    else:
                        table, column, add = NAMED_COLUMNS[name]
                        _set(content, column, named_id(table, add, value))
                if write_tags and file_fields:
                    path = Path(content.FolderPath or "")
                    try:
                        file_tags.write(path, file_fields)
                        _set(content, "FileSize", path.stat().st_size)
                    except Exception as e:
                        log.warning("Could not write tags to %s: %s", path, e)
                        errors.append(f"{path.name}: {e}")

        if fields and contents:
            _commit(db, update)
        return {"tracks": self.tracks(ids=[c.ID for c in contents]), "tagErrors": errors}

    def relocate_tracks(self, moves: dict[str, str]) -> list[dict]:
        db = self._writable_db()
        plan: list[tuple[t.DjmdContent, Path, str]] = []
        for content_id, raw in moves.items():
            target = Path(raw)
            if not target.is_absolute() or not target.is_file():
                raise FileNotFoundError(f"Can't find {target.name or raw}")
            plan.append((self._content(content_id), target, str(target).replace("\\", "/")))

        writes: dict[Path, dict[str, bytes]] = {}
        for content, _, stored in plan:
            dat = self._anlz_path(content)
            if dat is None:
                continue
            files = []
            for kind in ("DAT", "EXT", "2EX"):
                anlz = dat.with_suffix(f".{kind}")
                if not anlz.is_file():
                    continue
                tags = waveform.read_tags(anlz)
                if waveform.PATH_TAG in tags:
                    writes[anlz] = {waveform.PATH_TAG: waveform.build_path(tags[waveform.PATH_TAG][0], stored)}
                    files.append(anlz)
            if files:
                _back_up_once(content.ID, files)
        previous = {path: path.read_bytes() for path in writes}

        def update() -> None:
            for content, target, stored in plan:
                old = content.FolderPath
                _set(content, "FolderPath", stored)
                if content.OrgFolderPath == old:
                    _set(content, "OrgFolderPath", stored)
                _set(content, "FileNameL", target.name)
                _set(content, "FileSize", target.stat().st_size)

        try:
            for path, replace in writes.items():
                waveform.write_tags(path, replace)
            _commit(db, update)
        except Exception:
            for path, data in previous.items():
                try:
                    path.write_bytes(data)
                except OSError:
                    log.exception("Could not restore %s", path)
            raise
        self._content_cache.clear()
        return self.tracks(ids=[content.ID for content, _, _ in plan])

    def inspect_tracks(self, tracks: list) -> list[dict]:
        """Header checks for a batch of collection tracks.

        Each item is `{id, size?, mtime?}`. When size and mtime still match the file, the header
        is not read again and the result is `same`. Streaming tracks are left out.
        """
        if not isinstance(tracks, list):
            raise ValueError("tracks must be a list")
        out = []
        for item in tracks[:80]:
            if not isinstance(item, dict) or not item.get("id"):
                raise ValueError("tracks must be a list of track ids")
            content = self._content(str(item["id"]))
            stored_path = content.FolderPath or ""
            if not _is_local(stored_path):
                out.append({"id": content.ID, "size": 0, "mtime": None, "same": False, "issues": [], "audio": None})
                continue
            path = Path(stored_path)
            if "size" in item and "mtime" in item:
                cached = same_fingerprint(path, item.get("size"), item.get("mtime"))
                if cached is not None:
                    size, mtime = cached
                    out.append({
                        "id": content.ID,
                        "size": size,
                        "mtime": None if mtime is None else str(mtime),
                        "same": True,
                        "issues": [],
                        "audio": None,
                    })
                    continue
            result = inspect_file(path)
            out.append({"id": content.ID, "same": False, **result})
        return out

    def find_missing(self, ids: list[str], directory: str) -> dict:
        """Look under `directory` for files with the same name as each missing track."""
        root = Path(directory)
        if not root.is_dir():
            raise FileNotFoundError(f"Can't open the folder {directory}")
        wanted: dict[str, list[tuple[str, int | None]]] = defaultdict(list)
        for content_id in dict.fromkeys(ids):
            content = self._content(content_id)
            stored = content.FolderPath or ""
            path = Path(stored)
            if not _is_local(stored) or path.is_file():
                continue
            name = path.name or content.FileNameL or ""
            if name:
                wanted[name.casefold()].append((content.ID, content.FileSize))

        found: dict[str, list[Path]] = defaultdict(list)
        scanned = 0
        if wanted:
            for folder, dirs, files in os.walk(root):
                dirs[:] = [d for d in dirs if not d.startswith(".")]
                for name in files:
                    scanned += 1
                    key = name.casefold()
                    if key in wanted:
                        found[key].append(Path(folder, name))

        matches = []
        for key, rows in wanted.items():
            for content_id, size in rows:
                pick = _pick_candidate(found.get(key, []), size)
                matches.append({"id": content_id, "path": str(pick) if pick else None})
        return {"matches": matches, "scanned": scanned}

    def import_tracks(self, paths: list[str], playlist_id: str | None = None, import_artwork: bool = True) -> dict:
        """Add audio files to the collection, and optionally to the end of a playlist.

        Files already in the collection are not added twice, but still go into the playlist. A track
        that was removed comes back as the same row, without the cues and beatgrid deleted with it.
        """
        db = self._writable_db()
        pl = self._editable_playlist(db, playlist_id) if playlist_id else None
        C = t.DjmdContent
        failed: list[dict] = []
        wanted: list[tuple[Path, str]] = []
        for raw in dict.fromkeys(paths):
            path = Path(raw)
            if path.suffix.lower() not in IMPORT_SUFFIXES:
                failed.append({"path": raw, "error": f"rekordbox can't import {path.suffix or 'this'} files"})
            elif not path.is_absolute() or not path.is_file():
                failed.append({"path": raw, "error": "File not found"})
            else:
                wanted.append((path, _stored_path(path)))

        rows: dict[str, list[t.DjmdContent]] = defaultdict(list)
        stored = [s for _, s in wanted]
        for i in range(0, len(stored), 500):
            for row in db.query(C).filter(C.FolderPath.in_(stored[i : i + 500])):
                rows[row.FolderPath].append(row)

        infos: dict[str, dict] = {}
        for path, key in wanted:
            if any(not r.rb_local_deleted for r in rows.get(key, [])):
                continue
            try:
                infos[key] = file_tags.read(path)
            except Exception as e:
                log.warning("Could not read tags from %s: %s", path, e)
                infos[key] = {"fields": {}, "artwork": None, "length": 0, "bitrate": 0, "sampleRate": 0, "bitDepth": 0}

        ordered: list[str] = []
        added: list[str] = []
        skipped = 0
        named_id = _name_resolver(db)
        today = datetime.now().strftime("%Y-%m-%d")

        def create() -> None:
            nonlocal skipped
            for path, key in wanted:
                existing = rows.get(key, [])
                live = next((r for r in existing if not r.rb_local_deleted), None)
                if live is not None:
                    skipped += 1
                    ordered.append(str(live.ID))
                    continue
                if existing:
                    content = existing[0]
                    content.rb_local_deleted = 0
                    for column, value in stream_fields(path, infos.get(key)).items():
                        _set(content, column, value)
                    _set(content, "StockDate", today)
                else:
                    info = infos[key]
                    fields = _import_fields(info, named_id)
                    fields.setdefault("Title", path.stem)
                    content = db.add_content(key, **fields)
                    content.StockDate = today
                    content.DateCreated = today
                    if import_artwork and info["artwork"]:
                        try:
                            content.ImagePath = artwork.save(info["artwork"], self.share_dir, content.UUID)
                        except Exception as e:
                            log.warning("Could not save artwork for %s: %s", path, e)
                cid = str(content.ID)
                ordered.append(cid)
                added.append(cid)
            if pl is not None:
                present = {s.ContentID for s in pl.Songs}
                for cid in dict.fromkeys(ordered):
                    if cid not in present:
                        db.add_to_playlist(pl, cid)

        if wanted:
            _commit(db, create)
        self._content_cache.clear()
        return {
            "tracks": self.tracks(ids=added) if added else [],
            "ids": list(dict.fromkeys(ordered)),
            "skipped": skipped,
            "failed": failed,
            "playlist": self.playlist_tracks(pl.ID) if pl is not None else None,
        }

    def remove_from_collection(self, ids: list[str]) -> dict:
        """Hide tracks and drop the data a later import would otherwise restore.

        The content row stays, marked deleted, so titles can return. Cues, the beatgrid, and the
        stored file type do not. Import reads the file type from the audio file again.
        """
        db = self._writable_db()
        contents = [self._content(cid) for cid in dict.fromkeys(ids)]
        drop = {c.ID for c in contents}
        files = [path for content in contents for path in analysis_files(self.share_dir, content.AnalysisDataPath)]
        SP = t.DjmdSongPlaylist
        playlist_ids = [
            pid for (pid,) in db.session.execute(select(SP.PlaylistID).where(SP.ContentID.in_(drop)).distinct())
        ]

        def remove() -> None:
            for pid in playlist_ids:
                pl = db.get_playlist(ID=pid)
                if pl is None:
                    continue
                songs = _ordered_songs(pl)
                for song in songs:
                    if song.ContentID in drop:
                        db.delete(song)
                _renumber(db, [s for s in songs if s.ContentID not in drop])
            if drop:
                for cue in db.query(t.DjmdCue).filter(t.DjmdCue.ContentID.in_(drop)).all():
                    db.delete(cue)
            for content in contents:
                _set(content, "AnalysisDataPath", None)
                _set(content, "Analysed", 0)
                for column in STREAM_COLUMNS:
                    _set(content, column, None)
                _set(content, "rb_local_deleted", 1)

        if contents:
            _commit(db, remove)
            discard_analysis_files(files)
        self._content_cache.clear()
        return {"removed": sorted(drop), "playlists": self.playlists()}

    def convert_tracks(self, ids: list[str], preset: dict, mode: str, on_progress=None) -> dict:
        """Encode each track's audio file to `preset` and point the library at the result.

        `copy` adds a new track with the cues and analysis of the original. `update` repoints the
        track and leaves the old file. `update_delete` does the same and lists the old file in
        `originals` for the caller to remove. A track that fails is left as it was.
        """
        if mode not in converter.MODES:
            raise ValueError(f"Unknown conversion mode: {mode}")
        preset = converter.clean_preset(preset)
        db = self._writable_db()
        ordered = list(dict.fromkeys(ids))
        changed: list[str] = []
        originals: list[str] = []
        failed: list[dict] = []
        for done, content_id in enumerate(ordered):
            if on_progress:
                on_progress({"done": done, "total": len(ordered), "id": content_id})
            target: Path | None = None
            try:
                content = self._content(content_id)
                source = Path(content.FolderPath or "")
                target = converter.convert_file(source, preset)
                info = file_tags.read(target)
                offset = converter.delay_offset_ms(source, target)
                result_id = self._apply_conversion(db, content, target, info, offset, mode == "copy")
            except Exception as e:
                log.warning("Could not convert track %s: %s", content_id, e)
                if target is not None:
                    target.unlink(missing_ok=True)
                failed.append({"id": content_id, "error": str(e) or e.__class__.__name__})
                continue
            changed.append(result_id)
            if mode == "update_delete":
                originals.append(str(source))
        if on_progress:
            on_progress({"done": len(ordered), "total": len(ordered)})
        self._content_cache.clear()
        return {"tracks": self.tracks(ids=changed) if changed else [], "ids": changed, "originals": originals, "failed": failed}

    def _apply_conversion(
        self, db: Rekordbox6Database, content: t.DjmdContent, target: Path, info: dict, offset_ms: int, copy: bool
    ) -> str:
        stored = _stored_path(target)
        fields = stream_fields(target, info)
        old_files = analysis_files(self.share_dir, content.AnalysisDataPath)
        result: dict = {}

        def shifted(msec: int) -> int:
            return _clamp_msec(content, msec + offset_ms) if offset_ms else msec

        def update() -> None:
            row = content
            if copy:
                row = db.add_content(stored, Title=content.Title)
                for column in COPIED_COLUMNS:
                    if getattr(content, column) is not None:
                        _set(row, column, getattr(content, column))
                row.StockDate = datetime.now().strftime("%Y-%m-%d")
                for cue in content.Cues:
                    if cue.rb_local_deleted:
                        continue
                    clone = _new_cue(db, row, cue.Kind, shifted(cue.InMsec))
                    for column in ("Color", "ColorTableIndex", "Comment", "ActiveLoop", "BeatLoopSize"):
                        setattr(clone, column, getattr(cue, column))
                    if cue.OutMsec is not None and cue.OutMsec >= 0:
                        out = shifted(cue.OutMsec)
                        clone.OutMsec = out
                        clone.OutFrame = out * CDJ_FRAMES_PER_SECOND // 1000
                    db.add(clone)
            else:
                row.FolderPath = stored
                for cue in content.Cues:
                    if offset_ms and not cue.rb_local_deleted:
                        _move_cue(content, cue, {"start": cue.InMsec + offset_ms, "end": None})
            for column in STREAM_COLUMNS:
                _set(row, column, fields.get(column))
            for column in ("FileNameL", "FileSize", "Length"):
                if column in fields:
                    _set(row, column, fields[column])
            if old_files and old_files[0].with_suffix(".DAT").is_file():
                relative = row.AnalysisDataPath
                if copy:
                    relative = f"/PIONEER/USBANLZ/{row.UUID[:3]}/{row.UUID[3:]}/ANLZ0000.DAT"
                    row.AnalysisDataPath = relative
                    row.Analysed = content.Analysed
                result["files"] = (relative, target.name)
            result["id"] = str(row.ID)

        _commit(db, update)
        if "files" in result:
            relative, name = result["files"]
            dat = self.share_dir / relative.strip("/\\")
            for source in old_files:
                if not source.is_file():
                    continue
                path = dat.with_suffix(source.suffix)
                patched = anlz_build.patch_file(source.read_bytes(), {"PPTH": anlz_build.path_tag("?/" + name)})
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(patched)
        return result["id"]

    def apply_analysis(self, content_id: str, result: dict, options: dict) -> dict:
        """Write an analysis from the native analyser: the ANLZ files, BPM, key and analysed flag.

        A grid, key or waveform the track already has is kept unless `options["overwrite"]`. Cues
        in existing files are always kept; phrases go when the grid they were counted on changes.
        """
        db = self._writable_db()
        content = self._content(content_id)
        overwrite = bool(options.get("overwrite"))
        beats = [(int(n), int(tempo), int(ms)) for n, tempo, ms in result.get("beats") or []]
        features = anlz_build.features_from_bytes(base64.b64decode(result.get("waveform") or ""))
        audio_name = Path(content.FolderPath or "").name

        relative = content.AnalysisDataPath or (
            f"/PIONEER/USBANLZ/{content.UUID[:3]}/{content.UUID[3:]}/ANLZ0000.DAT"
        )
        dat = self.share_dir / relative.strip("/\\")
        paths = {kind: dat.with_suffix(f".{kind}") for kind in anlz_build.WAVE_TAGS}
        existing = {kind: waveform.read_tags(p) for kind, p in paths.items() if p.is_file()}
        has_grid = bool(waveform.decode_beatgrid(existing.get("DAT")))
        write_grid = bool(options.get("grid", True)) and bool(beats) and (overwrite or not has_grid)
        key = result.get("key")
        write_key = bool(options.get("key", True)) and key is not None and (overwrite or not content.KeyID)

        fresh = anlz_build.new_files(audio_name, beats if write_grid else [], features)
        waves = anlz_build.wave_tags(features) if len(features) else {}
        writes: dict[Path, bytes] = {}
        for kind, path in paths.items():
            tags = existing.get(kind)
            if tags is None:
                writes[path] = fresh[kind]
                continue
            replace = {k: waves[k] for k in anlz_build.WAVE_TAGS[kind] if k in waves and (overwrite or k not in tags)}
            drop: set[str] = set()
            if write_grid and kind == "DAT":
                replace[waveform.BEATGRID_TAG] = anlz_build.beatgrid_tag(beats)
            if write_grid and kind == "EXT":
                if waveform.EXTENDED_BEATGRID_TAG in tags:
                    replace[waveform.EXTENDED_BEATGRID_TAG] = waveform.clear_extended_beatgrid(
                        tags[waveform.EXTENDED_BEATGRID_TAG][0]
                    )
                drop.add(PHRASE_TAG)
            if replace or drop:
                writes[path] = anlz_build.patch_file(path.read_bytes(), replace, drop)

        if existing:
            _back_up_once(content.ID, [p for k, p in paths.items() if k in existing])
        previous = {path: path.read_bytes() if path.is_file() else None for path in writes}

        def update() -> None:
            if write_grid:
                _set(content, "BPM", int(round(float(result["bpm"]) * 100)))
            if write_key:
                _set(content, "KeyID", self._key_id(db, int(key["tonic"]), bool(key["minor"])))
            _set(content, "AnalysisDataPath", relative)
            _set(content, "Analysed", ANALYSED)
            duration = int(round(float(result.get("duration") or 0)))
            if duration and not content.Length:
                _set(content, "Length", duration)
            updated = str(content.AnalysisUpdated or "0")
            content.AnalysisUpdated = str(int(updated) + 1 if updated.isdigit() else 1)

        try:
            for path, data in writes.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                tmp = path.with_name(path.name + ".tmp")
                tmp.write_bytes(data)
                os.replace(tmp, path)
            _commit(db, update)
        except Exception:
            for path, data in previous.items():
                try:
                    if data is None:
                        path.unlink(missing_ok=True)
                    else:
                        path.write_bytes(data)
                except OSError:
                    log.exception("Could not restore %s", path)
            raise
        self._content_cache.clear()
        return self.tracks(ids=[content.ID])[0]

    def _key_id(self, db: Rekordbox6Database, tonic: int, minor: bool) -> str:
        """The key row for a key, in whichever notation the library already uses, added if missing."""
        rows = db.query(t.DjmdKey).all()
        names = _key_names(tonic, minor)
        for row in rows:
            if row.ScaleName in names:
                return row.ID
        camelot = sum(1 for row in rows if CAMELOT.fullmatch(row.ScaleName or ""))
        name = names[0] if camelot * 2 >= len(rows) else names[1]
        row = t.DjmdKey.create(
            ID=str(db.generate_unused_id(t.DjmdKey)), ScaleName=name, Seq=len(rows) + 1, UUID=str(uuid4())
        )
        db.add(row)
        db.flush()
        return row.ID

    def _editable_playlist(self, db: Rekordbox6Database, playlist_id: str) -> t.DjmdPlaylist:
        pl = db.get_playlist(ID=playlist_id)
        if pl is None or pl.ID in HIDDEN_PLAYLISTS:
            raise KeyError(f"Unknown playlist {playlist_id}")
        if pl.Attribute != NORMAL_PLAYLIST:
            raise ValueError("Tracks can only be added to or removed from a normal playlist")
        return pl

    def add_to_playlist(self, playlist_id: str, track_ids: list[str]) -> list[str]:
        db = self._writable_db()
        pl = self._editable_playlist(db, playlist_id)
        present = {s.ContentID for s in pl.Songs}
        new = [self._content(cid).ID for cid in dict.fromkeys(track_ids) if cid not in present]

        def add() -> None:
            for cid in new:
                db.add_to_playlist(pl, cid)

        if new:
            _commit(db, add)
        return self.playlist_tracks(pl.ID)

    def remove_from_playlist(self, playlist_id: str, track_ids: list[str]) -> list[str]:
        db = self._writable_db()
        pl = self._editable_playlist(db, playlist_id)
        drop = set(track_ids)
        songs = _ordered_songs(pl)
        if any(s.ContentID in drop for s in songs):

            def remove() -> None:
                for song in songs:
                    if song.ContentID in drop:
                        db.delete(song)
                _renumber(db, [s for s in songs if s.ContentID not in drop])

            _commit(db, remove)
        return self.playlist_tracks(pl.ID)

    def move_in_playlist(self, playlist_id: str, track_ids: list[str], index: int) -> list[str]:
        """Move tracks so they sit together before the track currently at `index`."""
        db = self._writable_db()
        pl = self._editable_playlist(db, playlist_id)
        moving = set(track_ids)
        songs = _ordered_songs(pl)
        index = max(0, min(int(index), len(songs)))
        before = sum(1 for s in songs[:index] if s.ContentID not in moving)
        rest = [s for s in songs if s.ContentID not in moving]
        picked = [s for s in songs if s.ContentID in moving]
        order = rest[:before] + picked + rest[before:]
        if order != songs:
            _commit(db, lambda: _renumber(db, order))
        return self.playlist_tracks(pl.ID)

    def create_playlist(
        self, name: str, parent_id: str = "root", folder: bool = False, track_ids: list[str] | None = None
    ) -> dict:
        db = self._writable_db()
        name = name.strip() or ("New folder" if folder else "New playlist")
        parent = None if parent_id in (None, "", "root") else parent_id
        created: list[str] = []

        def create() -> None:
            make = db.create_playlist_folder if folder else db.create_playlist
            pl = make(name, parent=parent)
            created.append(pl.ID)
            if not folder:
                for cid in dict.fromkeys(track_ids or []):
                    db.add_to_playlist(pl, self._content(cid))

        _commit(db, create)
        return {"id": created[0], "playlists": self.playlists()}

    def rename_playlist(self, playlist_id: str, name: str) -> list[dict]:
        db = self._writable_db()
        name = name.strip()
        if not name:
            raise ValueError("A playlist needs a name")
        pl = db.get_playlist(ID=playlist_id)
        if pl is None or pl.ID in HIDDEN_PLAYLISTS:
            raise KeyError(f"Unknown playlist {playlist_id}")
        if pl.Name != name:
            _commit(db, lambda: db.rename_playlist(pl, name))
        return self.playlists()

    def delete_playlist(self, playlist_id: str) -> list[dict]:
        db = self._writable_db()
        pl = db.get_playlist(ID=playlist_id)
        if pl is None or pl.ID in HIDDEN_PLAYLISTS:
            raise KeyError(f"Unknown playlist {playlist_id}")
        _commit(db, lambda: db.delete_playlist(pl))
        return self.playlists()

    def _tag(self, db: Rekordbox6Database, tag_id: str) -> t.DjmdMyTag:
        tag = db.get_my_tag(ID=tag_id)
        if tag is None:
            raise KeyError(f"Unknown tag {tag_id}")
        if tag.Attribute == FOLDER:
            raise ValueError("Pick a tag rather than a tag folder")
        return tag

    def add_tag(self, tag_id: str, track_ids: list[str]) -> list[dict]:
        db = self._writable_db()
        tag = self._tag(db, tag_id)
        SMT = t.DjmdSongMyTag
        tagged = [cid for (cid,) in db.session.execute(select(SMT.ContentID).where(SMT.MyTagID == tag.ID))]
        present = set(tagged)
        new = [self._content(cid).ID for cid in dict.fromkeys(track_ids) if cid not in present]

        def add() -> None:
            for no, cid in enumerate(new, len(tagged) + 1):
                db.add(SMT.create(ID=str(uuid4()), UUID=str(uuid4()), MyTagID=tag.ID, ContentID=cid, TrackNo=no))

        if new:
            _commit(db, add)
        return self.tracks(ids=list(track_ids))

    def remove_tag(self, tag_id: str, track_ids: list[str]) -> list[dict]:
        db = self._writable_db()
        tag = self._tag(db, tag_id)
        SMT = t.DjmdSongMyTag
        rows = db.query(SMT).filter(SMT.MyTagID == tag.ID, SMT.ContentID.in_(track_ids)).all()

        def remove() -> None:
            for row in rows:
                db.delete(row)

        if rows:
            _commit(db, remove)
        return self.tracks(ids=list(track_ids))

    def _writable_db(self) -> Rekordbox6Database:
        if not self.writable or self.db is None:
            raise RuntimeError("Unlock editing to make changes to the library")
        if get_rekordbox_pid():
            raise RuntimeError(WRITE_BLOCKED)
        return self.db

    def resolve_media(self, kind: str, content_id: str, size: str = "") -> str | None:
        content = self._content(content_id)
        if kind == "audio":
            path = Path(content.FolderPath or "")
            return str(path) if path.is_file() else None
        if kind == "art" and content.ImagePath:
            path = self.share_dir / content.ImagePath.lstrip("/\\")
            if size in ("s", "m"):
                sized = path.with_name(f"{path.stem}_{size}{path.suffix}")
                if sized.is_file():
                    return str(sized)
            return str(path) if path.is_file() else None
        return None
