//! CSRF Protection Module for TigerScan
//! Cross-Site Request Forgery protection

use crate::encryption::{constant_time_eq, generate_token};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

// =============================================================================
// CONSTANTS
// =============================================================================

pub const CSRF_TOKEN_LENGTH: usize = 32;
pub const CSRF_EXPIRY: u64 = 24; // hours

// =============================================================================
// CSRF PROTECTOR
// =============================================================================

/// CSRF Protector
pub struct CsrfProtector {
    tokens: RwLock<HashMap<String, CsrfToken>>,
    key: Vec<u8>,
    expiry: Duration,
}

impl Default for CsrfProtector {
    fn default() -> Self {
        Self::new()
    }
}

impl CsrfProtector {
    /// Create a new CSRF protector
    pub fn new() -> Self {
        Self {
            tokens: RwLock::new(HashMap::new()),
            key: generate_token(32).as_bytes().to_vec(),
            expiry: Duration::from_secs(CSRF_EXPIRY * 60 * 60),
        }
    }

    /// Generate CSRF token for session
    pub fn generate(&self, session_id: &str) -> String {
        let token = generate_token(CSRF_TOKEN_LENGTH);
        
        // Store token
        if let Ok(mut tokens) = self.tokens.write() {
            tokens.insert(
                format!("{}:{}", session_id, &token),
                CsrfToken {
                    token: token.clone(),
                    created: Instant::now(),
                    session_id: session_id.to_string(),
                },
            );
        }
        
        token
    }

    /// Validate CSRF token
    pub fn validate(&self, session_id: &str, token: &str) -> bool {
        let key = format!("{}:{}", session_id, token);
        
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => return false,
        };
        
        if let Some(csrf_token) = tokens.get(&key) {
            // Check expiry
            if csrf_token.created + self.expiry > Instant::now() {
                return true;
            }
        }
        
        false
    }

    /// Invalidate token
    pub fn invalidate(&self, session_id: &str, token: &str) {
        let key = format!("{}:{}", session_id, token);
        
        if let Ok(mut tokens) = self.tokens.write() {
            tokens.remove(&key);
        }
    }

    /// Invalidate all tokens for session
    pub fn invalidate_session(&self, session_id: &str) {
        if let Ok(mut tokens) = self.tokens.write() {
            tokens.retain(|key, _| !key.starts_with(session_id));
        }
    }

    /// Cleanup expired tokens
    pub fn cleanup(&self) {
        if let Ok(mut tokens) = self.tokens.write() {
            tokens.retain(|_, token| token.created + self.expiry > Instant::now());
        }
    }
}

// =============================================================================
// CSRF TOKEN
// =============================================================================

/// CSRF Token
#[derive(Debug, Clone)]
struct CsrfToken {
    token: String,
    created: Instant,
    session_id: String,
}

// =============================================================================
// MIDDLEWARE HELPERS
// =============================================================================

/// Extract CSRF token from header
pub fn get_csrf_token(headers: &[(String, String)]) -> Option<String> {
    for (name, value) in headers {
        if name.to_lowercase() == "x-csrf-token" {
            return Some(value.clone());
        }
    }
    None
}

/// Extract session ID from cookie or header
pub fn get_session_id(headers: &[(String, String)]) -> Option<String> {
    for (name, value) in headers {
        if name.to_lowercase() == "cookie" {
            for cookie in value.split(';') {
                let cookie = cookie.trim();
                if cookie.starts_with("session_id=") {
                    return Some(cookie[10..].to_string());
                }
            }
        }
    }
    None
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csrf() {
        let protector = CsrfProtector::new();
        
        let session = "session123";
        let token = protector.generate(session);
        
        assert!(protector.validate(session, &token));
        assert!(!protector.validate("wrong_session", &token));
        assert!(!protector.validate(session, "wrong_token"));
    }

    #[test]
    fn test_invalidate() {
        let protector = CsrfProtector::new();
        
        let session = "session123";
        let token = protector.generate(session);
        
        protector.invalidate(session, &token);
        
        assert!(!protector.validate(session, &token));
    }
}