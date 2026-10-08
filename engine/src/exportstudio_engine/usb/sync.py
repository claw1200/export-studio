"""Incremental master-library to USB publication.

Audio, analysis, and both device databases are staged on the volume and
checked before anything replaces the live export. Device playlists, history,
and tracks that this app did not create are left in place.
"""

import hashlib
import json
import os
import shutil
import time
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path
from xml.sax.saxutils import escape

from . import browse
from .anlzio import rewrite_ppth
from .device import (
    CueRow,
    DeviceLibrary,
    DevicePlaylist,
    DeviceTrack,
    read_device,
    to_pdb,
    write_onelibrary,
)
from .onelib import connect as connect_library
from .paths import analysis_dir, audio_relative, inside, path_key, pioneer_dir, volume_path

STAGE = ".exportstudio-publication"
HIGH_ID = 2_000_000_000
ANLZ_KINDS = ("DAT", "EXT", "2EX")


class PublicationError(RuntimeError):
    pass


class Conflict(PublicationError):
    pass


class VerifyError(PublicationError):
    pass


@dataclass
class CueSource:
    kind: int
    start_ms: int
    end_ms: int | None = None
    color: int | None = None
    color_index: int | None = None
    comment: str = ""
    active_loop: bool = False
    beat_loop: int = 0


@dataclass
class SourceTrack:
    master_id: str
    title: str
    artist: str
    album: str
    path: Path
    analysis: Path | None
    filename: str
    bpm_x100: int = 12000
    duration: int = 1
    bitrate: int = 320
    sample_rate: int = 44100
    bit_depth: int = 0
    file_type: int = 1
    file_size: int = 0
    rating: int = 0
    color: int = 0
    comment: str = ""
    year: int = 0
    track_no: int = 0
    hot_cue_auto: bool = True
    date_added: str = ""
    genre: str = ""
    label: str = ""
    key: str = ""
    cues: list[CueSource] = field(default_factory=list)


@dataclass
class SourcePlaylist:
    master_id: str
    name: str
    parent_id: str | None
    seq: int
    folder: bool
    track_ids: list[str]


@dataclass
class ExportBundle:
    playlists: list[SourcePlaylist]
    tracks: list[SourceTrack]
    dbid: str = "0"
    settings_dir: Path | None = None


def _manifest_path(root: Path) -> Path:
    return root / "exportstudio" / "manifest.json"


def load_manifest(root: Path) -> dict:
    path = _manifest_path(root)
    if not path.is_file():
        return {"tracks": {}, "playlists": {}, "audio": [], "analysis": []}
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return {"tracks": {}, "playlists": {}, "audio": [], "analysis": []}
    data.setdefault("tracks", {})
    data.setdefault("playlists", {})
    data.setdefault("audio", [])
    data.setdefault("analysis", [])
    return data


def _signature(volume: Path) -> tuple:
    root, _name = pioneer_dir(volume)
    parts = []
    for rel in ("rekordbox/export.pdb", "rekordbox/exportLibrary.db", "exportstudio/manifest.json"):
        path = root / rel
        try:
            st = path.stat()
        except OSError:
            parts.append((rel, None, None))
        else:
            parts.append((rel, st.st_mtime_ns, st.st_size))
    return tuple(parts)


def _same_bytes(left: Path, right: Path) -> bool:
    try:
        if left.stat().st_size != right.stat().st_size:
            return False
    except OSError:
        return False
    with left.open("rb") as a, right.open("rb") as b:
        while True:
            chunk_a = a.read(1024 * 1024)
            chunk_b = b.read(1024 * 1024)
            if chunk_a != chunk_b:
                return False
            if not chunk_a:
                return True


def _allocate(preferred: int | None, used: set[int]) -> int:
    if preferred and 0 < int(preferred) < HIGH_ID and int(preferred) not in used:
        used.add(int(preferred))
        return int(preferred)
    candidate = 1
    while candidate in used:
        candidate += 1
        if candidate >= HIGH_ID:
            raise PublicationError("No free export id")
    used.add(candidate)
    return candidate


