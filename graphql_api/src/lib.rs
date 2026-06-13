//! Complete GraphQL API for TigerScan
//! 
//! Full GraphQL schema with queries, mutations, and subscriptions.
//! 
//! ## Schema
//! 
//! - Blocks queries
//! - Transactions queries
//! - Tokens queries
//! - NFTs queries
//! - Analytics queries
//! - Real-time subscriptions
//! - Batch queries with cursor-based pagination
//! - Full-text search
//! 
//! ## Usage
//! 
//! ```ignore
//! let schema = TigerScanSchema::build().finish();
//! let graphql = async_graphql::Schema::build(schema).data(pool).finish();
//! ```

pub mod schema;
pub mod queries;
pub mod mutations;
pub mod subscriptions;
pub mod resolvers;

pub use schema::*;
pub use queries::*;
pub use mutations::*;
pub use subscriptions::*;
pub use resolvers::*;