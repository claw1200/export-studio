//! Changing analysis files: swapping tags in place and building the path tag.

use crate::decode::TAG_HEADER;

/// The `PPTH` tag holding the audio file's path as the analysis files record it.
#[must_use]
pub fn path_tag(name: &str) -> Vec<u8> {
    let mut body: Vec<u8> = name.encode_utf16().flat_map(u16::to_be_bytes).collect();
    body.extend([0, 0]);
    let header_len = u32::try_from(TAG_HEADER + 4).unwrap_or(0);
    let total = header_len + u32::try_from(body.len()).unwrap_or(0);
    let mut out = b"PPTH".to_vec();
    out.extend(header_len.to_be_bytes());
    out.extend(total.to_be_bytes());
    out.extend(u32::try_from(body.len()).unwrap_or(0).to_be_bytes());
    out.extend(body);
    out
}

/// `data` with the tags of the given kinds swapped in place, or added at the end when the file has none.
/// Every other tag is kept byte for byte.
pub fn replace_tags(data: &[u8], replace: &[(&str, Vec<u8>)]) -> std::io::Result<Vec<u8>> {
    patch_tags(data, replace, &[])
}

/// Like [`replace_tags`], and the tags of the `drop` kinds are removed.
pub fn patch_tags(data: &[u8], replace: &[(&str, Vec<u8>)], drop: &[&str]) -> std::io::Result<Vec<u8>> {
    let bad = |why: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, why.to_owned());
    if data.len() < 12 || &data[..4] != b"PMAI" {
        return Err(bad("not an analysis file"));
    }
    let header = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut out = data.get(..header).ok_or_else(|| bad("short header"))?.to_vec();
    let mut pending: Vec<(&str, &Vec<u8>)> = replace.iter().map(|(k, v)| (*k, v)).collect();
    let mut pos = header;
    while pos + TAG_HEADER <= data.len() {
        let len = u32::from_be_bytes([data[pos + 8], data[pos + 9], data[pos + 10], data[pos + 11]]) as usize;
        if len < TAG_HEADER || pos + len > data.len() {
            break;
        }
        let kind = &data[pos..pos + 4];
        if let Some(at) = pending.iter().position(|(k, _)| k.as_bytes() == kind) {
            out.extend(pending.remove(at).1);
        } else if !replace.iter().any(|(k, _)| k.as_bytes() == kind) && !drop.iter().any(|k| k.as_bytes() == kind) {
            out.extend(&data[pos..pos + len]);
        }
        pos += len;
    }
    for (_, tag) in pending {
        out.extend(tag);
    }
    let total = u32::try_from(out.len()).map_err(|_| bad("file too large"))?;
    out[8..12].copy_from_slice(&total.to_be_bytes());
    Ok(out)
}

/// A `PQTZ` tag with the given beats, keeping the header of `tag`.
#[must_use]
pub fn build_beatgrid(tag: &[u8], times: &[u32], beats: &[u16], bpms: &[f64]) -> Vec<u8> {
    let len_header = u32::from_be_bytes([tag[4], tag[5], tag[6], tag[7]]) as usize;
    let mut header = tag[..len_header].to_vec();
    let body: Vec<u8> = times
        .iter()
        .zip(beats)
        .zip(bpms)
        .flat_map(|((time, beat), bpm)| {
            let tempo = (bpm * 100.0).round_ties_even() as u16;
            [beat.to_be_bytes().as_slice(), &tempo.to_be_bytes(), &time.to_be_bytes()].concat()
        })
        .collect();
    let total = u32::try_from(len_header + body.len()).unwrap_or(0);
    header[8..12].copy_from_slice(&total.to_be_bytes());
    header[20..24].copy_from_slice(&u32::try_from(times.len()).unwrap_or(0).to_be_bytes());
    header.extend(body);
    header
}

/// An empty `PQT2`, as rekordbox writes for many tracks. Its per-beat data can't be rebuilt, so players fall back to `PQTZ`.
#[must_use]
pub fn clear_extended_beatgrid(tag: &[u8]) -> Vec<u8> {
    let len_header = u32::from_be_bytes([tag[4], tag[5], tag[6], tag[7]]) as usize;
    let mut header = tag[..len_header].to_vec();
    header[24..].fill(0);
    header[8..12].copy_from_slice(&u32::try_from(len_header).unwrap_or(0).to_be_bytes());
    header
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Tags;

    fn file(tags: &[Vec<u8>]) -> Vec<u8> {
        let mut data = b"PMAI".to_vec();
        data.extend(28u32.to_be_bytes());
        data.extend([0u8; 20]);
        tags.iter().for_each(|t| data.extend(t));
        data
    }

    fn other(kind: &str) -> Vec<u8> {
        let mut t = kind.as_bytes().to_vec();
        t.extend(12u32.to_be_bytes());
        t.extend(16u32.to_be_bytes());
        t.extend([1, 2, 3, 4]);
        t
    }

    #[test]
    fn swaps_the_path_tag_and_keeps_the_rest() {
        let data = file(&[path_tag("?/old.wav"), other("PQTZ")]);
        let patched = replace_tags(&data, &[("PPTH", path_tag("?/new name.wav"))]).unwrap();
        let tags = Tags::parse(patched.clone()).unwrap();
        let ppth = tags.first("PPTH").unwrap();
        let text: Vec<u16> = ppth[16..ppth.len() - 2].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        assert_eq!(String::from_utf16(&text).unwrap(), "?/new name.wav");
        assert_eq!(tags.first("PQTZ").unwrap(), other("PQTZ").as_slice());
        assert_eq!(u32::from_be_bytes([patched[8], patched[9], patched[10], patched[11]]) as usize, patched.len());
    }

    #[test]
    fn adds_a_tag_the_file_lacks() {
        let patched = replace_tags(&file(&[other("PQTZ")]), &[("PPTH", path_tag("?/a.wav"))]).unwrap();
        assert!(Tags::parse(patched).unwrap().first("PPTH").is_some());
    }

    #[test]
    fn refuses_other_files() {
        assert!(replace_tags(b"nope", &[]).is_err());
    }
}
