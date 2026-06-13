//! ENS Service Errors
//! Comprehensive error handling

use thiserror::Error;

/// ENS service errors
#[derive(Error, Debug)]
pub enum Error {
    #[error("Resolution error: {0}")]
    Resolution(String),
    
    #[error("Registry error: {0}")]
    Registry(String),
    
    #[error("Resolver error: {0}")]
    Resolver(String),
    
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Cache error: {0}")]
    Cache(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Invalid name: {0}")]
    InvalidName(String),
    
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Create resolution error
    pub fn resolution(msg: impl Into<String>) -> Self {
        Self::Resolution(msg.into())
    }
    
    /// Create registry error
    pub fn registry(msg: impl Into<String>) -> Self {
        Self::Registry(msg.into())
    }
    
    /// Create validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
    
    /// Create not found error
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    
    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Resolution(_) | Self::Database(_) | Self::Internal(_))
    }
}