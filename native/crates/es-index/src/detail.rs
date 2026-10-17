//! One track's detail: its cues, beat grid and waveforms, as the interface draws them.

use std::collections::HashMap;
use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use es_anlz::decode::{self, Tags};
use es_db::{Error, Library, Result};
use serde::Serialize;

/// The waveform sample rate the interface assumes, in columns per second.
const DETAIL_RATE: i64 = 150;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub id: String,
    pub kind: &'static str,
    pub slot: i64,
    pub start: i64,
    pub end: Option<i64>,
    pub color: Option<i64>,
    pub color_index: Option<i64>,
    pub color_code: Option<u8>,
    pub comment: String,
    pub rgb: Option<String>,
}

impl Cue {
    pub(crate) fn apply_analysis_color(&mut self, colors: &HashMap<(u32, u32), decode::HotCueColor>) {
        let slot = if self.slot >= 5 { self.slot - 1 } else { self.slot };
        let color = (self.kind == "hot")
            .then(|| u32::try_from(slot).ok().zip(u32::try_from(self.start).ok()).and_then(|key| colors.get(&key)))
            .flatten();
        self.color_code = color.and_then(|color| color.code);
        self.rgb = color.map(|color| color.rgb.clone());
    }
}

#[derive(Debug, Serialize)]
pub struct Grid {
    pub times: Vec<u32>,
    pub beats: Vec<u16>,
    pub bpms: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct Colour {
    pub length: usize,
    pub heights: String,
    pub colors: String,
}

#[derive(Debug, Serialize)]
pub struct Bands {
    pub length: usize,
    pub bands: String,
}

#[derive(Debug, Serialize)]
pub struct Mono {
    pub length: usize,
    pub heights: String,
    pub whiteness: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    pub rate: i64,
    pub rgb: Option<Colour>,
    pub three_band: Option<Bands>,
    pub three_band_preview: Option<Bands>,
    pub mono: Option<Mono>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub id: String,
    pub file_exists: bool,
    pub cues: Vec<Cue>,
    pub beatgrid: Option<Grid>,
    pub waveform: Waveform,
}

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn tags(share: &Path, relative: Option<&str>, suffix: &str) -> Option<Tags> {
    let path = es_anlz::file(share, relative?, suffix)?;
    Tags::read(&path).ok()
}

/// Cues in the order the interface lists them: hot cues first, by slot then time.
pub fn cues(lib: &Library, content_id: &str, ext: Option<&Tags>) -> Result<Vec<Cue>> {
    let colors = ext.map(decode::hot_cue_colors).unwrap_or_default();
    let mut out: Vec<Cue> = lib
        .cues(content_id)?
        .into_iter()
        .map(|q| {
            let hot = q.kind > 0;
            let mut cue = Cue {
                rgb: None,
                id: q.id,
                kind: if hot { "hot" } else { "memory" },
                slot: q.kind,
                start: q.start_ms,
                end: q.end_ms.filter(|e| *e >= 0),
                color: q.color.filter(|c| *c >= 0),
                color_index: q.color_index,
                color_code: None,
                comment: q.comment,
            };
            cue.apply_analysis_color(&colors);
            cue
        })
        .collect();
    out.sort_by_key(|c| (c.kind != "hot", c.slot, c.start));
    Ok(out)
}

/// A track's cues, with hot cue colours read from its analysis file.
pub fn cues_for(lib: &Library, share: &Path, id: &str) -> Result<Vec<Cue>> {
    let files = lib.track_files(id)?.ok_or_else(|| Error::NotFound(format!("track {id}")))?;
    let ext = tags(share, files.analysis_path.as_deref(), "EXT");
    cues(lib, id, ext.as_ref())
}

pub fn track_detail(lib: &Library, share: &Path, id: &str) -> Result<Detail> {
    let files = lib.track_files(id)?.ok_or_else(|| Error::NotFound(format!("track {id}")))?;
    let relative = files.analysis_path.as_deref();
    let dat = tags(share, relative, "DAT");
    let ext = tags(share, relative, "EXT");
    let ex2 = tags(share, relative, "2EX");

    let rgb = ext.as_ref().and_then(decode::rgb);
    let bands = |kind: &str| ex2.as_ref().and_then(|t| decode::bands(t, kind)).map(|b| Bands { length: b.length, bands: b64(&b.bands) });
    let waveform = Waveform {
        rate: DETAIL_RATE,
        three_band: bands("PWV7"),
        three_band_preview: bands("PWV6"),
        mono: if rgb.is_some() {
            None
        } else {
            ext.as_ref().and_then(decode::mono).map(|m| Mono { length: m.length, heights: b64(&m.heights), whiteness: b64(&m.whiteness) })
        },
        rgb: rgb.map(|r| Colour { length: r.length, heights: b64(&r.heights), colors: b64(&r.colors) }),
    };
    Ok(Detail {
        id: id.to_owned(),
        file_exists: Path::new(&files.folder_path).is_file(),
        cues: cues(lib, id, ext.as_ref())?,
        beatgrid: dat.as_ref().and_then(decode::beatgrid).map(|g| Grid { times: g.times, beats: g.beats, bpms: g.bpms }),
        waveform,
    })
}

/// A track's beat grid as its analysis file holds it.
pub fn beatgrid_for(lib: &Library, share: &Path, id: &str) -> Result<Option<Grid>> {
    let files = lib.track_files(id)?.ok_or_else(|| Error::NotFound(format!("track {id}")))?;
    let dat = tags(share, files.analysis_path.as_deref(), "DAT");
    Ok(dat.as_ref().and_then(decode::beatgrid).map(|g| Grid { times: g.times, beats: g.beats, bpms: g.bpms }))
}

/// A short waveform for a browser row.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bands: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rgb_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heights: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<String>,
}

