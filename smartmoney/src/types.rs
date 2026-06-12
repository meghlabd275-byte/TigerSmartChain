//! Smart Money Types

use serde::{Deserialize, Serialize};

// =============================================================================
// SMART MONEY
// =============================================================================

/// Trader
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trader {
    pub address: String,
    pub volume: u64,
    pub pnl: i64,
    pub win_rate: f64,
    pub trades: u64,
    pub first_trade: u64,
}

/// Trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub trader: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub profit: i64,
    pub timestamp: u64,
}

/// Portfolio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub address: String,
    pub tokens: std::collections::HashMap<String, u64>,
    pub total_value: u64,
}

impl Portfolio {
    pub fn new(address: String) -> Self {
        Self {
            address,
            tokens: std::collections::HashMap::new(),
            total_value: 0,
        }
    }
}

/// Tracker
pub struct Tracker {
    traders: std::collections::HashMap<String, Trader>,
    trades: Vec<Trade>,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            traders: std::collections::HashMap::new(),
            trades: vec![],
        }
    }

    /// Add trader
    pub fn add_trader(&mut self, trader: Trader) {
        self.traders.insert(trader.address.clone(), trader);
    }

    /// Get trader
    pub fn get_trader(&self, address: &str) -> Option<&Trader> {
        self.traders.get(address)
    }

    /// Add trade
    pub fn add_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}