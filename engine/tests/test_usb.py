"""USB browse and sync. These tests use temporary directories only."""

import os
import struct
import tempfile
import unittest
from datetime import date
from pathlib import Path

from exportstudio_engine.usb.anlzio import ppth_path
from exportstudio_engine.usb.browse import detail, tracks
from exportstudio_engine.usb.device import (
    DeviceHistory,
    DevicePlaylist,
    DeviceTrack,
    read_device,
    to_pdb,
    write_onelibrary,
)
from exportstudio_engine.usb.paths import analysis_dir
from exportstudio_engine.usb.pdb import (
    EntryRow,
    NamedRow,
    PdbLibrary,
    PlaylistRow,
    TrackRow,
    build_pdb,
    read_pdb,
    table_page,
)
from exportstudio_engine.usb.sync import Conflict, ExportBundle, SourcePlaylist, SourceTrack, VerifyError, sync_volume
from exportstudio_engine.usb.volumes import eject_volume, list_volumes


def _anlz(path: str) -> bytes:
    encoded = path.encode("utf-16-be") + b"\x00\x00"
    body = bytearray(28)
    body[:4] = b"PMAI"
    struct.pack_into(">I", body, 4, 28)
    tag = b"PPTH" + struct.pack(">III", 16, 16 + len(encoded), len(encoded)) + encoded
    body += tag
    struct.pack_into(">I", body, 8, len(body))
    return bytes(body)


class ParseTests(unittest.TestCase):
    def test_analysis_hash_matches_the_published_example(self) -> None:
        path = "/Contents/Hosanna, Westend/Drum Death - Extended Mix/20130688_drum_death_(extended_mix).mp3"
        self.assertEqual(analysis_dir(path, "PIONEER"), "/PIONEER/USBANLZ/P002/0002583E")

    def test_devicesql_index_page_and_round_trip(self) -> None:
        library = PdbLibrary(
            artists=[NamedRow(1, "DJ"), NamedRow(2, "Café")],
            albums=[NamedRow(1, "Album", artist_id=1)],
            tracks=[
                TrackRow(
                    1,
                    "Song",
                    artist_id=1,
                    album_id=1,
                    path="/Contents/DJ/Album/song.mp3",
                    filename="song.mp3",
                    analysis="/PIONEER/USBANLZ/P000/00000001/ANLZ0000.DAT",
                    comment="Hi",
                    bpm_x100=12800,
                    duration=10,
                )
            ],
            playlists=[PlaylistRow(1, "Folder", folder=True, seq=1), PlaylistRow(2, "Set", parent_id=1, seq=1)],
            entries=[EntryRow(1, 1, 2)],
        )
        data = build_pdb(library, today=date(2026, 10, 8))
        for table in range(20):
            page = table_page(data, table)
            self.assertEqual(page[0x1B], 0x64)
            self.assertEqual(struct.unpack_from("<H", page, 0x20)[0], 0x1FFF)
        got = read_pdb(data)
        self.assertEqual(got.tracks[0].title, "Song")
        self.assertTrue(got.tracks[0].analysis.endswith("ANLZ0000.DAT"))
        self.assertEqual(got.artists[1].name, "Café")
        self.assertEqual(
            [(row.export_id, row.name, row.parent_id, row.folder) for row in got.playlists],
            [(1, "Folder", 0, True), (2, "Set", 1, False)],
        )

    def test_onelibrary_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "exportLibrary.db"
            library_in = read_device_from_rows()
            write_onelibrary(path, library_in, encrypt=False)
            self.assertTrue(path.read_bytes().startswith(b"SQLite format 3"))
            from exportstudio_engine.usb.onelib import connect
            from exportstudio_engine.usb.device import from_onelibrary

            conn = connect(path)
            try:
                got = from_onelibrary(conn, "PIONEER")
            finally:
                conn.close()
            self.assertEqual(got.tracks[0].title, "Song")
            self.assertEqual(got.tracks[0].cues[0].start_ms, 1500)
            self.assertEqual(got.playlists[0].track_ids, [1])
            self.assertEqual(got.histories[0].name, "Last night")

    def test_volume_list_uses_the_existing_root_spelling(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / "Empty").mkdir()
            stick = root / "Stick"
            (stick / "PIONEER" / "rekordbox").mkdir(parents=True)
            (stick / "PIONEER" / "rekordbox" / "export.pdb").write_bytes(b"x")
            dot = root / "Dot"
            (dot / ".PIONEER" / "rekordbox").mkdir(parents=True)
            (dot / ".PIONEER" / "rekordbox" / "exportLibrary.db").write_bytes(b"x")
            found = {row["name"]: row for row in list_volumes(root)}
            self.assertFalse(found["Empty"]["hasLibrary"])
            self.assertEqual(found["Stick"]["rootName"], "PIONEER")
            self.assertTrue(found["Stick"]["hasLibrary"])
            self.assertEqual(found["Dot"]["rootName"], ".PIONEER")
            self.assertFalse(found["Stick"]["ejectable"])