def _allocate_high(used: set[int]) -> int:
    candidate = HIGH_ID
    while candidate in used:
        candidate += 1
    used.add(candidate)
    return candidate


def _claim(rows, pinned: set[int], allocate_new) -> dict[int, int]:
    """Move device-owned ids out of the way of ids this sync is keeping."""
    renamed = {}
    for row in rows:
        if row.export_id in pinned:
            renamed[row.export_id] = allocate_new()
            row.export_id = renamed[row.export_id]
        else:
            pinned.add(row.export_id)
    return renamed


def _unique_audio(relative: str, export_id: int, used: set[str]) -> str:
    path = Path(relative)
    parent = path.parent.as_posix()
    if not parent.startswith("/"):
        parent = "/" + parent
    candidate = relative
    suffix = 0
    while path_key(candidate) in used:
        suffix += 1
        stem = f"{path.stem}-{export_id}" if suffix == 1 else f"{path.stem}-{export_id}-{suffix}"
        candidate = f"{parent}/{stem}{path.suffix}"
    used.add(path_key(candidate))
    return candidate


def _report(on_progress, phase: str, **info) -> None:
    if on_progress is not None:
        on_progress({"phase": phase, **info})


def _sync_document(playlists: list[SourcePlaylist], dbid: str, previous: str | None) -> str:
    stamps: dict[str, str] = {}
    if previous:
        import re

        for node_id, stamp in re.findall(r'Id="([^"]+)"[^>]*Timestamp="(\d+)"', previous):
            stamps.setdefault(node_id, stamp)
        for stamp, node_id in re.findall(r'Timestamp="(\d+)"[^>]*Id="([^"]+)"', previous):
            stamps.setdefault(node_id, stamp)
    now = str(int(time.time() * 1000))
    lines = ['<?xml version="1.0" encoding="UTF-8"?>', "", f'<Playlists DBID="{escape(dbid)}">']
    lines.append(
        f'  <Playlist Id="0" ParentId="0" Attribute="1" CheckType="2" Timestamp="{stamps.get("0", now)}"/>'
    )
    included = {row.master_id for row in playlists}
    for row in playlists:
        try:
            node = format(int(row.master_id), "X")
        except ValueError:
            continue
        parent = "0"
        if row.parent_id and row.parent_id in included:
            try:
                parent = format(int(row.parent_id), "X")
            except ValueError:
                parent = "0"
        attribute = "1" if row.folder else "0"
        check = "2" if row.folder else "1"
        lines.append(
            "  <Playlist "
            f'Id="{node}" ParentId="{parent}" Attribute="{attribute}" CheckType="{check}" '
            f'Timestamp="{stamps.get(node, now)}"/>'
        )
    lines.append("</Playlists>")
    return "\r\n".join(lines) + "\r\n"


def _settings_to_copy(source: Path, root: Path, volume: Path) -> list[tuple[Path, str]]:
    from pyrekordbox.mysettings import get_mysetting_paths

    found = list(get_mysetting_paths(source, deep=True))
    found.extend(source.rglob("djprofile.nxs"))
    copies = []
    seen = set()
    for path in found:
        if not path.is_file() or path.name in seen:
            continue
        seen.add(path.name)
        relative = f"/{root.name}/{path.name}"
        try:
            live = volume_path(relative, volume)
        except ValueError:
            continue
        if live.exists():
            continue
        copies.append((path, relative))
    return copies


def _hash(path: Path) -> str:
    digest = hashlib.sha1()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _locate(relative: str, stage: Path, volume: Path, *, staged: set[str]) -> Path | None:
    """A replaced file has to be in the stage. An unchanged file may stay on the volume."""
    key = path_key(relative)
    roots = (stage,) if key in staged else (stage, volume)
    for root in roots:
        try:
            path = volume_path(relative, root)
        except ValueError:
            return None
        if path.is_file():
            return path
    return None


