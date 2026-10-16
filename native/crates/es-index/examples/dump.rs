//! Prints the installed library's rows as JSON, for comparing against another reader.
//! `cargo run -p es-index --example dump -- tracks|playlists|histories|tags`

use es_db::Library;

fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "tracks".into());
    let path = Library::installed_path().expect("no home directory");
    let lib = Library::open_snapshot(&path).expect("open the installed library");
    let share = path.with_file_name("share");
    let json = match what.as_str() {
        "tracks" => serde_json::to_string(&es_index::track_rows(&lib, &share, None).unwrap()),
        "playlists" => serde_json::to_string(&es_index::playlist_nodes(&lib).unwrap()),
        "histories" => serde_json::to_string(&es_index::history_nodes(&lib).unwrap()),
        "tags" => serde_json::to_string(&es_index::tag_nodes(&lib).unwrap()),
        other => panic!("unknown view {other}"),
    };
    println!("{}", json.unwrap());
}
