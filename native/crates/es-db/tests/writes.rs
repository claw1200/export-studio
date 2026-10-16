use es_db::fixture::{self, Rows};
use es_db::write::{OnRemove, DEPENDENTS};
use es_db::{Error, Guard, Library};
use tempfile::TempDir;

struct Scene {
    _dir: TempDir,
    lib: Library,
}

fn scene() -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 100).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        let rows = Rows(&conn);
        for (id, title) in [("1", "One"), ("2", "Two"), ("3", "Three")] {
            rows.track(id, title).unwrap();
        }
        rows.playlist("10", "Warm up", 0).unwrap();
        rows.playlist("11", "Peak", 0).unwrap();
        for (number, id) in [(1, "1"), (2, "2"), (3, "3")] {
            rows.song("10", id, number).unwrap();
        }
        rows.song("11", "2", 1).unwrap();
    }
    let lib = Library::open_live(&path, Guard::Off).unwrap();
    Scene { _dir: dir, lib }
}

fn ids(items: &[String]) -> Vec<String> {
    items.to_vec()
}

fn counter(lib: &Library) -> i64 {
    let conn = fixture::connect(lib.path()).unwrap();
    conn.query_row("SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn remove_hides_the_track_everywhere_and_closes_the_gap() {
    let mut s = scene();
    s.lib.write(|tx| tx.remove_tracks(&["2".into()])).unwrap();

    assert_eq!(s.lib.tracks(None).unwrap().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["1", "3"]);
    assert_eq!(s.lib.playlist_track_ids("10").unwrap(), ids(&["1".into(), "3".into()]));
    assert!(s.lib.playlist_track_ids("11").unwrap().is_empty());

    let conn = fixture::connect(s.lib.path()).unwrap();
    let numbers: Vec<i64> = conn
        .prepare("SELECT TrackNo FROM djmdSongPlaylist WHERE PlaylistID = '10' ORDER BY TrackNo")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(numbers, [1, 2]);
}

#[test]
fn playlist_counts_always_match_their_lists() {
    let mut s = scene();
    s.lib.write(|tx| tx.remove_tracks(&["1".into()])).unwrap();
    for node in s.lib.playlists().unwrap() {
        let listed = i64::try_from(s.lib.playlist_track_ids(&node.id).unwrap().len()).unwrap();
        assert_eq!(node.count, Some(listed), "playlist {}", node.name);
    }
}

#[test]
fn a_restored_track_returns_with_no_playlist_links() {
    let mut s = scene();
    s.lib.write(|tx| tx.remove_tracks(&["2".into()])).unwrap();
    s.lib.write(|tx| tx.restore_track("2")).unwrap();

    assert!(s.lib.tracks(Some(&["2".into()])).unwrap().len() == 1);
    assert_eq!(s.lib.playlist_track_ids("10").unwrap(), ids(&["1".into(), "3".into()]));
    assert!(s.lib.playlist_track_ids("11").unwrap().is_empty());
}

#[test]
fn restore_clears_links_a_foreign_tool_left_on_a_removed_track() {
    let mut s = scene();
    s.lib.write(|tx| tx.remove_tracks(&["2".into()])).unwrap();
    {
        let conn = fixture::connect(s.lib.path()).unwrap();
        Rows(&conn).song("10", "2", 9).unwrap();
        Rows(&conn).link("djmdSongMyTag", "t1", "2").unwrap();
    }
    s.lib.write(|tx| tx.restore_track("2")).unwrap();

    assert_eq!(s.lib.playlist_track_ids("10").unwrap(), ids(&["1".into(), "3".into()]));
    let conn = fixture::connect(s.lib.path()).unwrap();
    let tags: i64 = conn.query_row("SELECT count(*) FROM djmdSongMyTag", [], |r| r.get(0)).unwrap();
    assert_eq!(tags, 0);
}

#[test]
fn each_edit_takes_one_new_update_count_and_stamps_what_it_touched() {
    let mut s = scene();
    s.lib.write(|tx| tx.remove_tracks(&["1".into(), "2".into()])).unwrap();
    assert_eq!(counter(&s.lib), 101);

    let conn = fixture::connect(s.lib.path()).unwrap();
    let stamped: Vec<i64> = conn
        .prepare("SELECT rb_local_usn FROM djmdContent WHERE rb_local_deleted = 1")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(stamped, [101, 101]);

    s.lib.write(|tx| tx.remove_tracks(&["1".into()])).unwrap();
    assert_eq!(counter(&s.lib), 101, "removing a removed track changes nothing");
}

#[test]
fn a_failed_edit_leaves_everything_as_it_was() {
    let mut s = scene();
    let result = s.lib.write(|tx| {
        tx.remove_tracks(&["1".into()])?;
        tx.restore_track("missing")
    });
    assert!(matches!(result, Err(Error::NotFound(_))));
    assert_eq!(s.lib.tracks(None).unwrap().len(), 3);
    assert_eq!(counter(&s.lib), 100);
}

#[test]
fn adding_to_a_playlist_appends_once() {
    let mut s = scene();
    let added = s.lib.write(|tx| tx.add_to_playlist("11", &["2".into(), "3".into()])).unwrap();
    assert_eq!(added, 1);
    assert_eq!(s.lib.playlist_track_ids("11").unwrap(), ids(&["2".into(), "3".into()]));
}

#[test]
fn adding_a_removed_track_to_a_playlist_is_refused() {
    let mut s = scene();
    s.lib.write(|tx| tx.remove_tracks(&["3".into()])).unwrap();
    let result = s.lib.write(|tx| tx.add_to_playlist("11", &["3".into()]));
    assert!(matches!(result, Err(Error::NotFound(_))));
}

#[test]
fn tracks_cannot_be_added_to_a_smart_playlist() {
    let mut s = scene();
    {
        let conn = fixture::connect(s.lib.path()).unwrap();
        Rows(&conn).playlist("12", "Smart", 4).unwrap();
    }
    let result = s.lib.write(|tx| tx.add_to_playlist("12", &["1".into()]));
    assert!(matches!(result, Err(Error::Refused(_))));
}

#[test]
fn a_snapshot_cannot_write() {
    let s = scene();
    let mut snap = Library::open_snapshot(s.lib.path()).unwrap();
    assert!(matches!(snap.write(|tx| tx.remove_tracks(&["1".into()])), Err(Error::ReadOnly)));
    assert_eq!(snap.tracks(None).unwrap().len(), 3);
}

#[test]
fn every_table_pointing_at_a_track_is_classified() {
    let s = scene();
    let conn = fixture::connect(s.lib.path()).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for table in tables {
        let has_content_id = conn
            .prepare(&format!("PRAGMA table_info(`{table}`)"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .any(|name| name.unwrap() == "ContentID");
        if has_content_id && table != "djmdContent" {
            assert!(DEPENDENTS.iter().any(|(name, _)| *name == table), "{table} is not in DEPENDENTS");
        }
    }
    assert!(DEPENDENTS.iter().any(|(_, action)| *action == OnRemove::Keep));
}
