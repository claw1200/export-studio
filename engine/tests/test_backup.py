"""Library backups. These tests use temporary directories and plain SQLite databases only."""

import os
import sqlite3
import tempfile
import time
import unittest
import zipfile
from pathlib import Path

from exportstudio_engine.backup import BLOCKED, Backups


def _verify(path: Path) -> dict:
    with sqlite3.connect(path) as conn:
        if conn.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise ValueError("damaged")
        return {"tracks": conn.execute("SELECT COUNT(*) FROM tracks").fetchone()[0], "playlists": 0}


def _make_library(root: Path, titles: list[str]) -> Path:
    src = root / "rekordbox" / "master.db"
    src.parent.mkdir(parents=True)
    with sqlite3.connect(src) as conn:
        conn.execute("CREATE TABLE tracks (title TEXT)")
        conn.executemany("INSERT INTO tracks VALUES (?)", [(t,) for t in titles])
    (src.parent / "masterPlaylists6.xml").write_text("<playlists/>")
    anlz = src.parent / "share" / "PIONEER" / "USBANLZ" / "abc" / "ANLZ0000.DAT"
    anlz.parent.mkdir(parents=True)
    anlz.write_bytes(b"PMAI original")
    art = src.parent / "share" / "PIONEER" / "Artwork" / "a.jpg"
    art.parent.mkdir(parents=True)
    art.write_bytes(b"\xff\xd8 jpeg")
    return src


def _titles(src: Path) -> list[str]:
    with sqlite3.connect(src) as conn:
        return [r[0] for r in conn.execute("SELECT title FROM tracks ORDER BY rowid")]


class BackupTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.src = _make_library(self.root, ["one", "two"])
        self.running = False
        self.backups = Backups(self.root / "backups", verify=_verify, rekordbox_running=lambda: self.running)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _touch_db(self, title: str) -> None:
        with sqlite3.connect(self.src) as conn:
            conn.execute("INSERT INTO tracks VALUES (?)", (title,))
        later = time.time() + len(_titles(self.src))
        os.utime(self.src, (later, later))

    def test_round_trip(self) -> None:
        made = self.backups.create(self.src, "manual")
        self.assertEqual(made["tracks"], 2)
        listed = self.backups.list_backups(self.src)
        self.assertEqual([b["id"] for b in listed], [made["id"]])
        self.assertFalse(listed[0]["otherLibrary"])
        with zipfile.ZipFile(made["path"]) as zf:
            names = set(zf.namelist())
        self.assertIn("master.db", names)
        self.assertIn("masterPlaylists6.xml", names)
        self.assertIn("share/PIONEER/USBANLZ/abc/ANLZ0000.DAT", names)
        self.assertIn("share/PIONEER/Artwork/a.jpg", names)

        self._touch_db("three")
        anlz = self.src.parent / "share" / "PIONEER" / "USBANLZ" / "abc" / "ANLZ0000.DAT"
        anlz.write_bytes(b"PMAI edited")
        extra = self.src.parent / "share" / "PIONEER" / "USBANLZ" / "new" / "ANLZ0000.DAT"
        extra.parent.mkdir(parents=True)
        extra.write_bytes(b"new")

        result = self.backups.restore(made["id"], self.src)
        self.assertEqual(result["tracks"], 2)
        self.assertEqual(_titles(self.src), ["one", "two"])
        self.assertEqual(anlz.read_bytes(), b"PMAI original")
        self.assertFalse(extra.exists())
        self.assertFalse(any(self.src.parent.glob(".exportstudio-restore-*")))

        kept = [b for b in self.backups.list_backups() if b["reason"] == "pre-restore"]
        self.assertEqual(len(kept), 1)
        self.assertEqual(result["kept"], kept[0]["path"])
        with zipfile.ZipFile(kept[0]["path"]) as zf:
            self.assertEqual(zf.read("share/PIONEER/USBANLZ/abc/ANLZ0000.DAT"), b"PMAI edited")
        self.assertFalse(any((self.root / "backups").glob("replaced-*")))

    def test_automatic_skips_when_unchanged(self) -> None:
        self.assertIsNotNone(self.backups.create(self.src, "before-edit"))
        self.assertIsNone(self.backups.create(self.src, "daily"))
        self._touch_db("three")
        self.assertIsNotNone(self.backups.create(self.src, "daily"))
        self.assertIsNotNone(self.backups.create(self.src, "manual"))

    def test_retention_keeps_manual(self) -> None:
        self.backups.set_settings(keep=5)
        manual = self.backups.create(self.src, "manual")
        for i in range(7):
            self._touch_db(f"t{i}")
            self.backups.create(self.src, "daily")
        listed = self.backups.list_backups()
        self.assertEqual(sum(b["reason"] == "daily" for b in listed), 5)
        self.assertIn(manual["id"], [b["id"] for b in listed])

    def test_partial_and_broken_zips_are_ignored(self) -> None:
        folder = self.root / "backups"
        folder.mkdir()
        (folder / "exportstudio-x.zip.partial").write_bytes(b"half")
        (folder / "broken.zip").write_bytes(b"not a zip")
        self.assertEqual(self.backups.list_backups(), [])

    def test_restore_when_database_missing(self) -> None:
        made = self.backups.create(self.src, "manual")
        self.src.unlink()
        self.backups.restore(made["id"], self.src)
        self.assertEqual(_titles(self.src), ["one", "two"])

    def test_restore_over_corrupt_database(self) -> None:
        made = self.backups.create(self.src, "manual")
        self.src.write_bytes(b"garbage" * 1000)
        result = self.backups.restore(made["id"], self.src)
        self.assertEqual(_titles(self.src), ["one", "two"])
        self.assertTrue(result["kept"])

    def test_damaged_database_is_not_backed_up(self) -> None:
        self.src.write_bytes(b"garbage" * 1000)
        with self.assertRaises(RuntimeError):
            self.backups.create(self.src, "manual")
        self.assertEqual(self.backups.list_backups(), [])
        self.assertFalse(any((self.root / "backups").glob("*.partial")))

    def test_restore_blocked_while_rekordbox_runs(self) -> None:
        made = self.backups.create(self.src, "manual")
        self.running = True
        with self.assertRaisesRegex(RuntimeError, BLOCKED):
            self.backups.restore(made["id"], self.src)

    def test_unknown_or_unsafe_ids_are_rejected(self) -> None:
        with self.assertRaises(KeyError):
            self.backups.restore("../evil", self.src)
        with self.assertRaises(KeyError):
            self.backups.delete("missing")

    def test_unexpected_files_block_restore(self) -> None:
        folder = self.root / "backups"
        folder.mkdir()
        with zipfile.ZipFile(folder / "evil.zip", "w") as zf:
            zf.writestr("manifest.json", "{}")
            zf.writestr("master.db", b"")
            zf.writestr("../outside.txt", b"x")
        with self.assertRaisesRegex(ValueError, "unexpected file"):
            self.backups.restore("evil", self.src)
        self.assertEqual(_titles(self.src), ["one", "two"])


if __name__ == "__main__":
    unittest.main()
