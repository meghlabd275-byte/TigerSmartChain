//! Blockchain Chain

use crate::types::*;
use std::collections::HashMap;

// =============================================================================
// CHAIN
// =============================================================================

/// Blockchain
pub struct Blockchain {
    config: ChainConfig,
    blocks: HashMap<u64, Block>,
    headers: HashMap<u64, BlockHeader>,
   Genesis: Option<Genesis>,
}

impl Blockchain {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            config,
            blocks: HashMap::new(),
            headers: HashMap::new(),
            genesis: None,
        }
    }

    /// Set genesis
    pub fn set_genesis(&mut self, genesis: Genesis) {
        self.genesis = Some(genesis);
    }

    /// Get block by number
    pub fn get_block(&self, number: u64) -> Option<&Block> {
        self.blocks.get(&number)
    }

    /// Get block by hash
    pub fn get_block_by_hash(&self, hash: &str) -> Option<&Block> {
        self.blocks.values().find(|b| b.header.hash == hash)
    }

    /// Insert block
    pub fn insert_block(&mut self, block: Block) {
        let number = block.header.number;
        self.blocks.insert(number, block);
    }

    /// Get current height
    pub fn height(&self) -> u64 {
        self.blocks.keys().max().copied().unwrap_or(0)
    }

    /// Get header by number
    pub fn get_header(&self, number: u64) -> Option<&BlockHeader> {
        self.headers.get(&number)
    }
}

// =============================================================================
// CHAIN MANAGER
// =============================================================================

/// Chain Manager
pub struct ChainManager {
    chain: Blockchain,
}

impl ChainManager {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            chain: Blockchain::new(config),
        }
    }

    /// Get chain
    pub fn chain(&self) -> &Blockchain {
        &self.chain
    }

    /// Get chain mut
    pub fn chain_mut(&mut self) -> &mut Blockchain {
        &mut self.chain
    }

    /// Validate block
    pub fn validate_block(&self, block: &Block) -> Result<(), String> {
        // Validate block
        Ok(())
    }

    /// Process block
    pub fn process_block(&mut self, block: Block) -> Result<(), String> {
        self.chain.insert_block(block);
        Ok(())
    }
}