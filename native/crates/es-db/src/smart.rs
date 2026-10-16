//! Smart playlists: the conditions rekordbox keeps as XML, run against the collection.

use rusqlite::types::Value;
use rusqlite::{params_from_iter, OptionalExtension};

use crate::live::TRACK_LIVE;
use crate::{Error, Library, Result};

const ALL: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Condition {
    property: String,
    operator: i64,
    unit: String,
    left: String,
    right: String,
}

fn unescape(value: &str) -> String {
    value.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let mut from = 0;
    while let Some(found) = tag[from..].find(&key) {
        let at = from + found;
        let boundary = at == 0 || tag.as_bytes()[at - 1].is_ascii_whitespace();
        let start = at + key.len();
        if boundary {
            let end = tag[start..].find('"')?;
            return Some(unescape(&tag[start..start + end]));
        }
        from = start;
    }
    None
}

fn parse(xml: &str) -> Result<(i64, Vec<Condition>)> {
    let bad = || Error::Refused("this smart playlist's conditions can't be read".into());
    let mut tags = xml.split('<').filter(|t| !t.is_empty());
    let root = tags.find(|t| t.starts_with("NODE")).ok_or_else(bad)?;
    let logical = attribute(root, "LogicalOperator").and_then(|v| v.parse().ok()).unwrap_or(ALL);
    let conditions = xml
        .split('<')
        .filter(|t| t.starts_with("CONDITION"))
        .map(|tag| {
            Ok(Condition {
                property: attribute(tag, "PropertyName").ok_or_else(bad)?,
                operator: attribute(tag, "Operator").and_then(|v| v.parse().ok()).ok_or_else(bad)?,
                unit: attribute(tag, "ValueUnit").unwrap_or_default(),
                left: attribute(tag, "ValueLeft").unwrap_or_default(),
                right: attribute(tag, "ValueRight").unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((logical, conditions))
}

/// The SQL expression a property reads, and whether it compares as a number or a date.
fn column(property: &str) -> Option<(&'static str, Kind)> {
    Some(match property {
        "artist" => ("ar.Name", Kind::Text),
        "album" => ("al.Name", Kind::Text),
        "albumArtist" => ("aa.Name", Kind::Text),
        "originalArtist" => ("oa.Name", Kind::Text),
        "bpm" => ("c.BPM", Kind::Number),
        "grouping" => ("c.ColorID", Kind::Text),
        "comments" => ("c.Commnt", Kind::Text),
        "producer" => ("co.Name", Kind::Text),
        "stockDate" => ("c.StockDate", Kind::Date),
        "dateCreated" => ("substr(c.created_at, 1, 10)", Kind::Date),
        "counter" => ("c.DJPlayCount", Kind::Number),
        "fileName" => ("c.FileNameL", Kind::Text),
        "genre" => ("ge.Name", Kind::Text),
        "key" => ("k.ScaleName", Kind::Text),
        "label" => ("la.Name", Kind::Text),
        "rating" => ("c.Rating", Kind::Number),
        "dateReleased" => ("c.ReleaseDate", Kind::Date),
        "remixedBy" => ("rm.Name", Kind::Text),
        "duration" => ("c.Length", Kind::Number),
        "name" => ("c.Title", Kind::Text),
        "year" => ("c.ReleaseYear", Kind::Number),
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Number,
    Date,
}

fn value(kind: Kind, text: &str) -> Result<Value> {
    let bad = || Error::Refused(format!("'{text}' is not a valid value in this smart playlist"));
    Ok(match kind {
        Kind::Number => Value::Integer(text.trim().parse().map_err(|_| bad())?),
        _ => Value::Text(text.to_owned()),
    })
}

/// A condition as SQL with its parameters; `None` for a property rekordbox keeps that we can't read.
fn clause(cond: &Condition) -> Result<Option<(String, Vec<Value>)>> {
    if cond.property == "myTag" {
        let id: i64 = cond.left.trim().parse().map_err(|_| Error::Refused("a tag condition needs a tag".into()))?;
        let id = if id < 0 { id + (1 << 32) } else { id };
        let exists = "EXISTS (SELECT 1 FROM djmdSongMyTag s WHERE s.ContentID = c.ID AND s.rb_local_deleted = 0 AND s.MyTagID = ?)";
        return Ok(match cond.operator {
            8 => Some((exists.to_owned(), vec![Value::Text(id.to_string())])),
            9 => Some((format!("NOT {exists}"), vec![Value::Text(id.to_string())])),
            _ => None,
        });
    }
    let Some((expr, kind)) = column(&cond.property) else { return Ok(None) };
    let in_last = matches!(cond.operator, 6 | 7);
    if in_last {
        let amount: i64 = cond.left.trim().parse().map_err(|_| Error::Refused("a date range needs a number".into()))?;
        let unit = match cond.unit.as_str() {
            "day" => "days",
            "month" => "months",
            other => return Err(Error::Refused(format!("unknown unit '{other}' in a smart playlist"))),
        };
        let op = if cond.operator == 6 { ">" } else { "<" };
        return Ok(Some((format!("{expr} {op} date('now', 'localtime', '-{amount} {unit}')"), Vec::new())));
    }
    if cond.left.is_empty() {
        return Ok(Some(match cond.operator {
            1 => (format!("{expr} IS NULL"), Vec::new()),
            2 => (format!("{expr} IS NOT NULL"), Vec::new()),
            _ => ("0".to_owned(), Vec::new()),
        }));
    }
    let left = value(kind, &cond.left)?;
    let like = |pattern: String| (format!("{expr} LIKE ?"), vec![Value::Text(pattern)]);
    Ok(Some(match cond.operator {
        1 => (format!("{expr} = ?"), vec![left]),
        2 => (format!("{expr} != ?"), vec![left]),
        3 => (format!("{expr} > ?"), vec![left]),
        4 => (format!("{expr} < ?"), vec![left]),
        5 => (format!("{expr} BETWEEN ? AND ?"), vec![left, value(kind, &cond.right)?]),
        8 => like(format!("%{}%", cond.left)),
        9 => {
            let (sql, params) = like(format!("%{}%", cond.left));
            (format!("NOT ({sql})"), params)
        }
        10 => like(format!("{}%", cond.left)),
        11 => like(format!("%{}", cond.left)),
        other => return Err(Error::Refused(format!("unknown operator {other} in a smart playlist"))),
    }))
}

impl Library {
    /// Content ids a smart playlist currently matches, live tracks only.
    pub(crate) fn smart_track_ids(&self, playlist_id: &str) -> Result<Vec<String>> {
        let xml: Option<String> = self
            .conn()
            .query_row("SELECT SmartList FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0", [playlist_id], |row| row.get(0))
            .optional()?
            .flatten();
        let Some(xml) = xml.filter(|x| !x.trim().is_empty()) else { return Ok(Vec::new()) };
        let (logical, conditions) = parse(&xml)?;
        let mut parts = Vec::new();
        let mut params: Vec<Value> = Vec::new();
        for cond in &conditions {
            if let Some((sql, p)) = clause(cond)? {
                parts.push(format!("({sql})"));
                params.extend(p);
            }
        }
        let joiner = if logical == ALL { " AND " } else { " OR " };
        let filter = if parts.is_empty() { "1".to_owned() } else { parts.join(joiner) };
        let sql = format!(
            "SELECT c.ID FROM djmdContent c \
             LEFT JOIN djmdArtist ar ON ar.ID = c.ArtistID LEFT JOIN djmdArtist oa ON oa.ID = c.OrgArtistID \
             LEFT JOIN djmdArtist co ON co.ID = c.ComposerID LEFT JOIN djmdArtist rm ON rm.ID = c.RemixerID \
             LEFT JOIN djmdAlbum al ON al.ID = c.AlbumID LEFT JOIN djmdArtist aa ON aa.ID = al.AlbumArtistID \
             LEFT JOIN djmdGenre ge ON ge.ID = c.GenreID LEFT JOIN djmdLabel la ON la.ID = c.LabelID \
             LEFT JOIN djmdKey k ON k.ID = c.KeyID \
             WHERE {TRACK_LIVE} AND ({filter}) ORDER BY c.rowid"
        );
        let mut stmt = self.conn().prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_are_read_from_the_xml() {
        let xml = r#"<NODE Id="-5" LogicalOperator="2" AutomaticUpdate="0"><CONDITION PropertyName="artist" Operator="8" ValueUnit="" ValueLeft="A &amp; B" ValueRight=""/></NODE>"#;
        let (logical, conditions) = parse(xml).unwrap();
        assert_eq!((logical, conditions.len(), conditions[0].left.as_str()), (2, 1, "A & B"));
        assert!(parse("nothing here").is_err());
    }
}
