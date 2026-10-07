import logging
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

from . import tags as file_tags
from . import waveform

log = logging.getLogger(__name__)

DB_SUFFIXES = ("", "-wal", "-shm")
FILE_TYPES = {1: "MP3", 4: "M4A", 5: "FLAC", 11: "WAV", 12: "AIFF"}
CLOUD_LIBRARY_SYNC = "100000"
HIDDEN_PLAYLISTS = {CLOUD_LIBRARY_SYNC}
WRITE_BLOCKED = "Close rekordbox before editing the library"
LOOP_KIND = 4
HOT_CUE_KINDS = tuple(kind for kind in range(1, 10) if kind != LOOP_KIND)
HOT_CUE_PALETTE_SIZE = 62
CDJ_FRAMES_PER_SECOND = 150
FOLDER = 1
MIN_GRID_BPM = 40
MAX_GRID_BPM = 300
BEATS_PER_BAR = 4
ANLZ_BACKUP_DIR = Path.home() / ".exportstudio" / "anlz-backup"
NORMAL_PLAYLIST = 0

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

        rows = []
        for r in self.db.session.execute(stmt):
            (cid, title, artist_n, album_n, genre_n, label_n, remixer_n, org_n, composer_n,
             key_n, bpm, length, rating, color_id, comment, year, stock, created, plays,
             bitrate, rate, depth, ftype, path, size, image, track_no, anlz, created_at) = r
            path = path or ""
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
                "dateCreated": created or "",
                "playCount": int(plays) if plays and str(plays).isdigit() else 0,
                "bitrate": bitrate or 0,
                "sampleRate": rate or 0,
                "bitDepth": depth or 0,
                "fileType": FILE_TYPES.get(ftype) or Path(path).suffix.lstrip(".").upper(),
                "path": path,
                "fileSize": size or 0,
                "hasArtwork": bool(image),
                "trackNo": track_no or 0,
                "analysed": bool(anlz),
                "myTags": tags_by_track.get(cid, []),
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
        if pl.is_smart_playlist:
            return [cid for (cid,) in self.db.get_playlist_contents(pl, t.DjmdContent.ID).all()]
        SP = t.DjmdSongPlaylist
        stmt = select(SP.ContentID).where(SP.PlaylistID == playlist_id).order_by(SP.TrackNo)
        return [cid for (cid,) in self.db.session.execute(stmt)]

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
        SH = t.DjmdSongHistory
        stmt = select(SH.ContentID).where(SH.HistoryID == history_id).order_by(SH.TrackNo)
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

    def _cues(self, content: t.DjmdContent, ext: waveform.Tags | None) -> list[dict]:
        cues = []
        for q in content.Cues:
            if q.rb_local_deleted:
                continue
            cues.append({
                "id": q.ID,
                "kind": "hot" if q.Kind and q.Kind > 0 else "memory",
                "slot": q.Kind or 0,
                "start": q.InMsec or 0,
                "end": q.OutMsec if q.OutMsec is not None and q.OutMsec >= 0 else None,
                "color": q.Color if q.Color is not None and q.Color >= 0 else None,
                "colorIndex": q.ColorTableIndex,
                "comment": q.Comment or "",
            })
        anlz_colors = waveform.hot_cue_colors(ext)
        for c in cues:
            if c["kind"] == "hot":
                c["rgb"] = anlz_colors.get((c["slot"], c["start"]))
        cues.sort(key=lambda c: (c["kind"] != "hot", c["slot"], c["start"]))
        return cues

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
        names: dict[tuple, str] = {}

        def named_id(table, add: str, name: str) -> str | None:
            if not name:
                return None
            key = (table, name)
            if key not in names:
                row = db.query(table).filter_by(Name=name).first()
                names[key] = (row or getattr(db, add)(name)).ID
            return names[key]

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
