//! GraphQL Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// GRAPHQL SERVICE
// =============================================================================

/// Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub hash: String,
    pub number: u64,
    pub timestamp: u64,
    pub transactions: Vec<String>,
    pub gas_used: u64,
}

/// Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub gas: u64,
    pub input: Vec<u8>,
    pub status: String,
}

/// Token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: u64,
}

/// Query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub query: String,
    pub variables: std::collections::HashMap<String, String>,
}