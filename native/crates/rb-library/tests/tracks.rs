use rb_library::fixture::{self, Rows};
use rb_library::{Error, Guard, Library, TrackEdit};
use tempfile::TempDir;

fn scene() -> (TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 200).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        let rows = Rows(&conn);
        rows.track("1", "One").unwrap();
        rows.track("2", "Two").unwrap();
        conn.execute("INSERT INTO djmdArtist (ID, Name) VALUES ('a1', 'Known')", []).unwrap();
    }
    (dir, Library::open_live(&path, Guard::Off).unwrap())
}

fn count(lib: &Library, sql: &str) -> i64 {
    fixture::connect(lib.path()).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn names_are_matched_or_added_once() {
    let (_dir, mut lib) = scene();
    let edit = TrackEdit { artist: Some("Known".into()), remixer: Some("New Person".into()), genre: Some("Dub".into()), ..TrackEdit::default() };
    lib.write(|tx| tx.update_tracks(&["1".into(), "2".into()], &edit)).unwrap();

    let tracks = lib.tracks(None).unwrap();
    assert!(tracks.iter().all(|t| t.artist == "Known" && t.remixer == "New Person" && t.genre == "Dub"));
    assert_eq!(count(&lib, "SELECT count(*) FROM djmdArtist"), 2);
    assert_eq!(count(&lib, "SELECT count(*) FROM djmdGenre"), 1);
}

#[test]
fn numbers_colour_and_text_are_stored_and_cleared() {
    let (_dir, mut lib) = scene();
    let edit = TrackEdit {
        title: Some("Renamed".into()),
        comment: Some("note".into()),
        year: Some(1999),
        track_no: Some(4),
        rating: Some(3),
        color: Some(5),
        ..TrackEdit::default()
    };
    lib.write(|tx| tx.update_tracks(&["1".into()], &edit)).unwrap();
    let t = lib.tracks(Some(&["1".into()])).unwrap().remove(0);
    assert_eq!((t.title.as_str(), t.comment.as_str(), t.year, t.track_no, t.rating, t.color_id.as_str()), ("Renamed", "note", 1999, 4, 3, "5"));

    let clear = TrackEdit { comment: Some(String::new()), color: Some(0), artist: Some(String::new()), ..TrackEdit::default() };
    lib.write(|tx| tx.update_tracks(&["1".into()], &clear)).unwrap();
    let t = lib.tracks(Some(&["1".into()])).unwrap().remove(0);
    assert_eq!((t.comment.as_str(), t.color_id.as_str(), t.artist.as_str()), ("", "", ""));
}

#[test]
fn only_changed_tracks_take_an_update_count() {
    let (_dir, mut lib) = scene();
    lib.write(|tx| tx.update_tracks(&["1".into()], &TrackEdit { title: Some("Same".into()), ..TrackEdit::default() })).unwrap();
    assert_eq!(count(&lib, "SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'"), 201);
    lib.write(|tx| tx.update_tracks(&["1".into()], &TrackEdit { title: Some("Same".into()), ..TrackEdit::default() })).unwrap();
    assert_eq!(count(&lib, "SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'"), 201);
}

#[test]
fn removed_tracks_cannot_be_edited() {
    let (_dir, mut lib) = scene();
    lib.write(|tx| tx.remove_tracks(&["2".into()])).unwrap();
    let result = lib.write(|tx| tx.update_tracks(&["2".into()], &TrackEdit { title: Some("x".into()), ..TrackEdit::default() }));
    assert!(matches!(result, Err(Error::NotFound(_))));
}
