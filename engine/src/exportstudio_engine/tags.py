import re
from pathlib import Path

from mutagen import File as open_audio
from mutagen.flac import FLAC
from mutagen.id3 import COMM, ID3, TALB, TCOM, TCON, TDRC, TIT2, TOPE, TPE1, TPE4, TPUB, TRCK
from mutagen.mp4 import MP4, MP4FreeForm

ID3_FRAMES = {
    "title": TIT2,
    "artist": TPE1,
    "album": TALB,
    "genre": TCON,
    "label": TPUB,
    "remixer": TPE4,
    "originalArtist": TOPE,
    "composer": TCOM,
    "year": TDRC,
    "trackNo": TRCK,
}

MP4_KEYS = {
    "title": "\xa9nam",
    "artist": "\xa9ART",
    "album": "\xa9alb",
    "genre": "\xa9gen",
    "composer": "\xa9wrt",
    "comment": "\xa9cmt",
    "year": "\xa9day",
}
MP4_FREEFORM = {
    "label": "----:com.apple.iTunes:LABEL",
    "remixer": "----:com.apple.iTunes:REMIXER",
    "originalArtist": "----:com.apple.iTunes:ORIGINAL ARTIST",
}

VORBIS_KEYS = {
    "title": "TITLE",
    "artist": "ARTIST",
    "album": "ALBUM",
    "genre": "GENRE",
    "label": "LABEL",
    "remixer": "REMIXER",
    "originalArtist": "ORIGINALARTIST",
    "composer": "COMPOSER",
    "comment": "COMMENT",
    "year": "DATE",
    "trackNo": "TRACKNUMBER",
}

FILE_FIELDS = set(ID3_FRAMES) | {"comment"}


def _text(value) -> str:
    if value is None or value == 0:
        return ""
    return str(value).strip()


def _write_id3(tags: ID3, fields: dict) -> None:
    for name, value in fields.items():
        text = _text(value)
        if name == "comment":
            tags.delall("COMM")
            if text:
                tags.add(COMM(encoding=3, lang="eng", desc="", text=[text]))
            continue
        frame = ID3_FRAMES[name]
        tags.delall(frame.__name__)
        if text:
            tags.add(frame(encoding=3, text=[text]))


def _write_mp4(audio: MP4, fields: dict) -> None:
    if audio.tags is None:
        audio.add_tags()
    tags = audio.tags
    for name, value in fields.items():
        text = _text(value)
        if name == "trackNo":
            total = tags.get("trkn", [(0, 0)])[0][1]
            if text:
                tags["trkn"] = [(int(value), total)]
            else:
                tags.pop("trkn", None)
            continue
        key = MP4_KEYS.get(name) or MP4_FREEFORM[name]
        if not text:
            tags.pop(key, None)
        elif key in MP4_FREEFORM.values():
            tags[key] = [MP4FreeForm(text.encode("utf-8"))]
        else:
            tags[key] = [text]


def _write_vorbis(audio, fields: dict) -> None:
    if audio.tags is None:
        audio.add_tags()
    for name, value in fields.items():
        key = VORBIS_KEYS[name]
        text = _text(value)
        if text:
            audio.tags[key] = [text]
        elif key in audio.tags:
            del audio.tags[key]


def _first_text(value) -> str:
    if value is None:
        return ""
    if isinstance(value, (list, tuple)):
        value = value[0] if value else ""
    if isinstance(value, MP4FreeForm) or isinstance(value, bytes):
        value = bytes(value).decode("utf-8", "replace")
    if hasattr(value, "text"):
        value = value.text[0] if value.text else ""
    return str(value).strip()


def _number(text: str) -> int:
    match = re.match(r"\s*(\d+)", text or "")
    return int(match.group(1)) if match else 0


def _read_id3(tags: ID3) -> tuple[dict, bytes | None]:
    fields = {name: _first_text(tags.get(frame.__name__)) for name, frame in ID3_FRAMES.items()}
    comments = tags.getall("COMM")
    fields["comment"] = _first_text(comments[0]) if comments else ""
    pictures = tags.getall("APIC")
    front = next((p for p in pictures if p.type == 3), pictures[0] if pictures else None)
    return fields, front.data if front else None


def _read_mp4(tags) -> tuple[dict, bytes | None]:
    fields = {name: _first_text(tags.get(key)) for name, key in MP4_KEYS.items()}
    fields.update({name: _first_text(tags.get(key)) for name, key in MP4_FREEFORM.items()})
    track = tags.get("trkn")
    fields["trackNo"] = str(track[0][0]) if track else ""
    covers = tags.get("covr") or []
    return fields, bytes(covers[0]) if covers else None


def _read_vorbis(audio) -> tuple[dict, bytes | None]:
    tags = audio.tags or {}
    fields = {name: _first_text(tags.get(key)) for name, key in VORBIS_KEYS.items()}
    pictures = list(getattr(audio, "pictures", []) or [])
    front = next((p for p in pictures if p.type == 3), pictures[0] if pictures else None)
    return fields, front.data if front else None


def read(path: Path) -> dict:
    """Read the track fields, stream info and front cover from an audio file's own tags."""
    audio = open_audio(path)
    if audio is None:
        raise ValueError(f"Unsupported file type: {path.suffix}")
    fields: dict = {}
    art: bytes | None = None
    if isinstance(audio, MP4):
        if audio.tags is not None:
            fields, art = _read_mp4(audio.tags)
    elif isinstance(audio, FLAC):
        fields, art = _read_vorbis(audio)
    elif isinstance(audio.tags, ID3):
        fields, art = _read_id3(audio.tags)
    fields = {name: value for name, value in fields.items() if value}
    for name in ("year", "trackNo"):
        if name in fields:
            fields[name] = _number(fields[name])
    info = audio.info
    return {
        "fields": fields,
        "artwork": art,
        "length": int(round(getattr(info, "length", 0) or 0)),
        "bitrate": int(round((getattr(info, "bitrate", 0) or 0) / 1000)),
        "sampleRate": int(getattr(info, "sample_rate", 0) or 0),
        "bitDepth": int(getattr(info, "bits_per_sample", 0) or 0),
    }


def write(path: Path, fields: dict) -> None:
    """Write the given track fields into the audio file's own tags.

    Fields that the file format has no place for (rating, colour) must be filtered out first.
    """
    if not path.is_file():
        raise FileNotFoundError(f"File not found: {path}")
    audio = open_audio(path)
    if audio is None:
        raise ValueError(f"Unsupported file type: {path.suffix}")
    if isinstance(audio, MP4):
        _write_mp4(audio, fields)
        audio.save()
    elif isinstance(audio, FLAC):
        _write_vorbis(audio, fields)
        audio.save()
    else:
        if audio.tags is None:
            audio.add_tags()
        if not isinstance(audio.tags, ID3):
            raise ValueError(f"Unsupported tag format in {path.name}")
        _write_id3(audio.tags, fields)
        audio.save(v2_version=3)
