//! Transaction Simulation Service for TigerScan
//! 
//! This module provides full transaction simulation with state overrides.
//! 
//! ## Features
//! 
//! - eth_call simulation
//! - debug_traceCall simulation
//! - State overrides (balance, nonce, code, storage)
//! - Multi-call simulation
//! - Gas estimation
//! - Token balance simulation
//! - MEV detection
//! 
//! ## Usage
//! 
//! ```ignore
//! let sim = SimulationService::new("http://localhost:8545").await?;
//! 
//! // Simulate a transaction
//! let result = sim.simulate_call(tx).await?;
//! 
//! // Simulate with state overrides
//! let result = sim.simulate_with_overrides(tx, overrides).await?;
//! ```

pub mod service;
pub mod types;

pub use service::SimulationService;
pub use types::*;