//! Converting tracks' audio files with ffmpeg and pointing the library at the results.

use std::path::{Path, PathBuf};

use rb_anlz::decode::Tags;
use rb_anlz::write::{patch_tags, path_tag, timeline_patches};
use rb_library::{ConversionTarget, Library, Stream};

pub use es_convert::{build_command, clean_preset, convert_file, delay_offset_ms, encode, ffmpeg_path, matches_format, output_path, Preset};

use crate::import::file_type_code;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A new track beside the original.
    Copy,
    /// The track points at the new file; the old one stays.
    Update,
    /// Like `Update`, and the old file is listed for the caller to remove.
    UpdateDelete,
}

impl Mode {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "copy" => Ok(Self::Copy),
            "update" => Ok(Self::Update),
            "update_delete" => Ok(Self::UpdateDelete),
            other => Err(Error::Other(format!("Unknown conversion mode: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub ids: Vec<String>,
    pub originals: Vec<String>,
    pub failed: Vec<(String, String)>,
}

fn stored_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn convert_one(lib: &mut Library, share: &Path, id: &str, preset: &Preset, mode: Mode) -> Result<(String, PathBuf)> {
    let files = lib.track_files(id)?.ok_or_else(|| Error::Other(format!("Unknown track {id}")))?;
    let source = PathBuf::from(&files.folder_path);
    let target = convert_file(&source, preset)?;
    let done = (|| -> Result<String> {
        let info = es_tags::read(&target).map_err(|e| Error::Other(e.to_string()))?;
        let offset = delay_offset_ms(&source, &target).unwrap_or(0);
        let nonzero = |v: i64| (v != 0).then_some(v);
        let old_files: Vec<PathBuf> = files.analysis_path.as_deref().map(|rel| rb_anlz::files(share, rel)).unwrap_or_default().into_iter().filter(|p| p.is_file()).collect();
        let carries = old_files.iter().any(|p| p.extension().is_some_and(|e| e == "DAT"));
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let extension = target.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let change = ConversionTarget {
            stored: stored_path(&target),
            file_name: name.clone(),
            file_size: target.metadata().ok().and_then(|m| i64::try_from(m.len()).ok()),
            file_type: file_type_code(&extension),
            stream: Stream {
                length: nonzero(info.length),
                bit_rate: nonzero(info.bitrate),
                sample_rate: nonzero(info.sample_rate),
                bit_depth: nonzero(info.bit_depth),
            },
            offset_ms: offset,
            carry_analysis: carries,
        };
        let converted = lib.write(|tx| tx.convert_track(id, mode == Mode::Copy, &change))?;
        if let Some(relative) = &converted.analysis {
            let dat = share.join(relative.trim_matches(['/', '\\']));
            for source_file in &old_files {
                let Some(suffix) = source_file.extension() else { continue };
                let path = dat.with_extension(suffix);
                let bytes = std::fs::read(source_file).map_err(|e| Error::Other(e.to_string()))?;
                let mut replace = vec![("PPTH", path_tag(&format!("?/{name}")))];
                if offset != 0 {
                    if let Ok(tags) = Tags::parse(bytes.clone()) {
                        replace.extend(timeline_patches(&tags, offset));
                    }
                }
                let patched = patch_tags(&bytes, &replace, &[]).map_err(|e| Error::Other(e.to_string()))?;
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| Error::Other(e.to_string()))?;
                }
                std::fs::write(&path, patched).map_err(|e| Error::Other(e.to_string()))?;
            }
        }
        Ok(converted.id)
    })();
    match done {
        Ok(new_id) => Ok((new_id, source)),
        Err(e) => {
            let _ = std::fs::remove_file(&target);
            Err(e)
        }
    }
}

/// Encode each track's audio file to `preset` and point the library at the result. A track that fails is left
/// as it was.
pub fn convert_tracks(lib: &mut Library, share: &Path, ids: &[String], preset: &Preset, mode: Mode, progress: &dyn Fn(usize, usize, Option<&str>)) -> Outcome {
    let mut ordered: Vec<&String> = Vec::new();
    for id in ids {
        if !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    let mut outcome = Outcome::default();
    for (done, id) in ordered.iter().enumerate() {
        progress(done, ordered.len(), Some(id));
        match convert_one(lib, share, id, preset, mode) {
            Ok((new_id, source)) => {
                outcome.ids.push(new_id);
                if mode == Mode::UpdateDelete {
                    outcome.originals.push(source.to_string_lossy().into_owned());
                }
            }
            Err(e) => {
                let message = e.to_string();
                outcome.failed.push(((*id).clone(), if message.is_empty() { "Error".into() } else { message }));
            }
        }
    }
    progress(ordered.len(), ordered.len(), None);
    outcome
}
