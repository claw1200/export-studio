"""Newline-delimited JSON-RPC over stdin/stdout.

Request:  {"id": 1, "method": "library.tracks", "params": {...}}
Response: {"id": 1, "result": ...} | {"id": 1, "error": {"message": ...}}
Event:    {"event": "library.changed", "data": {...}}
"""

import json
import logging
import sys
import threading
import traceback

from .library import Library

log = logging.getLogger("clubready_engine")

_out = sys.stdout
_out_lock = threading.Lock()


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
        self._lock = threading.Lock()
        self.methods = {
            "library.open": self.open,
            "library.reload": self.reload,
            "library.info": lambda: self.lib.info(),
            "library.tracks": lambda: self.lib.tracks(),
            "library.playlists": lambda: self.lib.playlists(),
            "library.histories": lambda: self.lib.histories(),
            "library.myTags": lambda: self.lib.my_tags(),
            "playlist.tracks": lambda id: self.lib.playlist_tracks(id),
            "history.tracks": lambda id: self.lib.history_tracks(id),
            "track.detail": lambda id: self.lib.track_detail(id),
            "media.resolve": lambda kind, id, size="": self.lib.resolve_media(kind, id, size),
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
        self.lib.open()
        emit("library.reloaded")
        return self.lib.info()

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
        while not stop.wait(2.0):
            lib = self.library
            if lib is None or lib.db is None:
                continue
            changed = lib.source_changed()
            if changed and not notified:
                emit("library.changed", {})
            notified = changed


def main() -> None:
    # stdout is reserved for the protocol; anything else that prints goes to stderr
    sys.stdout = sys.stderr
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
