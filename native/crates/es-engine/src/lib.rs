//! Use cases. Each takes the storage crates it needs and does one thing the app offers.

pub mod artwork;
pub mod import;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Db(#[from] es_db::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
