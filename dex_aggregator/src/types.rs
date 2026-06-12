//! DEX Aggregator Types

use serde::{Deserialize, Serialize};

// =============================================================================
// DEX AGGREGATOR
// =============================================================================

/// Route
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub path: Vec<String>,
    pub dex: String,
    pub amount_out: u64,
    pub gas_estimate: u64,
}

/// Swap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Swap {
    pub from_token: String,
    pub to_token: String,
    pub amount_in: u64,
    pub routes: Vec<Route>,
    pub best_price: u64,
}

/// DEX Aggregator
pub struct Aggregator {
    routes: std::collections::HashMap<String, Vec<Route>>,
}

impl Aggregator {
    pub fn new() -> Self {
        Self {
            routes: std::collections::HashMap::new(),
        }
    }

    /// Add route
    pub fn add_route(&mut self, key: String, route: Route) {
        self.routes.entry(key).or_insert_with(Vec::new).push(route);
    }

    /// Find best route
    pub fn find_best_route(&self, key: &str) -> Option<&Route> {
        self.routes.get(key).and_then(|routes| routes.first())
    }
}

impl Default for Aggregator {
    fn default() -> Self {
        Self::new()
    }
}