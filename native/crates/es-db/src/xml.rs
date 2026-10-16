//! `masterPlaylists6.xml`, the file rekordbox keeps beside `master.db` listing every playlist with
//! the time it last changed. Edits are made on the text so the rest of the file is kept as it is.

pub(crate) enum XmlOp {
    Add { id: String, parent: String, attribute: i64 },
    Remove(String),
    Touch(String),
}

/// A playlist id as the file writes it: hexadecimal, `0` for the root.
fn hex(id: &str) -> String {
    if id == "root" {
        return "0".into();
    }
    id.parse::<u64>().map_or_else(|_| id.to_owned(), |n| format!("{n:X}"))
}

fn node_start(id: &str) -> String {
    format!("<NODE Id=\"{}\"", hex(id))
}

pub(crate) fn apply(text: &str, ops: &[XmlOp], now_ms: u128) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    for op in ops {
        match op {
            XmlOp::Remove(id) => {
                let start = node_start(id);
                lines.retain(|line| !line.trim_start().starts_with(&start));
            }
            XmlOp::Touch(id) => {
                let start = node_start(id);
                for line in lines.iter_mut().filter(|l| l.trim_start().starts_with(&start)) {
                    *line = replace_timestamp(line, now_ms);
                }
            }
            XmlOp::Add { id, parent, attribute } => {
                let start = node_start(id);
                if lines.iter().any(|l| l.trim_start().starts_with(&start)) {
                    continue;
                }
                let node = format!(
                    "        <NODE Id=\"{}\" ParentId=\"{}\" Attribute=\"{attribute}\" Timestamp=\"{now_ms}\" Lib_Type=\"0\" CheckType=\"0\"/>",
                    hex(id),
                    hex(parent)
                );
                match lines.iter().position(|l| l.trim() == "</PLAYLISTS>") {
                    Some(at) => lines.insert(at, node),
                    None => lines.push(node),
                }
            }
        }
    }
    let mut out = lines.join(newline);
    out.push_str(newline);
    out
}

fn replace_timestamp(line: &str, now_ms: u128) -> String {
    const KEY: &str = "Timestamp=\"";
    let Some(start) = line.find(KEY).map(|i| i + KEY.len()) else { return line.to_owned() };
    let Some(len) = line[start..].find('"') else { return line.to_owned() };
    format!("{}{now_ms}{}", &line[..start], &line[start + len..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<MASTER_PLAYLIST Version=\"3.0.0\">\n    <PLAYLISTS>\n        <NODE Id=\"5C24CD2\" ParentId=\"0\" Attribute=\"0\" Timestamp=\"1\" Lib_Type=\"0\" CheckType=\"0\"/>\n        <NODE Id=\"AB\" ParentId=\"0\" Attribute=\"1\" Timestamp=\"2\" Lib_Type=\"0\" CheckType=\"0\"/>\n    </PLAYLISTS>\n</MASTER_PLAYLIST>\n";

    #[test]
    fn adds_a_node_with_hex_ids_before_the_end_of_the_list() {
        let out = apply(FILE, &[XmlOp::Add { id: "92".into(), parent: "171".into(), attribute: 0 }], 99);
        assert!(out.contains("<NODE Id=\"5C\" ParentId=\"AB\" Attribute=\"0\" Timestamp=\"99\""));
        assert!(out.find("Id=\"5C\"").unwrap() < out.find("</PLAYLISTS>").unwrap());
    }

    #[test]
    fn removes_only_the_named_node() {
        let out = apply(FILE, &[XmlOp::Remove("96619730".into())], 5);
        assert!(!out.contains("Id=\"5C24CD2\""));
        assert!(out.contains("Id=\"AB\""));
    }

    #[test]
    fn touching_changes_only_the_timestamp() {
        let out = apply(FILE, &[XmlOp::Touch("171".into())], 777);
        assert!(out.contains("<NODE Id=\"AB\" ParentId=\"0\" Attribute=\"1\" Timestamp=\"777\" Lib_Type"));
        assert!(out.contains("Timestamp=\"1\""));
    }

    #[test]
    fn adding_twice_keeps_one_node() {
        let op = || XmlOp::Add { id: "92".into(), parent: "root".into(), attribute: 0 };
        let out = apply(FILE, &[op(), op()], 1);
        assert_eq!(out.matches("Id=\"5C\"").count(), 1);
    }
}
