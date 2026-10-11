"""Browse a rekordbox volume through the same track and playlist shapes as the master library."""

from pathlib import Path

from .. import waveform
from ..library import FILE_TYPES
from .device import DeviceLibrary, DeviceTrack, read_device
from .ids import parse_track_key, track_key
from .paths import pioneer_dir, volume_path

_cache: dict[str, tuple] = {}


def _signature(volume: Path) -> tuple:
    root, _name = pioneer_dir(volume)
    parts = []
    for rel in ("rekordbox/export.pdb", "rekordbox/exportLibrary.db", "rbxport/manifest.json"):
        path = root / rel
        try:
            st = path.stat()
        except OSError:
            parts.append((rel, None, None))
        else:
            parts.append((rel, st.st_mtime_ns, st.st_size))
    return tuple(parts)


def invalidate(volume: Path | None = None) -> None:
    if volume is None:
        _cache.clear()
        return
    _cache.pop(str(volume.resolve()), None)


def load(volume: Path) -> DeviceLibrary:
    volume = volume.resolve()
    if not volume.is_dir():
        raise FileNotFoundError(f"Volume is not mounted: {volume}")
    sig = _signature(volume)
    key = str(volume)
    hit = _cache.get(key)
    if hit and hit[0] == sig:
        return hit[1]
    library = read_device(volume)
    if library is None:
        raise FileNotFoundError("This volume has no rekordbox library")
    _cache[key] = (sig, library)
    return library


def _abs(volume: Path, relative: str) -> Path | None:
    if not relative:
        return None
    try:
        return volume_path(relative, volume)
    except ValueError:
        return None


def _cue_sort(volume: Path, track: DeviceTrack) -> dict:
    phrase = False
    if track.analysis:
        base = _abs(volume, track.analysis)
        if base is not None:
            phrase = waveform.file_has_tag(base.with_suffix(".EXT"), waveform.PHRASE_TAG)
    return waveform.cue_sort_facts((cue.kind for cue in track.cues), phrase)


def track_row(volume: Path, track: DeviceTrack) -> dict:
    audio = _abs(volume, track.path)
    path = str(audio) if audio is not None else ""
    return {
        "id": track_key(str(volume), track.export_id),
        "title": track.title or Path(track.filename or track.path).stem or "Untitled",
        "artist": track.artist,
        "album": track.album,
        "genre": track.genre,
        "label": track.label,
        "remixer": "",
        "originalArtist": "",
        "composer": "",
        "key": track.key,
        "bpm": (track.bpm_x100 or 0) / 100,
        "duration": track.duration,
        "rating": track.rating,
        "color": track.color,
        "comment": track.comment,
        "year": track.year,
        "dateAdded": track.date_added,
        "dateCreated": "",
        "playCount": track.play_count,
        "bitrate": track.bitrate,
        "sampleRate": track.sample_rate,
        "bitDepth": track.bit_depth,
        "fileType": FILE_TYPES.get(track.file_type) or Path(track.filename or track.path).suffix.lstrip(".").upper(),
        "path": path,
        "missing": audio is None or not audio.is_file(),
        "streaming": False,
        "fileSize": track.file_size,
        "hasArtwork": False,
        "trackNo": track.track_no,
        "analysed": bool(track.analysis),
        "myTags": [],
        **_cue_sort(volume, track),
    }


def playlists(volume: Path) -> list[dict]:
    library = load(volume)
    return [
        {
            "id": str(row.export_id),
            "name": row.name,
            "parentId": str(row.parent_id) if row.parent_id else "root",
            "seq": row.seq,
            "kind": "folder" if row.folder else "playlist",
            "count": None if row.folder else len(row.track_ids),
        }
        for row in library.playlists
    ]


def tracks(volume: Path, playlist_id: str | None = None) -> list[dict]:
    library = load(volume)
    if playlist_id in (None, "", "root"):
        chosen = library.tracks
    else:
        playlist = next((row for row in library.playlists if str(row.export_id) == str(playlist_id)), None)
        if playlist is None:
            raise KeyError(f"Unknown device playlist {playlist_id}")
        by_id = {row.export_id: row for row in library.tracks}
        chosen = [by_id[item] for item in playlist.track_ids if item in by_id]
    return [track_row(volume, row) for row in chosen]


