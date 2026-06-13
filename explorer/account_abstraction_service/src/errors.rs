//! AA Errors

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Entry point error: {0}")]
    EntryPoint(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Bundler error: {0}")]
    Bundler(String),
    
    #[error("Wallet error: {0}")]
    Wallet(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;