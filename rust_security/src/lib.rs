//! TigerSmartChain Security Module
//! 
//! This module provides security-critical functionality for the blockchain explorer:
//! - Address validation and verification
//! - Signature verification
//! - Reentrancy protection
//! - Rate limiting
//! - Input sanitization
//! - Honeypot detection

pub mod address;
pub mod signature;
pub mod validation;
pub mod rate_limiter;
pub mod honeypot;
pub mod sanitize;

pub use address::AddressValidator;
pub use signature::SignatureVerifier;
pub use validation::InputValidator;
pub use rate_limiter::RateLimiter;
pub use honeypot::HoneypotDetector;
pub use sanitize::Sanitizer;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Invalid address format: {0}")]
    InvalidAddress(String),
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Potential honeypot detected")]
    HoneypotDetected,
    
    #[error("Reentrancy detected")]
    ReentrancyDetected,
}

pub type Result<T> = std::result::Result<T, SecurityError>;