class EjectTests(unittest.TestCase):
    def test_only_a_listed_removable_volume_is_ejected(self) -> None:
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as raw:
            target = Path(raw) / "Stick"
            target.mkdir()
            with self.assertRaises(ValueError):
                eject_volume(str(target), [{"path": str(target), "ejectable": False}])
            with patch("exportstudio_engine.usb.volumes.subprocess.run") as run:
                eject_volume(str(target), [{"path": str(target), "ejectable": True}])
            run.assert_called_once()
            self.assertEqual(run.call_args.args[0][:2], ["diskutil", "eject"])
            self.assertEqual(Path(run.call_args.args[0][2]).resolve(), target.resolve())


def read_device_from_rows():
    from exportstudio_engine.usb.device import CueRow, DeviceLibrary

    return DeviceLibrary(
        "PIONEER",
        tracks=[
            DeviceTrack(
                1,
                "9",
                "Song",
                artist="DJ",
                album="Album",
                path="/Contents/DJ/Album/song.mp3",
                filename="song.mp3",
                cues=[CueRow(kind=1, start_ms=1500, comment="drop")],
            )
        ],
        playlists=[DevicePlaylist(1, "4", "Set", track_ids=[1])],
        histories=[DeviceHistory(3, "Last night", track_ids=[1])],
    )


