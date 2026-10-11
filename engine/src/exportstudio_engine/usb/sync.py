"""Publish playlists to a USB stick with rbxport's exporter.

Audio, analysis, artwork, Device Library, and OneLibrary are written by the
vendored rbxport sync (`es-usb`). That is the same pipeline that keeps the
two device databases in agreement. Tracks and playlists already on the stick
are reconciled by that exporter.
"""

import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path


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
    artwork: Path | None = None
    analysed: int = 0
    play_count: int = 0
    disc_no: int = 0
    date_created: str = ""
    isrc: str = ""
    release_date: str = ""
    my_tag_ids: list[str] = field(default_factory=list)


@dataclass
class SourcePlaylist:
    master_id: str
    name: str
    parent_id: str | None
    seq: int
    folder: bool
    track_ids: list[str]


@dataclass
class SourceMyTag:
    id: str
    seq: int
    name: str
    attribute: int
    parent_id: str | None


@dataclass
class ExportBundle:
    playlists: list[SourcePlaylist]
    tracks: list[SourceTrack]
    dbid: str = "0"
    settings_dir: Path | None = None
    my_tags: list[SourceMyTag] = field(default_factory=list)


def _num(value) -> int:
    try:
        number = int(value)
    except (TypeError, ValueError):
        return 0
    return number if number > 0 else 0


def _int(value, default: int = 0) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def find_settings() -> Path | None:
    """Directory that holds rekordbox My Settings, when this machine has them."""
    home = Path.home()
    bases = (
        home / "Library" / "Application Support" / "Pioneer",
        home / "Library" / "Pioneer",
        home / "AppData" / "Roaming" / "Pioneer",
    )
    for base in bases:
        if not base.is_dir():
            continue
        for path in base.rglob("MYSETTING.DAT"):
            return path.parent
    return None


def _exporter() -> Path:
    name = "es-usb.exe" if os.name == "nt" else "es-usb"
    candidates: list[Path] = []
    env = os.environ.get("EXPORTSTUDIO_USB_EXPORT")
    if env:
        candidates.append(Path(env))
    if getattr(sys, "frozen", False):
        candidates.append(Path(sys.executable).resolve().parent / name)
    repo = Path(__file__).resolve().parents[4]
    candidates.append(repo / "vendor" / "rbxport" / "target" / "release" / name)
    candidates.append(repo / "vendor" / "rbxport" / "target" / "debug" / name)
    for path in candidates:
        if path.is_file():
            return path
    raise FileNotFoundError(
        "The USB export program is not built. From the app directory, run npm run build:native."
    )


def _analysis_files(analysis: Path | None) -> list[str]:
    if analysis is None:
        return []
    dat = analysis if analysis.suffix.upper() == ".DAT" else analysis.with_suffix(".DAT")
    found = []
    for kind in ("DAT", "EXT", "2EX"):
        path = dat.with_suffix(f".{kind}")
        if path.is_file():
            found.append(str(path))
    return found


