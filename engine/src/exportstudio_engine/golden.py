"""Exports rekordbox's own analysis of local tracks, to score our analyser against.

    uv run python -m exportstudio_engine.golden out.json [--limit N]

Each track gets its path, rekordbox's beatgrid and key, and the raw waveform tag bodies, which the
Rust `golden` example compares with its own results.
"""

import argparse
import base64
import json
import random
import struct
import sys
from pathlib import Path

from pyrekordbox.db6 import tables as t

from . import waveform
from .library import Library

WAVE_TAGS = {"DAT": ["PWAV", "PWV2"], "EXT": ["PWV3", "PWV4", "PWV5"], "2EX": ["PWV6", "PWV7"]}


def _tag_body(tags: waveform.Tags | None, kind: str) -> dict | None:
    if not tags or kind not in tags:
        return None
    tag = tags[kind][0]
    len_header, _len_tag, entry_bytes, count = struct.unpack_from(">IIII", tag, 4)
    return {
        "entryBytes": entry_bytes,
        "count": count,
        "data": base64.b64encode(bytes(tag[len_header:])).decode("ascii"),
    }


def export(out: Path, limit: int | None, seed: int) -> int:
    lib = Library()
    lib.open()
    db = lib.db
    keys = {k.ID: k.ScaleName for k in db.query(t.DjmdKey).all()}
    contents = [
        c
        for c in db.get_content()
        if c.AnalysisDataPath and c.FolderPath and not c.rb_local_deleted and Path(c.FolderPath).is_file()
    ]
    random.Random(seed).shuffle(contents)
    tracks = []
    for content in contents:
        if limit is not None and len(tracks) >= limit:
            break
        anlz = lib._anlz(content)
        grid = waveform.decode_beatgrid(anlz["DAT"])
        if not grid:
            continue
        tracks.append(
            {
                "id": content.ID,
                "title": content.Title or "",
                "path": content.FolderPath,
                "bpm": (content.BPM or 0) / 100,
                "key": keys.get(content.KeyID),
                "beats": grid,
                "waves": {
                    kind: _tag_body(anlz[file], kind) for file, kinds in WAVE_TAGS.items() for kind in kinds
                },
            }
        )
    out.write_text(json.dumps({"tracks": tracks}))
    lib.close()
    return len(tracks)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("out", type=Path)
    parser.add_argument("--limit", type=int)
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()
    count = export(args.out, args.limit, args.seed)
    print(f"Exported {count} tracks to {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
