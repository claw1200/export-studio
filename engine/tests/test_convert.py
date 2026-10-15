import struct
import tempfile
import unittest
import wave
from pathlib import Path
from unittest import mock

from exportstudio_engine import convert


def _wav(path: Path, rate: int = 44100) -> None:
    with wave.open(str(path), "wb") as f:
        f.setnchannels(2)
        f.setsampwidth(2)
        f.setframerate(rate)
        f.writeframes(struct.pack("<hh", 0, 0) * rate)


class PresetTests(unittest.TestCase):
    def test_lossless_defaults(self) -> None:
        self.assertEqual(
            convert.clean_preset({"format": "wav"}), {"format": "wav", "bitDepth": 16, "sampleRate": 48000}
        )

    def test_lossy_takes_a_bitrate(self) -> None:
        self.assertEqual(convert.clean_preset({"format": "mp3", "bitrate": 320}), {"format": "mp3", "bitrate": 320})

    def test_rejects_unknown_values(self) -> None:
        with self.assertRaises(ValueError):
            convert.clean_preset({"format": "ogg"})
        with self.assertRaises(ValueError):
            convert.clean_preset({"format": "mp3", "bitrate": 1})
        with self.assertRaises(ValueError):
            convert.clean_preset({"format": "flac", "bitDepth": 8})


class OutputPathTests(unittest.TestCase):
    def test_same_name_new_extension(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "tune.wav"
            self.assertEqual(convert.output_path(source, ".flac"), Path(directory) / "tune.flac")

    def test_suffix_when_taken(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "tune.wav"
            (Path(directory) / "tune.flac").touch()
            self.assertEqual(convert.output_path(source, ".flac"), Path(directory) / "tune (1).flac")

    def test_suffix_when_same_extension(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "tune.wav"
            self.assertEqual(convert.output_path(source, ".wav"), Path(directory) / "tune (1).wav")


class CommandTests(unittest.TestCase):
    def test_mp3_uses_lame_at_the_bitrate(self) -> None:
        command = convert.build_command("ffmpeg", Path("a.wav"), Path("b.mp3"), {"format": "mp3", "bitrate": 320})
        self.assertIn("libmp3lame", command)
        self.assertIn("320k", command)

    def test_24_bit_flac_uses_32_bit_samples(self) -> None:
        preset = {"format": "flac", "bitDepth": 24, "sampleRate": 48000}
        command = convert.build_command("ffmpeg", Path("a.wav"), Path("b.flac"), preset)
        self.assertIn("s32", command)
        self.assertIn("48000", command)


class ConvertFileTests(unittest.TestCase):
    def setUp(self) -> None:
        try:
            convert.ffmpeg_path()
        except convert.ConvertError:
            self.skipTest("ffmpeg is not installed")

    def test_wav_becomes_flac_beside_the_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "tune.wav"
            _wav(source)
            target = convert.convert_file(source, {"format": "flac", "bitDepth": 16, "sampleRate": 48000})
            self.assertEqual(target, Path(directory) / "tune.flac")
            self.assertTrue(target.stat().st_size > 0)
            self.assertEqual(sorted(p.name for p in Path(directory).iterdir()), ["tune.flac", "tune.wav"])

    def test_failure_leaves_nothing_behind(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "broken.wav"
            source.write_bytes(b"not audio")
            with self.assertRaises(convert.ConvertError):
                convert.convert_file(source, {"format": "mp3", "bitrate": 320})
            self.assertEqual([p.name for p in Path(directory).iterdir()], ["broken.wav"])

    def test_missing_source(self) -> None:
        with self.assertRaises(convert.ConvertError):
            convert.convert_file(Path("/nonexistent/tune.wav"), {"format": "mp3", "bitrate": 320})


def _mp3(path: Path, delay: int | None, tag: bytes = b"Info", id3: bool = False) -> None:
    body = b"\xff\xfb\x90\x00" + bytes(32)
    if delay is not None:
        body += tag + struct.pack(">I", 1) + struct.pack(">I", 10)
        body += b"LAME3.100" + bytes(12) + ((delay << 12) | 1000).to_bytes(3, "big") + bytes(60)
    prefix = b"ID3\x03\x00\x00\x00\x00\x00\x05" + bytes(5) if id3 else b""
    path.write_bytes(prefix + body + bytes(400))


class Mp3DelayTests(unittest.TestCase):
    def test_reads_the_encoder_delay_and_adds_the_decoder_delay(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "a.mp3"
            _mp3(path, 576)
            self.assertEqual(convert.mp3_delay(path), (1105, 44100))

    def test_skips_an_id3_tag(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "a.mp3"
            _mp3(path, 576, tag=b"Xing", id3=True)
            self.assertEqual(convert.mp3_delay(path), (1105, 44100))

    def test_no_tag_means_no_correction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "a.mp3"
            _mp3(path, None)
            self.assertIsNone(convert.mp3_delay(path))
            self.assertEqual(convert.delay_offset_ms(Path(directory) / "a.wav", path), 0)

    def test_offset_direction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            mp3 = Path(directory) / "a.mp3"
            _mp3(mp3, 576)
            wav = Path(directory) / "a.wav"
            self.assertEqual(convert.delay_offset_ms(wav, mp3), 25)
            self.assertEqual(convert.delay_offset_ms(mp3, wav), -25)
            self.assertEqual(convert.delay_offset_ms(mp3, mp3), 0)


class ResolverTests(unittest.TestCase):
    def test_environment_wins(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / "ffmpeg"
            fake.touch()
            with mock.patch.dict("os.environ", {convert.FFMPEG_ENV: str(fake)}):
                self.assertEqual(convert.ffmpeg_path(), str(fake))


if __name__ == "__main__":
    unittest.main()
