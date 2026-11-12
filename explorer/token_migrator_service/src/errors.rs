//! Migrator errors.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("invalid input: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
