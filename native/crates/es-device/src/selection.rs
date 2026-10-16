//! The playlists a stick was last synced with, as library playlist ids.
//!
//! The exporter keeps a playlist this library wrote earlier and drops it when a later sync leaves it
//! out, so the sync window has to start from that set.

use std::collections::BTreeSet;
use std::path::Path;

use crate::paths::pioneer_dir;

fn library_number(value: &str) -> u64 {
    value.parse::<u64>().ok().filter(|n| *n > 0).unwrap_or(0)
}

/// Library playlist ids the stick already records, in `library_ids` order. A missing or unreadable
/// record contributes nothing; sync-record ticks count only when the file belongs to `db_id`.
#[must_use]
pub fn recorded_playlist_ids(volume: &Path, library_ids: &[String], db_id: u64) -> Vec<String> {
    let (root, _) = pioneer_dir(volume);
    let mut wanted = manifest_ids(&root);
    wanted.extend(sync_ids(&root, db_id));
    library_ids.iter().filter(|id| wanted.contains(&library_number(id))).cloned().collect()
}

fn manifest_ids(root: &Path) -> BTreeSet<u64> {
    let Ok(text) = std::fs::read_to_string(root.join("exportstudio").join("manifest.json")) else { return BTreeSet::new() };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else { return BTreeSet::new() };
    let Some(rows) = data.get("playlists").and_then(serde_json::Value::as_array) else { return BTreeSet::new() };
    rows.iter()
        .filter(|row| row.is_object() && !row.get("device_only").and_then(serde_json::Value::as_bool).unwrap_or(false))
        .filter_map(|row| row.get("library_id").and_then(serde_json::Value::as_u64))
        .filter(|n| *n > 0)
        .collect()
}

fn sync_ids(root: &Path, db_id: u64) -> BTreeSet<u64> {
    if db_id == 0 {
        return BTreeSet::new();
    }
    for name in ["playlists3.sync", "playlists3Plus.sync"] {
        let Ok(text) = std::fs::read_to_string(root.join("rekordbox").join(name)) else { continue };
        if let Some(parsed) = parse_sync(&text, db_id) {
            return parsed;
        }
    }
    BTreeSet::new()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_owned())
}

/// Ticked playlist ids, or `None` when the text is not a sync record. An empty set means the record
/// belongs to another library.
fn parse_sync(text: &str, db_id: u64) -> Option<BTreeSet<u64>> {
    let head = &text[text.find('<')?..];
    let root_tag = head.split('>').find(|t| t.trim_start().starts_with('<') && !t.trim_start().starts_with("<?"))?;
    let file_db = attribute(root_tag, "DBID").unwrap_or_else(|| "0".into()).parse::<u64>().ok()?;
    if file_db != db_id {
        return Some(BTreeSet::new());
    }
    let mut ids = BTreeSet::new();
    for tag in text.split('<').filter(|t| t.starts_with("NODE")) {
        if attribute(tag, "Attribute").as_deref() == Some("1") || attribute(tag, "CheckType").as_deref() != Some("1") {
            continue;
        }
        if let Some(id) = attribute(tag, "Id").and_then(|h| u64::from_str_radix(&h, 16).ok()).filter(|n| *n > 0) {
            ids.insert(id);
        }
    }
    Some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNC: &str = r#"<?xml version="1.0"?><SYNC DBID="42"><NODE Id="A" Attribute="0" CheckType="1"/><NODE Id="B" Attribute="1" CheckType="1"/><NODE Id="C" Attribute="0" CheckType="0"/></SYNC>"#;

    #[test]
    fn only_ticked_playlists_of_this_library_count() {
        assert_eq!(parse_sync(SYNC, 42), Some(BTreeSet::from([10])));
        assert_eq!(parse_sync(SYNC, 7), Some(BTreeSet::new()));
        assert_eq!(parse_sync("not xml", 42), None);
    }

    #[test]
    fn the_manifest_lists_playlists_this_app_wrote() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("PIONEER/exportstudio")).unwrap();
        std::fs::write(
            dir.path().join("PIONEER/exportstudio/manifest.json"),
            r#"{"playlists":[{"library_id":5},{"library_id":6,"device_only":true},{"library_id":0}]}"#,
        )
        .unwrap();
        let ids = ["4".to_owned(), "5".to_owned(), "6".to_owned()];
        assert_eq!(recorded_playlist_ids(dir.path(), &ids, 0), ["5"]);
    }
}
