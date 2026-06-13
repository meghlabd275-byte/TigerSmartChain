//! Trace Storage for TigerScan
//! 
//! This module provides storage for trace data. In production, this would connect
//! to a database. For now, it provides an in-memory implementation.

use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum TraceStorageError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    StorageError(String),
}

// =============================================================================
// STORAGE
// =============================================================================

/// In-memory trace storage
pub struct TraceStorage {
    traces: Arc<RwLock<HashMap<String, Vec<IndexedTrace>>>,
    state_diffs: Arc<RwLock<HashMap<String, Vec<IndexedStateDiff>>>,
    creations: Arc<RwLock<HashMap<String, ContractCreation>>>,
    selfdestructs: Arc<RwLock<HashMap<String, SelfDestruct>>>,
}

impl TraceStorage {
    /// Create new trace storage
    pub fn new() -> Self {
        Self {
            traces: Arc::new(RwLock::new(HashMap::new())),
            state_diffs: Arc::new(RwLock::new(HashMap::new())),
            creations: Arc::new(RwLock::new(HashMap::new())),
            selfdestructs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store traces for a transaction
    pub async fn store_traces(&self, tx_hash: &str, traces: Vec<IndexedTrace>) -> Result<(), TraceStorageError> {
        let mut traces_map = self.traces.write().await;
        traces_map.insert(tx_hash.to_string(), traces);
        Ok(())
    }

    /// Get traces for a transaction
    pub async fn get_traces(&self, tx_hash: &str) -> Result<Vec<IndexedTrace>, TraceStorageError> {
        let traces_map = self.traces.read().await;
        traces_map
            .get(tx_hash)
            .cloned()
            .ok_or_else(|| TraceStorageError::NotFound(format!("Traces for tx {}", tx_hash)))
    }

    /// Store state diffs for a transaction
    pub async fn store_state_diffs(&self, tx_hash: &str, diffs: Vec<IndexedStateDiff>) -> Result<(), TraceStorageError> {
        let mut diffs_map = self.state_diffs.write().await;
        diffs_map.insert(tx_hash.to_string(), diffs);
        Ok(())
    }

    /// Get state diffs for a transaction
    pub async fn get_state_diffs(&self, tx_hash: &str) -> Result<Vec<IndexedStateDiff>, TraceStorageError> {
        let diffs_map = self.state_diffs.read().await;
        diffs_map
            .get(tx_hash)
            .cloned()
            .ok_or_else(|| TraceStorageError::NotFound(format!("State diffs for tx {}", tx_hash)))
    }

    /// Store contract creation
    pub async fn store_creation(&self, creation: ContractCreation) -> Result<(), TraceStorageError> {
        let mut creations_map = self.creations.write().await;
        creations_map.insert(creation.address.clone(), creation);
        Ok(())
    }

    /// Get contract creation by address
    pub async fn get_creation(&self, address: &str) -> Result<ContractCreation, TraceStorageError> {
        let creations_map = self.creations.read().await;
        creations_map
            .get(address)
            .cloned()
            .ok_or_else(|| TraceStorageError::NotFound(format!("Creation for {}", address)))
    }

    /// Get contract creations in a block
    pub async fn get_creations_in_block(&self, block_number: u64) -> Result<Vec<ContractCreation>, TraceStorageError> {
        let creations_map = self.creations.read().await;
        let creations: Vec<ContractCreation> = creations_map
            .values()
            .filter(|c| c.block_number == block_number)
            .cloned()
            .collect();
        Ok(creations)
    }

    /// Store self-destruct
    pub async fn store_selfdestruct(&self, sd: SelfDestruct) -> Result<(), TraceStorageError> {
        let mut sd_map = self.selfdestructs.write().await;
        sd_map.insert(sd.address.clone(), sd);
        Ok(())
    }

    /// Get self-destruct by address
    pub async fn get_selfdestruct(&self, address: &str) -> Result<SelfDestruct, TraceStorageError> {
        let sd_map = self.selfdestructs.read().await;
        sd_map
            .get(address)
            .cloned()
            .ok_or_else(|| TraceStorageError::NotFound(format!("Self-destruct for {}", address)))
    }

    /// Get self-destructs in a block
    pub async fn get_selfdestructs_in_block(&self, block_number: u64) -> Result<Vec<SelfDestruct>, TraceStorageError> {
        let sd_map = self.selfdestructs.read().await;
        let sd_list: Vec<SelfDestruct> = sd_map
            .values()
            .filter(|sd| sd.block_number == block_number)
            .cloned()
            .collect();
        Ok(sd_list)
    }

    /// Get all traces in a block range
    pub async fn get_traces_in_range(&self, from_block: u64, to_block: u64) -> Result<HashMap<String, Vec<IndexedTrace>>, TraceStorageError> {
        let traces_map = self.traces.read().await;
        let result: HashMap<String, Vec<IndexedTrace>> = traces_map
            .iter()
            .filter(|(_, traces)| {
                traces.iter().any(|t| t.block_number >= from_block && t.block_number <= to_block)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(result)
    }

    /// Clear all data
    pub async fn clear(&self) -> Result<(), TraceStorageError> {
        self.traces.write().await.clear();
        self.state_diffs.write().await.clear();
        self.creations.write().await.clear();
        self.selfdestructs.write().await.clear();
        Ok(())
    }

    /// Get storage statistics
    pub async fn get_stats(&self) -> (usize, usize, usize, usize) {
        let traces = self.traces.read().await;
        let diffs = self.state_diffs.read().await;
        let creations = self.creations.read().await;
        let sd = self.selfdestructs.read().await;
        (traces.len(), diffs.len(), creations.len(), sd.len())
    }
}

impl Default for TraceStorage {
    fn default() -> Self {
        Self::new()
    }
}