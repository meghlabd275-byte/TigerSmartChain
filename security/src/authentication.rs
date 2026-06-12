//! Authentication Module for TigerScan
//! JWT authentication, API keys, OAuth2, and user management

use crate::encryption::{constant_time_eq, generate_token, PasswordHasher};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

// =============================================================================
// CONSTANTS
// =============================================================================

pub const JWT_ALGORITHM: Algorithm = Algorithm::RS256;
pub const DEFAULT_ACCESS_EXPIRY: i64 = 15; // minutes
pub const DEFAULT_REFRESH_EXPIRY: i64 = 7; // days

// =============================================================================
// TYPES
// =============================================================================

/// JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,          // User ID
    pub email: String,         // User email
    pub tier: String,          // API tier
    pub scopes: Vec<String>,   // Access scopes
    pub api_key_id: Option<String>,
    pub iat: i64,             // Issued at
    pub exp: i64,             // Expiration
    pub iss: String,          // Issuer
}

/// User struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub tier: String,
    pub scopes: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_login: Option<i64>,
}

/// API Key struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub key_hash: String,
    pub name: String,
    pub user_id: String,
    pub tier: String,
    pub scopes: Vec<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

/// Token pair response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub token_type: String,
}

// =============================================================================
// AUTHENTICATOR
// =============================================================================

/// Main authenticator struct
pub struct Authenticator {
    jwt_secret: Vec<u8>,
    jwt_issuer: String,
    access_expiry: Duration,
    refresh_expiry: Duration,
    password_hasher: PasswordHasher,
    users: RwLock<HashMap<String, User>>,
    api_keys: RwLock<HashMap<String, ApiKey>>,
    refresh_tokens: RwLock<HashMap<String, String>>,
    blacklist: RwLock<HashMap<String, i64>>,
}

impl Default for Authenticator {
    fn default() -> Self {
        Self::new()
    }
}

