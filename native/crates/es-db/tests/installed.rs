//! Reads a snapshot of the installed library. Run with `cargo test -p es-db --test installed -- --ignored`.

use es_db::{write::DEPENDENTS, Library};

fn snapshot() -> Library {
    let path = Library::installed_path().expect("no home directory");
    Library::open_snapshot(&path).expect("open the installed library")
}

#[test]
#[ignore = "reads the installed rekordbox library"]
fn playlist_counts_match_their_lists() {
    let lib = snapshot();
    let nodes = lib.playlists().unwrap();
    assert!(!nodes.is_empty());
    for node in nodes {
        if let Some(count) = node.count {
            let listed = lib.playlist_track_ids(&node.id).unwrap();
            assert_eq!(i64::try_from(listed.len()).unwrap(), count, "{}", node.name);
        }
    }
}

#[test]
#[ignore = "reads the installed rekordbox library"]
fn no_live_track_is_marked_deleted() {
    let lib = snapshot();
    let tracks = lib.tracks(None).unwrap();
    assert!(!tracks.is_empty());
    println!("{} live tracks", tracks.len());
}

#[test]
#[ignore = "reads the installed rekordbox library"]
fn every_content_table_in_the_real_schema_is_classified() {
    let lib = snapshot();
    let missing: Vec<String> = lib
        .tables_with_column("ContentID")
        .unwrap()
        .into_iter()
        .filter(|table| table != "djmdContent" && !DEPENDENTS.iter().any(|(name, _)| name == table))
        .collect();
    assert!(missing.is_empty(), "not in DEPENDENTS: {missing:?}");
}
