//! Lists the backups in a folder and, with `--create`, makes a manual one of a library copy.
//! `cargo run -p es-node --example backups -- ROOT [--create LIBRARY_DB]`

use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("a backups folder");
    let backups = es_backup::Backups::new(
        &root,
        |path: &Path| {
            rb_library::verify(path)
                .map(|c| es_backup::Counts { tracks: c.tracks, playlists: c.playlists })
                .map_err(|e| es_backup::Error::Message(e.to_string()))
        },
        rb_library::rekordbox_running,
    );
    if args.next().as_deref() == Some("--create") {
        let db = args.next().expect("a library");
        let made = backups.create(Path::new(&db), "manual", &|_| {}).unwrap();
        println!("{}", made.map_or("skipped".into(), |m| m["id"].to_string()));
    }
    println!("{}", serde_json::to_string(&backups.list(None)).unwrap());
}
