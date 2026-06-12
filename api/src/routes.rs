//! API Routes for TigerScan

use crate::handlers::*;
use axum::{
    routing::{get, post},
    Router,
};

// =============================================================================
// ROUTES
// =============================================================================

/// Build routes
pub fn build_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Block routes
        .route("/blocks/:block_number", get(get_block))
        .route("/blocks/latest", get(get_latest_block))
        
        // Transaction routes
        .route("/txs/:tx_hash", get(get_transaction))
        
        // Address routes
        .route("/addresses/:address", get(get_address))
        
        // Token routes
        .route("/tokens/:address", get(get_token))
        .route("/tokens", get(get_tokens))
        
        // Search
        .route("/search", get(search))
        
        // Stats
        .route("/stats", get(get_stats))
        
        // Health
        .route("/health", get(health_check))
}

/// Health check
pub async fn health_check() -> &'static str {
    "OK"
}