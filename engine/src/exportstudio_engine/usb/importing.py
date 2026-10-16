"""Copy a stick's playlists into the rekordbox library.

File placement follows `rbl_export::device_import`: copies land under
`Music/PioneerDJ/Imported from Device/Contents/<Artist>/<Album>/<file>`,
an identical copy is reused, and a file already at that path is never
replaced. Tracks this library already exported (their master content id)
are reused without a copy.
"""

import logging
import os
import unicodedata
from collections import defaultdict
from pathlib import Path

from pyrekordbox.anlz import structs

from .. import waveform
from ..library import WRITE_BLOCKED
from .device import DeviceLibrary, DevicePlaylist, DeviceTrack
from .paths import volume_path

log = logging.getLogger(__name__)

DIR_NAME_MAX = 48
FILE_NAME_MAX = 120
IMPORTED_FROM_DEVICE = ("PioneerDJ", "Imported from Device")


def destination(music: Path) -> Path:
    root = music
    for part in IMPORTED_FROM_DEVICE:
        root = root / part
    return root


def fat_safe(name: str) -> str:
    text = unicodedata.normalize("NFC", name)
    out = []
    for char in text:
        if char in '/\\:*?"<>|' or ord(char) < 0x20:
            out.append("_")
        else:
            out.append(char)
    cleaned = "".join(out)
    while cleaned.endswith(".") or cleaned.endswith(" "):
        cleaned = cleaned[:-1]
    return cleaned or "Unknown"


def dir_name(name: str) -> str:
    safe = fat_safe(name)
    if len(safe) <= DIR_NAME_MAX:
        return safe
    cut = safe[:DIR_NAME_MAX]
    while cut.endswith(".") or cut.endswith(" "):
        cut = cut[:-1]
    return cut or "Unknown"


def fat_file_name(name: str) -> str:
    safe = fat_safe(name)
    if len(safe.encode("utf-8")) <= FILE_NAME_MAX:
        return safe
    stem, dot, extension = safe.rpartition(".")
    suffix = f".{extension}" if dot and stem and len(extension.encode("utf-8")) < FILE_NAME_MAX else ""
    body = stem if suffix else safe
    end = FILE_NAME_MAX - len(suffix.encode("utf-8"))
    raw = body.encode("utf-8")[: max(0, end)].decode("utf-8", "ignore")
    while raw.endswith(".") or raw.endswith(" "):
        raw = raw[:-1]
    return f"{raw or 'Unknown'}{suffix}"


def numbered(file_name: str, n: int) -> str:
    limit = 48
    stem, dot, extension = file_name.rpartition(".")
    if not dot:
        stem, extension = file_name, ""
    else:
        extension = f".{extension}"
    suffix = f"-{n}"
    length = len(file_name) + len(suffix)
    if length > limit:
        keep = max(0, limit - len(suffix) - len(extension))
        stem = stem[:keep]
    return f"{stem}{suffix}{extension}"


def candidate(root: Path, artist: str, album: str, file_name: str, *, contents: bool = True) -> Path:
    artist_name = dir_name(artist.strip() or "UnknownArtist")
    album_name = dir_name(album.strip() or "UnknownAlbum")
    base = root / "Contents" if contents else root
    return base / artist_name / album_name / fat_file_name(file_name)


def _same_bytes(left: Path, right: Path) -> bool:
    try:
        if left.stat().st_size != right.stat().st_size:
            return False
    except OSError:
        return False
    with left.open("rb") as src, right.open("rb") as dst:
        while True:
            a = src.read(1024 * 1024)
            b = dst.read(1024 * 1024)
            if a != b:
                return False
            if not a:
                return True


def existing_copy(source: Path, wanted: Path) -> Path | None:
    parent = wanted.parent
    name = wanted.name
    if parent is None or not name:
        return None
    for n in range(10_000):
        at = parent / (name if n == 0 else numbered(name, n))
        if not at.exists():
            return None
        if at.is_file() and _same_bytes(source, at):
            return at
    return None


