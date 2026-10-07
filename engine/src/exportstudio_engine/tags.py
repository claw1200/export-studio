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
