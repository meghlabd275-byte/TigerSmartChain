//! TigerScan API Gateway
//! 
//! High-performance API Gateway with tiered access control (Free/Pro/Enterprise)
//! Built in Rust for security and ultra-low latency

use actix_web::{web, App, HttpServer, HttpResponse, Responder, middleware};
use actix_cors::Cors;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

// ============================================================================
// CONFIGURATION
// ============================================================================

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub token_expiry_hours: i64,
    pub db_path: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            jwt_secret: "tigerscan-secret-key-change-in-production".to_string(),
            token_expiry_hours: 24,
            db_path: "./data/api_gateway.db".to_string(),
        }
    }
}

// ============================================================================
// TIERED ACCESS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiTier {
    Free,
    Pro,
    Enterprise,
}

impl ApiTier {
    pub fn rate_limit(&self) -> usize {
        match self {
            ApiTier::Free => 5,          // 5 requests/minute
            ApiTier::Pro => 100,        // 100 requests/minute
            ApiTier::Enterprise => 10000, // Unlimited
        }
    }

    pub fn daily_limit(&self) -> Option<u64> {
        match self {
            ApiTier::Free => Some(1000),     // 1000/day
            ApiTier::Pro => Some(100000),    // 100k/day
            ApiTier::Enterprise => None,       // Unlimited
        }
    }

    pub fn features(&self) -> Vec<&str> {
        match self {
            ApiTier::Free => vec!["basic_read"],
            ApiTier::Pro => vec!["basic_read", "advanced_queries", "webhooks"],
            ApiTier::Enterprise => vec!["basic_read", "advanced_queries", "webhooks", "dedicated_support", "custom_rates"],
        }
    }
}

// ============================================================================
// MODELS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub key: String,
    pub name: String,
    pub tier: ApiTier,
    pub user_id: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
    pub is_active: bool,
    pub daily_requests: u64,
    pub total_requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub tier: ApiTier,
    pub created_at: i64,
    pub is_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageLog {
    pub id: String,
    pub api_key_id: String,
    pub endpoint: String,
    pub method: String,
    pub status_code: u16,
    pub latency_ms: u64,
    pub timestamp: i64,
}

// ============================================================================
// API KEY MANAGER
// ============================================================================

#[derive(Clone)]
pub struct ApiKeyManager {
    keys: Arc<RwLock<HashMap<String, ApiKey>>>,
    usage: Arc<RwLock<HashMap<String, Vec<UsageLog>>>>,
    daily_usage: Arc<RwLock<HashMap<String, u64>>>,
}

impl ApiKeyManager {
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            usage: Arc::new(RwLock::new(HashMap::new())),
            daily_usage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new API key
    pub async fn create_key(&self, name: &str, tier: ApiTier, user_id: &str) -> ApiKey {
        let key = ApiKey {
            id: Uuid::new_v4().to_string(),
            key: generate_secure_key(),
            name: name.to_string(),
            tier,
            user_id: user_id.to_string(),
            created_at: chrono::Utc::now().timestamp(),
            last_used: None,
            is_active: true,
            daily_requests: 0,
            total_requests: 0,
        };

        let mut keys = self.keys.write().await;
        keys.insert(key.key.clone(), key.clone());
        
        key
    }

    /// Validate API key
    pub async fn validate_key(&self, key: &str) -> Option<ApiKey> {
        let keys = self.keys.read().await;
        keys.get(key).cloned()
    }

    /// Check rate limit
    pub async fn check_rate_limit(&self, key: &str) -> Result<(), RateLimitError> {
        let keys = self.keys.read().await;
        
        if let Some(api_key) = keys.get(key) {
            if !api_key.is_active {
                return Err(RateLimitError::KeyInactive);
            }

            // Check tier limits
            let tier = api_key.tier;
            let mut daily_usage = self.daily_usage.write().await;
            let usage = daily_usage.entry(key.to_string()).or_insert(0);
            
            if let Some(limit) = tier.daily_limit() {
                if *usage >= limit {
                    return Err(RateLimitError::DailyLimitExceeded);
                }
            }
            
            *usage += 1;
            
            // Update key stats
            drop(daily_usage);
            drop(keys);
            
            let mut keys = self.keys.write().await;
            if let Some(api_key) = keys.get_mut(key) {
                api_key.daily_requests += 1;
                api_key.total_requests += 1;
                api_key.last_used = Some(chrono::Utc::now().timestamp());
            }
            
            Ok(())
        } else {
            Err(RateLimitError::InvalidKey)
        }
    }

