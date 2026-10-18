use rb_library::fixture::{self, Rows};
use rb_library::{Guard, Library};

fn lib_with(xml: &str) -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 1).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        let rows = Rows(&conn);
        for (id, title) in [("1", "Alpha One"), ("2", "Beta"), ("3", "Alpha Two")] {
            rows.track(id, title).unwrap();
        }
        conn.execute("UPDATE djmdContent SET BPM = 12800, Rating = 3 WHERE ID IN ('1','2')", []).unwrap();
        conn.execute("UPDATE djmdContent SET BPM = 9000 WHERE ID = '3'", []).unwrap();
        conn.execute("UPDATE djmdContent SET rb_local_deleted = 1 WHERE ID = '2'", []).unwrap();
        rows.playlist("10", "Smart", 4).unwrap();
        conn.execute("UPDATE djmdPlaylist SET SmartList = ?1 WHERE ID = '10'", [xml]).unwrap();
    }
    let lib = Library::open_live(&path, Guard::Off).unwrap();
    (dir, lib)
}

fn cond(property: &str, operator: i64, left: &str) -> String {
    format!(r#"<CONDITION PropertyName="{property}" Operator="{operator}" ValueUnit="" ValueLeft="{left}" ValueRight=""/>"#)
}

#[test]
fn all_conditions_must_match_and_removed_tracks_never_do() {
    let xml = format!(r#"<NODE Id="1" LogicalOperator="1" AutomaticUpdate="0">{}{}</NODE>"#, cond("name", 8, "alpha"), cond("bpm", 3, "10000"));
    let (_dir, lib) = lib_with(&xml);
    assert_eq!(lib.playlist_track_ids("10").unwrap(), ["1"]);
}

#[test]
fn any_condition_may_match() {
    let xml = format!(r#"<NODE Id="1" LogicalOperator="2" AutomaticUpdate="0">{}{}</NODE>"#, cond("name", 10, "Alpha T"), cond("rating", 1, "3"));
    let (_dir, lib) = lib_with(&xml);
    assert_eq!(lib.playlist_track_ids("10").unwrap(), ["1", "3"]);
}

#[test]
fn text_operators_treat_like_metacharacters_literally() {
    let (_dir, lib) = lib_with("<NODE/>");
    let conn = fixture::connect(lib.path()).unwrap();
    conn.execute("UPDATE djmdContent SET Title = 'x middle x', FileNameL = 'x middle x' WHERE ID = '3'", []).unwrap();
    for text in ["%", "_", "\\", "\\%", "\\_"] {
        let title = format!("{text} middle {text}");
        conn.execute("UPDATE djmdContent SET Title = ?1, FileNameL = ?1 WHERE ID = '1'", [&title]).unwrap();
        for property in ["name", "fileName"] {
            for operator in [8, 9, 10, 11] {
                let xml = format!(r#"<NODE LogicalOperator="1">{}</NODE>"#, cond(property, operator, text));
                conn.execute("UPDATE djmdPlaylist SET SmartList = ?1 WHERE ID = '10'", [&xml]).unwrap();
                let expected = if operator == 9 { ["3"] } else { ["1"] };
                assert_eq!(lib.playlist_track_ids("10").unwrap(), expected, "{property} operator {operator}, text {text:?}");
            }
        }
    }
}

#[test]
fn unreadable_conditions_are_an_error_not_every_track() {
    let (_dir, lib) = lib_with(&format!(r#"<NODE Id="1" LogicalOperator="1" AutomaticUpdate="0">{}</NODE>"#, cond("bpm", 3, "fast")));
    assert!(lib.playlist_track_ids("10").is_err());
}