def _verify(stage: Path, volume: Path, library: DeviceLibrary, staged_files: set[str]) -> None:
    root, root_name = pioneer_dir(stage)
    pdb_path = root / "rekordbox" / "export.pdb"
    plus_path = root / "rekordbox" / "exportLibrary.db"
    if not pdb_path.is_file() or not plus_path.is_file():
        raise VerifyError("Staged export is missing a database")
    from .pdb import read_pdb

    try:
        pdb = read_pdb(pdb_path.read_bytes())
    except Exception as exc:
        raise VerifyError(f"Staged DeviceSQL database is unreadable: {exc}") from exc
    conn = connect_library(plus_path)
    try:
        from .device import from_onelibrary

        plus = from_onelibrary(conn, root_name)
    except Exception as exc:
        raise VerifyError(f"Staged library database is unreadable: {exc}") from exc
    finally:
        conn.close()

    def pairs(rows) -> dict[int, str]:
        found = {}
        for row in rows:
            if row.export_id in found:
                raise VerifyError(f"Duplicate track id {row.export_id}")
            found[row.export_id] = path_key(row.path)
        return found

    pdb_paths = pairs(pdb.tracks)
    plus_paths = pairs(plus.tracks)
    if pdb_paths != plus_paths:
        raise VerifyError("Device databases disagree on track paths")
    model_paths = pairs(library.tracks)
    if model_paths != pdb_paths:
        raise VerifyError("Staged databases do not match the sync")

    def membership(rows) -> dict[int, list[int]]:
        found = {}
        for row in rows:
            if row.export_id in found:
                raise VerifyError(f"Duplicate playlist id {row.export_id}")
            found[row.export_id] = list(row.track_ids)
        return found

    if membership(plus.playlists) != {row.export_id: list(row.track_ids) for row in library.playlists}:
        raise VerifyError("Device databases disagree on playlist membership")
    pdb_members: dict[int, list[tuple[int, int]]] = {}
    for entry in pdb.entries:
        pdb_members.setdefault(entry.playlist_id, []).append((entry.seq, entry.track_id))
    for row in library.playlists:
        ordered = [track for _seq, track in sorted(pdb_members.get(row.export_id, []))]
        if ordered != list(row.track_ids):
            raise VerifyError("Device databases disagree on playlist membership")
        missing = [track for track in row.track_ids if track not in model_paths]
        if missing:
            raise VerifyError("A playlist refers to a missing track")

    for track in library.tracks:
        if _locate(track.path, stage, volume, staged=staged_files) is None:
            raise VerifyError(f"Audio file is missing: {track.path}")
        if track.analysis and _locate(track.analysis, stage, volume, staged=staged_files) is None:
            raise VerifyError(f"Analysis file is missing: {track.analysis}")


def _publish(stage: Path, volume: Path, files: list[str]) -> None:
    for relative in files:
        src = volume_path(relative, stage)
        dest = volume_path(relative, volume)
        if not src.is_file():
            raise PublicationError(f"Staged file disappeared: {relative}")
        dest.parent.mkdir(parents=True, exist_ok=True)
        temporary = dest.with_name(dest.name + ".exportstudio-tmp")
        shutil.copy2(src, temporary)
        os.replace(temporary, dest)


def _delete_owned(volume: Path, relative_paths: list[str]) -> None:
    for relative in relative_paths:
        try:
            path = volume_path(relative, volume)
        except ValueError:
            continue
        if path.is_file():
            path.unlink()


def recover(volume: Path) -> None:
    """Finish a publication that was interrupted after its plan was saved."""
    stage = volume / STAGE
    marker = stage / "publication.json"
    if not stage.exists():
        return
    if not marker.is_file():
        shutil.rmtree(stage, ignore_errors=True)
        return
    plan = json.loads(marker.read_text())
    _publish(stage, volume, plan.get("files", []))
    _delete_owned(volume, plan.get("obsolete", []))
    shutil.rmtree(stage, ignore_errors=True)


def _clear_stage(stage: Path) -> None:
    marker = stage / "publication.json"
    if marker.is_file():
        return
    shutil.rmtree(stage, ignore_errors=True)