def copy_new(source: Path, wanted: Path) -> Path:
    """Copy `source` beside `wanted`, never replacing a file. The copy is checked byte for byte."""
    parent = wanted.parent
    name = wanted.name
    if parent is None or not name:
        raise ValueError("a copy needs a folder")
    parent.mkdir(parents=True, exist_ok=True)
    for n in range(10_000):
        target = parent / (name if n == 0 else numbered(name, n))
        try:
            fd = os.open(target, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            continue
        try:
            with source.open("rb") as src, os.fdopen(fd, "wb") as dst:
                while True:
                    block = src.read(1024 * 1024)
                    if not block:
                        break
                    dst.write(block)
                dst.flush()
                os.fsync(dst.fileno())
            if not _same_bytes(source, target):
                raise OSError(f"the copy of {source} did not match the device's file")
            return target
        except Exception:
            target.unlink(missing_ok=True)
            raise
    raise FileExistsError(f"no free name for {wanted}")


def place_copy(source: Path, wanted: Path) -> tuple[Path, bool]:
    """Return `(path, created)`. An identical earlier copy is reused."""
    found = existing_copy(source, wanted)
    if found is not None:
        return found, False
    return copy_new(source, wanted), True


def _content_id(library, master_id: str | None) -> str | None:
    if not master_id or getattr(library, "db", None) is None:
        return None
    from pyrekordbox.db6 import tables as t

    row = library.db.query(t.DjmdContent).filter(t.DjmdContent.ID == str(master_id)).one_or_none()
    if row is None or getattr(row, "rb_local_deleted", 0):
        return None
    path = Path(getattr(row, "FolderPath", "") or "")
    if path.is_file():
        return str(row.ID)
    return None


def _audio(volume: Path, track: DeviceTrack) -> Path | None:
    if not track.path:
        return None
    try:
        path = volume_path(track.path, volume)
    except ValueError:
        return None
    return path if path.is_file() else None


def _custom_folder(folder: str | Path | None) -> Path | None:
    if not folder:
        return None
    path = Path(folder).expanduser()
    if not path.is_absolute():
        raise ValueError("Choose a folder to import into")
    return path


def _children(device: DeviceLibrary) -> dict[int, list[DevicePlaylist]]:
    children: dict[int, list[DevicePlaylist]] = defaultdict(list)
    known = {row.export_id for row in device.playlists}
    for row in device.playlists:
        parent = row.parent_id if row.parent_id in known else 0
        children[parent].append(row)
    for rows in children.values():
        rows.sort(key=lambda row: (row.seq, row.export_id))
    return children


def _ordered_tracks(children: dict[int, list[DevicePlaylist]], by_id: dict[int, DeviceTrack]) -> list[int]:
    seen: list[int] = []
    stack = list(reversed(children.get(0, [])))
    while stack:
        row = stack.pop()
        if row.folder:
            stack.extend(reversed(children.get(row.export_id, [])))
        for track_id in row.track_ids:
            if track_id not in seen and track_id in by_id:
                seen.append(track_id)
    return seen


def preview_import(library, volume: Path, device: DeviceLibrary, *, folder: str | Path | None = None) -> dict:
    """What `import_library` would do, without touching the library or copying anything."""
    custom = _custom_folder(folder)
    root = custom if custom is not None else destination(Path.home() / "Music")
    by_id = {track.export_id: track for track in device.tracks}
    children = _children(device)
    nodes = library.playlists()

    tracks = []
    for track_id in _ordered_tracks(children, by_id):
        track = by_id[track_id]
        if _content_id(library, track.master_id):
            status = "reuse"
        elif _audio(volume, track) is None:
            status = "missing"
        else:
            status = "copy"
        tracks.append(
            {
                "title": track.title or track.filename or "Untitled",
                "artist": track.artist,
                "status": status,
            }
        )

    playlists = []

    def walk(parent_export: int, parent: str | None, depth: int) -> None:
        for row in children.get(parent_export, []):
            name = row.name.strip() or "Untitled"
            kind = "folder" if row.folder else "playlist"
            found = None
            if parent is not None:
                found = next(
                    (n for n in nodes if n["parentId"] == parent and n["kind"] == kind and (n["name"] or "Untitled") == name),
                    None,
                )
            playlists.append(
                {
                    "name": name,
                    "folder": row.folder,
                    "depth": depth,
                    "tracks": 0 if row.folder else len([t for t in row.track_ids if t in by_id]),
                    "exists": found is not None,
                }
            )
            if row.folder:
                walk(row.export_id, found["id"] if found else None, depth + 1)

    walk(0, "root", 0)
    counts = {status: sum(1 for t in tracks if t["status"] == status) for status in ("reuse", "copy", "missing")}
    return {
        "destination": str(root),
        "layout": "Artist/Album" if custom is not None else "Contents/Artist/Album",
        "playlists": playlists,
        "tracks": tracks,
        "counts": counts,
    }


def _analysis_bytes(volume: Path, track: DeviceTrack) -> dict[str, bytes]:
    if not track.analysis:
        return {}
    try:
        base = volume_path(track.analysis, volume)
    except ValueError:
        return {}
    files = {}
    for kind in ("DAT", "EXT", "2EX"):
        path = base.with_suffix(f".{kind}")
        if path.is_file():
            files[kind] = path.read_bytes()
    return files


def _ext_cues(ext: bytes | None) -> list[dict]:
    """Cues from an ANLZ extended file, for sticks whose database holds none. Hot slot 1 is A."""
    if not ext:
        return []
    out = []
    for tag in waveform.tags_from_bytes(ext).get(waveform.HOT_CUE_TAG, []):
        try:
            entries = structs.AnlzTag.parse(bytes(tag)).content.entries
        except Exception:
            continue
        for entry in entries:
            slot = int(entry.hot_cue)
            loop = int(entry.type) == 2
            if slot:
                kind = slot if slot < 4 else slot + 1
            else:
                kind = 4 if loop else 0
            end = int(entry.loop_time)
            out.append(
                {
                    "kind": kind,
                    "start": int(entry.time),
                    "end": end if loop and end >= 0 else None,
                    "colorIndex": int(entry.color_id) or None,
                    "comment": entry.comment or "",
                    "activeLoop": False,
                }
            )
    return out


def _carry(library, volume: Path, track: DeviceTrack, content_id: str) -> None:
    """Copy the device's analysis (beatgrid, waveform), cues and track numbers onto a new library row."""
    anlz = _analysis_bytes(volume, track)
    cues = [
        {
            "kind": cue.kind,
            "start": cue.start_ms,
            "end": cue.end_ms,
            "colorIndex": cue.color_index,
            "comment": cue.comment,
            "activeLoop": cue.active_loop,
        }
        for cue in track.cues
    ] or _ext_cues(anlz.get("EXT"))
    fields = {"bpm": track.bpm_x100, "key": track.key, "rating": track.rating, "color": track.color}
    library.apply_device_data(content_id, anlz, fields, cues)


def import_device(
    library, volume: Path, *, music: Path | None = None, folder: str | Path | None = None, on_progress=None
) -> dict:
    """Recreate the stick's playlists in the open library."""
    from . import browse

    if not getattr(library, "writable", False):
        raise RuntimeError("Unlock editing to import tracks")
    if library.rekordbox_running():
        raise RuntimeError(WRITE_BLOCKED)

    device = browse.load(volume)
    return import_library(library, volume, device, music=music, folder=folder, on_progress=on_progress)


def import_library(
    library,
    volume: Path,
    device: DeviceLibrary,
    *,
    music: Path | None = None,
    folder: str | Path | None = None,
    on_progress=None,
) -> dict:
    if not any(not row.folder for row in device.playlists):
        raise ValueError("This USB has no playlists to import")
    custom = _custom_folder(folder)
    root = custom if custom is not None else destination(music if music is not None else Path.home() / "Music")
    by_id = {track.export_id: track for track in device.tracks}
    children = _children(device)

    nodes = library.playlists()
    failed: list[dict] = []
    copied = 0
    reused = 0
    created_playlists = 0
    resolved: dict[int, str] = {}

    def report(done: int, total: int, name: str) -> None:
        if on_progress is not None:
            on_progress({"phase": "import", "done": done, "total": total, "name": name})

    def content_for(track: DeviceTrack) -> str | None:
        nonlocal copied, reused
        existing = _content_id(library, track.master_id)
        if existing:
            reused += 1
            return existing
        audio = _audio(volume, track)
        if audio is None:
            failed.append({"title": track.title or track.filename or "Untitled", "error": "Audio file is missing on the USB"})
            return None
        file_name = track.filename or audio.name
        wanted = candidate(root, track.artist, track.album, file_name, contents=custom is None)
        try:
            placed, created = place_copy(audio, wanted)
        except OSError as err:
            failed.append({"title": track.title or file_name, "error": str(err)})
            return None
        try:
            result = library.import_tracks([str(placed)], None, True)
        except Exception:
            if created:
                placed.unlink(missing_ok=True)
            raise
        if result["failed"] or not result["ids"]:
            if created:
                placed.unlink(missing_ok=True)
            detail = result["failed"][0]["error"] if result["failed"] else "Couldn’t import the track"
            failed.append({"title": track.title or file_name, "error": detail})
            return None
        if created:
            copied += 1
        else:
            reused += 1
        content_id = str(result["ids"][0])
        if not result["skipped"]:
            try:
                _carry(library, volume, track, content_id)
            except Exception:
                log.exception("Could not carry the device data for %s", track.title)
        return content_id

    seen = _ordered_tracks(children, by_id)
    total = len(seen)
    for index, track_id in enumerate(seen):
        track = by_id[track_id]
        report(index, total, track.title or track.filename or "Untitled")
        content = content_for(track)
        if content:
            resolved[track_id] = content
    report(total, total, "")

    def find(parent: str, name: str, folder: bool) -> dict | None:
        kind = "folder" if folder else "playlist"
        return next(
            (
                node
                for node in nodes
                if node["parentId"] == parent and node["kind"] == kind and (node["name"] or "Untitled") == name
            ),
            None,
        )

    def ensure(row: DevicePlaylist, parent: str) -> str:
        nonlocal created_playlists, nodes
        name = row.name.strip() or "Untitled"
        found = find(parent, name, row.folder)
        if found is not None:
            playlist_id = found["id"]
        else:
            track_ids = [] if row.folder else [resolved[item] for item in row.track_ids if item in resolved]
            created = library.create_playlist(name, parent, row.folder, track_ids)
            playlist_id = created["id"]
            nodes = created["playlists"]
            created_playlists += 1
            return playlist_id
        if not row.folder:
            track_ids = [resolved[item] for item in row.track_ids if item in resolved]
            if track_ids:
                library.add_to_playlist(playlist_id, track_ids)
        return playlist_id

    def walk(parent_export: int, parent: str) -> None:
        for row in children.get(parent_export, []):
            playlist_id = ensure(row, parent)
            if row.folder:
                walk(row.export_id, playlist_id)

    walk(0, "root")
    return {
        "playlists": created_playlists,
        "tracks": len(resolved),
        "copied": copied,
        "reused": reused,
        "failed": failed,
    }