/// Cue stars and a short waveform for a row on screen.
#[derive(Debug, Serialize)]
pub struct Preview {
    pub id: String,
    pub memory: bool,
    pub hot: bool,
    pub phrase: bool,
    pub cues: Vec<Cue>,
    pub waveform: Option<Overview>,
}

impl Preview {
    #[must_use]
    pub fn blank(id: &str) -> Self {
        Self { id: id.to_owned(), memory: false, hot: false, phrase: false, cues: Vec::new(), waveform: None }
    }

    /// `cues` unpainted; their colours come from `ext`.
    #[must_use]
    pub fn build(id: &str, mut cues: Vec<Cue>, ext: Option<&Tags>, ex2: Option<&Tags>) -> Self {
        let colors = ext.map(decode::hot_cue_colors).unwrap_or_default();
        for cue in &mut cues {
            cue.apply_analysis_color(&colors);
        }
        cues.sort_by_key(|c| (c.kind != "hot", c.slot, c.start));
        let hot = cues.iter().any(|c| (1..=9).contains(&c.slot) && c.slot != 4);
        let memory = cues.iter().any(|c| c.slot == 0);
        let waveform = decode::overview(ext, ex2).map(|o| Overview {
            band_length: o.band_length,
            bands: o.band_length.map(|_| b64(&o.bands)),
            rgb_length: o.colour_length,
            heights: o.colour_length.map(|_| b64(&o.heights)),
            colors: o.colour_length.map(|_| b64(&o.colors)),
        });
        Self { id: id.to_owned(), memory, hot, phrase: ext.is_some_and(|e| e.first(es_anlz::PHRASE_TAG).is_some()), cues, waveform }
    }
}

/// Previews for up to 80 distinct ids, in the order asked.
pub fn previews(lib: &Library, share: &Path, ids: &[String]) -> Result<Vec<Preview>> {
    let mut wanted: Vec<&String> = Vec::new();
    for id in ids {
        if !wanted.contains(&id) && wanted.len() < 80 {
            wanted.push(id);
        }
    }
    let mut out = Vec::new();
    for id in wanted {
        let Some(files) = lib.track_files(id)? else { continue };
        let ext = tags(share, files.analysis_path.as_deref(), "EXT");
        let ex2 = tags(share, files.analysis_path.as_deref(), "2EX");
        out.push(Preview::build(id, cues(lib, id, None)?, ext.as_ref(), ex2.as_ref()));
    }
    Ok(out)
}
