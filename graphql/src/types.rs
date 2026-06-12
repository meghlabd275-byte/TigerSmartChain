//! GraphQL Types

use serde::{Deserialize, Serialize};

// =============================================================================
// GRAPHQL
// =============================================================================

/// Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub queries: Vec<String>,
    pub mutations: Vec<String>,
}

/// Query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub name: String,
    pub fields: Vec<String>,
    pub args: Vec<String>,
}

/// Mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    pub name: String,
    pub fields: Vec<String>,
    pub args: Vec<String>,
}