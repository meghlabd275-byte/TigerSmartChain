//! WebSocket API for TigerScan
//! 
//! Real-time subscriptions via WebSocket for:
//! - New blocks
//! - New transactions
//! - Pending transactions (mempool)
//! - Token transfers
//! - NFT transfers
//! - Logs
//! - Gas price updates
//! 
//! ## Usage
//! 
//! ```ignore
//! let server = WSServer::new("0.0.0.0:8080");
//! server.run().await?;
//! ```

pub mod server;
pub mod client;
pub mod messages;
pub mod subscriptions;

pub use server::*;
pub use client::*;
pub use messages::*;
pub use subscriptions::*;