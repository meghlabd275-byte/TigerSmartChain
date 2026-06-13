//! TigerScan WebSocket Service - Production-grade real-time blockchain updates
//! Built with Rust for ultra-low latency and maximum security

pub mod config;
pub mod connection;
pub mod error;
pub mod events;
pub mod handler;
pub mod heartbeat;
pub mod metrics;
pub mod protocol;
pub mod server;
pub mod subscription;
pub mod types;
pub mod validation;

pub use config::Config;
pub use connection::Connection;
pub use error::{Error, Result};
pub use events::{BlockEvent, Event, EventType, MempoolEvent, TokenTransferEvent, TxEvent};
pub use handler::Handler;
pub use heartbeat::Heartbeat;
pub use metrics::Metrics;
pub use protocol::Protocol;
pub use server::Server;
pub use subscription::{Channel, Subscription};
pub use types::*;
pub use validation::*;