"""Read-only access to a rekordbox 6/7 library.

The live ``master.db`` (plus its WAL/SHM files) is copied into a private snapshot
directory and only that copy is ever opened, so the real library is never touched
and rekordbox can keep running.
"""

import logging
import shutil
import tempfile
import time
from collections import defaultdict
from pathlib import Path

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
HIDDEN_PLAYLISTS = {"100000"}  # Cloud Library Sync


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


class Library:
    def __init__(self, db_path: Path | None = None):
        self.src = Path(db_path) if db_path else _rekordbox_db_path()
        self.db_dir = self.src.parent
        self.share_dir = self.db_dir / "share"
        self.db: Rekordbox6Database | None = None
        self._snapshot_dir: Path | None = None
        self.snapshot_signature: tuple | None = None
        self.snapshot_time: float = 0
        self._content_cache: dict[str, dict] = {}

    # -- lifecycle -------------------------------------------------------------------

    def open(self) -> None:
        last_err: Exception | None = None
        for _ in range(3):
            try:
                self._open_snapshot()
                return
            except Exception as e:  # torn copy while rekordbox was writing; retry
                last_err = e
                log.warning("Snapshot open failed, retrying: %s", e)
                time.sleep(0.3)
        raise RuntimeError(f"Could not open rekordbox library: {last_err}")

    def _open_snapshot(self) -> None:
        self.close()
        signature = _source_signature(self.src)
        snap = Path(tempfile.mkdtemp(prefix="clubready-"))
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
        self._snapshot_dir = snap
        self.snapshot_signature = signature
        self.snapshot_time = time.time()
        self._content_cache.clear()

    def close(self) -> None:
        if self.db is not None:
            try:
                self.db.close()
                self.db.engine.dispose()
            except Exception:
                pass
            self.db = None
        if self._snapshot_dir is not None:
            shutil.rmtree(self._snapshot_dir, ignore_errors=True)
            self._snapshot_dir = None

    def source_changed(self) -> bool:
        return self.snapshot_signature is not None and _source_signature(self.src) != self.snapshot_signature

    def _q(self, *entities):
        assert self.db is not None, "Library not open"
        return self.db.session.execute(select(*entities))

    # -- queries -------------------------------------------------------------------

    def info(self) -> dict:
        return {
            "dbPath": str(self.src),
            "snapshotTime": self.snapshot_time,
            "rekordboxRunning": bool(get_rekordbox_pid()),
            "sourceChanged": self.source_changed(),
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
        counts: dict[str, int] = defaultdict(int)
        for (pid,) in self._q(SP.PlaylistID):
            counts[pid] += 1
        out = []
        for pid, name, parent, seq, attr in self._q(P.ID, P.Name, P.ParentID, P.Seq, P.Attribute).all():
            if pid in HIDDEN_PLAYLISTS:
                continue
            kind = {0: "playlist", 1: "folder", 4: "smart"}.get(attr, "playlist")
            out.append({
                "id": pid, "name": name or "", "parentId": parent or "root", "seq": seq or 0,
                "kind": kind, "count": counts.get(pid, 0) if kind == "playlist" else None,
            })
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
        counts: dict[str, int] = defaultdict(int)
        for (hid,) in self._q(SH.HistoryID):
            counts[hid] += 1
        return [
            {"id": hid, "name": name or "", "parentId": parent or "root", "seq": seq or 0,
             "kind": "folder" if attr == 1 else "playlist", "date": created or "",
             "count": counts.get(hid, 0) if attr != 1 else None}
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
            {"id": mid, "name": name or "", "parentId": parent or "root", "seq": seq or 0,
             "kind": "folder" if attr == 1 else "tag"}
            for mid, name, parent, seq, attr in self._q(M.ID, M.Name, M.ParentID, M.Seq, M.Attribute).all()
        ]

    # -- per-track detail ----------------------------------------------------------

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
        anlz_colors = waveform.hot_cue_colors(anlz["EXT"])
        for c in cues:
            if c["kind"] == "hot":
                c["rgb"] = anlz_colors.get((c["slot"], c["start"]))
        cues.sort(key=lambda c: (c["kind"] != "hot", c["slot"], c["start"]))
        path = Path(content.FolderPath or "")
        rgb = waveform.decode_rgb(anlz["EXT"])
        return {
            "id": content.ID,
            "fileExists": path.is_file(),
            "cues": cues,
            "beatgrid": waveform.decode_beatgrid(anlz["DAT"]),
            "waveform": {
                "rate": waveform.DETAIL_RATE,
                "rgb": rgb,
                "threeBand": waveform.decode_three_band(anlz["2EX"]),
                "mono": None if rgb else waveform.decode_mono(anlz["EXT"]),
            },
        }

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
