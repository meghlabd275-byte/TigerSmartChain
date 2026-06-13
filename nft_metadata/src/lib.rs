//! NFT Metadata Indexer for TigerScan
//! 
//! This module provides NFT metadata fetching with IPFS/Arweave support.
//! 
//! ## Features
//! 
//! - IPFS metadata fetching
//! - Arweave metadata fetching
//! - HTTP metadata fetching
//! - Trait analysis
//! - Rarity calculation
//! - Floor price tracking
//! - Collection analytics

pub mod indexer;
pub mod types;
pub mod ipfs;
pub mod arweave;

pub use indexer::NFTMetadataIndexer;
pub use types::*;
pub use ipfs::IPFSClient;
pub use arweave::ArweaveClient;