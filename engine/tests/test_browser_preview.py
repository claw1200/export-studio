"""Browser waveform thumbnails and cue stars, from analysis tags rather than a library."""

import base64
import struct
import tempfile
import unittest
from pathlib import Path

import numpy as np

from exportstudio_engine import anlz_build, waveform


class BrowserPreviewTest(unittest.TestCase):
    def test_sort_counts_each_placed_cue_and_one_star_per_kind(self) -> None:
        facts = waveform.cue_sort_facts([0, 0, 1, 4, 5], True)
        self.assertEqual(facts, {"cueCount": 4, "cueStars": 3, "phrase": True})
        bare = waveform.cue_sort_facts([4], False)
        self.assertEqual(bare, {"cueCount": 0, "cueStars": 0, "phrase": False})

    def test_file_has_tag_skips_a_tag_body_to_find_a_phrase(self) -> None:
        def tag(kind: bytes, payload: bytes) -> bytes:
            return kind + struct.pack(">II", 12, 12 + len(payload)) + payload

        body = tag(b"PWV5", b"x" * 40) + tag(b"PSSI", b"phrase")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ANLZ0000.EXT"
            path.write_bytes(b"PMAI" + struct.pack(">I", 8) + body)
            self.assertTrue(waveform.file_has_tag(path, "PSSI"))
            self.assertFalse(waveform.file_has_tag(path, "PQTZ"))
            self.assertFalse(waveform.file_has_tag(path.with_name("missing.EXT"), "PSSI"))

    def test_browser_cue_keeps_a_loop_and_drops_an_empty_out_point(self) -> None:
        cue = waveform.browser_cue("1", 0, 1500, -1, -1, None, "")
        self.assertEqual(cue["kind"], "memory")
        self.assertIsNone(cue["end"])
        loop = waveform.browser_cue("2", 4, 0, 8000, None, None, "drop")
        self.assertEqual((loop["kind"], loop["end"], loop["comment"]), ("hot", 8000, "drop"))

    def test_cue_marks_keep_one_of_each(self) -> None:
        self.assertEqual(waveform.cue_marks([]), (False, False))
        self.assertEqual(waveform.cue_marks([0, 0, None]), (True, False))
        self.assertEqual(waveform.cue_marks([1, 1, 5]), (False, True))
        self.assertEqual(waveform.cue_marks([4]), (False, False))
        self.assertEqual(waveform.cue_marks([0, 3, 4]), (True, True))

    def test_preview_is_short_bands_until_a_phrase_tag_is_added(self) -> None:
        features = np.full((300, anlz_build.FEATURES), 0.4)
        features[10, 0] = 1
        files = anlz_build.new_files("track.mp3", [], features)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for kind, data in files.items():
                (root / f"ANLZ0000.{kind}").write_bytes(data)
            ext = waveform.read_tags(root / "ANLZ0000.EXT")
            ex2 = waveform.read_tags(root / "ANLZ0000.2EX")
            self.assertFalse(waveform.has_phrase(ext))
            preview = waveform.browser_waveform(ext, ex2)
            self.assertIsNotNone(preview)
            assert preview is not None
            raw = base64.b64decode(preview["bands"])
            self.assertLessEqual(preview["bandLength"], waveform.BROWSER_WAVE_COLUMNS)
            self.assertEqual(len(raw), preview["bandLength"] * 3)
            self.assertGreater(max(raw), 0)
            colours = base64.b64decode(preview["heights"])
            self.assertEqual(len(colours), preview["rgbLength"])
            self.assertEqual(len(base64.b64decode(preview["colors"])), preview["rgbLength"] * 3)

            phrase = struct.pack(">4sII", b"PSSI", 12, 12)
            ext_path = root / "ANLZ0000.EXT"
            ext_path.write_bytes(anlz_build.patch_file(files["EXT"], {"PSSI": phrase}))
            self.assertTrue(waveform.has_phrase(waveform.read_tags(ext_path)))


if __name__ == "__main__":
    unittest.main()
