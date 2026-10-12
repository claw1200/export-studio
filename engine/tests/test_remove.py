"""Analysis files removed with a track, so a later import cannot restore the beatgrid."""

import tempfile
import unittest
from pathlib import Path

from exportstudio_engine.library import analysis_files, discard_analysis_files, file_type_code, stream_fields


class AnalysisFilesTest(unittest.TestCase):
    def test_lists_the_three_analysis_files_inside_the_share_folder(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            share = Path(directory) / "share"
            relative = "/PIONEER/USBANLZ/abc/def012/ANLZ0000.DAT"
            paths = analysis_files(share, relative)

        self.assertEqual([path.name for path in paths], ["ANLZ0000.DAT", "ANLZ0000.EXT", "ANLZ0000.2EX"])
        self.assertTrue(str(paths[0]).endswith("share/PIONEER/USBANLZ/abc/def012/ANLZ0000.DAT"))

    def test_ignores_a_missing_path_and_a_path_outside_the_share_folder(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            share = Path(directory) / "share"
            self.assertEqual(analysis_files(share, None), [])
            self.assertEqual(analysis_files(share, ""), [])
            self.assertEqual(analysis_files(share, "../../secrets.DAT"), [])

    def test_deletes_the_files_that_hold_the_beatgrid(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            share = Path(directory) / "share"
            paths = analysis_files(share, "/PIONEER/USBANLZ/abc/def012/ANLZ0000.DAT")
            paths[0].parent.mkdir(parents=True)
            paths[0].write_bytes(b"grid")
            paths[1].write_bytes(b"cues")
            discard_analysis_files(paths)

            self.assertFalse(paths[0].exists())
            self.assertFalse(paths[1].exists())
            self.assertFalse(paths[2].exists())


class StreamFieldsTest(unittest.TestCase):
    def test_wav_path_is_not_stored_as_mp3(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            path.write_bytes(b"RIFF")
            fields = stream_fields(path, {"bitrate": 1411, "sampleRate": 44100, "bitDepth": 16, "length": 320})

        self.assertEqual(file_type_code(path), 11)
        self.assertEqual(fields["FileType"], 11)
        self.assertEqual(fields["BitRate"], 1411)
        self.assertEqual(fields["SampleRate"], 44100)
        self.assertEqual(fields["BitDepth"], 16)
        self.assertEqual(fields["FileNameL"], "tune.wav")
        self.assertEqual(fields["FileSize"], 4)

    def test_aif_uses_the_aiff_code(self) -> None:
        self.assertEqual(file_type_code(Path("tune.aif")), 12)
        self.assertEqual(file_type_code(Path("tune.mp3")), 1)
