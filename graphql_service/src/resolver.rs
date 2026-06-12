//! GraphQL Resolver

use super::types::*;

// =============================================================================
// RESOLVER
// =============================================================================

/// Resolver
pub struct Resolver {
    blocks: std::collections::HashMap<u64, Block>,
    transactions: std::collections::HashMap<String, Transaction>,
    tokens: std::collections::HashMap<String, Token>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            blocks: std::collections::HashMap::new(),
            transactions: std::collections::HashMap::new(),
            tokens: std::collections::HashMap::new(),
        }
    }

    /// Resolve block
    pub fn resolve_block(&self, number: u64) -> Option<&Block> {
        self.blocks.get(&number)
    }

    /// Resolve transaction
    pub fn resolve_transaction(&self, hash: &str) -> Option<&Transaction> {
        self.transactions.get(hash)
    }

    /// Resolve token
    pub fn resolve_token(&self, address: &str) -> Option<&Token> {
        self.tokens.get(address)
    }

    /// Add block
    pub fn add_block(&mut self, block: Block) {
        self.blocks.insert(block.number, block);
    }

    /// Add transaction
    pub fn add_transaction(&mut self, tx: Transaction) {
        self.transactions.insert(tx.hash.clone(), tx);
    }

    /// Add token
    pub fn add_token(&mut self, token: Token) {
        self.tokens.insert(token.address.clone(), token);
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}