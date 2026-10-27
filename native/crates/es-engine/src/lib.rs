//! Use cases. Each takes the storage crates it needs and does one thing the app offers.

pub mod analysis;
pub mod artwork;
pub mod browse;
pub mod convert;
pub mod device_import;
pub mod import;
pub mod library;
pub mod relocate;
pub mod place;
pub mod sync;

pub use es_convert::inspect;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Db(#[from] rb_library::Error),
    #[error(transparent)]
    Device(#[from] rb_device::Error),
    #[error(transparent)]
    Backup(#[from] es_backup::Error),
    #[error("{0}")]
    Other(String),
    /// The stick holds something the export would overwrite.
    #[error("{0}")]
    Conflict(String),
    /// The export wrote the stick but its own check of it failed.
    #[error("{0}")]
    Verify(String),
    /// The export program could not publish the device databases.
    #[error("{0}")]
    Publication(String),
    #[error("USB changes require review")]
    ReviewRequired(rb_model::conflict::Inspection),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<es_convert::Error> for Error {
    fn from(error: es_convert::Error) -> Self {
        Self::Other(error.0)
    }
}
