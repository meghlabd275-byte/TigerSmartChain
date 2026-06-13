//! Trace Indexer for TigerScan
//! 
//! This module provides full internal transaction tracking through trace methods.
//! It indexes:
//! - Call traces (internal transactions)
//! - State diffs (balance/storage changes)
//! - Contract creations
//! - Self-destructs
//! 
//! ## Features
//! 
//! - Real-time trace indexing
//! - Historical trace backfilling
//! - Contract creation tracking
//! - Balance change tracking
//! - Storage change tracking
//! - Call graph reconstruction
//! 
//! ## Usage
//! 
//! ```ignore
//! let indexer = TraceIndexer::new("http://localhost:8545").await?;
//! indexer.start().await?;
//! ```

pub mod indexer;
pub mod types;
pub mod storage;

pub use indexer::TraceIndexer;
pub use types::*;
pub use storage::TraceStorage;