"""File-list metadata: tags on a file that is not in the collection."""

import tempfile
import unittest
import wave
from pathlib import Path

from mutagen.id3 import TBPM, TKEY, TIT2, TPE1
from mutagen.wave import WAVE

from exportstudio_engine.tags import browse, browse_many


class FileBrowseTest(unittest.TestCase):
    def test_browse_reads_title_artist_bpm_and_key(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Tune.wav"
            with wave.open(str(path), "w") as handle:
                handle.setnchannels(1)
                handle.setsampwidth(2)
                handle.setframerate(44100)
                handle.writeframes(b"\x00\x00" * 44100)
            audio = WAVE(path)
            audio.add_tags()
            audio.tags.add(TIT2(encoding=3, text="Night"))
            audio.tags.add(TPE1(encoding=3, text="DJ"))
            audio.tags.add(TBPM(encoding=3, text="128.5"))
            audio.tags.add(TKEY(encoding=3, text="8A"))
            audio.save()

            row = browse(path)

        self.assertEqual(row["title"], "Night")
        self.assertEqual(row["artist"], "DJ")
        self.assertEqual(row["bpm"], 128.5)
        self.assertEqual(row["key"], "8A")
        self.assertEqual(row["duration"], 1)
        self.assertFalse(row["collected"])
        self.assertFalse(row["hasArtwork"])
        self.assertNotIn("artwork", row)

    def test_browse_uses_the_filename_when_tags_cannot_be_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Loose.mp3"
            path.write_bytes(b"not audio")
            row = browse(path)
        self.assertEqual(row["title"], "Loose")
        self.assertEqual(row["fileType"], "MP3")
        self.assertFalse(row["collected"])

    def test_browse_many_keeps_batches_small(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            paths = []
            for n in range(81):
                path = folder / f"{n}.mp3"
                path.write_bytes(b"x")
                paths.append(str(path))
            rows = browse_many(paths)
        self.assertEqual(len(rows), 80)