def _job(volume: Path, bundle: ExportBundle) -> dict:
    tracks = []
    for source in bundle.tracks:
        analysed = source.analysed
        if analysed <= 0 and source.analysis is not None and source.analysis.is_file():
            analysed = 105
        tracks.append(
            {
                "id": _num(source.master_id),
                "sourcePath": str(source.path),
                "title": source.title,
                "artist": source.artist,
                "album": source.album,
                "genre": source.genre,
                "label": source.label,
                "key": source.key,
                "comment": source.comment,
                "dateAdded": source.date_added,
                "releaseDate": source.release_date,
                "bpmX100": max(0, source.bpm_x100),
                "durationSec": max(0, source.duration),
                "rating": max(0, source.rating),
                "colorId": max(0, source.color),
                "year": max(0, source.year),
                "bitrate": max(0, source.bitrate),
                "sampleRate": max(0, source.sample_rate),
                "fileSize": max(0, source.file_size),
                "analysis": _analysis_files(source.analysis),
                "artwork": str(source.artwork) if source.artwork and source.artwork.is_file() else None,
                "myTags": [_num(tag_id) for tag_id in source.my_tag_ids if _num(tag_id)],
                "metadata": {
                    "trackNumber": max(0, source.track_no),
                    "discNumber": max(0, source.disc_no),
                    "bitDepth": max(0, source.bit_depth),
                    "playCount": max(0, source.play_count),
                    "analysed": max(0, analysed),
                    "hotCueAutoLoad": source.hot_cue_auto,
                    "dateCreated": source.date_created,
                    "isrc": source.isrc,
                },
                "cues": [
                    {
                        "kind": cue.kind,
                        "timeMs": max(0, cue.start_ms),
                        "endMs": cue.end_ms,
                        "color": cue.color,
                        "colorCode": cue.color_index or 0,
                        "comment": cue.comment,
                        "activeLoop": cue.active_loop,
                        "beatLoopSize": max(0, cue.beat_loop),
                    }
                    for cue in source.cues
                ],
            }
        )
    known = {row["id"] for row in tracks}
    playlists = []
    for row in bundle.playlists:
        members = [_num(track_id) for track_id in row.track_ids]
        missing = [track_id for track_id in members if track_id not in known]
        if missing:
            raise ValueError(f"Playlist '{row.name}' refers to a track that is not in this export")
        playlists.append(
            {
                "id": _num(row.master_id),
                "name": row.name,
                "parentId": _num(row.parent_id),
                "folder": row.folder,
                "trackIds": members,
            }
        )
    tags = []
    for tag in bundle.my_tags:
        tag_id = _num(tag.id)
        if not tag_id:
            continue
        tags.append(
            {
                "id": tag_id,
                "seq": max(0, tag.seq),
                "name": tag.name,
                "attribute": tag.attribute,
                "parent": _num(tag.parent_id),
            }
        )
    return {
        "destination": str(volume),
        "dbId": _num(bundle.dbid),
        "automatic": False,
        "settingsDir": str(bundle.settings_dir) if bundle.settings_dir and bundle.settings_dir.is_dir() else None,
        "myTags": tags,
        "tracks": tracks,
        "playlists": playlists,
    }


def _raise_export(kind: str, message: str) -> None:
    if kind == "conflict":
        raise Conflict(message)
    if "verif" in message.lower() or "disagree" in message.lower():
        raise VerifyError(message)
    raise PublicationError(message)


