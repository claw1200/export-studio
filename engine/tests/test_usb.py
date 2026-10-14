"""USB browse and sync. These tests use temporary directories only."""

import json
import struct
import tempfile
import unittest
from datetime import date
from pathlib import Path

from exportstudio_engine.usb.anlzio import ppth_path
from exportstudio_engine.usb.browse import detail, libraries, tracks
from exportstudio_engine.usb.device import read_device
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
from exportstudio_engine.usb.selection import recorded_playlist_ids
from exportstudio_engine.usb.importing import (
    candidate,
    copy_new,
    destination,
    existing_copy,
    fat_safe,
    numbered,
)
from exportstudio_engine.usb.sync import (
    CueSource,
    ExportBundle,
    SourcePlaylist,
    SourceTrack,
    _job,
    sync_volume,
)
from exportstudio_engine.usb.volumes import eject_volume, format_volume, list_volumes


class ParseTests(unittest.TestCase):
    def test_analysis_hash_matches_the_published_example(self) -> None:
        path = "/Contents/Hosanna, Westend/Drum Death - Extended Mix/20130688_drum_death_(extended_mix).mp3"
        self.assertEqual(analysis_dir(path, "PIONEER"), "/PIONEER/USBANLZ/P002/0002583E")

    def test_devicesql_index_page_and_round_trip(self) -> None:
        library = PdbLibrary(
            artists=[NamedRow(1, "DJ"), NamedRow(2, "Café")],
            albums=[NamedRow(1, "Album", artist_id=1)],
            genres=[NamedRow(4, "House")],
            labels=[NamedRow(2, "Label")],
            keys=[NamedRow(3, "8A")],
            tracks=[
                TrackRow(
                    1,
                    "Song",
                    artist_id=1,
                    album_id=1,
                    genre_id=4,
                    label_id=2,
                    key_id=3,
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
        _type, _candidate, first, _last = struct.unpack_from("<4I", data, 0x1C)
        index = table_page(data, 0)
        self.assertEqual(struct.unpack_from("<I", index, 0x0C)[0], first + 1)
        data_page = data[(first + 1) * 4096 : (first + 2) * 4096]
        self.assertEqual(struct.unpack_from("<I", data_page, 0x2C)[0], 0x000C0700)
        self.assertEqual(struct.unpack_from("<I", data_page, 0x3C)[0], 0x00100001)
        self.assertEqual(struct.unpack_from("<HH", data_page, 0x40), (0xAE49, 0x03DD))
        _type, empty_page, _first, _last = struct.unpack_from("<4I", data, 0x1C + 9 * 16)
        self.assertEqual(struct.unpack_from("<I", table_page(data, 9), 0x0C)[0], empty_page)
        _type, _candidate, prop_index, _last = struct.unpack_from("<4I", data, 0x1C + 19 * 16)
        prop = data[(prop_index + 1) * 4096 : (prop_index + 2) * 4096]
        self.assertEqual(struct.unpack_from("<I", prop, 0x2C)[0], 1)
        self.assertIn(b"2026-10-08", prop)
        library.preserved[19] = [bytes.fromhex("80020000000000000000000017323032362D30392D3137191E0B3130303003000000000000000000")]
        rewritten = build_pdb(library, today=date(2026, 10, 8))
        _type, _candidate, prop_index, _last = struct.unpack_from("<4I", rewritten, 0x1C + 19 * 16)
        prop = rewritten[(prop_index + 1) * 4096 : (prop_index + 2) * 4096]
        self.assertEqual(struct.unpack_from("<I", prop, 0x2C)[0], 1)
        got = read_pdb(data)
        self.assertEqual(got.tracks[0].title, "Song")
        self.assertEqual(got.tracks[0].genre_id, 4)
        self.assertEqual(got.tracks[0].key_id, 3)
        self.assertTrue(got.tracks[0].analysis.endswith("ANLZ0000.DAT"))
        self.assertEqual([(row.export_id, row.name) for row in got.genres], [(4, "House")])
        self.assertEqual([(row.export_id, row.name) for row in got.labels], [(2, "Label")])
        self.assertEqual([(row.export_id, row.name) for row in got.keys], [(3, "8A")])
        self.assertEqual(got.artists[1].name, "Café")
        self.assertEqual(
            [(row.export_id, row.name, row.parent_id, row.folder) for row in got.playlists],
            [(1, "Folder", 0, True), (2, "Set", 1, False)],
        )

    def test_onelibrary_round_trip(self) -> None:
        from exportstudio_engine.usb.device import from_onelibrary
        from exportstudio_engine.usb.onelib import connect, create_empty

        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "exportLibrary.db"
            conn = connect(path, encrypt=False)
            try:
                create_empty(conn)
                conn.execute(
                    "INSERT INTO content(content_id, title, path, fileName) VALUES (?, ?, ?, ?)",
                    (1, "Song", "/Contents/DJ/Album/song.mp3", "song.mp3"),
                )
                conn.execute(
                    "INSERT INTO cue(cue_id, content_id, kind, inUsec, cueComment) VALUES (?, ?, ?, ?, ?)",
                    (1, 1, 1, 1_500_000, "drop"),
                )
                conn.execute(
                    """INSERT INTO playlist(
                        playlist_id, name, sequenceNo, attribute, playlist_id_parent
                    ) VALUES (?, ?, ?, ?, ?)""",
                    (1, "Set", 0, 0, 0),
                )
                conn.execute(
                    "INSERT INTO playlist_content(playlist_id, content_id, sequenceNo) VALUES (?, ?, ?)",
                    (1, 1, 1),
                )
                conn.execute(
                    """INSERT INTO history(
                        history_id, name, sequenceNo, attribute, history_id_parent
                    ) VALUES (?, ?, ?, ?, ?)""",
                    (3, "Last night", 0, 0, 0),
                )
                conn.execute(
                    "INSERT INTO history_content(history_id, content_id, sequenceNo) VALUES (?, ?, ?)",
                    (3, 1, 1),
                )
                conn.commit()
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


class SyncTests(unittest.TestCase):
    def _bundle(self, src: Path, analysis: Path, title: str = "Song") -> ExportBundle:
        return ExportBundle(
            playlists=[SourcePlaylist("10", "Set", None, 1, False, ["1"])],
            tracks=[
                SourceTrack(
                    "1",
                    title,
                    "DJ",
                    "Album",
                    src,
                    analysis,
                    src.name,
                    bpm_x100=12800,
                    duration=10,
                    comment="Hi",
                    cues=[CueSource(0, 30000, comment="intro")],
                    analysed=105,
                )
            ],
            dbid="9",
        )

    def test_export_writes_both_libraries_and_a_second_sync_keeps_the_track(self) -> None:
        from exportstudio_engine.anlz_build import path_tag
        from exportstudio_engine.usb.onelib import connect

        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            volume = root / "VOL"
            volume.mkdir()
            src = root / "song.mp3"
            src.write_bytes(b"audio-v1")
            analysis = root / "ANLZ0000.DAT"
            tag = path_tag("/local/song.mp3")
            body = bytearray(28)
            body[:4] = b"PMAI"
            struct.pack_into(">I", body, 4, 28)
            body += tag
            struct.pack_into(">I", body, 8, len(body))
            analysis.write_bytes(body)
            analysis.with_suffix(".EXT").write_bytes(body)
            sync_volume(volume, self._bundle(src, analysis))
            device = read_device(volume)
            self.assertEqual(len(device.tracks), 1)
            self.assertEqual(device.tracks[0].title, "Song")
            self.assertEqual(device.tracks[0].path, "/Contents/DJ/Album/song.mp3")
            self.assertTrue((volume / device.tracks[0].path.lstrip("/")).is_file())
            stored = (volume / device.tracks[0].analysis.lstrip("/")).read_bytes()
            self.assertEqual(ppth_path(stored), device.tracks[0].path)
            self.assertIn((30000).to_bytes(4, "big"), stored)
            extended = (volume / device.tracks[0].analysis.lstrip("/")).with_suffix(".EXT").read_bytes()
            self.assertIn("intro".encode("utf-16-be"), extended)
            self.assertEqual([row.name for row in device.playlists], ["Set"])
            self.assertEqual(device.playlists[0].track_ids, [device.tracks[0].export_id])
            rows = tracks(volume)
            info = detail(rows[0]["id"])
            self.assertTrue(info["fileExists"])
            sync_bytes = (volume / "PIONEER" / "rekordbox" / "playlists3.sync").read_bytes()
            self.assertEqual(sync_bytes, (volume / "PIONEER" / "rekordbox" / "playlists3Plus.sync").read_bytes())
            self.assertIn(b'DBID="9"', sync_bytes)
            self.assertIn(b"\r\n", sync_bytes)
            conn = connect(volume / "PIONEER" / "rekordbox" / "exportLibrary.db")
            try:
                content = conn.execute(
                    "SELECT title, path, analysisDataFilePath, analysedBits FROM content"
                ).fetchall()
                playlists = conn.execute("SELECT name FROM playlist").fetchall()
                cues = conn.execute("SELECT kind, inUsec FROM cue").fetchall()
            finally:
                conn.close()
            self.assertEqual(len(content), 1)
            self.assertEqual(content[0]["title"], "Song")
            self.assertEqual(content[0]["path"], device.tracks[0].path)
            self.assertEqual(content[0]["analysisDataFilePath"], device.tracks[0].analysis)
            self.assertEqual(content[0]["analysedBits"], 41)
            self.assertEqual([row["name"] for row in playlists], ["Set"])
            self.assertEqual(cues[0]["kind"], 0)
            self.assertEqual(cues[0]["inUsec"], 30_000_000)
            first = device.tracks[0].export_id
            sync_volume(volume, self._bundle(src, analysis))
            again = read_device(volume)
            self.assertEqual(len(again.tracks), 1)
            self.assertEqual(again.tracks[0].export_id, first)
            self.assertEqual(again.tracks[0].title, "Song")
            libs = libraries(volume)
            self.assertEqual([row["id"] for row in libs], ["device", "one"])
            self.assertEqual(
                [playlist["name"] for lib in libs for playlist in lib["playlists"]],
                ["Set", "Set"],
            )


class SelectionTests(unittest.TestCase):
    def test_manifest_and_matching_sync_record_become_library_ids(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            volume = Path(raw)
            root = volume / "PIONEER"
            (root / "exportstudio").mkdir(parents=True)
            (root / "rekordbox").mkdir()
            (root / "exportstudio" / "manifest.json").write_text(
                json.dumps(
                    {
                        "version": 1,
                        "playlists": [
                            {"device_only": False, "library_id": 16, "name": "Kept"},
                            {"device_only": True, "library_id": 32, "name": "jungle"},
                            {"library_id": 0, "name": "Loose"},
                        ],
                    }
                ),
                encoding="utf-8",
            )
            (root / "rekordbox" / "playlists3.sync").write_text(
                """<?xml version="1.0" encoding="UTF-8"?>\r\n\r\n"""
                """<Sync DBID="9" AutomaticSync="0" AllPlaylists="0" IncludeCue="1" ForcedSync="0" Timestamp="0">\r\n"""
                """  <Playlists>\r\n"""
                """    <NODE Id="0" ParentId="0" Attribute="1" Lib_Type="0" Dev_ID="0" Timestamp="0" CheckType="2"/>\r\n"""
                """    <NODE Id="14" ParentId="0" Attribute="0" Lib_Type="0" Dev_ID="1" Timestamp="5" CheckType="1"/>\r\n"""
                """  </Playlists>\r\n"""
                """</Sync>\r\n""",
                encoding="utf-8",
            )
            self.assertEqual(recorded_playlist_ids(volume, ["16", "20", "32", "99"], 9), ["16", "20"])

    def test_another_library_and_a_missing_record_add_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            volume = Path(raw)
            root = volume / "PIONEER" / "rekordbox"
            root.mkdir(parents=True)
            (root / "playlists3.sync").write_text(
                '<Sync DBID="9"><Playlists>'
                '<NODE Id="14" Attribute="0" CheckType="1"/>'
                "</Playlists></Sync>",
                encoding="utf-8",
            )
            self.assertEqual(recorded_playlist_ids(volume, ["20"], 8), [])
            self.assertEqual(recorded_playlist_ids(volume / "empty", ["20"], 9), [])


class OverwriteJobTests(unittest.TestCase):
    def test_overwrite_is_optional_on_the_export_job(self) -> None:
        bundle = ExportBundle(
            playlists=[SourcePlaylist("10", "Set", None, 1, False, ["1"])],
            tracks=[
                SourceTrack("1", "Song", "DJ", "Album", Path("/song.mp3"), None, "song.mp3"),
            ],
        )
        self.assertFalse(_job(Path("/Volumes/USB"), bundle)["overwrite"])
        self.assertTrue(_job(Path("/Volumes/USB"), bundle, overwrite=True)["overwrite"])


class ImportCopyTests(unittest.TestCase):
    def test_names_follow_the_vendor_import_layout(self) -> None:
        self.assertEqual(
            destination(Path("/Users/dj/Music")),
            Path("/Users/dj/Music/PioneerDJ/Imported from Device"),
        )
        self.assertEqual(fat_safe("A/B:C*D?E"), "A_B_C_D_E")
        self.assertEqual(numbered("song.mp3", 1), "song-1.mp3")
        self.assertEqual(
            candidate(Path("/music"), "DJ", "", "song.mp3"),
            Path("/music/Contents/DJ/UnknownAlbum/song.mp3"),
        )

    def test_an_identical_copy_is_reused_and_a_different_file_is_kept(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            source = root / "song.mp3"
            source.write_bytes(b"audio-v1")
            wanted = candidate(root / "imported", "DJ", "Album", "song.mp3")
            first = copy_new(source, wanted)
            self.assertEqual(first, wanted)
            self.assertEqual(first.read_bytes(), b"audio-v1")
            self.assertEqual(existing_copy(source, wanted), first)
            other = root / "other.mp3"
            other.write_bytes(b"audio-v2")
            second = copy_new(other, wanted)
            self.assertEqual(second.name, "song-1.mp3")
            self.assertEqual(wanted.read_bytes(), b"audio-v1")
            self.assertEqual(existing_copy(other, wanted), second)


class FormatTests(unittest.TestCase):
    def test_only_a_listed_removable_volume_is_formatted(self) -> None:
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as raw:
            target = Path(raw) / "Stick"
            target.mkdir()
            listed = [{"path": str(target), "ejectable": False, "name": "Stick"}]
            with self.assertRaises(ValueError):
                format_volume(str(target), listed)
            listed = [{"path": str(target), "ejectable": True, "name": "My Stick"}]
            info = {"RemovableMedia": True, "Ejectable": True, "Internal": False, "DeviceIdentifier": "disk9s1"}
            with (
                patch("exportstudio_engine.usb.volumes._disk_info", return_value=info),
                patch("exportstudio_engine.usb.volumes.subprocess.run") as run,
            ):
                format_volume(str(target), listed)
            run.assert_called_once()
            command = run.call_args.args[0]
            self.assertEqual(command[:3], ["diskutil", "eraseVolume", "FAT32"])
            self.assertEqual(command[3], "MY_STICK")
            self.assertEqual(command[4], "disk9s1")


if __name__ == "__main__":
    unittest.main()
