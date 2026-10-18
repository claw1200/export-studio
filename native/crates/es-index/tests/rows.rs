use rb_library::fixture::{self, Rows};
use rb_library::{Guard, Library};
use es_index::{history_nodes, playlist_nodes, track_rows};

#[test]
fn rows_resolve_names_and_ignore_removed_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 1).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        conn.execute("INSERT INTO djmdArtist (ID, Name) VALUES ('a1', 'Artist')", []).unwrap();
        conn.execute(
            "INSERT INTO djmdContent (ID, Title, ArtistID, FolderPath, BPM, FileType, ColorID) \
             VALUES ('1', '', 'a1', '/nowhere/Song One.flac', 17450, 5, '3')",
            [],
        )
        .unwrap();
        let rows = Rows(&conn);
        rows.track("2", "Gone").unwrap();
        rows.playlist("10", "Mix", 0).unwrap();
        rows.song("10", "1", 1).unwrap();
        rows.song("10", "2", 2).unwrap();
        conn.execute("INSERT INTO djmdHistory (ID, Name, Attribute, ParentID, Seq) VALUES ('h1', 'Set', 0, 'root', 1)", [])
            .unwrap();
        conn.execute("INSERT INTO djmdSongHistory (ID, HistoryID, ContentID, TrackNo) VALUES ('s1', 'h1', '2', 1)", [])
            .unwrap();
        conn.execute("INSERT INTO djmdSongHistory (ID, HistoryID, ContentID, TrackNo) VALUES ('s2', 'h1', '1', 2)", [])
            .unwrap();
        rows.link("djmdCue", "q1", "1").unwrap();
        conn.execute("UPDATE djmdCue SET Kind = 0", []).unwrap();
    }
    let mut lib = Library::open_live(&path, Guard::Off).unwrap();
    lib.write(|tx| tx.remove_tracks(&["2".into()])).unwrap();

    let share = dir.path().join("share");
    let rows = track_rows(&lib, &share, None).unwrap();
    assert_eq!(rows.len(), 1);
    let one = &rows[0];
    assert_eq!(one.title, "Song One");
    assert_eq!(one.artist, "Artist");
    assert_eq!(one.bpm, 174.5);
    assert_eq!((one.file_type.as_str(), one.stored_type), ("FLAC", Some("FLAC")));
    assert_eq!(one.color, 3);
    assert!(one.missing && !one.streaming);
    assert_eq!((one.cue_count, one.cue_stars), (1, 1));

    let json = serde_json_like(&playlist_nodes(&lib).unwrap());
    assert!(json.contains("count: Some(Some(1))"), "{json}");
    let history = history_nodes(&lib).unwrap();
    assert_eq!(history[0].count, Some(Some(1)), "a removed track is not counted in history");
}

fn serde_json_like<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}
