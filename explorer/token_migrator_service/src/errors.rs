//! Migrator Errors

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Migration error: {0}")]
    Migration(String),
}

pub type Result<T> = std::result::Result<T, Error>;