//! TigerScan ENS Resolution Service - Production-grade ENS resolution
//! Built with Rust for security and performance

#![forbid(unsafe_code)]

mod cache;
mod crypto;
mod database;
mod errors;
mod registry;
mod resolver;
mod reverse;
mod server;
mod types;
mod validation;

pub use cache::ENSCache;
pub use crypto::*;
pub use database::Database;
pub use errors::{Error, Result};
pub use registry::ENSRegistry;
pub use resolver::Resolver;
pub use reverse::ReverseLookup;
pub use server::Server;
pub use types::*;
pub use validation::*;