//! Historical Gas Tracker for TigerScan
//! 
//! This module provides comprehensive gas analytics with historical data.
//! 
//! ## Features
//! 
//! - Real-time gas prices
//! - Historical gas analysis
//! - Gas price predictions
//! - Fee market analysis
//! - Burn tracking (EIP-1559)
//! 
//! ## Usage
//! 
//! ```ignore
//! let tracker = GasTracker::new("http://localhost:8545").await?;
//! 
//! // Get current gas prices
//! let prices = tracker.get_gas_prices().await?;
//! 
//! // Get historical data
//! let history = tracker.get_history(7).await?;
//! ```

pub mod service;
pub mod types;

pub use service::GasTracker;
pub use types::*;