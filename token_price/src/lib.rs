//! Token Price Service for TigerScan
//! 
//! This module provides real-time token price feeds with CoinGecko integration.
//! 
//! ## Features
//! 
//! - Real-time price fetching from CoinGecko API
//! - Price history with configurable time ranges
//! - Multiple price updates per token
//! - Market cap and volume data
//! - Price alerts and notifications
//! - Rate limiting with caching
//! - Multi-DEX price aggregation
//! 
//! ## Usage
//! 
//! ```ignore
//! let price_service = TokenPriceService::new("https://api.coingecko.com/api/v3").await?;
//! 
//! // Get current price
//! let price = price_service.get_price("0x...0", "usd").await?;
//! 
//! // Get price history
//! let history = price_service.get_price_history("0x...0", "usd", 7).await?;
//! ```

pub mod service;
pub mod types;
pub mod cache;

pub use service::TokenPriceService;
pub use types::*;
pub use cache::PriceCache;