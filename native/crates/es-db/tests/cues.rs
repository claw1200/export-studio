use es_db::fixture::{self, Rows};
use es_db::{Error, Guard, Library};
use tempfile::TempDir;

fn scene() -> (TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 10).unwrap();
    {
        let conn = fixture::connect(&path).unwrap();
        conn.execute("INSERT INTO djmdContent (ID, Title, UUID, Length) VALUES ('1', 'One', 'u-1', 200)", []).unwrap();
        Rows(&conn).playlist("10", "Mix", 0).unwrap();
    }
    (dir, Library::open_live(&path, Guard::Off).unwrap())
}

fn cues(lib: &Library) -> Vec<(i64, i64)> {
    let mut out: Vec<_> = lib.cues("1").unwrap().into_iter().map(|c| (c.kind, c.start_ms)).collect();
    out.sort_unstable();
    out
}

#[test]
fn a_hot_cue_replaces_the_one_in_its_slot_and_is_clamped_to_the_track() {
    let (_dir, mut lib) = scene();
    lib.write(|tx| tx.set_hot_cue("1", 2, 1000)).unwrap();
    lib.write(|tx| tx.set_hot_cue("1", 2, 5000)).unwrap();
    lib.write(|tx| tx.set_hot_cue("1", 3, 999_999)).unwrap();
    assert_eq!(cues(&lib), [(2, 5000), (3, 200_000)]);
}

#[test]
fn loops_and_unknown_slots_are_not_hot_cue_slots() {
    let (_dir, mut lib) = scene();
    assert!(matches!(lib.write(|tx| tx.set_hot_cue("1", 4, 0)), Err(Error::Refused(_))));
    assert!(matches!(lib.write(|tx| tx.set_hot_cue("1", 10, 0)), Err(Error::Refused(_))));
}

#[test]
fn a_memory_cue_is_added_once_per_time() {
    let (_dir, mut lib) = scene();
    for _ in 0..2 {
        lib.write(|tx| tx.add_memory_cue("1", 2500)).unwrap();
    }
    lib.write(|tx| tx.add_memory_cue("1", 3000)).unwrap();
    assert_eq!(cues(&lib), [(0, 2500), (0, 3000)]);
}

#[test]
fn deleting_and_colouring_cues() {
    let (_dir, mut lib) = scene();
    lib.write(|tx| tx.set_hot_cue("1", 1, 100)).unwrap();
    lib.write(|tx| tx.add_memory_cue("1", 200)).unwrap();
    let all = lib.cues("1").unwrap();
    let hot = all.iter().find(|c| c.kind == 1).unwrap().id.clone();
    let memory = all.iter().find(|c| c.kind == 0).unwrap().id.clone();

    lib.write(|tx| tx.set_cue_color("1", &hot, Some(12))).unwrap();
    assert_eq!(lib.cues("1").unwrap().iter().find(|c| c.kind == 1).unwrap().color_index, Some(12));
    assert!(matches!(lib.write(|tx| tx.set_cue_color("1", &hot, Some(63))), Err(Error::Refused(_))));
    assert!(matches!(lib.write(|tx| tx.set_cue_color("1", &memory, Some(1))), Err(Error::NotFound(_))));

    lib.write(|tx| tx.delete_cue("1", &memory)).unwrap();
    assert_eq!(cues(&lib), [(1, 100)]);
}

#[test]
fn a_new_cue_carries_the_frame_position_and_the_track_uuid() {
    let (_dir, mut lib) = scene();
    lib.write(|tx| tx.set_hot_cue("1", 1, 2000)).unwrap();
    let conn = fixture::connect(lib.path()).unwrap();
    let (frame, uuid, out): (i64, String, i64) = conn
        .query_row("SELECT InFrame, ContentUUID, OutMsec FROM djmdCue", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert_eq!((frame, uuid.as_str(), out), (300, "u-1", -1));
}
