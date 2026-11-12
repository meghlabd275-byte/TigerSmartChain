//! API Routes for TigerScan

use crate::handlers::*;
use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;

// =============================================================================
// ROUTES
// =============================================================================

/// Build the router. Every route is backed by a handler that reads the node.
pub fn build_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Blocks
        .route("/blocks/latest", get(get_latest_block))
        .route("/blocks/hash/:block_hash", get(get_block_by_hash))
        .route("/blocks/:block_number", get(get_block))

        // Transactions
        .route("/txs/:tx_hash", get(get_transaction))

        // Addresses
        .route("/addresses/:address", get(get_address))

        // Tokens
        .route("/tokens", get(get_tokens))
        .route("/tokens/:address", get(get_token))

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

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_router_builds() {
        let _router: Router<Arc<AppState>> = build_routes();
    }

    #[tokio::test]
    async fn the_health_check_answers_ok() {
        assert_eq!(health_check().await, "OK");
    }
}