    /// Get key info
    pub async fn get_key_info(&self, key: &str) -> Option<ApiKey> {
        let keys = self.keys.read().await;
        keys.get(key).cloned()
    }

    /// Revoke key
    pub async fn revoke_key(&self, key: &str) -> bool {
        let mut keys = self.keys.write().await;
        if let Some(api_key) = keys.get_mut(key) {
            api_key.is_active = false;
            return true;
        }
        false
    }

    /// Get usage stats
    pub async fn get_usage_stats(&self, key: &str) -> Option<UsageStats> {
        let keys = self.keys.read().await;
        let api_key = keys.get(key)?;
        
        let daily_usage = self.daily_usage.read().await;
        let daily = daily_usage.get(key).unwrap_or(&0);
        
        Some(UsageStats {
            tier: api_key.tier,
            daily_requests: *daily,
            total_requests: api_key.total_requests,
            remaining: api_key.tier.daily_limit().map(|l| l - daily),
        })
    }

    /// Reset daily usage (called at midnight)
    pub async fn reset_daily_usage(&self) {
        let mut daily_usage = self.daily_usage.write().await;
        for usage in daily_usage.values_mut() {
            *usage = 0;
        }
        let mut keys = self.keys.write().await;
        for key in keys.values_mut() {
            key.daily_requests = 0;
        }
    }
}

impl Default for ApiKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub tier: ApiTier,
    pub daily_requests: u64,
    pub total_requests: u64,
    pub remaining: Option<u64>,
}

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Debug)]
pub enum RateLimitError {
    InvalidKey,
    KeyInactive,
    RateLimitExceeded,
    DailyLimitExceeded,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidKey => "invalid API key",
            Self::KeyInactive => "API key is inactive",
            Self::RateLimitExceeded => "rate limit exceeded",
            Self::DailyLimitExceeded => "daily limit exceeded",
        };
        f.write_str(message)
    }
}

impl std::error::Error for RateLimitError {}

impl actix_web::error::ResponseError for RateLimitError {
    fn error_response(&self) -> HttpResponse {
        match self {
            Self::InvalidKey => HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Invalid API key"
            })),
            Self::KeyInactive => HttpResponse::Forbidden().json(serde_json::json!({
                "error": "API key is inactive"
            })),
            Self::RateLimitExceeded => HttpResponse::TooManyRequests().json(serde_json::json!({
                "error": "Rate limit exceeded",
                "retry_after": 60
            })),
            Self::DailyLimitExceeded => HttpResponse::TooManyRequests().json(serde_json::json!({
                "error": "Daily limit exceeded",
                "upgrade": "https://tigerscan.com/api/pricing"
            })),
        }
    }
}

// ============================================================================
// HELPERS
// ============================================================================

fn generate_secure_key() -> String {
    use base64::Engine;
    use rand::RngCore;
    // OsRng draws from the operating system CSPRNG, so a key cannot be
    // predicted from a previously issued one.
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!("tsk_{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// Hash a password with bcrypt. A keyed hash is the only correct way to store
/// a credential: a plain digest is reversible with a rainbow table.
fn hash_password(password: &str) -> String {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap_or_else(|_| String::new())
}

fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

// ============================================================================
// ROUTES
// ============================================================================

#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    pub tier: String,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self { success: true, data: Some(data), error: None }
    }

    pub fn error(msg: &str) -> Self {
        Self { success: false, data: None, error: Some(msg.to_string()) }
    }
}

/// Health check
async fn health() -> impl Responder {
    HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().timestamp()
    })))
}

/// Create API key
async fn create_key(
    req: web::Json<CreateKeyRequest>,
    manager: web::Data<ApiKeyManager>,
) -> impl Responder {
    let tier = match req.tier.as_str() {
        "pro" => ApiTier::Pro,
        "enterprise" => ApiTier::Enterprise,
        _ => ApiTier::Free,
    };

    let key = manager
        .create_key(&req.name, tier, "user_id")
        .await;

    HttpResponse::Ok().json(ApiResponse::success(key))
}

