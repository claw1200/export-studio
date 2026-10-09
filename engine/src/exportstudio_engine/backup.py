"""Zip backups of the rekordbox library: the database, playlist XML files and the share folder
(analysis files and artwork). Audio files are never included."""

import json
import logging
import os
import shutil
import tempfile
import time
import zipfile
from datetime import datetime
from pathlib import Path
from typing import Callable

from pyrekordbox import Rekordbox6Database
from pyrekordbox.db6 import tables as t
from pyrekordbox.utils import get_rekordbox_pid
from sqlalchemy import func, select, text

log = logging.getLogger(__name__)

BACKUP_DIR = Path.home() / ".exportstudio" / "backups"
DB_ARC = "master.db"
DB_SUFFIXES = ("", "-wal")
SIDE_FILES = ("masterPlaylists6.xml", "automixPlaylist6.xml")
SHARE = Path("share") / "PIONEER"
MANIFEST = "manifest.json"
SETTINGS = "settings.json"
PARTIAL = ".partial"
AUTO_REASONS = {"before-edit", "daily"}
REASONS = AUTO_REASONS | {"manual", "pre-restore"}
KEEP_CHOICES = (5, 10, 20)
DEFAULT_SETTINGS = {"auto": True, "keep": 10}
STORED_SUFFIXES = {".jpg", ".jpeg", ".png", ".webp"}
COPY_ATTEMPTS = 3
FORMAT_VERSION = 1
BLOCKED = "Close rekordbox before restoring a backup"

Progress = Callable[[dict], None]
Verify = Callable[[Path], dict]


def verify_rekordbox(db_path: Path) -> dict:
    """Open a copy of a rekordbox database, check it and count what is in it."""
    db = Rekordbox6Database(path=db_path)
    try:
        rows = db.session.execute(text("PRAGMA quick_check")).fetchall()
        if [r[0] for r in rows] != ["ok"]:
            raise ValueError(f"The database is damaged: {rows[0][0] if rows else 'no result'}")
        C = t.DjmdContent
        tracks = db.session.execute(select(func.count()).select_from(C).where(C.rb_local_deleted == 0)).scalar()
        playlists = db.session.execute(select(func.count()).select_from(t.DjmdPlaylist)).scalar()
        return {"tracks": int(tracks or 0), "playlists": int(playlists or 0)}
    finally:
        try:
            db.close()
            db.engine.dispose()
        except Exception:
            pass


def _share_files(db_dir: Path) -> list[Path]:
    root = db_dir / SHARE
    if not root.is_dir():
        return []
    return sorted(p for p in root.rglob("*") if p.is_file() and not p.is_symlink())


def signature(src: Path) -> list:
    """Changes whenever the database, the side files or anything in the share folder changes."""
    sig = []
    for name in [src.name + suf for suf in DB_SUFFIXES] + list(SIDE_FILES):
        try:
            st = (src.parent / name).stat()
            sig.append([name, st.st_mtime_ns, st.st_size])
        except FileNotFoundError:
            sig.append([name, None, None])
    count = size = newest = 0
    for path in _share_files(src.parent):
        st = path.stat()
        count += 1
        size += st.st_size
        newest = max(newest, st.st_mtime_ns)
    sig.append(["share", count, size, newest])
    return sig


def _safe_member(name: str) -> bool:
    path = Path(name)
    if path.is_absolute() or ".." in path.parts or "\\" in name:
        return False
    return (
        name == MANIFEST
        or name in {DB_ARC + suf for suf in DB_SUFFIXES}
        or name in SIDE_FILES
        or (len(path.parts) > 2 and Path(*path.parts[:2]) == SHARE)
    )


def _move(path: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(path), str(target))