class SyncTests(unittest.TestCase):
    def _bundle(self, src: Path, analysis: Path, title: str = "Song") -> ExportBundle:
        return ExportBundle(
            playlists=[SourcePlaylist("10", "Set", None, 1, False, ["1"])],
            tracks=[
                SourceTrack("1", title, "DJ", "Album", src, analysis, "song?.mp3", bpm_x100=12800, duration=10, comment="Hi")
            ],
            dbid="9",
        )

    def test_round_trip_keeps_ids_and_device_playlists(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            volume = root / "VOL"
            volume.mkdir()
            src = root / "song.mp3"
            src.write_bytes(b"audio-v1")
            analysis = root / "ANLZ0000.DAT"
            analysis.write_bytes(_anlz("/local/song.mp3"))
            sync_volume(volume, self._bundle(src, analysis), encrypt=False)
            device = read_device(volume)
            self.assertEqual(device.tracks[0].path, "/Contents/DJ/Album/song_.mp3")
            stored = (volume / device.tracks[0].analysis.lstrip("/")).read_bytes()
            self.assertEqual(ppth_path(stored), device.tracks[0].path)
            self.assertTrue((volume / "PIONEER" / "rekordbox" / "export.pdb").is_file())
            self.assertTrue((volume / "PIONEER" / "rekordbox" / "exportLibrary.db").is_file())
            rows = tracks(volume)
            info = detail(rows[0]["id"])
            self.assertTrue(info["fileExists"])
            self.assertTrue(info["path"].endswith("song_.mp3"))
            first = device.tracks[0].export_id
            device.tracks.append(
                DeviceTrack(50, None, "Extra", "Other", "Album", path="/Contents/Other/Album/extra.mp3", filename="extra.mp3")
            )
            device.playlists.append(DevicePlaylist(50, None, "On device", track_ids=[50]))
            device.histories.append(DeviceHistory(7, "Last night", track_ids=[50]))
            extra = volume / "Contents" / "Other" / "Album" / "extra.mp3"
            extra.parent.mkdir(parents=True)
            extra.write_bytes(b"extra")
            (volume / "PIONEER" / "rekordbox" / "export.pdb").write_bytes(to_pdb(device))
            write_onelibrary(volume / "PIONEER" / "rekordbox" / "exportLibrary.db", device, encrypt=False)
            sync_volume(volume, self._bundle(src, analysis), encrypt=False)
            again = read_device(volume)
            self.assertEqual(again.track(first).title, "Song")
            self.assertEqual(again.track(first).export_id, first)
            self.assertTrue(any(row.title == "Extra" for row in again.tracks))
            self.assertTrue(any(row.name == "On device" for row in again.playlists))
            self.assertTrue(any(row.name == "Last night" for row in again.histories))
            sync_bytes = (volume / "PIONEER" / "rekordbox" / "playlists3.sync").read_bytes()
            self.assertEqual(sync_bytes, (volume / "PIONEER" / "rekordbox" / "playlists3Plus.sync").read_bytes())
            self.assertIn(b'DBID="9"', sync_bytes)
            self.assertIn(b"\r\n", sync_bytes)
            self.assertIn(b"?>\r\n\r\n<Playlists", sync_bytes)

    def test_failed_publication_leaves_the_export(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            volume = root / "VOL"
            volume.mkdir()
            src = root / "song.mp3"
            src.write_bytes(b"audio-v1")
            analysis = root / "ANLZ0000.DAT"
            analysis.write_bytes(_anlz("/local/song.mp3"))
            sync_volume(volume, self._bundle(src, analysis), encrypt=False)
            previous = (volume / "PIONEER" / "rekordbox" / "export.pdb").read_bytes()
            src.write_bytes(b"audio-v2-xxxx")

            def break_stage(event: dict) -> None:
                if event["phase"] == "staged":
                    for path in (volume / ".exportstudio-publication").rglob("*.mp3"):
                        path.unlink()

            with self.assertRaises(VerifyError):
                sync_volume(volume, self._bundle(src, analysis, "Changed"), encrypt=False, on_progress=break_stage)
            self.assertEqual((volume / "PIONEER" / "rekordbox" / "export.pdb").read_bytes(), previous)
            self.assertEqual((volume / "Contents" / "DJ" / "Album" / "song_.mp3").read_bytes(), b"audio-v1")
            self.assertFalse((volume / ".exportstudio-publication").exists())

            def touch_destination(event: dict) -> None:
                if event["phase"] == "verifying":
                    os.utime(volume / "PIONEER" / "rekordbox" / "export.pdb", None)

            with self.assertRaises(Conflict):
                sync_volume(volume, self._bundle(src, analysis, "Changed"), encrypt=False, on_progress=touch_destination)
            self.assertEqual((volume / "Contents" / "DJ" / "Album" / "song_.mp3").read_bytes(), b"audio-v1")
            self.assertFalse((volume / ".exportstudio-publication").exists())

    def test_pdb_only_destination_gains_the_other_database(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            volume = root / "VOL"
            (volume / "PIONEER" / "rekordbox").mkdir(parents=True)
            foreign = build_pdb(
                PdbLibrary(
                    tracks=[TrackRow(3, "Kept", artist_id=1, path="/Contents/Other/Album/extra.mp3", filename="extra.mp3")],
                    artists=[NamedRow(1, "Other")],
                    playlists=[PlaylistRow(9, "Foreign")],
                ),
                today=date(2026, 10, 8),
            )
            (volume / "PIONEER" / "rekordbox" / "export.pdb").write_bytes(foreign)
            extra = volume / "Contents" / "Other" / "Album" / "extra.mp3"
            extra.parent.mkdir(parents=True)
            extra.write_bytes(b"extra")
            src = root / "song.mp3"
            src.write_bytes(b"audio-v1")
            analysis = root / "ANLZ0000.DAT"
            analysis.write_bytes(_anlz("/local/song.mp3"))
            sync_volume(volume, self._bundle(src, analysis), encrypt=False)
            device = read_device(volume)
            self.assertTrue(any(row.title == "Kept" for row in device.tracks))
            self.assertTrue(any(row.name == "Foreign" for row in device.playlists))
            self.assertTrue((volume / "PIONEER" / "rekordbox" / "exportLibrary.db").is_file())


if __name__ == "__main__":
    unittest.main()
