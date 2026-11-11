//! TigerSmartChain Security Module
//!
//! Security-critical helpers for the explorer and its services:
//!
//! - [`address`] — address validation and EIP-55 checksum handling.
//! - [`signature`] — EIP-191 and EIP-712 signature recovery and verification.
//! - [`validation`] — strict input validation and a bytecode feature scanner.
//! - [`rate_limiter`] — token-bucket and sliding-window rate limiting.
//! - [`honeypot`] — reports contract capabilities that warrant review.
//! - [`sanitize`] — output escaping for HTML and JavaScript contexts.

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
