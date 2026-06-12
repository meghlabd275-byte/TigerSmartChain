//! Secure HTTP Headers Module for TigerScan
//! Production-grade security headers

use std::collections::HashMap;

// =============================================================================
// CONSTANTS
// =============================================================================

pub const HSTS_MAX_AGE: &str = "31536000; includeSubDomains; preload";
pub const CSP_DEFAULT: &str = "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' https://api.tigerscan.io wss://api.tigerscan.io; font-src 'self' data:;";
pub const CSP_STRICT: &str = "default-src 'none'; script-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none';";

// =============================================================================
// HEADERS
// =============================================================================

/// Security headers
pub struct SecurityHeaders {
    hsts_enabled: bool,
    csp_enabled: bool,
    x_frame_options: XFrameOption,
    x_content_type: bool,
    x_xss_protection: bool,
    referrer_policy: ReferrerPolicy,
    permissions_policy: PermissionsPolicy,
}

impl Default for SecurityHeaders {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityHeaders {
    /// Create new security headers
    pub fn new() -> Self {
        Self {
            hsts_enabled: true,
            csp_enabled: true,
            x_frame_options: XFrameOption::Deny,
            x_content_type: true,
            x_xss_protection: true,
            referrer_policy: ReferrerPolicy::StrictOriginWhenCrossOrigin,
            permissions_policy: PermissionsPolicy::default(),
        }
    }

    /// Get all headers as map
    pub fn get_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        // HSTS
        if self.hsts_enabled {
            headers.insert(
                "Strict-Transport-Security".to_string(),
                HSTS_MAX_AGE.to_string(),
            );
        }

        // CSP
        if self.csp_enabled {
            headers.insert("Content-Security-Policy".to_string(), CSP_DEFAULT.to_string());
        }

        // X-Frame-Options
        headers.insert(
            "X-Frame-Options".to_string(),
            self.x_frame_options.to_string(),
        );

        // X-Content-Type-Options
        if self.x_xss_protection {
            headers.insert("X-Content-Type-Options".to_string(), "nosniff".to_string());
        }

        // X-XSS-Protection
        if self.x_xss_protection {
            headers.insert("X-XSS-Protection".to_string(), "1; mode=block".to_string());
        }

        // Referrer-Policy
        headers.insert(
            "Referrer-Policy".to_string(),
            self.referrer_policy.to_string(),
        );

        // Permissions-Policy
        headers.insert(
            "Permissions-Policy".to_string(),
            self.permissions_policy.to_string(),
        );

        // Cache control for sensitive data
        headers.insert("Cache-Control".to_string(), "no-store, no-cache, must-revalidate".to_string());
        headers.insert("Pragma".to_string(), "no-cache".to_string());

        headers
    }

    /// Apply to response (mock implementation)
    pub fn apply(&self, response: &mut HashMap<String, String>) {
        for (key, value) in self.get_headers() {
            response.insert(key, value);
        }
    }
}

// =============================================================================
// X-FRAME-OPTIONS
// =============================================================================

#[derive(Debug, Clone, Copy)]
pub enum XFrameOption {
    Deny,
    SameOrigin,
    AllowFrom(String),
}

impl Default for XFrameOption {
    fn default() -> Self {
        Self::Deny
    }
}

impl ToString for XFrameOption {
    fn to_string(&self) -> String {
        match self {
            XFrameOption::Deny => "DENY".to_string(),
            XFrameOption::SameOrigin => "SAMEORIGIN".to_string(),
            XFrameOption::AllowFrom(uri) => format!("ALLOW-FROM {}", uri),
        }
    }
}

// =============================================================================
// REFERRER-POLICY
// =============================================================================

#[derive(Debug, Clone, Copy)]
pub enum ReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    Origin,
    OriginWhenCrossOrigin,
    SameOrigin,
    StrictOrigin,
    StrictOriginWhenCrossOrigin,
}

impl Default for ReferrerPolicy {
    fn default() -> Self {
        Self::StrictOriginWhenCrossOrigin
    }
}

