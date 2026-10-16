//! USB sticks and drives rekordbox has exported to: finding them and reading their libraries.

pub mod model;
pub mod paths;
pub mod read;
pub mod selection;
pub mod volumes;

pub use model::{Cue, Device, History, LibraryKind, Playlist, Track};
pub use volumes::Volume;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("database: {0}")]
    Db(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::Message(message.into()))
}
