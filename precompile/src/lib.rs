//! TigerScan Precompile Module

pub mod types;
pub mod ecrecover;
pub mod sha256;
pub mod modexp;
pub mod bn128;
pub mod identity;
pub mod ripemd160;
pub mod blake2f;

pub use types::*;