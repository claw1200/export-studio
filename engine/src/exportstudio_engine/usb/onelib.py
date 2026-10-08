"""Device Library Plus / OneLibrary database.

Plain SQLite is accepted so tests can avoid a cipher. A real stick is
SQLCipher 4, opened with the passphrase published by pyrekordbox.
"""

import sqlite3
from pathlib import Path

from pyrekordbox.utils import deobfuscate

# Obfuscated OneLibrary passphrase from pyrekordbox (MIT), decoded at runtime.
_KEY_BLOB = b"PN_1dH8$oLJY)16j_RvM6qphWw`476>;C1cWmI#se(PG`j}~xAjlufj?`#0i{;=glh(SkW)y0>n?YEiD`l%t("

SCHEMA = """
CREATE TABLE album(album_id integer primary key, name varchar, artist_id integer, image_id integer, isComplation integer, nameForSearch varchar);
CREATE TABLE artist(artist_id integer primary key, name varchar, nameForSearch varchar);
CREATE TABLE category(category_id integer primary key, menuItem_id integer, sequenceNo integer, isVisible integer);
CREATE TABLE color(color_id integer primary key, name varchar);
CREATE TABLE content(content_id integer primary key, title varchar, titleForSearch varchar, subtitle varchar, bpmx100 integer, length integer, trackNo integer, discNo integer, artist_id_artist integer, artist_id_remixer integer, artist_id_originalArtist integer, artist_id_composer integer, artist_id_lyricist integer, album_id integer, genre_id integer, label_id integer, key_id integer, color_id integer, image_id integer, djComment varchar, rating integer, releaseYear integer, releaseDate varchar, dateCreated varchar, dateAdded varchar, path varchar, fileName varchar, fileSize integer, fileType integer, bitrate integer, bitDepth integer, samplingRate integer, isrc varchar, djPlayCount integer, isHotCueAutoLoadOn integer, isKuvoDeliverStatusOn integer, kuvoDeliveryComment varchar, masterDbId integer, masterContentId integer, analysisDataFilePath varchar, analysedBits integer, contentLink integer, hasModified integer, cueUpdateCount integer, analysisDataUpdateCount integer, informationUpdateCount integer);
CREATE TABLE cue(cue_id integer primary key, content_id integer, kind integer, colorTableIndex integer, cueComment varchar, isActiveLoop integer, beatLoopNumerator integer, beatLoopDenominator integer, inUsec integer, outUsec integer, in150FramePerSec integer, out150FramePerSec integer, inMpegFrameNumber integer, outMpegFrameNumber integer, inMpegAbs integer, outMpegAbs integer, inDecodingStartFramePosition integer, outDecodingStartFramePosition integer, inFileOffsetInBlock integer, OutFileOffsetInBlock integer, inNumberOfSampleInBlock integer, outNumberOfSampleInBlock integer);
CREATE TABLE genre(genre_id integer primary key, name varchar);
CREATE TABLE history(history_id integer primary key, sequenceNo integer, name varchar, attribute integer, history_id_parent integer);
CREATE TABLE history_content(history_id integer, content_id integer, sequenceNo integer);
CREATE TABLE hotCueBankList(hotCueBankList_id integer primary key, sequenceNo integer, name varchar, image_id integer, attribute integer, hotCueBankList_id_parent integer);
CREATE TABLE hotCueBankList_cue(hotCueBankList_id integer, cue_id integer, sequenceNo integer);
CREATE TABLE image(image_id integer primary key, path varchar);
CREATE TABLE "key"(key_id integer primary key, name varchar);
CREATE TABLE label(label_id integer primary key, name varchar);
CREATE TABLE menuItem(menuItem_id integer primary key, kind integer, name varchar);
CREATE TABLE myTag(myTag_id integer primary key, sequenceNo integer, name varchar, attribute integer, myTag_id_parent integer);
CREATE TABLE myTag_content(myTag_id integer, content_id integer);
CREATE TABLE playlist(playlist_id integer primary key, sequenceNo integer, name varchar, image_id integer, attribute integer, playlist_id_parent integer);
CREATE TABLE playlist_content(playlist_id integer, content_id integer, sequenceNo integer);
CREATE TABLE property(deviceName varchar, dbVersion varchar, numberOfContents integer, createdDate varchar, backGroundColorType integer, myTagMasterDBID integer);
CREATE TABLE recommendedLike(content_id_1 integer, content_id_2 integer, rating integer, createdDate integer);
CREATE TABLE sort(sort_id integer primary key, menuItem_id integer, sequenceNo integer, isVisible integer, isSelectedAsSubColumn integer);
"""

