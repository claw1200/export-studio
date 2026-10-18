//! Changing a track's analysis: a hand-edited beat grid, and the results of our own analyser.

use std::path::{Path, PathBuf};

use rb_anlz::build::{self, Beat};
use rb_anlz::decode::{self, Tags};
use rb_anlz::write::{build_beatgrid, clear_extended_beatgrid, patch_tags};
use rb_library::{AnalysisWrite, CueMove, Library};

use crate::relocate::back_up_once;
use crate::{Error, Result};

const MIN_GRID_BPM: f64 = 40.0;
const MAX_GRID_BPM: f64 = 300.0;

fn other(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}

#[derive(Debug, Clone)]
pub struct Grid {
    pub times: Vec<i64>,
    pub beats: Vec<i64>,
    pub bpms: Vec<f64>,
}

fn check(grid: &Grid) -> Result<()> {
    let (times, beats, bpms) = (&grid.times, &grid.beats, &grid.bpms);
    if times.is_empty() || times.len() != beats.len() || times.len() != bpms.len() {
        return Err(Error::Other("A beatgrid needs matching times, beats and tempos".into()));
    }
    if times[0] < 0 || times.windows(2).any(|w| w[1] <= w[0]) {
        return Err(Error::Other("Beatgrid times must start at 0 or later and keep increasing".into()));
    }
    if beats.iter().any(|b| !(1..=4).contains(b)) {
        return Err(Error::Other("Beat numbers must be between 1 and 4".into()));
    }
    if bpms.iter().any(|b| !(MIN_GRID_BPM..=MAX_GRID_BPM).contains(b)) {
        return Err(Error::Other(format!("Tempo must be between {MIN_GRID_BPM} and {MAX_GRID_BPM} BPM")));
    }
    Ok(())
}

/// Write `data` to each path through a temporary file; on any failure put back what was there before.
fn write_all(writes: &[(PathBuf, Vec<u8>)], previous: &[(PathBuf, Option<Vec<u8>>)], then: impl FnOnce() -> Result<()>) -> Result<()> {
    let result = (|| -> Result<()> {
        for (path, data) in writes {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(other)?;
            }
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, path)).map_err(other)?;
        }
        then()
    })();
    if result.is_err() {
        for (path, old) in previous {
            let restored = match old {
                Some(bytes) => std::fs::write(path, bytes),
                None => std::fs::remove_file(path).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
            };
            if let Err(e) = restored {
                eprintln!("[es-engine] could not restore {}: {e}", path.display());
            }
        }
    }
    result
}

/// Replace a track's beat grid, move the cues given, and return the tempo when the grid has only one.
pub fn set_beatgrid(lib: &mut Library, share: &Path, id: &str, grid: &Grid, moves: &[CueMove]) -> Result<Option<f64>> {
    check(grid)?;
    let files = lib.track_files(id)?.ok_or_else(|| Error::Other(format!("Unknown track {id}")))?;
    let dat = files
        .analysis_path
        .as_deref()
        .and_then(|rel| rb_anlz::file(share, rel, "DAT"))
        .filter(|p| p.is_file())
        .ok_or_else(|| Error::Other("This track has no analysis file to hold a beatgrid".into()))?;
    let old_dat = std::fs::read(&dat).map_err(other)?;
    let tags = Tags::parse(old_dat.clone()).map_err(other)?;
    let pqtz = tags.first("PQTZ").ok_or_else(|| Error::Other("This track has no beatgrid to edit".into()))?;
    let times: Vec<u32> = grid.times.iter().map(|t| u32::try_from(*t).unwrap_or(0)).collect();
    let beats: Vec<u16> = grid.beats.iter().map(|b| u16::try_from(*b).unwrap_or(0)).collect();
    let bpms: Vec<f64> = grid.bpms.iter().map(|b| (b * 100.0).round_ties_even() / 100.0).collect();
    let mut writes = vec![(dat.clone(), patch_tags(&old_dat, &[("PQTZ", build_beatgrid(pqtz, &times, &beats, &bpms))], &[]).map_err(other)?)];
    let mut previous = vec![(dat.clone(), Some(old_dat.clone()))];

    let ext = dat.with_extension("EXT");
    if ext.is_file() {
        let old_ext = std::fs::read(&ext).map_err(other)?;
        let ext_tags = Tags::parse(old_ext.clone()).map_err(other)?;
        if let Some(pqt2) = ext_tags.first("PQT2") {
            writes.push((ext.clone(), patch_tags(&old_ext, &[("PQT2", clear_extended_beatgrid(pqt2))], &[]).map_err(other)?));
            previous.push((ext, Some(old_ext)));
        }
    }
    back_up_once(id, &writes.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>()).map_err(other)?;

    let constant = (bpms.windows(2).all(|w| w[0] == w[1])).then(|| bpms[0]);
    let bpm_x100 = constant.map(|b| (b * 100.0).round_ties_even() as i64);
    write_all(&writes, &previous, || {
        lib.write(|tx| tx.apply_beatgrid(id, bpm_x100, moves))?;
        Ok(())
    })?;
    Ok(constant)
}