class Backups:
    def __init__(
        self,
        root: Path = BACKUP_DIR,
        verify: Verify = verify_rekordbox,
        rekordbox_running: Callable[[], object] = get_rekordbox_pid,
    ):
        self.root = Path(root)
        self.verify = verify
        self.rekordbox_running = rekordbox_running

    def settings(self) -> dict:
        try:
            stored = json.loads((self.root / SETTINGS).read_text())
        except (FileNotFoundError, ValueError):
            stored = {}
        out = dict(DEFAULT_SETTINGS)
        if isinstance(stored.get("auto"), bool):
            out["auto"] = stored["auto"]
        if stored.get("keep") in KEEP_CHOICES:
            out["keep"] = stored["keep"]
        return out

    def set_settings(self, auto: bool | None = None, keep: int | None = None) -> dict:
        out = self.settings()
        if auto is not None:
            out["auto"] = bool(auto)
        if keep is not None:
            if keep not in KEEP_CHOICES:
                raise ValueError(f"Keep must be one of {', '.join(map(str, KEEP_CHOICES))}")
            out["keep"] = keep
        self.root.mkdir(parents=True, exist_ok=True)
        (self.root / SETTINGS).write_text(json.dumps(out))
        self._prune(out["keep"])
        return out

    def list_backups(self, src: Path | None = None) -> list[dict]:
        out = []
        if not self.root.is_dir():
            return out
        for path in self.root.glob("*.zip"):
            try:
                with zipfile.ZipFile(path) as zf:
                    manifest = json.loads(zf.read(MANIFEST))
            except (zipfile.BadZipFile, KeyError, ValueError, OSError) as e:
                log.warning("Skipping unreadable backup %s: %s", path.name, e)
                continue
            manifest.update(id=path.stem, path=str(path), size=path.stat().st_size)
            manifest["otherLibrary"] = src is not None and manifest.get("dbPath") != str(src)
            out.append(manifest)
        out.sort(key=lambda m: m.get("created", 0), reverse=True)
        return out

    def latest_automatic(self) -> dict | None:
        return next((m for m in self.list_backups() if m.get("reason") in AUTO_REASONS), None)

    def create(self, src: Path, reason: str, on_progress: Progress | None = None) -> dict | None:
        """Back up the library at `src`. Automatic backups are skipped (None) when nothing changed."""
        if reason not in REASONS:
            raise ValueError(f"Unknown backup reason: {reason}")
        src = Path(src)
        if not src.is_file():
            raise FileNotFoundError(f"Can't find the rekordbox library at {src}")
        sig = signature(src)
        if reason in AUTO_REASONS:
            latest = next(iter(self.list_backups()), None)
            if latest is not None and latest.get("signature") == sig:
                return None
        progress = on_progress or (lambda _event: None)
        progress({"stage": "checking", "reason": reason, "done": 0, "total": 0})

        snap = Path(tempfile.mkdtemp(prefix="exportstudio-backup-"))
        try:
            counts = self._copy_verified(src, snap)
            entries = [(snap / (src.name + suf), DB_ARC + suf) for suf in DB_SUFFIXES]
            entries = [(p, arc) for p, arc in entries if p.exists()]
            entries += [(src.parent / n, n) for n in SIDE_FILES if (src.parent / n).is_file()]
            entries += [(p, p.relative_to(src.parent).as_posix()) for p in _share_files(src.parent)]
            manifest = {
                "version": FORMAT_VERSION,
                "reason": reason,
                "dbPath": str(src),
                "verified": True,
                "signature": sig,
                **counts,
            }
            result = self._write(entries, manifest, reason, progress)
        finally:
            shutil.rmtree(snap, ignore_errors=True)
        if reason in AUTO_REASONS:
            self._prune(self.settings()["keep"])
        return result

    def _copy_verified(self, src: Path, snap: Path) -> dict:
        last_err: Exception | None = None
        for attempt in range(COPY_ATTEMPTS):
            for suf in ("", "-wal", "-shm"):
                (snap / (src.name + suf)).unlink(missing_ok=True)
            for suf in DB_SUFFIXES:
                p = src.with_name(src.name + suf)
                if p.exists():
                    shutil.copy2(p, snap / p.name)
            try:
                return self.verify(snap / src.name)
            except Exception as e:
                last_err = e
                log.warning("Backup copy failed its check (attempt %d): %s", attempt + 1, e)
                time.sleep(0.3)
        raise RuntimeError(f"The library could not be backed up because it failed its check: {last_err}")

    def _write(self, entries: list[tuple[Path, str]], manifest: dict, reason: str, progress: Progress) -> dict:
        self.root.mkdir(parents=True, exist_ok=True)
        now = datetime.now()
        stem = f"exportstudio-{now:%Y%m%d-%H%M%S}-{reason}"
        n = 1
        while (self.root / f"{stem}.zip").exists():
            n += 1
            stem = f"exportstudio-{now:%Y%m%d-%H%M%S}-{reason}-{n}"
        final = self.root / f"{stem}.zip"
        partial = final.with_name(final.name + PARTIAL)
        manifest = {
            **manifest,
            "created": now.timestamp(),
            "createdAt": now.isoformat(timespec="seconds"),
            "files": len(entries),
            "bytes": sum(p.stat().st_size for p, _ in entries),
        }
        total = len(entries)
        try:
            with zipfile.ZipFile(partial, "w", zipfile.ZIP_DEFLATED, compresslevel=1) as zf:
                for done, (path, arc) in enumerate(entries, 1):
                    stored = path.suffix.lower() in STORED_SUFFIXES
                    zf.write(path, arc, compress_type=zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED)
                    if done % 50 == 0 or done == total:
                        progress({"stage": "saving", "reason": reason, "done": done, "total": total})
                zf.writestr(MANIFEST, json.dumps(manifest, indent=2))
            os.replace(partial, final)
        except BaseException:
            partial.unlink(missing_ok=True)
            raise
        return {**manifest, "id": stem, "path": str(final), "size": final.stat().st_size, "otherLibrary": False}

    def _prune(self, keep: int) -> None:
        automatic = [m for m in self.list_backups() if m.get("reason") in AUTO_REASONS]
        for manifest in automatic[keep:]:
            Path(manifest["path"]).unlink(missing_ok=True)

    def _path(self, backup_id: str) -> Path:
        path = self.root / f"{backup_id}.zip"
        if Path(backup_id).name != backup_id or not path.is_file():
            raise KeyError(f"Unknown backup {backup_id}")
        return path

    def delete(self, backup_id: str) -> None:
        self._path(backup_id).unlink()

    def restore(self, backup_id: str, src: Path, on_progress: Progress | None = None) -> dict:
        """Put a backup in place of the library at `src`. The files it replaces are kept as a
        pre-restore backup, or in a `replaced-*` folder if that can't be written."""
        if self.rekordbox_running():
            raise RuntimeError(BLOCKED)
        progress = on_progress or (lambda _event: None)
        path = self._path(backup_id)
        src = Path(src)
        db_dir = src.parent
        db_dir.mkdir(parents=True, exist_ok=True)
        progress({"stage": "checking", "reason": "restore", "done": 0, "total": 0})

        with zipfile.ZipFile(path) as zf:
            bad = zf.testzip()
            if bad is not None:
                raise ValueError(f"The backup is damaged ({bad})")
            names = [n for n in zf.namelist() if not n.endswith("/")]
            unsafe = [n for n in names if not _safe_member(n)]
            if unsafe:
                raise ValueError(f"The backup contains an unexpected file: {unsafe[0]}")
            if DB_ARC not in names:
                raise ValueError("The backup has no database in it")
            stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
            staging = db_dir / f".exportstudio-restore-{stamp}"
            staging.mkdir()
            try:
                members = [n for n in names if n != MANIFEST]
                for done, name in enumerate(members, 1):
                    zf.extract(name, staging)
                    if done % 50 == 0 or done == len(members):
                        progress({"stage": "extracting", "reason": "restore", "done": done, "total": len(members)})
                check = Path(tempfile.mkdtemp(prefix="exportstudio-check-"))
                try:
                    for suf in DB_SUFFIXES:
                        p = staging / (DB_ARC + suf)
                        if p.exists():
                            shutil.copy2(p, check / (src.name + suf))
                    counts = self.verify(check / src.name)
                finally:
                    shutil.rmtree(check, ignore_errors=True)
                replaced = self._swap(staging, src, stamp)
            finally:
                shutil.rmtree(staging, ignore_errors=True)

        kept = self._keep_replaced(replaced, src)
        return {"restored": backup_id, "kept": kept, **counts}

    def _swap(self, staging: Path, src: Path, stamp: str) -> Path:
        db_dir = src.parent
        replaced = self.root / f"replaced-{stamp}"
        targets = [(src.name + suf, DB_ARC + suf) for suf in DB_SUFFIXES]
        targets += [(n, n) for n in SIDE_FILES]
        targets.append((str(SHARE), str(SHARE)))
        moved_out: list[str] = []
        moved_in: list[str] = []
        try:
            shm = db_dir / (src.name + "-shm")
            if shm.exists():
                _move(shm, replaced / shm.name)
                moved_out.append(shm.name)
            for live, _ in targets:
                if (db_dir / live).exists():
                    _move(db_dir / live, replaced / live)
                    moved_out.append(live)
            for live, arc in targets:
                if (staging / arc).exists():
                    _move(staging / arc, db_dir / live)
                    moved_in.append(live)
        except Exception:
            log.exception("Restore failed, putting the library back")
            for live in moved_in:
                target = db_dir / live
                if target.is_dir():
                    shutil.rmtree(target, ignore_errors=True)
                else:
                    target.unlink(missing_ok=True)
            for live in moved_out:
                _move(replaced / live, db_dir / live)
            shutil.rmtree(replaced, ignore_errors=True)
            raise
        return replaced

    def _keep_replaced(self, replaced: Path, src: Path) -> str | None:
        if not replaced.is_dir():
            return None
        db = replaced / src.name
        entries = [(replaced / (src.name + suf), DB_ARC + suf) for suf in DB_SUFFIXES]
        entries = [(p, arc) for p, arc in entries if p.exists()]
        entries += [(replaced / n, n) for n in SIDE_FILES if (replaced / n).is_file()]
        entries += [(p, p.relative_to(replaced).as_posix()) for p in _share_files(replaced)]
        if not db.exists() or not entries:
            shutil.rmtree(replaced, ignore_errors=True)
            return None
        manifest = {"version": FORMAT_VERSION, "reason": "pre-restore", "dbPath": str(src), "verified": False}
        try:
            result = self._write(entries, manifest, "pre-restore", lambda _event: None)
        except Exception:
            log.exception("Could not zip the replaced library, leaving it in %s", replaced)
            return str(replaced)
        shutil.rmtree(replaced, ignore_errors=True)
        return result["path"]