def _anlz(volume: Path, relative: str) -> dict[str, waveform.Tags | None]:
    out: dict[str, waveform.Tags | None] = {"DAT": None, "EXT": None, "2EX": None}
    base = _abs(volume, relative)
    if base is None:
        return out
    for kind in out:
        path = base.with_suffix(f".{kind}")
        if path.is_file():
            try:
                out[kind] = waveform.read_tags(path)
            except Exception:
                out[kind] = None
    return out


def _cues(track: DeviceTrack, ext: waveform.Tags | None) -> list[dict]:
    cues = []
    for index, cue in enumerate(track.cues, 1):
        cues.append(
            {
                "id": f"{track.export_id}:{index}",
                "kind": "hot" if cue.kind and cue.kind > 0 else "memory",
                "slot": cue.kind or 0,
                "start": cue.start_ms,
                "end": cue.end_ms,
                "color": cue.color,
                "colorIndex": cue.color_index,
                "comment": cue.comment,
            }
        )
    colors = waveform.hot_cue_colors(ext)
    for cue in cues:
        if cue["kind"] == "hot":
            cue["rgb"] = colors.get((cue["slot"], cue["start"]))
    cues.sort(key=lambda cue: (cue["kind"] != "hot", cue["slot"], cue["start"]))
    return cues


def _blank_preview(key: str) -> dict:
    return {"id": key, "memory": False, "hot": False, "phrase": False, "cues": [], "waveform": None}


def previews(ids: list[str]) -> list[dict]:
    grouped: dict[str, list[str]] = {}
    for key in ids:
        volume_text, _export_id = parse_track_key(key)
        grouped.setdefault(volume_text, []).append(key)
    out = []
    for volume_text, keys in grouped.items():
        try:
            volume = Path(volume_text)
            library = load(volume)
        except (OSError, FileNotFoundError, ValueError):
            out.extend(_blank_preview(key) for key in keys)
            continue
        for key in keys:
            _volume, export_id = parse_track_key(key)
            track = library.track(export_id)
            if track is None:
                out.append(_blank_preview(key))
                continue
            anlz = _anlz(volume, track.analysis)
            stored = [
                waveform.browser_cue(
                    f"{track.export_id}:{index}",
                    cue.kind,
                    cue.start_ms,
                    cue.end_ms,
                    cue.color,
                    cue.color_index,
                    cue.comment,
                )
                for index, cue in enumerate(track.cues, 1)
            ]
            cues = waveform.paint_browser_cues(stored, waveform.hot_cue_colors(anlz["EXT"]))
            memory, hot = waveform.cue_marks(cue["slot"] for cue in cues)
            out.append(
                {
                    "id": key,
                    "memory": memory,
                    "hot": hot,
                    "phrase": waveform.has_phrase(anlz["EXT"]),
                    "cues": cues,
                    "waveform": waveform.browser_waveform(anlz["EXT"], anlz["2EX"]),
                }
            )
    return out


def detail(key: str) -> dict:
    volume_text, export_id = parse_track_key(key)
    volume = Path(volume_text)
    library = load(volume)
    track = library.track(export_id)
    if track is None:
        raise KeyError(f"Unknown USB track {key}")
    audio = _abs(volume, track.path)
    anlz = _anlz(volume, track.analysis)
    rgb = waveform.decode_rgb(anlz["EXT"])
    return {
        "id": key,
        "path": str(audio) if audio is not None else "",
        "fileExists": bool(audio and audio.is_file()),
        "cues": _cues(track, anlz["EXT"]),
        "beatgrid": waveform.decode_beatgrid(anlz["DAT"]),
        "waveform": {
            "rate": waveform.DETAIL_RATE,
            "rgb": rgb,
            "threeBand": waveform.decode_three_band(anlz["2EX"]),
            "threeBandPreview": waveform.decode_three_band(anlz["2EX"], waveform.PREVIEW_BANDS_TAG),
            "mono": None if rgb else waveform.decode_mono(anlz["EXT"]),
        },
    }


def resolve_media(kind: str, key: str) -> str | None:
    if kind != "audio":
        return None
    volume_text, export_id = parse_track_key(key)
    volume = Path(volume_text)
    library = load(volume)
    track = library.track(export_id)
    if track is None:
        return None
    audio = _abs(volume, track.path)
    return str(audio) if audio is not None and audio.is_file() else None