/// What the native analyser found for a track.
#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub beats: Vec<Beat>,
    /// Eight bytes per 1/150 s: peak and RMS of the whole signal and of the low, mid and high bands.
    pub waveform: Vec<u8>,
    pub key: Option<(i64, bool)>,
    pub bpm: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Replace a grid, key or waveform the track already has.
    pub overwrite: bool,
    pub grid: bool,
    pub key: bool,
}

/// Write an analysis: the analysis files, tempo, key and the analysed flag. Cues already in the files are
/// kept; phrases go when the grid they were counted on changes.
pub fn apply_analysis(lib: &mut Library, share: &Path, id: &str, result: &Analysis, options: Options) -> Result<()> {
    let files = lib.track_files(id)?.ok_or_else(|| Error::Other(format!("Unknown track {id}")))?;
    let features = build::features_from_bytes(&result.waveform);
    let audio_name = Path::new(&files.folder_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

    let relative = match files.analysis_path.clone() {
        Some(path) => path,
        None => {
            let uuid = lib.track_uuid(id)?.filter(|u| u.len() > 3).ok_or_else(|| Error::Other("This track has no UUID".into()))?;
            format!("/PIONEER/USBANLZ/{}/{}/ANLZ0000.DAT", &uuid[..3], &uuid[3..])
        }
    };
    let dat = share.join(relative.trim_matches(['/', '\\']));
    let kinds: Vec<(&str, &[&str], PathBuf)> = build::WAVE_TAGS.iter().map(|(kind, tags)| (*kind, *tags, dat.with_extension(kind))).collect();
    let existing: Vec<Option<(Vec<u8>, Tags)>> = kinds
        .iter()
        .map(|(_, _, path)| match std::fs::read(path) {
            Ok(bytes) => Tags::parse(bytes.clone()).map(|t| Some((bytes, t))).map_err(other),
            Err(_) => Ok(None),
        })
        .collect::<Result<_>>()?;
    let has_grid = existing[0].as_ref().and_then(|(_, t)| decode::beatgrid(t)).is_some_and(|g| !g.times.is_empty());
    let write_grid = options.grid && !result.beats.is_empty() && (options.overwrite || !has_grid);

    let fresh = build::new_files(&audio_name, if write_grid { &result.beats } else { &[] }, &features);
    let waves = if features.is_empty() { Vec::new() } else { build::wave_tags(&features) };
    let mut writes: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (index, (kind, wave_kinds, path)) in kinds.iter().enumerate() {
        let Some((bytes, tags)) = &existing[index] else {
            if let Some((_, data)) = fresh.iter().find(|(k, _)| k == kind) {
                writes.push((path.clone(), data.clone()));
            }
            continue;
        };
        let mut replace: Vec<(&str, Vec<u8>)> = wave_kinds
            .iter()
            .filter_map(|k| waves.iter().find(|(w, _)| w == k).map(|(_, t)| (*k, t.clone())))
            .filter(|(k, _)| options.overwrite || tags.first(k).is_none())
            .collect();
        let mut drop: Vec<&str> = Vec::new();
        if write_grid && *kind == "DAT" {
            replace.push(("PQTZ", build::beatgrid_tag(&result.beats)));
        }
        if write_grid && *kind == "EXT" {
            if let Some(pqt2) = tags.first("PQT2") {
                replace.push(("PQT2", clear_extended_beatgrid(pqt2)));
            }
            drop.push(rb_anlz::PHRASE_TAG);
        }
        if !replace.is_empty() || !drop.is_empty() {
            writes.push((path.clone(), patch_tags(bytes, &replace, &drop).map_err(other)?));
        }
    }

    let present: Vec<PathBuf> = kinds.iter().zip(&existing).filter(|(_, e)| e.is_some()).map(|((_, _, p), _)| p.clone()).collect();
    if !present.is_empty() {
        back_up_once(id, &present).map_err(other)?;
    }
    let previous: Vec<(PathBuf, Option<Vec<u8>>)> = writes.iter().map(|(p, _)| (p.clone(), std::fs::read(p).ok())).collect();
    let write = AnalysisWrite {
        relative_path: &relative,
        bpm_x100: write_grid.then(|| (result.bpm * 100.0).round_ties_even() as i64),
        key: if options.key { result.key } else { None },
        overwrite_key: options.overwrite,
        duration_secs: result.duration.round_ties_even() as i64,
    };
    write_all(&writes, &previous, || {
        lib.write(|tx| tx.apply_analysis(id, &write))?;
        Ok(())
    })
}