def sync_volume(volume: Path, bundle: ExportBundle, *, on_progress=None) -> dict:
    """Copy the selected playlists onto `volume` and publish both device databases."""
    from . import browse

    volume = volume.resolve()
    if not volume.is_dir():
        raise FileNotFoundError(f"Volume is not mounted: {volume}")
    if not any(not row.folder for row in bundle.playlists):
        raise ValueError("Select a playlist to sync")
    for source in bundle.tracks:
        if not source.path.is_file():
            raise FileNotFoundError(f"Audio file is missing: {source.path}")

    binary = _exporter()
    job_file = tempfile.NamedTemporaryFile(prefix="exportstudio-usb-", suffix=".json", delete=False)
    job_path = Path(job_file.name)
    try:
        job_file.write(json.dumps(_job(volume, bundle)).encode())
        job_file.close()
        proc = subprocess.Popen(
            [str(binary), str(job_path)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        result = None
        error = None
        assert proc.stdout is not None
        for line in proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            kind = event.get("event")
            if kind == "progress":
                if on_progress is not None:
                    on_progress(
                        {
                            "phase": event.get("phase") or "reading",
                            "done": event.get("done"),
                            "total": event.get("total"),
                            "name": event.get("name") or "",
                        }
                    )
            elif kind == "result":
                result = event
            elif kind == "error":
                error = event
        stderr = proc.stderr.read() if proc.stderr is not None else ""
        code = proc.wait()
        if proc.stdout is not None:
            proc.stdout.close()
        if proc.stderr is not None:
            proc.stderr.close()
    finally:
        job_path.unlink(missing_ok=True)

    if error is not None or code != 0 or result is None:
        message = ""
        kind = "export"
        if error is not None:
            message = str(error.get("message") or "")
            kind = str(error.get("kind") or "export")
        if not message:
            message = stderr.strip() or f"USB export failed ({code})"
        _raise_export(kind, message)

    browse.invalidate(volume)
    if on_progress is not None:
        on_progress({"phase": "done", "tracks": result["tracks"], "playlists": result["playlists"]})
    return {"volumeId": str(volume), "tracks": result["tracks"], "playlists": result["playlists"]}


def bundle_from_library(library, playlist_ids: list[str]) -> ExportBundle:
    """Build a sync bundle from the master snapshot. Smart lists are exported as they are now."""
    from collections import defaultdict

    from pyrekordbox.db6 import tables as t

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
    my_tags, tags_of = _my_tags(library, seen)
    share = getattr(library, "share_dir", None)
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
        image = getattr(content, "ImagePath", None) or ""
        artwork = None
        if image and share is not None:
            candidate = Path(share) / str(image).lstrip("/\\")
            if candidate.is_file():
                artwork = candidate
        analysed = _int(getattr(content, "Analysed", None), 0)
        sources.append(
            SourceTrack(
                master_id=track_id,
                title=row["title"],
                artist=row["artist"],
                album=row["album"],
                path=path,
                analysis=analysis,
                filename=Path(content.FileNameL or path.name).name,
                bpm_x100=_int(content.BPM, 0),
                duration=_int(content.Length or row["duration"], 0),
                bitrate=_int(content.BitRate or row["bitrate"], 0),
                sample_rate=_int(content.SampleRate or row["sampleRate"], 0),
                bit_depth=_int(content.BitDepth or row["bitDepth"], 0),
                file_size=_int(content.FileSize, path.stat().st_size if path.is_file() else _int(row["fileSize"], 0)),
                rating=_int(row["rating"], 0),
                color=_int(row["color"], 0),
                comment=row["comment"],
                year=_int(row["year"], 0),
                track_no=_int(row["trackNo"], 0),
                hot_cue_auto=True if hot is None or hot == "" else str(hot).upper() not in {"0", "OFF"},
                date_added=row["dateAdded"],
                genre=row["genre"],
                label=row["label"],
                key=row["key"],
                cues=cues,
                artwork=artwork,
                analysed=analysed,
                play_count=_int(getattr(content, "DJPlayCount", None), 0),
                disc_no=_int(getattr(content, "DiscNo", None), 0),
                date_created=str(getattr(content, "DateCreated", None) or ""),
                isrc=str(getattr(content, "ISRC", None) or ""),
                release_date=str(getattr(content, "ReleaseDate", None) or ""),
                my_tag_ids=tags_of.get(track_id, []),
            )
        )
    return ExportBundle(playlists, sources, dbid=dbid, settings_dir=find_settings(), my_tags=my_tags)


def _my_tags(library, track_ids: set[str]) -> tuple[list[SourceMyTag], dict[str, list[str]]]:
    from pyrekordbox.db6 import tables as t

    tags: list[SourceMyTag] = []
    of: dict[str, list[str]] = {}
    if library.db is None or not track_ids:
        return tags, of
    from sqlalchemy.exc import SQLAlchemyError

    try:
        for row in library.db.query(t.DjmdMyTag).all():
            if getattr(row, "rb_local_deleted", 0):
                continue
            parent = row.ParentID
            tags.append(
                SourceMyTag(
                    id=str(row.ID),
                    seq=_int(row.Seq, 0),
                    name=row.Name or "",
                    attribute=_int(row.Attribute, 0),
                    parent_id=None if parent in (None, "", "root") else str(parent),
                )
            )
        for row in library.db.query(t.DjmdSongMyTag).filter(t.DjmdSongMyTag.ContentID.in_(track_ids)).all():
            if getattr(row, "rb_local_deleted", 0):
                continue
            of.setdefault(str(row.ContentID), []).append(str(row.MyTagID))
    except SQLAlchemyError:
        return [], {}
    return tags, of
