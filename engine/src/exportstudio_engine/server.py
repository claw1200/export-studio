import json
import logging
import sys
import threading
import time
import traceback

from pathlib import Path

from . import tags as file_tags
from .backup import Backups
from .library import WRITE_BLOCKED, Library, _rekordbox_db_path
from .usb import browse
from .usb.selection import library_db_id, recorded_playlist_ids
from .usb.sync import bundle_from_library, sync_volume
from .usb.volumes import eject_volume, list_volumes

log = logging.getLogger("exportstudio_engine")

_out = sys.stdout
_out_lock = threading.Lock()

DAILY = 24 * 60 * 60
DAILY_CHECK_TICKS = 300


def _send(msg: dict) -> None:
    line = json.dumps(msg, separators=(",", ":"), default=str)
    with _out_lock:
        _out.write(line + "\n")
        _out.flush()


def emit(event: str, data: dict | None = None) -> None:
    _send({"event": event, "data": data or {}})


class Engine:
    def __init__(self) -> None:
        self.library: Library | None = None
        self.backups = Backups()
        self._backed_up = False
        self._lock = threading.Lock()
        self.methods = {
            "library.open": self.open,
            "library.reload": self.reload,
            "library.setWritable": lambda writable: self.set_writable(writable),
            "library.info": lambda: self.lib.info(),
            "library.tracks": lambda: self.lib.tracks(),
            "library.playlists": lambda: self.lib.playlists(),
            "library.histories": lambda: self.lib.histories(),
            "library.myTags": lambda: self.lib.my_tags(),
            "library.playlistMembers": lambda: self.lib.playlist_member_ids(),
            "files.read": lambda paths: file_tags.browse_many(paths),
            "playlist.tracks": lambda id: self.lib.playlist_tracks(id),
            "history.tracks": lambda id: self.lib.history_tracks(id),
            "track.detail": lambda id: self.track_detail(id),
            "track.previews": lambda ids: self.track_previews(ids),
            "cue.setHot": lambda id, kind, ms: self.lib.set_hot_cue(id, kind, ms),
            "cue.addMemory": lambda id, ms: self.lib.add_memory_cue(id, ms),
            "cue.delete": lambda id, cueId: self.lib.delete_cue(id, cueId),
            "cue.setColor": lambda id, cueId, colorIndex: self.lib.set_cue_color(id, cueId, colorIndex),
            "beatgrid.set": lambda id, times, beats, bpms, cues=None: self.lib.set_beatgrid(id, times, beats, bpms, cues),
            "track.update": lambda ids, fields, writeTags=False: self.lib.update_tracks(ids, fields, writeTags),
            "track.relocate": lambda moves: self.lib.relocate_tracks(moves),
            "track.findMissing": lambda ids, directory: self.lib.find_missing(ids, directory),
            "track.inspect": lambda tracks: self.lib.inspect_tracks(tracks),
            "track.import": lambda paths, playlistId=None, artwork=True: self.lib.import_tracks(paths, playlistId, artwork),
            "track.removeFromCollection": lambda ids: self.lib.remove_from_collection(ids),
            "track.applyAnalysis": lambda id, result, options: self.lib.apply_analysis(id, result, options),
            "playlist.add": lambda id, trackIds: self.lib.add_to_playlist(id, trackIds),
            "playlist.remove": lambda id, trackIds: self.lib.remove_from_playlist(id, trackIds),
            "playlist.move": lambda id, trackIds, index: self.lib.move_in_playlist(id, trackIds, index),
            "playlist.create": lambda name, parentId="root", folder=False, trackIds=None: self.lib.create_playlist(
                name, parentId, folder, trackIds
            ),
            "playlist.rename": lambda id, name: self.lib.rename_playlist(id, name),
            "playlist.delete": lambda id: self.lib.delete_playlist(id),
            "tag.add": lambda id, trackIds: self.lib.add_tag(id, trackIds),
            "tag.remove": lambda id, trackIds: self.lib.remove_tag(id, trackIds),
            "media.resolve": lambda kind, id, size="": self.resolve_media(kind, id, size),
            "usb.list": lambda: list_volumes(),
            "usb.eject": lambda volumeId: self.eject(volumeId),
            "usb.playlists": lambda volumeId: browse.playlists(Path(volumeId)),
            "usb.selection": lambda volumeId: self.selection(volumeId),
            "usb.tracks": lambda volumeId, playlistId=None: browse.tracks(Path(volumeId), playlistId),
            "usb.detail": lambda id: browse.detail(id),
            "usb.sync": lambda volumeId, playlistIds: self.sync(volumeId, playlistIds),
            "backup.list": lambda: self.backups.list_backups(self._source(required=False)),
            "backup.create": lambda: self.back_up("manual"),
            "backup.restore": lambda id: self.restore(id),
            "backup.delete": lambda id: self.delete_backup(id),
            "backup.settings": lambda: self.backups.settings(),
            "backup.setSettings": lambda auto=None, keep=None: self.backups.set_settings(auto, keep),
        }

    @property
    def lib(self) -> Library:
        if self.library is None or self.library.db is None:
            raise RuntimeError("Library not open")
        return self.library

    def open(self, dbPath: str | None = None) -> dict:
        if self.library is not None:
            self.library.close()
        self.library = Library(dbPath)
        self.library.open()
        return self.library.info()

    def reload(self) -> dict:
        self.lib.reopen()
        emit("library.reloaded")
        return self.lib.info()

    def set_writable(self, writable: bool) -> dict:
        lib = self.lib
        if writable and not lib.writable and not self._backed_up and self.backups.settings()["auto"]:
            if lib.rekordbox_running():
                raise RuntimeError(WRITE_BLOCKED)
            self.back_up("before-edit")
            self._backed_up = True
        lib.set_writable(bool(writable))
        return lib.info()

    def _source(self, required: bool = True) -> Path | None:
        if self.library is not None:
            return self.library.src
        try:
            return _rekordbox_db_path()
        except FileNotFoundError:
            if required:
                raise
            return None

    def back_up(self, reason: str) -> dict | None:
        try:
            result = self.backups.create(self._source(), reason, on_progress=lambda e: emit("backup.progress", e))
        finally:
            emit("backup.progress", {"stage": "done", "reason": reason})
        if result is not None:
            emit("backup.changed")
        return result

    def delete_backup(self, backup_id: str) -> list[dict]:
        self.backups.delete(backup_id)
        return self.backups.list_backups(self._source(required=False))

    def restore(self, backup_id: str) -> dict:
        src = self._source()
        if self.library is not None:
            self.library.close()
        try:
            result = self.backups.restore(backup_id, src, on_progress=lambda e: emit("backup.progress", e))
        finally:
            emit("backup.progress", {"stage": "done", "reason": "restore"})
            emit("backup.changed")
            try:
                if self.library is None:
                    self.library = Library(src)
                self.library.open()
            except Exception:
                log.exception("Could not open the library after restoring")
        emit("library.reloaded")
        return result

    def _daily_due(self) -> bool:
        if not self.backups.settings()["auto"]:
            return False
        latest = self.backups.latest_automatic()
        return latest is None or time.time() - latest.get("created", 0) >= DAILY

    def track_detail(self, id: str) -> dict:
        if str(id).startswith("usb:"):
            return browse.detail(str(id))
        return self.lib.track_detail(id)

    def track_previews(self, ids: list[str]) -> list[dict]:
        chosen = list(dict.fromkeys(str(i) for i in ids))[:80]
        library_ids = [i for i in chosen if not i.startswith("usb:")]
        usb_ids = [i for i in chosen if i.startswith("usb:")]
        by_id = {}
        if library_ids:
            by_id.update({row["id"]: row for row in self.lib.track_previews(library_ids)})
        if usb_ids:
            by_id.update({row["id"]: row for row in browse.previews(usb_ids)})
        return [by_id[i] for i in chosen if i in by_id]

    def resolve_media(self, kind: str, id: str, size: str = "") -> str | None:
        if str(id).startswith("usb:"):
            return browse.resolve_media(kind, str(id))
        return self.lib.resolve_media(kind, id, size)

    def selection(self, volumeId: str) -> dict:
        ids = [row["id"] for row in self.lib.playlists()]
        return {"playlistIds": recorded_playlist_ids(Path(volumeId), ids, library_db_id(self.lib))}

    def sync(self, volumeId: str, playlistIds: list[str]) -> dict:
        volume = Path(volumeId).resolve()
        allowed = {Path(item["path"]).resolve() for item in list_volumes()}
        if volume not in allowed:
            raise ValueError("Choose a volume from the device list")
        bundle = bundle_from_library(self.lib, list(playlistIds))

        def progress(event: dict) -> None:
            emit("usb.progress", event)

        result = sync_volume(volume, bundle, on_progress=progress)
        emit("usb.volumes", {"volumes": list_volumes()})
        return result

    def eject(self, volumeId: str) -> None:
        eject_volume(volumeId)
        emit("usb.volumes", {"volumes": list_volumes()})

    def handle(self, msg: dict) -> None:
        req_id = msg.get("id")
        method = msg.get("method", "")
        params = msg.get("params") or {}
        fn = self.methods.get(method)
        try:
            if fn is None:
                raise ValueError(f"Unknown method: {method}")
            with self._lock:
                result = fn(**params) if isinstance(params, dict) else fn(*params)
            _send({"id": req_id, "result": result})
        except Exception as e:
            log.error("%s failed: %s\n%s", method, e, traceback.format_exc())
            _send({"id": req_id, "error": {"message": str(e), "type": type(e).__name__}})

    def watch(self, stop: threading.Event) -> None:
        notified = False
        running: bool | None = None
        volumes: tuple | None = None
        ticks = 0
        while not stop.wait(2.0):
            ticks += 1
            if ticks % DAILY_CHECK_TICKS == 1:
                with self._lock:
                    if self.library is not None and self.library.db is not None:
                        try:
                            if self._daily_due():
                                self.back_up("daily")
                        except Exception:
                            log.exception("Daily backup failed")
            try:
                current = tuple(
                    (item["id"], item["hasLibrary"], item["rootName"], item["ejectable"]) for item in list_volumes()
                )
            except Exception:
                log.exception("Failed to list volumes")
                current = volumes
            if volumes is not None and current != volumes:
                try:
                    emit("usb.volumes", {"volumes": list_volumes()})
                except Exception:
                    log.exception("Failed to publish the volume list")
            volumes = current
            with self._lock:
                lib = self.library
                if lib is None or lib.db is None:
                    continue
                now = lib.rekordbox_running()
                if lib.writable and now:
                    try:
                        lib.set_writable(False)
                        emit("library.locked", lib.info())
                    except Exception:
                        log.exception("Failed to leave write mode")
                    notified = False
                elif running is not None and now != running:
                    emit("library.rekordbox", {"running": now})
                running = now
                changed = lib.source_changed()
                if changed and not notified:
                    emit("library.changed", {})
                notified = changed


def _send_prints_to_stderr() -> None:
    sys.stdout = sys.stderr


def main() -> None:
    _send_prints_to_stderr()
    logging.basicConfig(
        stream=sys.stderr, level=logging.INFO, format="[engine] %(levelname)s %(name)s: %(message)s"
    )
    logging.getLogger("pyrekordbox").setLevel(logging.ERROR)

    engine = Engine()
    stop = threading.Event()
    threading.Thread(target=engine.watch, args=(stop,), daemon=True).start()
    emit("engine.ready", {"version": "0.1.0"})

    try:
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                log.error("Invalid JSON: %s", line[:200])
                continue
            engine.handle(msg)
    finally:
        stop.set()
        if engine.library is not None:
            engine.library.close()
