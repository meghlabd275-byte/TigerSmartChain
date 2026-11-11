//! TigerScan NFT Rarity Calculator and Analytics Engine
//! NFT rarity scores, floor price tracking, analytics

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use ethers::providers::{Http, Provider};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use statistical::{mean, median};
use thiserror::Error;
use tracing::info;

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
    /// Sustained requests per second allowed per client address.
    pub rate_limit_per_second: u32,
    /// Largest burst a client may spend at once.
    pub rate_limit_burst: u32,
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
            rate_limit_per_second: 50,
            rate_limit_burst: 100,
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

    /// Score an NFT and rank it against its collection.
    ///
    /// The rank is the number of tokens in `collection` with a strictly higher
    /// score, plus one, so ties share a rank. The collection is a parameter
    /// because a rank is not a property of a token on its own — the previous
    /// signature had no way to compute one, which is why it returned a
    /// hardcoded `0`.
    pub fn calculate_rarity(&self, nft: &NFT, collection: &[NFT]) -> RarityResult {
        let score = self.score(nft);

        let rank = collection
            .iter()
            .filter(|other| self.score(other) > score + f64::EPSILON)
            .count() as u32
            + 1;

        // The (trait_type, value) pairs the collection carries that this token
        // does not, sorted for a stable response.
        let mut collection_traits: HashSet<(String, String)> = HashSet::new();
        for other in collection {
            for attr in &other.attributes {
                collection_traits.insert((attr.trait_type.clone(), attr.value.clone()));
                if collection_traits.len() >= self.config.max_traits {
                    break;
                }
            }
        }

        let own_traits: HashSet<(String, String)> = nft
            .attributes
            .iter()
            .map(|attr| (attr.trait_type.clone(), attr.value.clone()))
            .collect();

        let mut missing_traits: Vec<String> = collection_traits
            .difference(&own_traits)
            .map(|(trait_type, value)| format!("{trait_type}: {value}"))
            .collect();
        missing_traits.sort();

        let mut trait_scores = HashMap::new();
        for attr in &nft.attributes {
            trait_scores.insert(
                attr.trait_type.clone(),
                self.calculate_trait_rarity(&attr.trait_type, &attr.value),
            );
        }

        RarityResult {
            token_id: nft.token_id.clone(),
            rarity_score: score,
            rarity_rank: rank,
            rarity_tier: Self::tier_for(score),
            trait_scores,
            missing_traits,
        }
    }

    /// The rarity score of one token: the mean of `1 / frequency` over its
    /// traits.
    ///
    /// Traits are de-duplicated first. A contract that repeats a trait would
    /// otherwise have that trait counted twice, which inflates the token's
    /// score.
    fn score(&self, nft: &NFT) -> f64 {
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        let mut total = 0.0;
        let mut counted = 0usize;

        for attr in &nft.attributes {
            if !seen.insert((attr.trait_type.as_str(), attr.value.as_str())) {
                continue;
            }
            total += self.calculate_trait_rarity(&attr.trait_type, &attr.value);
            counted += 1;
        }

        if counted == 0 {
            return 0.0;
        }

        total / counted as f64
    }

    /// The tier a score falls in.
    fn tier_for(score: f64) -> RarityTier {
        match score {
            s if s >= 100.0 => RarityTier::Legendary,
            s if s >= 50.0 => RarityTier::Epic,
            s if s >= 20.0 => RarityTier::Rare,
            s if s >= 10.0 => RarityTier::Uncommon,
            _ => RarityTier::Common,
        }
    }

    /// Calculate rarity for a single trait: `1 / (count / total)`.
    ///
    /// An unknown trait scores 1.0. It carries no rarity signal, and returning a
    /// large number for it would make an unindexed trait look like the rarest
    /// thing in the collection.
    fn calculate_trait_rarity(&self, trait_type: &str, value: &str) -> f64 {
        let state = self.trait_stats.read();

        let total = state.totals.get(trait_type).copied().unwrap_or(0) as f64;
        let count = state
            .traits
            .get(trait_type)
            .and_then(|values| values.get(value))
            .copied()
            .unwrap_or(0) as f64;

        if total <= 0.0 || count <= 0.0 {
            return 1.0;
        }

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
    /// RPC provider, used by the ingestion paths that read chain state.
    #[allow(dead_code)]
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
        
        let total_nfts: u64 = holder_counts.values().map(|count| *count as u64).sum();
        let mut holder_dist: Vec<_> = holder_counts.into_iter()
            .map(|(address, count)| HolderStats {
                address,
                nft_count: count as u64,
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
        
        let collection: Vec<NFT> = nfts.values().cloned().collect();
        Some(self.rarity_calculator.calculate_rarity(target_nft, &collection))
    }

    /// The highest-rarity tokens in a collection, rarest first.
    ///
    /// Returns `None` when the collection is not indexed. That is distinct from
    /// `Some(vec![])`, which means the collection is known but holds no tokens —
    /// collapsing the two would let a typo in an address read as a valid empty
    /// answer.
    pub fn get_top_rarity(&self, address: &str, limit: usize) -> Option<Vec<NFT>> {
        let state = self.state.read();

        let nfts = state.nfts.get(address)?;

        let mut nfts: Vec<NFT> = nfts.values().cloned().collect();
        // `partial_cmp` returns `None` for a NaN score, which would panic in
        // `sort_by`. `total_cmp` is defined for every pair of floats.
        nfts.sort_by(|a, b| b.rarity_score.total_cmp(&a.rarity_score));
        nfts.truncate(limit);
        Some(nfts)
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
            // The oldest of the last five readings against the newest. The old
            // code consumed the iterator with `next()` first and then took
            // `last()` of what remained, so it compared the newest reading
            // against itself and could never report a trend.
            let window: Vec<f64> = prices.iter().rev().take(5).copied().collect();
            let first = window.last().copied().unwrap_or(0.0);
            let last = window.first().copied().unwrap_or(0.0);
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

    fn attribute(trait_type: &str, value: &str) -> NFTAttribute {
        NFTAttribute {
            trait_type: trait_type.to_string(),
            value: value.to_string(),
            display_type: None,
            rarity: 0.0,
        }
    }

    fn token(token_id: &str, attributes: Vec<NFTAttribute>) -> NFT {
        NFT {
            address: "0xcollection".to_string(),
            token_id: token_id.to_string(),
            owner: "0xowner".to_string(),
            name: None,
            description: None,
            image_url: None,
            attributes,
            rarity_score: 0.0,
            rarity_rank: 0,
        }
    }

    /// Build a calculator whose trait statistics come from `tokens`.
    fn calculator(tokens: &[NFT]) -> RarityCalculator {
        let calculator = RarityCalculator::new(Config::default());
        calculator.update_stats(tokens);
        calculator
    }

    #[test]
    fn rarer_trait_scores_higher() {
        // "Common" appears three times, "Rare" once.
        let tokens = vec![
            token("1", vec![attribute("bg", "Common")]),
            token("2", vec![attribute("bg", "Common")]),
            token("3", vec![attribute("bg", "Common")]),
            token("4", vec![attribute("bg", "Rare")]),
        ];
        let calculator = calculator(&tokens);

        let common = calculator.calculate_rarity(&tokens[0], &tokens);
        let rare = calculator.calculate_rarity(&tokens[3], &tokens);

        assert!(rare.rarity_score > common.rarity_score);
        assert_eq!(rare.rarity_rank, 1, "the rarest token ranks first");
        assert_eq!(common.rarity_rank, 2, "ties share the next rank");
    }

    #[test]
    fn ties_share_a_rank_and_the_next_rank_skips() {
        // Two tokens carry the rare "Blue" (2 of 5); three carry "Grey" (3 of 5).
        let tokens = vec![
            token("1", vec![attribute("bg", "Blue")]),
            token("2", vec![attribute("bg", "Blue")]),
            token("3", vec![attribute("bg", "Grey")]),
            token("4", vec![attribute("bg", "Grey")]),
            token("5", vec![attribute("bg", "Grey")]),
        ];
        let calculator = calculator(&tokens);

        let blue = calculator.calculate_rarity(&tokens[0], &tokens);
        let grey = calculator.calculate_rarity(&tokens[2], &tokens);

        assert_eq!(blue.rarity_rank, 1);
        assert_eq!(grey.rarity_rank, 3, "two tokens share rank 1, so the next is 3");
    }

    #[test]
    fn a_repeated_trait_is_counted_once() {
        let tokens = vec![
            token(
                "1",
                vec![attribute("bg", "Blue"), attribute("bg", "Blue")],
            ),
            token("2", vec![attribute("bg", "Grey")]),
            token("3", vec![attribute("bg", "Grey")]),
        ];
        let calculator = calculator(&tokens);

        let duplicated = calculator.calculate_rarity(&tokens[0], &tokens);
        let single = calculator.calculate_rarity(&tokens[1], &tokens);

        // Both carry exactly one distinct "Blue"/"Grey" trait, so their scores
        // must be the same size; a repeated trait would inflate the first.
        assert!((duplicated.rarity_score - single.rarity_score).abs() < f64::EPSILON);
    }

    #[test]
    fn unknown_trait_does_not_inflate_the_score() {
        let tokens = vec![
            token("1", vec![attribute("bg", "Blue")]),
            token("2", vec![attribute("bg", "Blue")]),
        ];
        let calculator = calculator(&tokens);

        // "Never indexed" has a count of zero; a large score here would make an
        // unindexed trait look like the rarest thing in the collection.
        let unknown = calculator.calculate_rarity(&token("9", vec![attribute("bg", "Never")]), &tokens);
        assert_eq!(unknown.rarity_score, 1.0);
    }

    #[test]
    fn missing_traits_lists_what_the_token_lacks() {
        let tokens = vec![
            token("1", vec![attribute("bg", "Blue"), attribute("eyes", "Laser")]),
            token("2", vec![attribute("bg", "Grey")]),
        ];
        let calculator = calculator(&tokens);

        let result = calculator.calculate_rarity(&tokens[1], &tokens);
        // Missing traits are keyed on the pair, so this token lacks both the
        // "Blue" background and the "Laser" eyes the collection carries.
        assert_eq!(
            result.missing_traits,
            vec!["bg: Blue".to_string(), "eyes: Laser".to_string()]
        );
    }

    #[test]
    fn a_token_with_no_traits_scores_zero() {
        let tokens = vec![token("1", vec![attribute("bg", "Blue")])];
        let calculator = calculator(&tokens);

        let bare = calculator.calculate_rarity(&token("2", vec![]), &tokens);
        assert_eq!(bare.rarity_score, 0.0);
        assert_eq!(bare.rarity_tier, RarityTier::Common);
    }

    #[test]
    fn top_rarity_is_ordered_and_bounded() {
        let tokens = vec![
            token("1", vec![attribute("bg", "Common")]),
            token("2", vec![attribute("bg", "Common")]),
            token("3", vec![attribute("bg", "Common")]),
            token("4", vec![attribute("bg", "Rare")]),
        ];
        let calculator = calculator(&tokens);

        // Mirror what the service does: score every token, then sort.
        let mut scored: Vec<NFT> = tokens
            .iter()
            .map(|nft| {
                let result = calculator.calculate_rarity(nft, &tokens);
                let mut nft = nft.clone();
                nft.rarity_score = result.rarity_score;
                nft
            })
            .collect();
        scored.sort_by(|a, b| b.rarity_score.total_cmp(&a.rarity_score));

        assert_eq!(scored[0].token_id, "4", "the rarest token sorts first");
        assert_eq!(scored.len(), 4);
    }
}