import logging
import shutil
import tempfile
import time
from collections import defaultdict
from pathlib import Path
from uuid import uuid4

from pyrekordbox import Rekordbox6Database
from pyrekordbox.config import get_config
from pyrekordbox.db6 import tables as t
from pyrekordbox.utils import get_rekordbox_pid
from sqlalchemy import event, select
from sqlalchemy.orm import aliased

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

    def tracks(self) -> list[dict]:
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
        tags_by_track: dict[str, list[str]] = defaultdict(list)
        for content_id, tag_id in self._q(t.DjmdSongMyTag.ContentID, t.DjmdSongMyTag.MyTagID):
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

    def _anlz(self, content: t.DjmdContent) -> dict[str, waveform.Tags | None]:
        out: dict[str, waveform.Tags | None] = {"DAT": None, "EXT": None, "2EX": None}
        if not content.AnalysisDataPath:
            return out
        dat = self.share_dir / content.AnalysisDataPath.strip("/\\")
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

    def cues(self, content_id: str) -> list[dict]:
        content = self._content(content_id)
        return self._cues(content, self._anlz(content)["EXT"])

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
