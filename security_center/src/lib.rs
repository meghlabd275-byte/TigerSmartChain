//! Security Center for TigerScan
//! 
//! This module provides security features including:
//! 
//! ## Features
//! 
//! - Real-time transaction alerts
//! - Transaction simulation and analysis
//! - Contract flagging and scam detection
//! - Phishing URL detection
//! - Wallet age analysis
//! - Behavior anomaly detection
//! - Multisig detection (Gnosis Safe)
//! - Token approval scanning
//! - Honeypot detection
//! 
//! ## Usage
//! 
//! ```ignore
//! let security = SecurityCenter::new().await?;
//! 
//! // Analyze a transaction
//! let result = security.analyze_transaction(tx).await?;
//! 
//! // Check for threats
//! let threats = security.scan_contract(address).await?;
//! ```

pub mod service;
pub mod types;

pub use service::SecurityCenter;
pub use types::*;