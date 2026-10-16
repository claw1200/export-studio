//! A throwaway library with the real table layout, for tests. Only the columns the crate reads or
//! writes are listed per table; the bookkeeping columns every rekordbox table carries are appended.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::{key, Result};

const SYNC_COLUMNS: &str = "`UUID` VARCHAR(255) DEFAULT NULL, `rb_data_status` INTEGER DEFAULT 0, \
    `rb_local_data_status` INTEGER DEFAULT 0, `rb_local_deleted` TINYINT(1) DEFAULT 0, \
    `rb_local_synced` TINYINT(1) DEFAULT 0, `usn` BIGINT DEFAULT NULL, `rb_local_usn` BIGINT DEFAULT NULL, \
    `created_at` DATETIME NOT NULL DEFAULT '2020-01-01 00:00:00.000 +00:00', \
    `updated_at` DATETIME NOT NULL DEFAULT '2020-01-01 00:00:00.000 +00:00'";

const TABLES: &[(&str, &str)] = &[
    ("djmdContent", "`ID` VARCHAR(255) PRIMARY KEY, `FolderPath` VARCHAR(255), `Title` VARCHAR(255), `ArtistID` VARCHAR(255), `AlbumID` VARCHAR(255), `GenreID` VARCHAR(255), `KeyID` VARCHAR(255), `BPM` INTEGER, `Length` INTEGER, `Rating` INTEGER, `FileType` INTEGER, `AnalysisDataPath` VARCHAR(255), `Analysed` INTEGER, `StockDate` VARCHAR(255), `LabelID` VARCHAR(255), `RemixerID` VARCHAR(255), `OrgArtistID` VARCHAR(255), `ComposerID` VARCHAR(255), `ColorID` VARCHAR(255), `Commnt` TEXT, `ReleaseYear` INTEGER, `DateCreated` VARCHAR(255), `DJPlayCount` INTEGER, `BitRate` INTEGER, `SampleRate` INTEGER, `BitDepth` INTEGER, `FileSize` INTEGER, `ImagePath` VARCHAR(255), `TrackNo` INTEGER, `ContentLink` INTEGER, `DeviceID` VARCHAR(255), `MasterDBID` VARCHAR(255), `MasterSongID` VARCHAR(255), `HotCueAutoLoad` VARCHAR(255), `FileNameL` VARCHAR(255), `rb_file_id` VARCHAR(255), `ReleaseDate` VARCHAR(255), `DiscNo` INTEGER, `ISRC` VARCHAR(255), `OrgFolderPath` VARCHAR(255), `AnalysisUpdated` INTEGER"),
    ("djmdDevice", "`ID` VARCHAR(255) PRIMARY KEY, `MasterDBID` VARCHAR(255)"),
    ("djmdProperty", "`DBID` VARCHAR(255) PRIMARY KEY"),
    ("djmdMenuItems", "`ID` VARCHAR(255) PRIMARY KEY, `Name` VARCHAR(255)"),
    ("djmdLabel", "`ID` VARCHAR(255) PRIMARY KEY, `Name` VARCHAR(255)"),
    ("djmdHistory", "`ID` VARCHAR(255) PRIMARY KEY, `Seq` INTEGER, `Name` VARCHAR(255), `Attribute` INTEGER, `ParentID` VARCHAR(255), `DateCreated` VARCHAR(255)"),
    ("djmdMyTag", "`ID` VARCHAR(255) PRIMARY KEY, `Seq` INTEGER, `Name` VARCHAR(255), `Attribute` INTEGER, `ParentID` VARCHAR(255)"),
    ("djmdArtist", "`ID` VARCHAR(255) PRIMARY KEY, `Name` VARCHAR(255)"),
    ("djmdAlbum", "`ID` VARCHAR(255) PRIMARY KEY, `Name` VARCHAR(255)"),
    ("djmdGenre", "`ID` VARCHAR(255) PRIMARY KEY, `Name` VARCHAR(255)"),
    ("djmdKey", "`ID` VARCHAR(255) PRIMARY KEY, `ScaleName` VARCHAR(255), `Seq` INTEGER"),
    ("djmdPlaylist", "`ID` VARCHAR(255) PRIMARY KEY, `Seq` INTEGER, `Name` VARCHAR(255), `Attribute` INTEGER, `ParentID` VARCHAR(255), `SmartList` TEXT"),
    ("djmdSongPlaylist", "`ID` VARCHAR(255) PRIMARY KEY, `PlaylistID` VARCHAR(255), `ContentID` VARCHAR(255), `TrackNo` INTEGER"),
    ("djmdSongMyTag", "`ID` VARCHAR(255) PRIMARY KEY, `MyTagID` VARCHAR(255), `ContentID` VARCHAR(255), `TrackNo` INTEGER"),
    ("djmdSongHistory", "`ID` VARCHAR(255) PRIMARY KEY, `HistoryID` VARCHAR(255), `ContentID` VARCHAR(255), `TrackNo` INTEGER"),
    ("djmdSongHotCueBanklist", "`ID` VARCHAR(255) PRIMARY KEY, `HotCueBanklistID` VARCHAR(255), `ContentID` VARCHAR(255), `TrackNo` INTEGER"),
    ("djmdSongRelatedTracks", "`ID` VARCHAR(255) PRIMARY KEY, `RelatedTracksID` VARCHAR(255), `ContentID` VARCHAR(255), `TrackNo` INTEGER"),
    ("djmdSongSampler", "`ID` VARCHAR(255) PRIMARY KEY, `SamplerID` VARCHAR(255), `ContentID` VARCHAR(255), `TrackNo` INTEGER"),
    ("djmdActiveCensor", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255), `InMsec` INTEGER, `OutMsec` INTEGER"),
    ("djmdCue", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255), `ContentUUID` VARCHAR(255), `Kind` INTEGER, `InMsec` INTEGER, `InFrame` INTEGER, `InMpegFrame` INTEGER, `InMpegAbs` INTEGER, `OutMsec` INTEGER, `OutFrame` INTEGER, `OutMpegFrame` INTEGER, `OutMpegAbs` INTEGER, `Color` INTEGER, `ColorTableIndex` INTEGER, `ActiveLoop` INTEGER, `Comment` VARCHAR(255), `BeatLoopSize` INTEGER"),
    ("djmdMixerParam", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255), `GainHigh` INTEGER"),
    ("contentFile", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255), `Path` VARCHAR(255)"),
    ("contentCue", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255), `Cues` TEXT"),
    ("contentActiveCensor", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255)"),
    ("djmdSongTagList", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255)"),
    ("djmdSongRequestList", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255)"),
    ("djmdCloudExportSongPlaylist", "`ID` VARCHAR(255) PRIMARY KEY, `ContentID` VARCHAR(255)"),
];

