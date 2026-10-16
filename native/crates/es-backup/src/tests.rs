use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::*;

struct Scene {
    _dir: tempfile::TempDir,
    db_dir: PathBuf,
    backups: Backups,
}

fn scene() -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let db_dir = dir.path().join("rekordbox");
    fs::create_dir_all(db_dir.join("share/PIONEER/Artwork/abc")).unwrap();
    fs::write(db_dir.join("master.db"), b"database one").unwrap();
    fs::write(db_dir.join("masterPlaylists6.xml"), b"<xml/>").unwrap();
    fs::write(db_dir.join("share/PIONEER/Artwork/abc/art.jpg"), b"jpeg").unwrap();
    let verify = |p: &Path| {
        let text = fs::read_to_string(p).map_err(Error::from)?;
        if text.contains("broken") { fail("The database is damaged") } else { Ok(Counts { tracks: 3, playlists: 2 }) }
    };
    let backups = Backups::new(dir.path().join("backups"), verify, || false);
    Scene { _dir: dir, db_dir, backups }
}

fn quiet() -> impl Fn(Value) {
    |_| {}
}

#[test]
fn a_manual_backup_holds_the_database_side_files_and_share_folder() {
    let s = scene();
    let made = s.backups.create(&s.db_dir.join("master.db"), "manual", &quiet()).unwrap().unwrap();
    assert_eq!((made["tracks"].as_i64(), made["playlists"].as_i64(), made["files"].as_i64()), (Some(3), Some(2), Some(3)));

    let mut zip = ZipArchive::new(File::open(made["path"].as_str().unwrap()).unwrap()).unwrap();
    let mut names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["manifest.json", "master.db", "masterPlaylists6.xml", "share/PIONEER/Artwork/abc/art.jpg"]);
    assert_eq!(s.backups.list(Some(&s.db_dir.join("master.db"))).len(), 1);
}

#[test]
fn an_automatic_backup_is_skipped_until_something_changes() {
    let s = scene();
    let db = s.db_dir.join("master.db");
    assert!(s.backups.create(&db, "before-edit", &quiet()).unwrap().is_some());
    assert!(s.backups.create(&db, "daily", &quiet()).unwrap().is_none());
    fs::write(&db, b"database two, longer").unwrap();
    assert!(s.backups.create(&db, "daily", &quiet()).unwrap().is_some());
}

#[test]
fn only_the_newest_automatic_backups_are_kept() {
    let s = scene();
    let db = s.db_dir.join("master.db");
    s.backups.set_settings(None, Some(5), None).unwrap();
    for n in 0..7 {
        fs::write(&db, format!("database {n} {}", "x".repeat(n))).unwrap();
        s.backups.create(&db, "before-edit", &quiet()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    s.backups.create(&db, "manual", &quiet()).unwrap();
    let all = s.backups.list(None);
    let automatic = all.iter().filter(|m| m["reason"] == "before-edit").count();
    assert_eq!((automatic, all.len()), (5, 6));
}

#[test]
fn a_database_that_fails_its_check_is_not_backed_up() {
    let s = scene();
    let db = s.db_dir.join("master.db");
    fs::write(&db, b"broken").unwrap();
    let err = s.backups.create(&db, "manual", &quiet()).unwrap_err().to_string();
    assert!(err.contains("failed its check"), "{err}");
    assert!(s.backups.list(None).is_empty());
}

#[test]
fn restoring_puts_the_old_library_back_and_keeps_what_it_replaced() {
    let s = scene();
    let db = s.db_dir.join("master.db");
    let made = s.backups.create(&db, "manual", &quiet()).unwrap().unwrap();
    fs::write(&db, b"database after edits").unwrap();
    fs::remove_file(s.db_dir.join("share/PIONEER/Artwork/abc/art.jpg")).unwrap();

    let result = s.backups.restore(made["id"].as_str().unwrap(), &db, &quiet()).unwrap();
    assert_eq!(fs::read(&db).unwrap(), b"database one");
    assert_eq!(fs::read(s.db_dir.join("share/PIONEER/Artwork/abc/art.jpg")).unwrap(), b"jpeg");
    let kept = result["kept"].as_str().expect("the replaced library is kept");
    assert!(Path::new(kept).is_file());
    assert!(s.backups.list(None).iter().any(|m| m["reason"] == "pre-restore"));
}

#[test]
fn restoring_is_refused_while_rekordbox_runs() {
    let dir = tempfile::tempdir().unwrap();
    let backups = Backups::new(dir.path(), |_| Ok(Counts { tracks: 0, playlists: 0 }), || true);
    assert_eq!(backups.restore("x", &dir.path().join("master.db"), &quiet()).unwrap_err().to_string(), BLOCKED);
}

#[test]
fn a_backup_with_files_outside_the_library_is_refused() {
    let s = scene();
    fs::create_dir_all(&s.backups.root).unwrap();
    let path = s.backups.root.join("evil.zip");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default();
    zip.start_file("master.db", options).unwrap();
    zip.write_all(b"database").unwrap();
    zip.start_file("../outside.txt", options).unwrap();
    zip.write_all(b"x").unwrap();
    zip.start_file(MANIFEST, options).unwrap();
    zip.write_all(b"{}").unwrap();
    zip.finish().unwrap();

    let err = s.backups.restore("evil", &s.db_dir.join("master.db"), &quiet()).unwrap_err().to_string();
    assert!(err.contains("unexpected file"), "{err}");
    assert_eq!(fs::read(s.db_dir.join("master.db")).unwrap(), b"database one");
}

#[test]
fn settings_are_validated_and_remembered() {
    let s = scene();
    assert_eq!(s.backups.settings(), json!({ "auto": true, "keep": 10, "interval": 12 }));
    s.backups.set_settings(Some(false), Some(20), Some(48)).unwrap();
    assert_eq!(s.backups.settings(), json!({ "auto": false, "keep": 20, "interval": 48 }));
    assert!(s.backups.set_settings(None, Some(7), None).is_err());
    assert!(s.backups.set_settings(None, None, Some(5)).is_err());
}

#[test]
fn deleting_an_unknown_backup_is_an_error() {
    let s = scene();
    assert!(s.backups.delete("nope").is_err());
    assert!(s.backups.delete("../etc").is_err());
}
