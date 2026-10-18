use std::fs;
use std::path::PathBuf;

use rb_model::source::{Cue, Source};
use rbl_anlz::cues::ExportCue;
use rbl_export::{SourceMyTag, SourcePlaylist, SourceTrack, SyncNode, SyncSource};

use crate::Error;

fn narrow_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn narrow_u8(value: u32) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// A measured loop end, or one derived from `BeatLoopSize` and tempo.
///
/// `BeatLoopSize` is `(numerator << 16) | denominator`. This is the same
/// derivation used when reading `djmdCue`.
fn loop_end(
    start_ms: u32,
    stored_end: Option<i64>,
    beat_loop_size: u32,
    bpm_x100: u32,
) -> Option<u32> {
    if let Some(end) = stored_end.and_then(|end| u32::try_from(end).ok()) {
        if end > start_ms {
            return Some(end);
        }
    }
    let numerator = u64::from(beat_loop_size >> 16);
    let denominator = u64::from(beat_loop_size & 0xffff);
    if numerator == 0 || denominator == 0 || bpm_x100 == 0 {
        return None;
    }
    let divisor = denominator.saturating_mul(u64::from(bpm_x100));
    let duration_ms = numerator
        .saturating_mul(6_000_000)
        .saturating_add(divisor / 2)
        / divisor;
    u32::try_from(u64::from(start_ms).saturating_add(duration_ms)).ok()
}

fn export_cue(cue: &Cue, bpm_x100: u32) -> ExportCue {
    let color = cue.color.unwrap_or(255);
    ExportCue {
        kind: cue.kind,
        time_ms: cue.time_ms,
        loop_time_ms: loop_end(cue.time_ms, cue.end_ms, cue.beat_loop_size, bpm_x100),
        color_id: if cue.kind == 0 {
            u8::try_from(color).ok().filter(|c| *c != 255).unwrap_or(0)
        } else {
            0
        },
        color_code: cue.color_code,
        comment: cue.comment.clone(),
        active_loop: cue.active_loop,
        loop_numerator: u16::try_from(cue.beat_loop_size >> 16).unwrap_or(0),
        loop_denominator: u16::try_from(cue.beat_loop_size & 0xffff).unwrap_or(0),
    }
}

fn analysis_bytes(paths: &[PathBuf]) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("DAT")
            .to_ascii_uppercase();
        let bytes = fs::read(path).map_err(|err| {
            Error::InvalidInput(format!(
                "Analysis file is unreadable ({}): {err}",
                path.display()
            ))
        })?;
        files.push((extension, bytes));
    }
    Ok(files)
}

pub(crate) fn prepare(
    job: &Source,
) -> Result<
    (
        Vec<SourceTrack>,
        Vec<SourcePlaylist>,
        Vec<SourceMyTag>,
        SyncSource,
    ),
    Error,
> {
    let mut tracks = Vec::with_capacity(job.tracks.len());
    let mut index_of: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for track in &job.tracks {
        if index_of.insert(track.id, tracks.len()).is_some() && track.id != 0 {
            return Err(Error::InvalidInput(format!(
                "Duplicate track id {}",
                track.id
            )));
        }
        let cues = track.cues.as_ref().map(|cues| {
            cues.iter()
                .map(|cue| export_cue(cue, track.bpm_x100))
                .collect::<Vec<_>>()
        });
        tracks.push(SourceTrack {
            metadata: rbl_core::ExportMetadata {
                track_number: track.metadata.track_number,
                disc_number: track.metadata.disc_number,
                bit_depth: track.metadata.bit_depth,
                play_count: track.metadata.play_count,
                analysed: track.metadata.analysed,
                hot_cue_auto_load: track.metadata.hot_cue_auto_load,
                date_created: track.metadata.date_created.clone(),
                isrc: track.metadata.isrc.clone(),
            },
            cues,
            device: None,
            id: track.id,
            source_path: track.source_path.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            genre: track.genre.clone(),
            label: track.label.clone(),
            key: track.key.clone(),
            comment: track.comment.clone(),
            date_added: track.date_added.clone(),
            release_date: track.release_date.clone(),
            bpm_x100: track.bpm_x100,
            duration_sec: narrow_u16(track.duration_sec),
            rating: narrow_u8(track.rating),
            color_id: narrow_u8(track.color_id),
            year: narrow_u16(track.year),
            bitrate: track.bitrate,
            sample_rate: track.sample_rate,
            file_size: track.file_size,
            analysis: analysis_bytes(&track.analysis)?,
            artwork: track.artwork.clone(),
            my_tags: track.my_tags.clone(),
            ..Default::default()
        });
    }

    let mut playlists = Vec::with_capacity(job.playlists.len());
    for playlist in &job.playlists {
        let mut track_indices = Vec::with_capacity(playlist.track_ids.len());
        for id in &playlist.track_ids {
            let Some(index) = index_of.get(id).copied() else {
                return Err(Error::InvalidInput(format!(
                    "Playlist '{}' refers to track {id}, which is not in this export",
                    playlist.name
                )));
            };
            track_indices.push(index);
        }
        playlists.push(SourcePlaylist {
            device_id: 0,
            device_only: false,
            id: playlist.id,
            name: playlist.name.clone(),
            parent_id: playlist.parent_id,
            folder: playlist.folder,
            track_indices,
            entries_changed_ms: playlist.entries_changed_ms,
        });
    }

    let tags = job
        .my_tags
        .iter()
        .map(|tag| SourceMyTag {
            id: tag.id,
            seq: tag.seq,
            name: tag.name.clone(),
            attribute: tag.attribute,
            parent: tag.parent,
        })
        .collect();
    let sync = SyncSource {
        db_id: job.db_id,
        automatic: job.automatic,
        tree: job
            .playlists
            .iter()
            .map(|playlist| SyncNode {
                id: playlist.id,
                parent: playlist.parent_id,
                attribute: u8::from(playlist.folder),
            })
            .collect(),
    };
    Ok((tracks, playlists, tags, sync))
}