impl Authenticator {
    /// Create a new authenticator
    pub fn new() -> Self {
        let mut secret = vec![0u8; 64];
        rand::thread_rng().fill_bytes(&mut secret);
        
        Self {
            jwt_secret: secret,
            jwt_issuer: "tigerscan".to_string(),
            access_expiry: Duration::minutes(DEFAULT_ACCESS_EXPIRY),
            refresh_expiry: Duration::days(DEFAULT_REFRESH_EXPIRY),
            password_hasher: PasswordHasher::new(),
            users: RwLock::new(HashMap::new()),
            api_keys: RwLock::new(HashMap::new()),
            refresh_tokens: RwLock::new(HashMap::new()),
            blacklist: RwLock::new(HashMap::new()),
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        secret: Vec<u8>,
        issuer: String,
        access_minutes: i64,
        refresh_days: i64,
    ) -> Self {
        Self {
            jwt_secret: secret,
            jwt_issuer: issuer,
            access_expiry: Duration::minutes(access_minutes),
            refresh_expiry: Duration::days(refresh_days),
            password_hasher: PasswordHasher::new(),
            users: RwLock::new(HashMap::new()),
            api_keys: RwLock::new(HashMap::new()),
            refresh_tokens: RwLock::new(HashMap::new()),
            blacklist: RwLock::new(HashMap::new()),
        }
    }

    // =============================================================================
    // JWT METHODS
    // =============================================================================

    /// Generate token pair for user
    pub fn generate_tokens(&self, user: &User) -> Result<TokenPair, String> {
        let now = Utc::now().timestamp();
        
        // Access token
        let access_claims = Claims {
            sub: user.id.clone(),
            email: user.email.clone(),
            tier: user.tier.clone(),
            scopes: user.scopes.clone(),
            api_key_id: None,
            iat: now,
            exp: now + self.access_expiry.num_seconds(),
            iss: self.jwt_issuer.clone(),
        };
        
        let access_token = encode(
            &Header::new(JWT_ALGORITHM),
            &access_claims,
            &EncodingKey::from_rsa_pem(&self.jwt_secret)
                .map_err(|e| format!("Failed to create encoding key: {}", e))?,
        )
        .map_err(|e| format!("Failed to encode access token: {}", e))?;
        
        // Refresh token
        let refresh_claims = Claims {
            sub: user.id.clone(),
            email: user.email.clone(),
            tier: user.tier.clone(),
            scopes: user.scopes.clone(),
            api_key_id: None,
            iat: now,
            exp: now + self.refresh_expiry.num_seconds(),
            iss: self.jwt_issuer.clone(),
        };
        
        let refresh_token = encode(
            &Header::new(JWT_ALGORITHM),
            &refresh_claims,
            &EncodingKey::from_rsa_pem(&self.jwt_secret)
                .map_err(|e| format!("Failed to create encoding key: {}", e))?,
        )
        .map_err(|e| format!("Failed to encode refresh token: {}", e))?;
        
        // Store refresh token
        {
            let mut tokens = self.refresh_tokens.write()
                .map_err(|_| "Lock error")?;
            tokens.insert(user.id.clone(), refresh_token.clone());
        }
        
        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: self.access_expiry.num_seconds(),
            token_type: "Bearer".to_string(),
        })
    }

    /// Validate JWT token
    pub fn validate_token(&self, token: &str) -> Result<Claims, String> {
        // Check blacklist
        let token_id = extract_token_id(token);
        if let Some(exp) = self.blacklist.read()
            .map_err(|_| "Lock error")?
            .get(&token_id) 
        {
            if Utc::now().timestamp() < *exp {
                return Err("Token is blacklisted".to_string());
            }
        }
        
        let validation = Validation::new(JWT_ALGORITHM);
        validation.set_issuer(&[&self.jwt_issuer]);
        
        let token_data: TokenData<Claims> = decode(
            token,
            &DecodingKey::from_rsa_pem(&self.jwt_secret)
                .map_err(|e| format!("Failed to create decoding key: {}", e))?,
            &validation,
        )
        .map_err(|e| format!("Invalid token: {}", e))?;
        
        Ok(token_data.claims)
    }

    /// Refresh tokens
    pub fn refresh_tokens(&self, refresh_token: &str) -> Result<TokenPair, String> {
        let claims = self.validate_token(refresh_token)?;
        
        // Remove old refresh token
        {
            let mut tokens = self.refresh_tokens.write()
                .map_err(|_| "Lock error")?;
            tokens.remove(&claims.sub);
        }
        
        // Get user
        let user = self.users.read()
            .map_err(|_| "Lock error")?
            .get(&claims.sub)
            .cloned()
            .ok_or("User not found")?;
        
        // Generate new tokens
        self.generate_tokens(&user)
    }

    /// Logout - invalidate tokens
    pub fn logout(&self, user_id: &str, token_id: &str) -> Result<(), String> {
        // Add to blacklist
        let exp = Utc::now().timestamp() + self.access_expiry.num_seconds();
        self.blacklist.write()
            .map_err(|_| "Lock error")?
            .insert(token_id.to_string(), exp);
        
        // Remove refresh token
        self.refresh_tokens.write()
            .map_err(|_| "Lock error")?
            .remove(user_id);
        
        Ok(())
    }

    // =============================================================================
    // USER MANAGEMENT
    // =============================================================================

    /// Create a new user
    pub fn create_user(
        &self,
        email: &str,
        password: &str,
        tier: &str,
        scopes: Vec<String>,
    ) -> Result<User, String> {
        let password_hash = self.password_hasher
            .hash(password)
            .map_err(|e| format!("Failed to hash password: {}", e))?;
        
        let now = Utc::now().timestamp();
        
        let user = User {
            id: Uuid::new_v4().to_string(),
            email: email.to_string(),
            password_hash,
            tier: tier.to_string(),
            scopes,
            created_at: now,
            updated_at: now,
            last_login: None,
        };
        
        // Store user
        self.users.write()
            .map_err(|_| "Lock error")?
            .insert(user.id.clone(), user.clone());
        
        Ok(user)
    }

    /// Get user by ID
    pub fn get_user(&self, user_id: &str) -> Option<User> {
        self.users.read()
            .ok()?
            .get(user_id)
            .cloned()
    }

    /// Get user by email
    pub fn get_user_by_email(&self, email: &str) -> Option<User> {
        self.users.read()
            .ok()?
            .values()
            .find(|u| u.email == email)
            .cloned()
    }

    /// Validate password
    pub fn validate_password(&self, user_id: &str, password: &str) -> bool {
        let user = match self.get_user(user_id) {
            Some(u) => u,
            None => return false,
        };
        
        self.password_hasher
            .verify(password, &user.password_hash)
            .unwrap_or(false)
    }

    /// Login with email/password
    pub fn login(&self, email: &str, password: &str) -> Result<TokenPair, String> {
        let user = self.get_user_by_email(email)
            .ok_or("Invalid credentials")?;
        
        if !self.validate_password(&user.id, password) {
            return Err("Invalid credentials".to_string());
        }
        
        // Update last login
        let now = Utc::now().timestamp();
        if let Some(u) = self.users.write()
            .ok()?
            .get_mut(&user.id) 
        {
            u.last_login = Some(now);
        }
        
        self.generate_tokens(&user)
    }

    // =============================================================================
    // API KEY MANAGEMENT
    // =============================================================================

    /// Create API key
    pub fn create_api_key(
        &self,
        user_id: &str,
        name: &str,
        tier: &str,
        scopes: Vec<String>,
    ) -> Result<String, String> {
        let key = generate_token(32);
        let key_hash = crate::encryption::sha256_hex(key.as_bytes());
        
        let now = Utc::now().timestamp();
        
        let api_key = ApiKey {
            id: Uuid::new_v4().to_string(),
            key_hash,
            name: name.to_string(),
            user_id: user_id.to_string(),
            tier: tier.to_string(),
            scopes,
            created_at: now,
            expires_at: now + (365 * 24 * 60 * 60), // 1 year
        };
        
        // Store API key
        self.api_keys.write()
            .map_err(|_| "Lock error")?
            .insert(api_key.key_hash.clone(), api_key);
        
        Ok(key)
    }

    /// Validate API key
    pub fn validate_api_key(&self, key: &str) -> Result<User, String> {
        let key_hash = crate::encryption::sha256_hex(key.as_bytes());
        
        let api_key = self.api_keys.read()
            .map_err(|_| "Lock error")?
            .get(&key_hash)
            .cloned()
            .ok_or("Invalid API key")?;
        
        // Check expiration
        if Utc::now().timestamp() > api_key.expires_at {
            return Err("API key expired".to_string());
        }
        
        // Get user
        self.get_user(&api_key.user_id)
            .ok_or("User not found".to_string())
    }

    /// Revoke API key
    pub fn revoke_api_key(&self, key: &str) -> Result<(), String> {
        let key_hash = crate::encryption::sha256_hex(key.as_bytes());
        
        self.api_keys.write()
            .map_err(|_| "Lock error")?
            .remove(&key_hash);
        
        Ok(())
    }

    // =============================================================================
    // BASIC AUTH
    // =============================================================================

    /// Validate basic authentication
    pub fn validate_basic_auth(&self, email: &str, password: &str) -> Result<TokenPair, String> {
        self.login(email, password)
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Extract token ID from JWT (for blacklisting)
fn extract_token_id(token: &str) -> String {
    // Simple extraction - in production use full JWT parsing
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() >= 2 {
        parts[1].to_string()
    } else {
        token.to_string()
    }
}

// =============================================================================
// MIDDLEWARE TRAIT
// =============================================================================

/// Authentication middleware result
#[derive(Debug, Clone)]
pub enum AuthResult {
    Jwt(Claims),
    ApiKey(User),
    Basic(User),
    None,
}

/// Extract authentication from request headers
pub fn extract_auth(
    auth_header: Option<&str>,
    api_key_header: Option<&str>,
) -> AuthResult {
    // Check API key first
    if let Some(key) = api_key_header {
        // Will be handled by authenticator
        return AuthResult::None;
    }
    
    // Check Bearer token
    if let Some(auth) = auth_header {
        if auth.starts_with("Bearer ") {
            let token = &auth[7..];
            // Will be validated by authenticator
            return AuthResult::None;
        }
        
        // Check Basic auth
        if auth.starts_with("Basic ") {
            // Will be handled by authenticator
            return AuthResult::None;
        }
    }
    
    AuthResult::None
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let auth = Authenticator::new();
        
        let user = auth.create_user(
            "test@tigerscan.io",
            "password123",
            "free",
            vec!["read".to_string()],
        ).unwrap();
        
        assert_eq!(user.email, "test@tigerscan.io");
        assert_eq!(user.tier, "free");
    }

    #[test]
    fn test_login() {
        let auth = Authenticator::new();
        
        auth.create_user(
            "test@tigerscan.io",
            "password123",
            "free",
            vec!["read".to_string()],
        ).unwrap();
        
        let tokens = auth.login("test@tigerscan.io", "password123").unwrap();
        
        assert!(!tokens.access_token.is_empty());
        assert_eq!(tokens.token_type, "Bearer");
    }

    #[test]
    fn test_token_validation() {
        let auth = Authenticator::new();
        
        let user = auth.create_user(
            "test@tigerscan.io",
            "password123",
            "free",
            vec!["read".to_string()],
        ).unwrap();
        
        let tokens = auth.generate_tokens(&user).unwrap();
        let claims = auth.validate_token(&tokens.access_token).unwrap();
        
        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.email, user.email);
    }

    #[test]
    fn test_api_key() {
        let auth = Authenticator::new();
        
        let user = auth.create_user(
            "test@tigerscan.io",
            "password123",
            "free",
            vec!["read".to_string()],
        ).unwrap();
        
        let key = auth.create_api_key(
            &user.id,
            "My API Key",
            "pro",
            vec!["read".to_string(), "write".to_string()],
        ).unwrap();
        
        let validated_user = auth.validate_api_key(&key).unwrap();
        assert_eq!(validated_user.id, user.id);
    }
}