def sync_volume(
    volume: Path,
    bundle: ExportBundle,
    *,
    on_progress=None,
    encrypt: bool = True,
    today: date | None = None,
) -> dict:
    volume = volume.resolve()
    if not volume.is_dir():
        raise FileNotFoundError(f"Volume is not mounted: {volume}")
    if not any(not row.folder for row in bundle.playlists):
        raise ValueError("Select a playlist to sync")
    for source in bundle.tracks:
        if not source.path.is_file():
            raise FileNotFoundError(f"Audio file is missing: {source.path}")

    recover(volume)
    before = _signature(volume)
    root, root_name = pioneer_dir(volume)
    existing = read_device(volume) or DeviceLibrary(root_name)
    manifest = load_manifest(root)
    owned_tracks = {int(item["exportId"]) for item in manifest["tracks"].values() if item.get("exportId")}
    owned_playlists = {int(item) for item in manifest["playlists"].values()}

    retained_tracks = [row for row in existing.tracks if row.export_id not in owned_tracks]
    retained_playlists = [row for row in existing.playlists if row.export_id not in owned_playlists]
    _report(on_progress, "reading", volume=str(volume))

    used_tracks: set[int] = set()
    export_of: dict[str, int] = {}
    for source in bundle.tracks:
        pinned = manifest["tracks"].get(source.master_id, {}).get("exportId")
        export_of[source.master_id] = _allocate(pinned, used_tracks)
    renamed_tracks = _claim(retained_tracks, used_tracks, lambda: _allocate_high(used_tracks))

    used_playlists: set[int] = set()
    playlist_of: dict[str, int] = {}
    for source in bundle.playlists:
        pinned = manifest["playlists"].get(source.master_id)
        playlist_of[source.master_id] = _allocate(pinned, used_playlists)
    renamed_playlists = _claim(retained_playlists, used_playlists, lambda: _allocate_high(used_playlists))
    for row in retained_playlists:
        if row.parent_id in renamed_playlists:
            row.parent_id = renamed_playlists[row.parent_id]
        row.track_ids = [renamed_tracks.get(item, item) for item in row.track_ids]

    used_paths = {path_key(row.path) for row in retained_tracks if row.path}
    assignments = []
    for source in bundle.tracks:
        export_id = export_of[source.master_id]
        previous = manifest["tracks"].get(source.master_id, {})
        relative = previous.get("audio") or audio_relative(source.artist, source.album, source.filename)
        relative = _unique_audio(relative, export_id, used_paths)
        analysis = ""
        if source.analysis is not None:
            analysis = analysis_dir(relative, root_name) + "/ANLZ0000.DAT"
        assignments.append((source, export_id, relative, analysis))

    kept_ids = {row.export_id for row in retained_tracks} | set(export_of.values())
    for row in retained_playlists:
        row.track_ids = [item for item in row.track_ids if item in kept_ids]

    new_tracks = []
    for source, export_id, relative, analysis in assignments:
        new_tracks.append(
            DeviceTrack(
                export_id=export_id,
                master_id=source.master_id,
                title=source.title,
                artist=source.artist,
                album=source.album,
                genre=source.genre,
                label=source.label,
                key=source.key,
                path=relative,
                filename=source.filename,
                analysis=analysis,
                comment=source.comment,
                bpm_x100=source.bpm_x100,
                duration=source.duration,
                bitrate=source.bitrate,
                sample_rate=source.sample_rate,
                bit_depth=source.bit_depth,
                file_type=source.file_type,
                file_size=source.file_size or source.path.stat().st_size,
                rating=source.rating,
                color=source.color,
                year=source.year,
                track_no=source.track_no,
                hot_cue_auto=source.hot_cue_auto,
                date_added=source.date_added,
                cues=[
                    CueRow(
                        kind=cue.kind,
                        start_ms=cue.start_ms,
                        end_ms=cue.end_ms,
                        color=cue.color,
                        color_index=cue.color_index,
                        comment=cue.comment,
                        active_loop=cue.active_loop,
                        beat_loop=cue.beat_loop,
                    )
                    for cue in source.cues
                ],
                owned=True,
            )
        )
    new_playlists = []
    for source in bundle.playlists:
        parent = playlist_of.get(source.parent_id or "", 0)
        new_playlists.append(
            DevicePlaylist(
                export_id=playlist_of[source.master_id],
                master_id=source.master_id,
                name=source.name or "Untitled",
                parent_id=parent,
                seq=source.seq,
                folder=source.folder,
                track_ids=[export_of[item] for item in source.track_ids if item in export_of],
                owned=True,
            )
        )
    library = DeviceLibrary(
        root_name,
        retained_tracks + new_tracks,
        retained_playlists + new_playlists,
        histories=list(existing.histories),
        device_name=existing.device_name,
        background=existing.background,
        tag_dbid=existing.tag_dbid,
        categories=existing.categories,
        sorts=existing.sorts,
        colors=existing.colors,
        pdb_rows=dict(existing.pdb_rows),
    )

    stage = volume / STAGE
    if stage.exists() and not (stage / "publication.json").is_file():
        shutil.rmtree(stage)
    stage.mkdir(parents=True, exist_ok=True)
    staged_root = stage / root_name
    planned: list[str] = []
    try:
        total = len(assignments)
        for index, (source, _export_id, relative, analysis) in enumerate(assignments, 1):
            _report(on_progress, "copy", done=index - 1, total=total, name=source.title or source.filename)
            live_audio = volume_path(relative, volume)
            staged_audio = volume_path(relative, stage)
            if not (live_audio.is_file() and _same_bytes(source.path, live_audio)):
                staged_audio.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source.path, staged_audio)
                planned.append(relative)
            if source.analysis is not None and analysis:
                for kind in ANLZ_KINDS:
                    companion = source.analysis.with_suffix(f".{kind}")
                    if not companion.is_file():
                        continue
                    rewritten = rewrite_ppth(companion.read_bytes(), relative)
                    relative_anlz = str(Path(analysis).with_suffix(f".{kind}")).replace("\\", "/")
                    if not relative_anlz.startswith("/"):
                        relative_anlz = "/" + relative_anlz
                    live_anlz = volume_path(relative_anlz, volume)
                    if live_anlz.is_file() and live_anlz.read_bytes() == rewritten:
                        continue
                    staged_anlz = volume_path(relative_anlz, stage)
                    staged_anlz.parent.mkdir(parents=True, exist_ok=True)
                    staged_anlz.write_bytes(rewritten)
                    planned.append(relative_anlz)
        _report(on_progress, "writing")

        previous_sync = ""
        live_sync = root / "rekordbox" / "playlists3.sync"
        if live_sync.is_file():
            previous_sync = live_sync.read_text(errors="replace")
        document = _sync_document(bundle.playlists, bundle.dbid, previous_sync or None)
        for name in ("playlists3.sync", "playlists3Plus.sync"):
            relative = f"/{root_name}/rekordbox/{name}"
            dest = volume_path(relative, stage)
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(document.encode("utf-8"))
            planned.append(relative)

        if bundle.settings_dir is not None and bundle.settings_dir.is_dir():
            for src, relative in _settings_to_copy(bundle.settings_dir, staged_root, volume):
                dest = volume_path(relative, stage)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dest)
                planned.append(relative)

        pdb_relative = f"/{root_name}/rekordbox/export.pdb"
        plus_relative = f"/{root_name}/rekordbox/exportLibrary.db"
        manifest_relative = f"/{root_name}/exportstudio/manifest.json"
        volume_path(pdb_relative, stage).parent.mkdir(parents=True, exist_ok=True)
        volume_path(pdb_relative, stage).write_bytes(to_pdb(library, today=today))
        dbid = int(bundle.dbid) if str(bundle.dbid).isdigit() else 0
        write_onelibrary(volume_path(plus_relative, stage), library, encrypt=encrypt, today=today, dbid=dbid)
        new_manifest = {
            "tracks": {
                source.master_id: {
                    "exportId": export_id,
                    "audio": relative,
                    "analysis": analysis,
                    "sourceSize": source.path.stat().st_size,
                    "sourceMtimeNs": source.path.stat().st_mtime_ns,
                }
                for source, export_id, relative, analysis in assignments
            },
            "playlists": {source.master_id: playlist_of[source.master_id] for source in bundle.playlists},
            "audio": [relative for _source, _export_id, relative, _analysis in assignments],
            "analysis": [analysis for _source, _export_id, _relative, analysis in assignments if analysis],
        }
        manifest_dest = volume_path(manifest_relative, stage)
        manifest_dest.parent.mkdir(parents=True, exist_ok=True)
        manifest_dest.write_text(json.dumps(new_manifest, indent=2))
        planned.extend([pdb_relative, plus_relative, manifest_relative])

        needed = sum(volume_path(relative, stage).stat().st_size for relative in planned)
        largest = max((volume_path(relative, stage).stat().st_size for relative in planned), default=0)
        stats = os.statvfs(volume)
        if stats.f_bavail * stats.f_frsize < needed + largest + 1024 * 1024:
            raise PublicationError("Not enough free space on the volume")

        _report(on_progress, "staged")
        _report(on_progress, "verifying")
        _verify(stage, volume, library, {path_key(item) for item in planned})
        if _signature(volume) != before:
            raise Conflict("The volume changed during sync")
        _report(on_progress, "publishing")

        owned_audio = {path_key(item) for item in manifest.get("audio", [])}
        owned_analysis = {path_key(item) for item in manifest.get("analysis", [])}
        for source in manifest["tracks"].values():
            if source.get("analysis"):
                base = Path(source["analysis"])
                for kind in ANLZ_KINDS:
                    owned_analysis.add(path_key(base.with_suffix(f".{kind}").as_posix()))
        keep_audio = {path_key(row.path) for row in library.tracks}
        keep_analysis = set()
        for row in library.tracks:
            if not row.analysis:
                continue
            base = Path(row.analysis)
            for kind in ANLZ_KINDS:
                keep_analysis.add(path_key(base.with_suffix(f".{kind}").as_posix()))
        obsolete = [
            item
            for item in list(manifest.get("audio", [])) + list(manifest.get("analysis", []))
            if path_key(item) in owned_audio | owned_analysis
            and path_key(item) not in keep_audio | keep_analysis
        ]
        for source in manifest["tracks"].values():
            if not source.get("analysis"):
                continue
            base = Path(source["analysis"])
            for kind in ANLZ_KINDS:
                relative = base.with_suffix(f".{kind}").as_posix()
                if path_key(relative) not in keep_analysis and relative not in obsolete:
                    obsolete.append(relative)

        # Databases and the manifest go last so a partial copy keeps the previous library readable.
        early = [item for item in planned if item not in {pdb_relative, plus_relative, manifest_relative}]
        ordered = early + [pdb_relative, plus_relative, manifest_relative]
        marker = stage / "publication.json"
        marker.write_text(json.dumps({"files": ordered, "obsolete": obsolete}))
        _publish(stage, volume, ordered)
        _delete_owned(volume, obsolete)
        shutil.rmtree(stage, ignore_errors=True)
        browse.invalidate(volume)
        _report(on_progress, "done", tracks=len(new_tracks), playlists=len(new_playlists))
        return {"volumeId": str(volume), "tracks": len(new_tracks), "playlists": len(new_playlists)}
    except Exception:
        _clear_stage(stage)
        raise


