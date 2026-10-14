"""The playlists a stick was last synced with, as library playlist ids.

The exporter keeps a playlist that this library previously wrote, and drops it
when a later sync leaves it out. The sync window has to start from that set.
"""

import json
import xml.etree.ElementTree as ET
from pathlib import Path

from .paths import pioneer_dir


def library_db_id(library) -> int:
    """`djmdProperty.DBID` for the open library, or 0 when it has none."""
    from pyrekordbox.db6 import tables as t

    prop = library.db.query(t.DjmdProperty).first() if library.db is not None else None
    raw = getattr(prop, "DBID", None) if prop is not None else None
    try:
        number = int(raw)
    except (TypeError, ValueError):
        return 0
    return number if number > 0 else 0


def recorded_playlist_ids(volume: Path, library_ids: list[str], db_id: int) -> list[str]:
    """Library playlist ids the stick already records, in `library_ids` order.

    A missing or unreadable record contributes nothing. Sync-record ticks count
    only when the file's library id is `db_id`.
    """
    root, _name = pioneer_dir(volume)
    wanted = _manifest_ids(root) | _sync_ids(root, db_id)
    chosen = []
    for item in library_ids:
        number = _library_number(item)
        if number and number in wanted:
            chosen.append(item)
    return chosen


def _library_number(value: str) -> int:
    try:
        number = int(value)
    except (TypeError, ValueError):
        return 0
    return number if number > 0 else 0


def _manifest_ids(root: Path) -> set[int]:
    path = root / "exportstudio" / "manifest.json"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return set()
    rows = data.get("playlists") if isinstance(data, dict) else None
    if not isinstance(rows, list):
        return set()
    ids: set[int] = set()
    for row in rows:
        if not isinstance(row, dict) or row.get("device_only"):
            continue
        try:
            number = int(row.get("library_id") or 0)
        except (TypeError, ValueError):
            continue
        if number > 0:
            ids.add(number)
    return ids


def _sync_ids(root: Path, db_id: int) -> set[int]:
    if db_id <= 0:
        return set()
    for name in ("playlists3.sync", "playlists3Plus.sync"):
        path = root / "rekordbox" / name
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            continue
        parsed = _parse_sync(text, db_id)
        if parsed is not None:
            return parsed
    return set()


def _parse_sync(text: str, db_id: int) -> set[int] | None:
    """Ticked playlist ids, or None when the file is not a sync record.

    An empty set means the file belongs to another library.
    """
    try:
        root = ET.fromstring(text)
    except ET.ParseError:
        return None
    try:
        file_db = int(root.attrib.get("DBID", "0"))
    except (TypeError, ValueError):
        return None
    if file_db != db_id:
        return set()
    ids: set[int] = set()
    for node in root.iter("NODE"):
        if node.attrib.get("Attribute") == "1" or node.attrib.get("CheckType") != "1":
            continue
        try:
            number = int(node.attrib.get("Id", ""), 16)
        except (TypeError, ValueError):
            continue
        if number > 0:
            ids.add(number)
    return ids