impl ToString for ReferrerPolicy {
    fn to_string(&self) -> String {
        match self {
            ReferrerPolicy::NoReferrer => "no-referrer".to_string(),
            ReferrerPolicy::NoReferrerWhenDowngrade => "no-referrer-when-downgrade".to_string(),
            ReferrerPolicy::Origin => "origin".to_string(),
            ReferrerPolicy::OriginWhenCrossOrigin => "origin-when-cross-origin".to_string(),
            ReferrerPolicy::SameOrigin => "same-origin".to_string(),
            ReferrerPolicy::StrictOrigin => "strict-origin".to_string(),
            ReferrerPolicy::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin".to_string(),
        }
    }
}

// =============================================================================
// PERMISSIONS-POLICY
// =============================================================================

#[derive(Debug, Clone)]
pub struct PermissionsPolicy {
    geolocation: bool,
    microphone: bool,
    camera: bool,
    payment: bool,
    usb: bool,
}

impl Default for PermissionsPolicy {
    fn default() -> Self {
        Self {
            geolocation: false,
            microphone: false,
            camera: false,
            payment: false,
            usb: false,
        }
    }
}

impl ToString for PermissionsPolicy {
    fn to_string(&self) -> String {
        let mut policies = Vec::new();

        policies.push(format!("geolocation={}", if self.geolocation { "()" } else { "()" }));
        policies.push(format!("microphone={}", if self.microphone { "()" } else { "()" }));
        policies.push(format!("camera={}", if self.camera { "()" } else { "()" }));
        policies.push(format!("payment={}", if self.payment { "()" } else { "()" }));
        policies.push(format!("usb={}", if self.usb { "()" } else { "()" }));

        policies.join(", ")
    }
}

// =============================================================================
// CORS
// =============================================================================

/// CORS configuration
pub struct CorsConfig {
    allowed_origins: Vec<String>,
    allowed_methods: Vec<String>,
    allowed_headers: Vec<String>,
    expose_headers: Vec<String>,
    max_age: u64,
    credentials: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec![
                "https://tigerscan.io".to_string(),
                "https://api.tigerscan.io".to_string(),
            ],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-API-Key".to_string(),
                "X-CSRF-Token".to_string(),
            ],
            expose_headers: vec![],
            max_age: 3600,
            credentials: true,
        }
    }
}

impl CorsConfig {
    /// Get CORS headers for request
    pub fn get_headers(&self, origin: &str) -> Option<HashMap<String, String>> {
        // Check if origin is allowed
        if !self.allowed_origins.contains(&origin.to_string()) && !self.allowed_origins.contains(&"*".to_string()) {
            return None;
        }

        let mut headers = HashMap::new();

        headers.insert("Access-Control-Allow-Origin".to_string(), origin.to_string());

        if self.credentials {
            headers.insert("Access-Control-Allow-Credentials".to_string(), "true".to_string());
        }

        headers.insert(
            "Access-Control-Allow-Methods".to_string(),
            self.allowed_methods.join(", "),
        );

        headers.insert(
            "Access-Control-Allow-Headers".to_string(),
            self.allowed_headers.join(", "),
        );

        if !self.expose_headers.is_empty() {
            headers.insert(
                "Access-Control-Expose-Headers".to_string(),
                self.expose_headers.join(", "),
            );
        }

        headers.insert("Access-Control-Max-Age".to_string(), self.max_age.to_string());

        Some(headers)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_headers() {
        let headers = SecurityHeaders::new();
        let map = headers.get_headers();

        assert!(map.contains_key("Strict-Transport-Security"));
        assert!(map.contains_key("Content-Security-Policy"));
        assert!(map.contains_key("X-Frame-Options"));
    }

    #[test]
    fn test_cors() {
        let cors = CorsConfig::default();
        let headers = cors.get_headers("https://tigerscan.io").unwrap();

        assert!(headers.contains_key("Access-Control-Allow-Origin"));
        assert!(headers.contains_key("Access-Control-Allow-Methods"));
    }
}