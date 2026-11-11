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
//! ```no_run
//! use token_price::TokenPriceService;
//!
//! # async fn example() -> Result<(), token_price::PriceError> {
//! let price_service = TokenPriceService::new("https://api.coingecko.com/api/v3");
//!
//! // USDT on Ethereum mainnet, addressed by its contract.
//! let usdt = "0xdac17f958d2ee523a2206206994597c13d831ec7";
//!
//! // Current price.
//! let price = price_service.get_price(usdt, "usd").await?;
//! println!("{} = {} USD", price.address, price.price);
//!
//! // Price history over the last 7 days.
//! let history = price_service.get_price_history(usdt, "usd", 7).await?;
//! println!("{} history points", history.prices.len());
//! # Ok(())
//! # }
//! ```
//!
//! Gas prices come from a chain rather than a price feed, so they need an
//! endpoint of their own:
//!
//! ```no_run
//! use token_price::TokenPriceService;
//!
//! # async fn example() -> Result<(), token_price::PriceError> {
//! let service = TokenPriceService::new("https://api.coingecko.com/api/v3")
//!     .with_rpc_url("https://ethereum-rpc.publicnode.com");
//! let gas = service.get_gas_price().await?;
//! println!("standard: {} wei", gas.standard);
//! # Ok(())
//! # }
//! ```

pub mod service;
pub mod types;
pub mod cache;

pub use service::{PriceError, TokenPriceService};
pub use types::*;
pub use cache::PriceCache;