/// Validate API key
async fn validate_key(
    key: web::Query<String>,
    manager: web::Data<ApiKeyManager>,
) -> impl Responder {
    match manager.validate_key(&key).await {
        Some(api_key) => HttpResponse::Ok().json(ApiResponse::success(api_key)),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::error("Invalid API key")),
    }
}

/// Get usage stats
async fn get_usage(
    key: web::Query<String>,
    manager: web::Data<ApiKeyManager>,
) -> impl Responder {
    match manager.get_usage_stats(&key).await {
        Some(stats) => HttpResponse::Ok().json(ApiResponse::success(stats)),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::error("Key not found")),
    }
}

/// Revoke API key
async fn revoke_key(
    key: web::Query<String>,
    manager: web::Data<ApiKeyManager>,
) -> impl Responder {
    if manager.revoke_key(&key).await {
        HttpResponse::Ok().json(ApiResponse::success("Key revoked"))
    } else {
        HttpResponse::NotFound().json(ApiResponse::<()>::error("Key not found"))
    }
}

// ============================================================================
// MIDDLEWARE
// ============================================================================

pub struct RateLimitMiddleware {
    manager: ApiKeyManager,
}

impl RateLimitMiddleware {
    pub fn new(manager: ApiKeyManager) -> Self {
        Self { manager }
    }
}

impl<S, B> actix_web::dev::Transform<S, actix_web::dev::ServiceRequest>
    for RateLimitMiddleware
where
    S: actix_web::dev::Service<
        actix_web::dev::ServiceRequest,
        Response = actix_web::dev::ServiceResponse<B>,
        Error = actix_web::Error,
    > + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = actix_web::dev::ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = RateLimitService<S>;
    type Future = std::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(RateLimitService {
            service: std::rc::Rc::new(service),
            manager: self.manager.clone(),
        }))
    }
}

/// The service produced by [`RateLimitMiddleware`]. It rejects a request whose
/// `X-API-Key` is unknown, inactive or over its daily quota before the handler
/// runs, then delegates to the wrapped service.
pub struct RateLimitService<S> {
    service: std::rc::Rc<S>,
    manager: ApiKeyManager,
}

impl<S, B> actix_web::dev::Service<actix_web::dev::ServiceRequest> for RateLimitService<S>
where
    S: actix_web::dev::Service<
        actix_web::dev::ServiceRequest,
        Response = actix_web::dev::ServiceResponse<B>,
        Error = actix_web::Error,
    > + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = actix_web::dev::ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = futures::future::LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: actix_web::dev::ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let manager = self.manager.clone();

        Box::pin(async move {
            // A key is required on every data route; /health is exempt so an
            // orchestrator probe does not need credentials.
            if req.path() != "/health" {
                match req.headers().get("X-API-Key") {
                    Some(key) => match key.to_str() {
                        Ok(key_str) => manager.check_rate_limit(key_str).await?,
                        Err(_) => {
                            return Err(RateLimitError::InvalidKey.into());
                        }
                    },
                    None => return Err(RateLimitError::InvalidKey.into()),
                }
            }
            service.call(req).await
        })
    }
}

// ============================================================================
// MAIN
// ============================================================================

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = ApiConfig::default();
    let manager = ApiKeyManager::new();

    info!("Starting TigerScan API Gateway on {}:{}", config.host, config.port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(manager.clone()))
            .wrap(
                Cors::default()
                    .allowed_origin("*")
                    .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                    .allowed_headers(vec!["Content-Type", "Authorization", "X-API-Key"])
            )
            .wrap(middleware::Logger::default())
            .wrap(RateLimitMiddleware::new(manager.clone()))
            .route("/health", web::get().to(health))
            .route("/api/keys", web::post().to(create_key))
            .route("/api/keys/validate", web::get().to(validate_key))
            .route("/api/keys/usage", web::get().to(get_usage))
            .route("/api/keys/revoke", web::get().to(revoke_key))
    })
    .bind((config.host.as_str(), config.port))?
    .run()
    .await
}

use log::info;
