//! TigerScan NFT Rarity Calculator and Analytics Engine
//! NFT rarity scores, floor price tracking, analytics

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use statistical::{mean, median, std_dev};
use thiserror::Error;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum NFTError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Collection error: {0}")]
    Collection(String),
    
    #[error("Metadata error: {0}")]
    Metadata(String),
    
    #[error("Analysis error: {0}")]
    Analysis(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub database_url: String,
    pub max_traits: usize,
    pub floor_price_window: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            max_traits: 100,
            floor_price_window: 100,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTCollection {
    pub address: String,
    pub name: String,
    pub symbol: Option<String>,
    pub contract_type: String,
    pub total_supply: u64,
    pub floor_price: String,
    pub volume_24h: String,
    pub volume_total: String,
    pub owners_count: u64,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFT {
    pub address: String,
    pub token_id: String,
    pub owner: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub attributes: Vec<NFTAttribute>,
    pub rarity_score: f64,
    pub rarity_rank: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTAttribute {
    pub trait_type: String,
    pub value: String,
    pub display_type: Option<String>,
    pub rarity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RarityResult {
    pub token_id: String,
    pub rarity_score: f64,
    pub rarity_rank: u32,
    pub rarity_tier: RarityTier,
    pub trait_scores: HashMap<String, f64>,
    pub missing_traits: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RarityTier {
    Legendary,
    Epic,
    Rare,
    Uncommon,
    Common,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionAnalytics {
    pub address: String,
    pub total_supply: u64,
    pub holders_count: u64,
    pub transfers_24h: u64,
    pub volume_24h: String,
    pub volume_7d: String,
    pub volume_total: String,
    pub floor_price: String,
    pub floor_price_change_24h: f64,
    pub avg_price: String,
    pub median_price: String,
    pub highest_price: String,
    pub lowest_price: String,
    pub trait_distribution: HashMap<String, HashMap<String, u32>>,
    pub holder_distribution: Vec<HolderStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolderStats {
    pub address: String,
    pub nft_count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPrice {
    pub price: String,
    pub timestamp: i64,
    pub collection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub prices: Vec<FloorPrice>,
    pub avg_24h: String,
    pub avg_7d: String,
    pub avg_30d: String,
    pub trend: Trend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Up,
    Down,
    Stable,
}

// ============================================================================
// Rarity Calculator
// ============================================================================

pub struct RarityCalculator {
    config: Config,
    trait_stats: Arc<RwLock<TraitStats>>,
}

#[derive(Debug, Default)]
pub struct TraitStats {
    pub traits: HashMap<String, HashMap<String, u32>>,
    pub totals: HashMap<String, u32>,
}

impl RarityCalculator {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            trait_stats: Arc::new(RwLock::new(TraitStats::default())),
        }
    }

    /// Calculate rarity for an NFT
    pub fn calculate_rarity(&self, nft: &NFT) -> RarityResult {
        let mut trait_scores = HashMap::new();
        let mut total_score = 0.0;
        
        for attr in &nft.attributes {
            let rarity = self.calculate_trait_rarity(&attr.trait_type, &attr.value);
            trait_scores.insert(attr.trait_type.clone(), rarity);
            total_score += rarity;
        }
        
        // Normalize score
        let rarity_score = total_score / nft.attributes.len().max(1) as f64;
        
        // Determine tier
        let rarity_tier = match rarity_score {
            s if s >= 100.0 => RarityTier::Legendary,
            s if s >= 50.0 => RarityTier::Epic,
            s if s >= 20.0 => RarityTier::Rare,
            s if s >= 10.0 => RarityTier::Uncommon,
            _ => RarityTier::Common,
        };
        
        RarityResult {
            token_id: nft.token_id.clone(),
            rarity_score,
            rarity_rank: 0, // Would be calculated from collection
            rarity_tier,
            trait_scores,
            missing_traits: vec![],
        }
    }

    /// Calculate rarity for a single trait
    fn calculate_trait_rarity(&self, trait_type: &str, value: &str) -> f64 {
        let state = self.trait_stats.read();
        
        let total = state.totals.get(trait_type).copied().unwrap_or(1) as f64;
        let count = state.traits.get(trait_type)
            .and_then(|v| v.get(value))
            .copied()
            .unwrap_or(0) as f64;
        
        if total == 0.0 || count == 0.0 {
            return 1.0;
        }
        
        // Rarity = 1 / (count / total)
        total / count
    }

    /// Update trait statistics
    pub fn update_stats(&self, nfts: &[NFT]) {
        let mut state = self.trait_stats.write();
        
        for nft in nfts {
            for attr in &nft.attributes {
                *state.totals.entry(attr.trait_type.clone()).or_insert(0) += 1;
                *state.traits
                    .entry(attr.trait_type.clone())
                    .or_insert_with(HashMap::new)
                    .entry(attr.value.clone())
                    .or_insert(0) += 1;
            }
        }
    }
}

// ============================================================================
// NFT Analytics Service
// ============================================================================

pub struct NFTAnalyticsService {
    config: Config,
    rpc: Provider<Http>,
    rarity_calculator: RarityCalculator,
    state: Arc<RwLock<NFTAnalyticsState>>,
}

#[derive(Debug)]
pub struct NFTAnalyticsState {
    pub collections: HashMap<String, NFTCollection>,
    pub nfts: HashMap<String, HashMap<String, NFT>>,
    pub floor_history: HashMap<String, Vec<FloorPrice>>,
}

impl NFTAnalyticsService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing NFT Analytics Service");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let service = Self {
            config: config.clone(),
            rpc,
            rarity_calculator: RarityCalculator::new(config.clone()),
            state: Arc::new(RwLock::new(NFTAnalyticsState {
                collections: HashMap::new(),
                nfts: HashMap::new(),
                floor_history: HashMap::new(),
            })),
        };
        
        info!("NFT Analytics Service initialized");
        Ok(service)
    }

    /// Get collection analytics
    pub fn get_collection_analytics(&self, address: &str) -> Option<CollectionAnalytics> {
        let state = self.state.read();
        let collection = state.collections.get(address)?;
        
        let floor_history = state.floor_history.get(address)?;
        
        // Calculate statistics
        let prices: Vec<f64> = floor_history.iter()
            .filter_map(|p| p.price.parse().ok())
            .collect();
        
        let avg = if !prices.is_empty() {
            mean(&prices)
        } else {
            0.0
        };
        
        let median_p = if !prices.is_empty() {
            median(&prices)
        } else {
            0.0
        };
        
        // Calculate floor change
        let floor_change = if prices.len() >= 2 {
            let current = prices.last().copied().unwrap_or(0.0);
            let previous = prices.get(prices.len() - 2).copied().unwrap_or(0.0);
            if previous > 0.0 {
                ((current - previous) / previous) * 100.0
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        // Determine trend
        let trend = if floor_change > 5.0 {
            Trend::Up
        } else if floor_change < -5.0 {
            Trend::Down
        } else {
            Trend::Stable
        };
        
        // Trait distribution
        let mut trait_dist: HashMap<String, HashMap<String, u32>> = HashMap::new();
        if let Some(nfts) = state.nfts.get(address) {
            for nft in nfts.values() {
                for attr in &nft.attributes {
                    *trait_dist
                        .entry(attr.trait_type.clone())
                        .or_insert_with(HashMap::new)
                        .entry(attr.value.clone())
                        .or_insert(0) += 1;
                }
            }
        }
        
        // Holder distribution
        let mut holder_counts: HashMap<String, u32> = HashMap::new();
        if let Some(nfts) = state.nfts.get(address) {
            for nft in nfts.values() {
                *holder_counts.entry(nft.owner.clone()).or_insert(0) += 1;
            }
        }
        
        let total_nfts = holder_counts.values().sum::<u64>();
        let mut holder_dist: Vec<_> = holder_counts.into_iter()
            .map(|(address, count)| HolderStats {
                address,
                nft_count: count,
                percentage: if total_nfts > 0 {
                    (count as f64 / total_nfts as f64) * 100.0
                } else {
                    0.0
                },
            })
            .collect();
        
        holder_dist.sort_by(|a, b| b.nft_count.cmp(&a.nft_count));
        holder_dist.truncate(100);
        
        Some(CollectionAnalytics {
            address: address.to_string(),
            total_supply: collection.total_supply,
            holders_count: collection.owners_count,
            transfers_24h: 0,
            volume_24h: collection.volume_24h.clone(),
            volume_7d: collection.volume_total.clone(),
            volume_total: collection.volume_total.clone(),
            floor_price: collection.floor_price.clone(),
            floor_price_change_24h: floor_change,
            avg_price: format!("{}", avg),
            median_price: format!("{}", median_p),
            highest_price: "0".to_string(),
            lowest_price: "0".to_string(),
            trait_distribution: trait_dist,
            holder_distribution: holder_dist,
        })
    }

    /// Calculate rarity for NFT
    pub fn calculate_rarity(&self, address: &str, token_id: &str) -> Option<RarityResult> {
        let state = self.state.read();
        
        let nfts = state.nfts.get(address)?;
        let target_nft = nfts.get(token_id)?;
        
        let mut result = self.rarity_calculator.calculate_rarity(target_nft);
        
        // Calculate rank: count how many NFTs in this collection have a higher rarity score
        let rank = nfts.values()
            .filter(|n| n.rarity_score > result.rarity_score)
            .count() as u32 + 1;
        result.rarity_rank = rank;
        
        Some(result)
    }

    /// Get top NFTs by rarity
    pub fn get_top_rarity(&self, address: &str, limit: usize) -> Vec<NFT> {
        let state = self.state.read();
        
        let nfts = state.nfts.get(address);
        
        if let Some(nfts) = nfts {
            let mut nfts: Vec<_> = nfts.values().cloned().collect();
            nfts.sort_by(|a, b| b.rarity_score.partial_cmp(&a.rarity_score).unwrap());
            nfts.truncate(limit);
            nfts
        } else {
            vec![]
        }
    }

    /// Get floor price history
    pub fn get_floor_price_history(&self, address: &str) -> Option<PriceHistory> {
        let state = self.state.read();
        
        let history = state.floor_history.get(address)?;
        
        let prices: Vec<f64> = history.iter()
            .filter_map(|p| p.price.parse().ok())
            .collect();
        
        let avg_24h = if prices.len() >= 24 {
            mean(&prices[prices.len()-24..])
        } else {
            mean(&prices)
        };
        
        let avg_7d = if prices.len() >= 7 {
            mean(&prices[prices.len()-7..])
        } else {
            mean(&prices)
        };
        
        let avg_30d = if prices.len() >= 30 {
            mean(&prices[prices.len()-30..])
        } else {
            mean(&prices)
        };
        
        let trend = if prices.len() >= 2 {
            let recent = prices.iter().rev().take(5);
            let first = recent.clone().last().copied().unwrap_or(0.0);
            let last = recent.next().copied().unwrap_or(0.0);
            if last > first * 1.05 {
                Trend::Up
            } else if last < first * 0.95 {
                Trend::Down
            } else {
                Trend::Stable
            }
        } else {
            Trend::Stable
        };
        
        Some(PriceHistory {
            prices: history.clone(),
            avg_24h: format!("{}", avg_24h),
            avg_7d: format!("{}", avg_7d),
            avg_30d: format!("{}", avg_30d),
            trend,
        })
    }

    /// Update floor price
    pub fn update_floor_price(&self, address: &str, price: &str) {
        let mut state = self.state.write();
        
        let history = state.floor_history
            .entry(address.to_string())
            .or_insert_with(Vec::new);
        
        history.push(FloorPrice {
            price: price.to_string(),
            timestamp: Utc::now().timestamp(),
            collection: address.to_string(),
        });
        
        // Keep only recent prices
        if history.len() > self.config.floor_price_window {
            history.drain(0..history.len() - self.config.floor_price_window);
        }
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTApiRequest {
    pub collection: Option<String>,
    pub token_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculate rarity score using statistical methods
pub fn calculate_rarity_score(trait_count: u32, total_supply: u64) -> f64 {
    if trait_count == 0 || total_supply == 0 {
        return 1.0;
    }
    
    let frequency = trait_count as f64 / total_supply as f64;
    
    // Higher score for rarer traits
    1.0 / frequency
}

/// Format rarity tier as string
pub fn format_rarity_tier(tier: RarityTier) -> &'static str {
    match tier {
        RarityTier::Legendary => "Legendary",
        RarityTier::Epic => "Epic",
        RarityTier::Rare => "Rare",
        RarityTier::Uncommon => "Uncommon",
        RarityTier::Common => "Common",
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rarity_score() {
        let score = calculate_rarity_score(1, 10000);
        assert!(score > 9000.0);
        
        let score = calculate_rarity_score(5000, 10000);
        assert!((score - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_format_rarity_tier() {
        assert_eq!(format_rarity_tier(RarityTier::Legendary), "Legendary");
        assert_eq!(format_rarity_tier(RarityTier::Common), "Common");
    }
}