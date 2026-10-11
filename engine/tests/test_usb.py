"""USB browse and sync. These tests use temporary directories only."""

import struct
import tempfile
import unittest
from datetime import date
from pathlib import Path

from exportstudio_engine.usb.anlzio import apply_cues, ppth_path
from exportstudio_engine.usb.browse import detail, tracks
from exportstudio_engine.usb.device import (
    DeviceHistory,
    DevicePlaylist,
    DeviceTrack,
    read_device,
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
from exportstudio_engine.usb.sync import (
    CueSource,
    ExportBundle,
    SourcePlaylist,
    SourceTrack,
    sync_volume,
)
from exportstudio_engine.usb.volumes import eject_volume, list_volumes


def _empty_lists(with_comments: bool) -> bytes:
    def pcob(cue_type: int) -> bytes:
        body = struct.pack(">IHHI", cue_type, 0, 0, 0xFFFFFFFF)
        return b"PCOB" + struct.pack(">II", 24, 12 + len(body)) + body

    def pco2(cue_type: int) -> bytes:
        body = struct.pack(">IHH", cue_type, 0, 0)
        return b"PCO2" + struct.pack(">II", 20, 12 + len(body)) + body

    body = bytearray(28)
    body[:4] = b"PMAI"
    struct.pack_into(">I", body, 4, 28)
    body += pcob(1) + pcob(0)
    if with_comments:
        body += pco2(1) + pco2(0)
    struct.pack_into(">I", body, 8, len(body))
    return bytes(body)


def _memory_cues(data: bytes) -> dict[bytes, list[tuple[int, str]]]:
    found: dict[bytes, list[tuple[int, str]]] = {b"PCOB": [], b"PCO2": []}
    header_len = struct.unpack_from(">I", data, 4)[0]
    pos = header_len
    while pos + 12 <= len(data):
        kind = data[pos : pos + 4]
        _len_header, len_tag = struct.unpack_from(">II", data, pos + 4)
        if len_tag < 12 or pos + len_tag > len(data):
            break
        if kind == b"PCOB" and struct.unpack_from(">I", data, pos + 12)[0] == 0:
            count = struct.unpack_from(">H", data, pos + 0x12)[0]
            entry = pos + 24
            for _ in range(count):
                found[b"PCOB"].append((struct.unpack_from(">I", data, entry + 0x20)[0], ""))
                entry += 56
        if kind == b"PCO2" and struct.unpack_from(">I", data, pos + 12)[0] == 0:
            count = struct.unpack_from(">H", data, pos + 0x10)[0]
            entry = pos + 20
            for _ in range(count):
                length = struct.unpack_from(">I", data, entry + 8)[0]
                comment_len = struct.unpack_from(">I", data, entry + 0x28)[0]
                raw = data[entry + 0x2C : entry + 0x2C + max(0, comment_len - 2)]
                found[b"PCO2"].append((struct.unpack_from(">I", data, entry + 0x14)[0], raw.decode("utf-16-be")))
                entry += length
        pos += len_tag
    return found


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

    def test_memory_cue_is_written_into_both_analysis_files(self) -> None:
        cue = CueSource(0, 30000, comment="intro", color=0xFF005C, color_index=46)
        dat = apply_cues(_empty_lists(False), [cue])
        ext = apply_cues(_empty_lists(True), [cue])
        self.assertEqual(_memory_cues(dat)[b"PCOB"], [(30000, "")])
        self.assertEqual(_memory_cues(dat)[b"PCO2"], [])
        self.assertEqual(_memory_cues(ext)[b"PCOB"], [(30000, "")])
        self.assertEqual(_memory_cues(ext)[b"PCO2"], [(30000, "intro")])
        self.assertEqual(apply_cues(_empty_lists(True), []), _empty_lists(True))
        memory = ext[ext.index(b"PCOB", ext.index(b"PCOB") + 4) :]
        self.assertEqual(struct.unpack_from(">II", memory, 4), (24, 80))
        self.assertEqual(struct.unpack_from(">IHHI", memory, 12), (0, 0, 1, 0))
        entry = memory[24:80]
        self.assertEqual(entry[0:4], b"PCPT")
        self.assertEqual(struct.unpack_from(">IIIII", entry, 4), (28, 56, 0, 0, 0x00010000))
        self.assertEqual(entry[0x1C:0x20], b"\x01\x00\x03\xe8")
        self.assertEqual(struct.unpack_from(">I", entry, 0x24)[0], 0xFFFFFFFF)
        comment = ext[ext.rindex(b"PCP2") :]
        self.assertEqual(struct.unpack_from(">II", comment, 4), (16, 88 + 12))
        self.assertEqual(comment[0x1C:0x1E], bytes((46, 1)))
        self.assertEqual(comment[0x2C + 12 : 0x2C + 16], bytes((46, 0xFF, 0x00, 0x5C)))

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


if __name__ == "__main__":
    unittest.main()
