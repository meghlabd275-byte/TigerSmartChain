//! MEV Types for TigerScan

use serde::{Deserialize, Serialize};

// =============================================================================
// MEV TYPES
// =============================================================================

/// MEV Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVTransaction {
    pub hash: String,
    pub block_number: u64,
    pub from: String,
    pub to: String,
    pub value: String,
    pub mev_type: MEVType,
    pub profit: u64,
    pub timestamp: i64,
}

/// MEV Type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MEVType {
    Arbitrage,
    Liquidation,
    Sandwich,
    FrontRun,
    BackRun,
    Unknown,
}

/// MEV Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVBlock {
    pub block_number: u64,
    pub total_profit: u64,
    pub transactions: Vec<MEVTransaction>,
    pub mev_type_breakdown: MEVBreakdown,
}

/// MEV Breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVBreakdown {
    pub arbitrage_count: usize,
    pub liquidation_count: usize,
    pub sandwich_count: usize,
    pub frontrun_count: usize,
    pub backrun_count: usize,
}

// =============================================================================
// ANALYTICS
// =============================================================================

/// MEV Analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVAnalytics {
    pub total_profit: u64,
    pub total_transactions: i64,
    pub by_type: MEVBreakdown,
    pub top_searchers: Vec<MEVSearcher>,
    pub period: AnalyticsPeriod,
}

/// MEV Searcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVSearcher {
    pub address: String,
    pub total_profit: u64,
    pub transaction_count: i64,
}

/// Analytics Period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalyticsPeriod {
    Hour,
    Day,
    Week,
    Month,
}

// =============================================================================
// CONFIG
// =============================================================================

/// MEV Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MEVConfig {
    pub chain_id: u64,
    pub min_profit: u64,
    pub detection_enabled: bool,
}

impl Default for MEVConfig {
    fn default() -> Self {
        Self {
            chain_id: 9001,
            min_profit: 1000000000, // 1 Gwei
            detection_enabled: true,
        }
    }
}