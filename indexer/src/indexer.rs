//! Indexer for TigerScan

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// INDEXER
// =============================================================================

/// Blockchain Indexer
pub struct Indexer {
    config: IndexerConfig,
    stats: Arc<RwLock<IndexerStats>>,
    running: Arc<RwLock<bool>>,
    current_block: Arc<RwLock<u64>>,
}

impl Indexer {
    /// Create new indexer
    pub fn new(config: IndexerConfig) -> Self {
        Self {
            config,
            stats: Arc::new(RwLock::new(IndexerStats {
                current_block: 0,
                indexed_block: 0,
                indexed_transactions: 0,
                indexed_logs: 0,
                indexed_tokens: 0,
                indexed_nfts: 0,
                last_update: Utc::now().timestamp(),
                processing_rate: 0.0,
            })),
            running: Arc::new(RwLock::new(false)),
            current_block: Arc::new(RwLock::new(0)),
        }
    }

    /// Start indexing
    pub async fn start(&self) -> Result<(), String> {
        *self.running.write().await = true;
        
        // Start block processing loop
        self.process_blocks().await
    }

    /// Stop indexing
    pub async fn stop(&self) {
        *self.running.write().await = false;
    }

    /// Is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get statistics
    pub async fn get_stats(&self) -> IndexerStats {
        self.stats.read().await.clone()
    }

    /// Get current block
    pub async fn get_current_block(&self) -> u64 {
        *self.current_block.read().await
    }

    /// Process blocks
    async fn process_blocks(&self) -> Result<(), String> {
        let start_block = self.config.start_block;
        let mut current = start_block;

        while self.is_running().await {
            // Get latest block from RPC
            let latest = self.get_latest_block().await?;
            
            if current >= latest {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }

            // Process batch
            let end = std::cmp::min(current + self.config.batch_size as u64, latest);
            
            for block_num in current..=end {
                if let Err(e) = self.process_block(block_num).await {
                    log::error!("Error processing block {}: {}", block_num, e);
                }
            }

            current = end + 1;
            *self.current_block.write().await = current;
            
            // Update stats
            self.update_stats(current).await;
        }

        Ok(())
    }

    /// Get latest block number
    async fn get_latest_block(&self) -> Result<u64, String> {
        // Would query RPC in production
        Ok(1000)
    }

    /// Process single block
    async fn process_block(&self, block_number: u64) -> Result<(), String> {
        // Fetch block data
        let _block = self.fetch_block(block_number).await?;
        
        // Process transactions
        // Process logs
        // Process tokens
        // Process NFTs
        
        Ok(())
    }

    /// Fetch block data
    async fn fetch_block(&self, _block_number: u64) -> Result<IndexedBlock, String> {
        // Would fetch from RPC
        Ok(IndexedBlock {
            number: 0,
            hash: String::new(),
            parent_hash: String::new(),
            timestamp: 0,
            transactions: vec![],
            logs: vec![],
            internal_txs: vec![],
            miner: String::new(),
            difficulty: String::new(),
            total_difficulty: String::new(),
            size: 0,
            gas_used: 0,
            gas_limit: 0,
            base_fee_per_gas: None,
        })
    }

    /// Update statistics
    async fn update_stats(&self, block: u64) {
        let mut stats = self.stats.write().await;
        stats.indexed_block = block;
        stats.last_update = Utc::now().timestamp();
    }

    /// Index a transaction
    pub async fn index_transaction(&self, tx: IndexedTransaction) -> Result<(), String> {
        let mut stats = self.stats.write().await;
        stats.indexed_transactions += 1;
        Ok(())
    }

    /// Index a log
    pub async fn index_log(&self, log: IndexedLog) -> Result<(), String> {
        let mut stats = self.stats.write().await;
        stats.indexed_logs += 1;
        Ok(())
    }

    /// Index a token
    pub async fn index_token(&self, token: IndexedToken) -> Result<(), String> {
        let mut stats = self.stats.write().await;
        stats.indexed_tokens += 1;
        Ok(())
    }
}

// =============================================================================
// BUILDER
// =============================================================================

/// Indexer Builder
pub struct IndexerBuilder {
    config: IndexerConfig,
}

impl IndexerBuilder {
    pub fn new() -> Self {
        Self {
            config: IndexerConfig::default(),
        }
    }

    pub fn chain_id(mut self, id: u64) -> Self {
        self.config.chain_id = id;
        self
    }

    pub fn rpc_url(mut self, url: &str) -> Self {
        self.config.rpc_url = url.to_string();
        self
    }

    pub fn ws_url(mut self, url: &str) -> Self {
        self.config.ws_url = url.to_string();
        self
    }

    pub fn start_block(mut self, block: u64) -> Self {
        self.config.start_block = block;
        self
    }

    pub fn batch_size(mut self, size: usize) -> Self {
        self.config.batch_size = size;
        self
    }

    pub fn build(self) -> Indexer {
        Indexer::new(self.config)
    }
}

impl Default for IndexerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indexer_creation() {
        let indexer = Indexer::new(IndexerConfig::default());
        assert!(!indexer.is_running().await);
    }

    #[test]
    fn test_builder() {
        let indexer = IndexerBuilder::new()
            .chain_id(56)
            .rpc_url("http://localhost:8545")
            .build();
        
        assert_eq!(indexer.config.chain_id, 56);
    }
}