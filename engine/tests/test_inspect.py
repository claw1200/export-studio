"""Header checks for files that no longer match the rekordbox row."""

import struct
import tempfile
import unittest
import wave
from pathlib import Path

from exportstudio_engine.inspect import inspect_file, same_fingerprint


def _wav(path: Path, seconds: int = 1, rate: int = 44100) -> None:
    with wave.open(str(path), "w") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(rate)
        handle.writeframes(b"\x00\x00" * rate * seconds)


def _float_wav(path: Path) -> None:
    channels, rate, bits = 2, 44100, 32
    block = channels * bits // 8
    fmt = struct.pack("<HHIIHH", 3, channels, rate, rate * block, block, bits)
    data = b"\x00" * block
    body = b"fmt " + struct.pack("<I", 16) + fmt + b"data" + struct.pack("<I", len(data)) + data
    path.write_bytes(b"RIFF" + struct.pack("<I", 4 + len(body)) + b"WAVE" + body)


def _codes(result: dict) -> set[str]:
    return {issue["code"] for issue in result["issues"]}


def _detail(result: dict, code: str) -> str:
    return next(issue["detail"] for issue in result["issues"] if issue["code"] == code)


class InspectFileTest(unittest.TestCase):
    def test_wav_stored_as_mp3(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            _wav(path)
            result = inspect_file(path, "MP3", path.stat().st_size, 1)

        self.assertEqual(_codes(result), {"type"})
        self.assertEqual(_detail(result, "type"), "Rekordbox says MP3, the file is WAV")

    def test_missing_file(self) -> None:
        path = Path(tempfile.gettempdir()) / "exportstudio-missing-tune.wav"
        result = inspect_file(path, "WAV", 100, 1)
        self.assertEqual(_codes(result), {"missing"})
        self.assertEqual(result["mtime"], None)

    def test_missing_wav_still_flagged_when_rekordbox_says_mp3(self) -> None:
        path = Path(tempfile.gettempdir()) / "exportstudio-missing-tune.wav"
        result = inspect_file(path, "MP3", 0, 0)
        self.assertEqual(_codes(result), {"missing", "type"})

    def test_empty_file_is_unreadable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "empty.wav"
            path.write_bytes(b"")
            result = inspect_file(path, "WAV", 0, 0)

        self.assertEqual(_codes(result), {"unreadable"})

    def test_float_wav_is_not_cdj_safe(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "float.wav"
            _float_wav(path)
            result = inspect_file(path, "WAV", path.stat().st_size, 0)

        self.assertEqual(_codes(result), {"incompatible"})
        self.assertEqual(_detail(result, "incompatible"), "WAV is 32-bit float")

    def test_size_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            _wav(path)
            result = inspect_file(path, "WAV", path.stat().st_size + 10, 1)

        self.assertEqual(_codes(result), {"size"})
        self.assertEqual(_detail(result, "size"), "File size doesn't match the library")

    def test_length_slack(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            _wav(path)
            size = path.stat().st_size
            inside = inspect_file(path, "WAV", size, 3)
            outside = inspect_file(path, "WAV", size, 4)

        self.assertNotIn("duration", _codes(inside))
        self.assertEqual(_codes(outside), {"duration"})
        self.assertEqual(_detail(outside, "duration"), "Length doesn't match the library")

    def test_matching_wav_has_no_issues(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            _wav(path)
            result = inspect_file(path, "WAV", path.stat().st_size, 1)

        self.assertEqual(result["issues"], [])
        self.assertRegex(result["mtime"], r"^\d+$")

    def test_fingerprint_skips_an_unchanged_file_and_notices_a_rewrite(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            _wav(path)
            stat = path.stat()
            self.assertEqual(
                same_fingerprint(path, stat.st_size, str(stat.st_mtime_ns)),
                (stat.st_size, stat.st_mtime_ns),
            )
            path.write_bytes(path.read_bytes() + b"\x00")
            self.assertIsNone(same_fingerprint(path, stat.st_size, stat.st_mtime_ns))
            missing = Path(directory) / "gone.wav"
            self.assertEqual(same_fingerprint(missing, 0, None), (0, None))
            self.assertIsNone(same_fingerprint(missing, 12, None))


if __name__ == "__main__":
    unittest.main()
