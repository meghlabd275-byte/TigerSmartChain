//! Analytics Engine - Main Entry Point

use crate::{
    AnalyticsConfig, TimeRange, NetworkStats, PriceData,
    tvl::TVLMetrics, whale::WhaleAlert, ranking::TokenRank,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Analytics engine
pub struct AnalyticsEngine {
    config: AnalyticsConfig,
    network_stats: Arc<RwLock<HashMap<u64, NetworkStats>>>,
    prices: Arc<RwLock<HashMap<String, PriceData>>>,
    whale_alerts: Arc<RwLock<Vec<WhaleAlert>>>,
    token_ranks: Arc<RwLock<Vec<TokenRank>>>,
    tvl_history: Arc<RwLock<Vec<TVLMetrics>>>,
}

impl AnalyticsEngine {
    /// Create new analytics engine
    pub fn new(config: AnalyticsConfig) -> Self {
        Self {
            config,
            network_stats: Arc::new(RwLock::new(HashMap::new())),
            prices: Arc::new(RwLock::new(HashMap::new())),
            whale_alerts: Arc::new(RwLock::new(Vec::new())),
            token_ranks: Arc::new(RwLock::new(Vec::new())),
            tvl_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Update network stats
    pub async fn update_network_stats(&self, stats: NetworkStats) {
        let mut stats_map = self.network_stats.write().await;
        stats_map.insert(stats.tps as u64, stats);
    }

    /// Get network stats
    pub async fn get_network_stats(&self, chain_id: u64) -> Option<NetworkStats> {
        let stats_map = self.network_stats.read().await;
        stats_map.get(&chain_id).cloned()
    }

    /// Update price data
    pub async fn update_price(&self, price: PriceData) {
        let mut prices = self.prices.write().await;
        prices.insert(price.token.clone(), price);
    }

    /// Get price for token
    pub async fn get_price(&self, token: &str) -> Option<PriceData> {
        let prices = self.prices.read().await;
        prices.get(token).cloned()
    }

    /// Get current TPS
    pub async fn get_tps(&self) -> f64 {
        let stats = self.network_stats.read().await;
        stats.values().map(|s| s.tps).fold(0.0, |a, b| a.max(b))
    }

    /// Get TVL history
    pub async fn get_tvl_history(&self, range: TimeRange) -> Vec<TVLMetrics> {
        let history = self.tvl_history.read().await;
        let cutoff = chrono::Utc::now().timestamp() - range.seconds();
        
        history
            .iter()
            .filter(|m| m.timestamp >= cutoff)
            .cloned()
            .collect()
    }

    /// Get whale alerts
    pub async fn get_whale_alerts(&self, limit: usize) -> Vec<WhaleAlert> {
        let alerts = self.whale_alerts.read().await;
        alerts.iter().take(limit).cloned().collect()
    }

    /// Get top tokens by volume
    pub async fn get_top_tokens(&self, limit: usize) -> Vec<TokenRank> {
        let ranks = self.token_ranks.read().await;
        ranks.iter().take(limit).cloned().collect()
    }

    /// Calculate market cap total
    pub async fn get_market_cap_total(&self) -> f64 {
        let prices = self.prices.read().await;
        prices.values().map(|p| p.market_cap).sum()
    }

    /// Detect whale activity
    pub async fn detect_whale(&self, address: &str, amount_usd: f64) -> Option<WhaleAlert> {
        if amount_usd >= self.config.whale_threshold_usd {
            Some(WhaleAlert {
                address: address.to_string(),
                amount_usd,
                timestamp: chrono::Utc::now().timestamp(),
                alert_type: "large_transfer".to_string(),
            })
        } else {
            None
        }
    }
}

use chrono::Utc;