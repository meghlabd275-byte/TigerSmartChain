//! ENS Resolution Service for TigerScan
//! 
//! This module provides .eth domain resolution.
//! 
//! ## Features
//! 
//! - Forward resolution (name -> address)
//! - Reverse resolution (address -> name)
//! - Text records
//! - Content hash
//! - ABI records
//! - Namewrapper support
//! 
//! ## Usage
//! 
//! ```ignore
//! let ens = ENSService::new("http://localhost:8545").await?;
//! 
//! // Forward resolution
//! let address = ens.resolve("example.eth").await?;
//! 
//! // Reverse resolution
//! let name = ens.reverse_resolve("0x...").await?;
//! ```

pub mod service;
pub mod types;

pub use service::ENSService;
pub use types::*;