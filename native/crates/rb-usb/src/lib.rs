//! USB export from caller-supplied records, without an application session or master database.

mod prepare;

use std::path::{Path, PathBuf};

use rb_model::source::Source;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidInput(String),
    #[error("USB sync conflict: {0}")]
    Conflict(String),
    #[error("Export stopped.")]
    Cancelled,
    #[error("the device went away during the export")]
    DeviceGone,
    #[error("the destination is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("nothing to export")]
    Empty,
    #[error("could not write exportLibrary.db: {0}")]
    Database(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<rbl_export::ExportError> for Error {
    fn from(error: rbl_export::ExportError) -> Self {
        match error {
            rbl_export::ExportError::Conflict(message) => Self::Conflict(message),
            rbl_export::ExportError::Cancelled => Self::Cancelled,
            rbl_export::ExportError::DeviceGone => Self::DeviceGone,
            rbl_export::ExportError::NotADirectory(path) => Self::NotADirectory(path),
            rbl_export::ExportError::Empty => Self::Empty,
            rbl_export::ExportError::Io(error) => Self::Io(error),
            rbl_export::ExportError::OneLibrary(message) => Self::Database(message),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub settings_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub stage: &'static str,
    pub done: usize,
    pub total: usize,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub verified: bool,
    pub up_to_date: bool,
    pub tracks: usize,
    pub playlists: usize,
    pub reused: usize,
    pub removed: usize,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub one_library: bool,
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Conflict(_) => "conflict",
            Self::Cancelled => "cancelled",
            Self::DeviceGone => "device",
            _ => "export",
        }
    }
}

/// Execute a recoverable export; an interrupted prior publication is recovered first.
pub fn export(
    destination: &Path,
    source: &Source,
    options: &Options,
    progress: &mut dyn FnMut(&Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<Report> {
    let (tracks, playlists, tags, sync) = prepare::prepare(source)?;
    if cancelled() {
        return Err(Error::Cancelled);
    }
    rbl_export::recover(destination)?;
    let settings = rbl_export::MY_SETTINGS_FILES
        .iter()
        .filter_map(|name| {
            options
                .settings_dir
                .as_ref()
                .map(|directory| (name, directory.join(name)))
        })
        .filter(|(name, path)| {
            path.is_file() && !rbl_export::export_root(destination).join(name).exists()
        })
        .map(|(name, path)| Ok(((*name).to_owned(), std::fs::read(path)?)))
        .collect::<std::io::Result<Vec<_>>>()?;
    let report = rbl_export::export_cancellable(
        destination,
        &tracks,
        &playlists,
        &tags,
        &rbl_export::ExportOptions {
            sync: Some(&sync),
            settings_files: &settings,
            ..Default::default()
        },
        &mut |event| {
            let (done, total) = if event.tracks_total > 0 && event.total == 0 {
                (event.tracks_done, event.tracks_total)
            } else {
                (event.done, event.total)
            };
            progress(&Progress {
                stage: event.stage,
                done,
                total,
                title: event.title.clone(),
            });
        },
        cancelled,
    )?;
    Ok(Report {
        verified: report.verified,
        up_to_date: report.up_to_date,
        tracks: report.tracks,
        playlists: report.playlists,
        reused: report.reused,
        removed: report.removed,
        skipped: report.skipped,
        failed: report.failed,
        one_library: report.one_library,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_model::source::{Playlist, Track};

    #[test]
    fn invalid_source_records_do_not_create_a_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("USB");
        let source = Source {
            playlists: vec![Playlist {
                id: 1,
                name: "Mix".into(),
                track_ids: vec![7],
                ..Default::default()
            }],
            ..Default::default()
        };
        let error = export(
            &destination,
            &source,
            &Options::default(),
            &mut |_| {},
            &|| false,
        )
        .unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
        assert!(!destination.exists());
    }

    #[test]
    fn duplicate_source_ids_are_rejected_before_reading_files() {
        let source = Source {
            tracks: vec![
                Track {
                    id: 7,
                    ..Default::default()
                },
                Track {
                    id: 7,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(matches!(
            prepare::prepare(&source),
            Err(Error::InvalidInput(_))
        ));
    }

    #[test]
    fn source_records_export_without_a_database_or_application_session() {
        let directory = tempfile::tempdir().unwrap();
        let audio = directory.path().join("track.wav");
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(16036u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend([1, 0, 1, 0]);
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend(16000u32.to_le_bytes());
        bytes.extend([2, 0, 16, 0]);
        bytes.extend(b"data");
        bytes.extend(16000u32.to_le_bytes());
        bytes.resize(16044, 0);
        std::fs::write(&audio, bytes).unwrap();
        let destination = directory.path().join("USB");
        std::fs::create_dir(&destination).unwrap();
        let source = Source {
            db_id: 4242,
            tracks: vec![Track {
                id: 7,
                source_path: audio,
                title: "SDK track".into(),
                duration_sec: 1,
                sample_rate: 8000,
                ..Default::default()
            }],
            playlists: vec![Playlist {
                id: 10,
                name: "SDK mix".into(),
                track_ids: vec![7],
                ..Default::default()
            }],
            ..Default::default()
        };
        let report = export(
            &destination,
            &source,
            &Options::default(),
            &mut |_| {},
            &|| false,
        )
        .unwrap();
        assert!(report.verified);
        let device = rb_device::read::read(&destination).unwrap().unwrap();
        assert_eq!(device.tracks[0].title, "SDK track");
        assert_eq!(device.playlists[0].name, "SDK mix");
        assert!(!destination.join("PIONEER/MYSETTING.DAT").exists());

        let settings = directory.path().join("settings");
        std::fs::create_dir(&settings).unwrap();
        std::fs::write(settings.join("MYSETTING.DAT"), b"supplied settings").unwrap();
        export(
            &destination,
            &source,
            &Options {
                settings_dir: Some(settings.clone()),
            },
            &mut |_| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(destination.join("PIONEER/MYSETTING.DAT")).unwrap(),
            b"supplied settings"
        );
        std::fs::write(settings.join("MYSETTING.DAT"), b"different settings").unwrap();
        export(
            &destination,
            &source,
            &Options {
                settings_dir: Some(settings),
            },
            &mut |_| {},
            &|| false,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(destination.join("PIONEER/MYSETTING.DAT")).unwrap(),
            b"supplied settings"
        );
    }

    #[test]
    fn cancellation_does_not_create_a_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("USB");
        assert!(matches!(
            export(
                &destination,
                &Source::default(),
                &Options::default(),
                &mut |_| {},
                &|| true
            ),
            Err(Error::Cancelled)
        ));
        assert!(!destination.exists());
    }
}