/// A new encrypted library under `dir`, with the update counter at `usn`. Returns its path.
pub fn build(dir: &Path, usn: i64) -> Result<PathBuf> {
    let path = dir.join("master.db");
    let conn = Connection::open(&path)?;
    conn.pragma_update(None, "key", key::master_key()?)?;
    for (name, columns) in TABLES {
        conn.execute_batch(&format!("CREATE TABLE `{name}` ({columns}, {SYNC_COLUMNS});"))?;
    }
    conn.execute_batch(
        "CREATE TABLE `agentRegistry` (`registry_id` VARCHAR(255) PRIMARY KEY, `int_1` BIGINT, `created_at` DATETIME NOT NULL DEFAULT '2020-01-01', `updated_at` DATETIME NOT NULL DEFAULT '2020-01-01');",
    )?;
    conn.execute("INSERT INTO agentRegistry (registry_id, int_1) VALUES ('localUpdateCount', ?1)", [usn])?;
    conn.execute("INSERT INTO djmdDevice (ID, MasterDBID) VALUES ('dev1', 'master-1')", [])?;
    conn.execute("INSERT INTO djmdProperty (DBID) VALUES ('4242')", [])?;
    conn.execute("INSERT INTO djmdMenuItems (ID, Name, rb_local_usn) VALUES ('m1', 'TRACK', 77)", [])?;
    Ok(path)
}

/// Plain inserts into a fixture, for setting up a scenario.
pub struct Rows<'a>(pub &'a Connection);

impl Rows<'_> {
    pub fn track(&self, id: &str, title: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO djmdContent (ID, Title, FolderPath) VALUES (?1, ?2, ?3)",
            params![id, title, format!("/music/{id}.mp3")],
        )?;
        Ok(())
    }

    pub fn playlist(&self, id: &str, name: &str, attribute: i64) -> Result<()> {
        self.0.execute(
            "INSERT INTO djmdPlaylist (ID, Name, Attribute, ParentID, Seq) VALUES (?1, ?2, ?3, 'root', 1)",
            params![id, name, attribute],
        )?;
        Ok(())
    }

    pub fn song(&self, playlist: &str, content: &str, number: i64) -> Result<()> {
        self.0.execute(
            "INSERT INTO djmdSongPlaylist (ID, PlaylistID, ContentID, TrackNo) VALUES (?1, ?2, ?3, ?4)",
            params![format!("{playlist}-{content}"), playlist, content, number],
        )?;
        Ok(())
    }

    /// A row in any of the plain `ContentID` tables.
    pub fn link(&self, table: &str, id: &str, content: &str) -> Result<()> {
        self.0.execute(&format!("INSERT INTO {table} (ID, ContentID) VALUES (?1, ?2)"), params![id, content])?;
        Ok(())
    }
}

/// Open the fixture for setup with the same key the crate uses.
pub fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "key", key::master_key()?)?;
    Ok(conn)
}