MENU_ITEMS = (
    (1, 128, "GENRE"),
    (2, 129, "ARTIST"),
    (3, 130, "ALBUM"),
    (4, 131, "TRACK"),
    (5, 133, "BPM"),
    (6, 134, "RATING"),
    (7, 135, "YEAR"),
    (8, 136, "REMIXER"),
    (9, 137, "LABEL"),
    (10, 138, "ORIGINAL ARTIST"),
    (11, 139, "KEY"),
    (12, 141, "CUE"),
    (13, 142, "COLOR"),
    (14, 146, "TIME"),
    (15, 147, "BITRATE"),
    (16, 148, "FILE NAME"),
    (17, 132, "PLAYLIST"),
    (18, 152, "HOT CUE BANK"),
    (19, 149, "HISTORY"),
    (20, 145, "SEARCH"),
    (21, 150, "COMMENTS"),
    (22, 140, "DATE ADDED"),
    (23, 151, "DJ PLAY COUNT"),
    (24, 144, "FOLDER"),
    (25, 161, "DEFAULT"),
    (26, 162, "ALPHABET"),
    (27, 170, "MATCHING"),
)
CATEGORIES = (
    (1, 1, 0, 0),
    (2, 2, 1, 1),
    (3, 3, 2, 1),
    (4, 4, 3, 1),
    (5, 17, 5, 1),
    (6, 5, 0, 0),
    (7, 6, 0, 0),
    (8, 7, 0, 0),
    (9, 8, 0, 0),
    (10, 9, 0, 0),
    (11, 10, 0, 0),
    (12, 11, 4, 1),
    (15, 13, 0, 0),
    (17, 24, 9, 1),
    (18, 20, 7, 1),
    (19, 14, 0, 0),
    (20, 15, 0, 0),
    (21, 16, 0, 0),
    (22, 19, 6, 1),
    (23, 18, 0, 0),
    (26, 27, 8, 1),
    (27, 22, 10, 1),
)
SORTS = (
    (0, 25, 1, 1, 0),
    (1, 26, 2, 1, 0),
    (2, 2, 3, 1, 0),
    (3, 3, 4, 1, 0),
    (4, 5, 5, 1, 0),
    (5, 6, 6, 1, 0),
    (6, 1, 0, 0, 0),
    (7, 21, 0, 0, 1),
    (8, 14, 0, 0, 0),
    (9, 8, 0, 0, 0),
    (10, 9, 0, 0, 0),
    (11, 10, 0, 0, 0),
    (12, 11, 7, 1, 0),
    (13, 15, 0, 0, 0),
    (15, 13, 0, 0, 0),
    (16, 23, 0, 0, 0),
    (17, 22, 0, 0, 0),
)
COLOR_NAMES = ("Pink", "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple")


def library_key() -> str:
    return deobfuscate(_KEY_BLOB)


def _cipher(path: Path, key: str) -> sqlite3.Connection:
    import sqlcipher3

    conn = sqlcipher3.connect(str(path))
    conn.execute("PRAGMA cipher = 'sqlcipher'")
    conn.execute("PRAGMA legacy = 4")
    escaped = key.replace("'", "''")
    conn.execute(f"PRAGMA key = '{escaped}'")
    return conn


def _row(cursor, row) -> dict:
    return {column[0]: row[index] for index, column in enumerate(cursor.description)}


def connect(path: Path, *, encrypt: bool | None = None) -> sqlite3.Connection:
    if path.is_file() and path.stat().st_size:
        if path.read_bytes()[:15] == b"SQLite format 3":
            conn = sqlite3.connect(str(path))
        else:
            conn = _cipher(path, library_key())
    elif encrypt:
        path.parent.mkdir(parents=True, exist_ok=True)
        conn = _cipher(path, library_key())
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(str(path))
    conn.row_factory = _row
    conn.execute("PRAGMA journal_mode = DELETE")
    conn.execute("PRAGMA synchronous = EXTRA")
    return conn


def _marked(name: str) -> str:
    return f"\ufffa{name}\ufffb"


def create_empty(conn: sqlite3.Connection) -> None:
    conn.executescript(SCHEMA)
    conn.executemany("INSERT INTO menuItem(menuItem_id, kind, name) VALUES (?, ?, ?)", [(i, k, _marked(n)) for i, k, n in MENU_ITEMS])
    conn.executemany(
        "INSERT INTO category(category_id, menuItem_id, sequenceNo, isVisible) VALUES (?, ?, ?, ?)",
        CATEGORIES,
    )
    conn.executemany(
        "INSERT INTO sort(sort_id, menuItem_id, sequenceNo, isVisible, isSelectedAsSubColumn) VALUES (?, ?, ?, ?, ?)",
        SORTS,
    )
    conn.executemany("INSERT INTO color(color_id, name) VALUES (?, ?)", list(enumerate(COLOR_NAMES, 1)))


def rows(conn: sqlite3.Connection, sql: str) -> list[sqlite3.Row]:
    return list(conn.execute(sql))