def find_settings() -> Path | None:
    home = Path.home()
    for base in (home / "Library" / "Application Support" / "Pioneer", home / "Library" / "Pioneer"):
        if base.is_dir():
            return base
    return None


def bundle_from_library(library, playlist_ids: list[str]) -> ExportBundle:
    """Build a sync bundle from the master snapshot. Smart lists are exported as they are now."""
    from collections import defaultdict

    from pyrekordbox.db6 import tables as t

    from ..library import FILE_TYPES
    from .pdb import FORMATS

    nodes = library.playlists()
    by_id = {row["id"]: row for row in nodes}
    children: dict[str, list[str]] = defaultdict(list)
    for row in nodes:
        children[row["parentId"]].append(row["id"])
    include: set[str] = set()
    for playlist_id in playlist_ids:
        if playlist_id not in by_id:
            raise KeyError(f"Unknown playlist {playlist_id}")
        cursor = playlist_id
        while cursor and cursor != "root":
            include.add(cursor)
            cursor = by_id[cursor]["parentId"]
        if by_id[playlist_id]["kind"] == "folder":
            stack = [playlist_id]
            while stack:
                current = stack.pop()
                for child in children.get(current, []):
                    include.add(child)
                    stack.append(child)
    if not any(by_id[item]["kind"] in ("playlist", "smart") for item in include):
        raise ValueError("Select a playlist to sync")

    ordered_ids = [row["id"] for row in nodes if row["id"] in include]
    playlists = []
    track_ids: list[str] = []
    for playlist_id in ordered_ids:
        node = by_id[playlist_id]
        members = [] if node["kind"] == "folder" else library.playlist_tracks(playlist_id)
        track_ids.extend(members)
        parent = node["parentId"]
        playlists.append(
            SourcePlaylist(
                master_id=playlist_id,
                name=node["name"] or "Untitled",
                parent_id=None if parent in (None, "", "root") or parent not in include else parent,
                seq=node["seq"] or 0,
                folder=node["kind"] == "folder",
                track_ids=members,
            )
        )
    seen: list[str] = []
    known = set()
    for track_id in track_ids:
        if track_id not in known:
            known.add(track_id)
            seen.append(track_id)
    rows = {row["id"]: row for row in library.tracks(ids=seen)} if seen else {}
    contents = {}
    if seen and library.db is not None:
        for content in library.db.query(t.DjmdContent).filter(t.DjmdContent.ID.in_(seen)).all():
            contents[content.ID] = content
    prop = library.db.query(t.DjmdProperty).first() if library.db is not None else None
    dbid = str(getattr(prop, "DBID", "") or "0")
    sources = []
    for track_id in seen:
        row = rows.get(track_id)
        content = contents.get(track_id)
        if row is None or content is None:
            continue
        path = Path(content.FolderPath or row["path"] or "")
        analysis = library._anlz_path(content)
        if analysis is not None and not analysis.is_file():
            dat = analysis.with_suffix(".DAT")
            analysis = dat if dat.is_file() else None
        file_type = content.FileType if content.FileType in FILE_TYPES else FORMATS.get(row["fileType"], 1)
        cues = []
        for cue in content.Cues:
            if getattr(cue, "rb_local_deleted", 0):
                continue
            cues.append(
                CueSource(
                    kind=cue.Kind or 0,
                    start_ms=cue.InMsec or 0,
                    end_ms=None if cue.OutMsec is None or cue.OutMsec < 0 else cue.OutMsec,
                    color=cue.Color if cue.Color is not None and cue.Color >= 0 else None,
                    color_index=cue.ColorTableIndex,
                    comment=cue.Comment or "",
                    active_loop=bool(getattr(cue, "ActiveLoop", 0)),
                    beat_loop=getattr(cue, "BeatLoopSize", 0) or 0,
                )
            )
        hot = getattr(content, "HotCueAutoLoad", None)
        sources.append(
            SourceTrack(
                master_id=track_id,
                title=row["title"],
                artist=row["artist"],
                album=row["album"],
                path=path,
                analysis=analysis,
                filename=Path(content.FileNameL or path.name).name,
                bpm_x100=int(content.BPM or 0),
                duration=int(content.Length or row["duration"] or 0),
                bitrate=int(content.BitRate or row["bitrate"] or 0),
                sample_rate=int(content.SampleRate or row["sampleRate"] or 0),
                bit_depth=int(content.BitDepth or row["bitDepth"] or 0),
                file_type=int(file_type or 1),
                file_size=int(content.FileSize or path.stat().st_size if path.is_file() else row["fileSize"] or 0),
                rating=int(row["rating"] or 0),
                color=int(row["color"] or 0),
                comment=row["comment"],
                year=int(row["year"] or 0),
                track_no=int(row["trackNo"] or 0),
                hot_cue_auto=True if hot is None or hot == "" else str(hot).upper() not in {"0", "OFF"},
                date_added=row["dateAdded"],
                genre=row["genre"],
                label=row["label"],
                key=row["key"],
                cues=cues,
            )
        )
    return ExportBundle(playlists, sources, dbid=dbid, settings_dir=find_settings())
