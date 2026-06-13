//! TigerScan Internal Transaction Indexer
//! 
//! Complete production-ready internal transaction indexer with real trace_call execution.
//! Uses debug_traceTransaction to capture all internal calls, state changes, and contract creations.
//! 
//! ## Security Features
//! - Rate limiting on RPC calls
//! - Circuit breaker for failed traces
//! - Input validation and sanitization
//! - Encrypted storage for sensitive data
//! 
//! ## Performance
//! - Async batch processing
//! - Concurrent block processing
//! - Redis caching for trace results

pub mod types;
pub mod indexer;
pub mod storage;
pub mod rpc;
pub mod security;

pub use types::*;
pub use indexer::InternalTxIndexer;
pub use storage::InternalTxStorage;
pub use rpc::TraceRpcClient;
pub use security::*;