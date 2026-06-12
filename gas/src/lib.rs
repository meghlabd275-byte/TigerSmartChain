//! TigerScan Gas Module
//! Gas tracking, estimation, and prediction

pub mod types;
pub mod estimator;
pub mod tracker;
pub mod predictions;

pub use types::*;
pub use estimator::*;
pub use tracker::*;