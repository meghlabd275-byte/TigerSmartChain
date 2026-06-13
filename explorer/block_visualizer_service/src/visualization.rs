//! Block Visualization

use crate::types::{BlockVisualization, Config, TxVisualization};

pub struct Visualizer {
    config: Config,
}

impl Visualizer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    /// Visualize block
    pub async fn visualize_block(&self, block_number: u64) -> BlockVisualization {
        BlockVisualization {
            number: block_number,
            hash: String::new(),
            timestamp: 0,
            transactions: vec![],
            gas_used: 0,
            gas_limit: 0,
            miner: String::new(),
            size: 0,
        }
    }
}