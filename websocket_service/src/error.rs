//! WebSocket Service Error Types
//! Comprehensive error handling with security context

use thiserror::Error;

/// WebSocket service errors
#[derive(Error, Debug)]
pub enum Error {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    #[error("Message error: {0}")]
    Message(String),
    
    #[error("Subscription error: {0}")]
    Subscription(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
    
    #[error("Authentication error: {0}")]
    Authentication(String),
    
    #[error("Authorization error: {0}")]
    Authorization(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Resource already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Internal server error: {0}")]
    Internal(String),
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Blockchain error: {0}")]
    Blockchain(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Create a connection error
    pub fn connection(msg: impl Into<String>) -> Self {
        Self::Connection(msg.into())
    }
    
    /// Create a protocol error
    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(msg.into())
    }
    
    /// Create a message error
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
    
    /// Create a subscription error
    pub fn subscription(msg: impl Into<String>) -> Self {
        Self::Subscription(msg.into())
    }
    
    /// Create a rate limit error
    pub fn rate_limit(msg: impl Into<String>) -> Self {
        Self::RateLimit(msg.into())
    }
    
    /// Create an authentication error
    pub fn authentication(msg: impl Into<String>) -> Self {
        Self::Authentication(msg.into())
    }
    
    /// Create an authorization error
    pub fn authorization(msg: impl Into<String>) -> Self {
        Self::Authorization(msg.into())
    }
    
    /// Create a not found error
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    
    /// Create an already exists error
    pub fn already_exists(msg: impl Into<String>) -> Self {
        Self::AlreadyExists(msg.into())
    }
    
    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
    
    /// Create a timeout error
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }
    
    /// Create a database error
    pub fn database(msg: impl Into<String>) -> Self {
        Self::Database(msg.into())
    }
    
    /// Create a blockchain error
    pub fn blockchain(msg: impl Into<String>) -> Self {
        Self::Blockchain(msg.into())
    }
    
    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout(_) | Self::Internal(_) | Self::Database(_))
    }
    
    /// Check if error is client error (4xx)
    pub fn is_client_error(&self) -> bool {
        matches!(self, 
            Self::Authentication(_) | Self::Authorization(_) | 
            Self::NotFound(_) | Self::AlreadyExists(_) |
            Self::RateLimit(_)
        )
    }
    
    /// Get HTTP status code equivalent
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Connection(_) => 400,
            Self::Protocol(_) => 400,
            Self::Message(_) => 400,
            Self::Subscription(_) => 400,
            Self::RateLimit(_) => 429,
            Self::Authentication(_) => 401,
            Self::Authorization(_) => 403,
            Self::NotFound(_) => 404,
            Self::AlreadyExists(_) => 409,
            Self::Internal(_) => 500,
            Self::Timeout(_) => 504,
            Self::Database(_) => 500,
            Self::Blockchain(_) => 502,
        }
    }
}