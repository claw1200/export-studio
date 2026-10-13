"""Header measurements for local audio files."""

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


class InspectFileTest(unittest.TestCase):
    def test_wav_header_is_measured(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            _wav(path)
            result = inspect_file(path)

        self.assertEqual(result["issues"], [])
        self.assertEqual(
            result["audio"],
            {"kind": "WAV", "sampleRate": 44100, "bitDepth": 16, "channels": 1, "bitrate": 0, "encoding": "pcm"},
        )

    def test_missing_file(self) -> None:
        path = Path(tempfile.gettempdir()) / "exportstudio-missing-tune.wav"
        result = inspect_file(path)
        self.assertEqual(_codes(result), {"missing"})
        self.assertEqual(result["mtime"], None)
        self.assertIsNone(result["audio"])

    def test_empty_file_is_unreadable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "empty.wav"
            path.write_bytes(b"")
            result = inspect_file(path)

        self.assertEqual(_codes(result), {"unreadable"})
        self.assertIsNone(result["audio"])

    def test_float_wav_is_measured_as_float(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "float.wav"
            _float_wav(path)
            result = inspect_file(path)

        self.assertEqual(result["issues"], [])
        self.assertEqual(result["audio"]["encoding"], "float")
        self.assertEqual(result["audio"]["bitDepth"], 32)
        self.assertEqual(result["audio"]["kind"], "WAV")

    def test_matching_wav_has_no_issues(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tune.wav"
            _wav(path)
            result = inspect_file(path)

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
