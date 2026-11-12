//! Block and Transaction Visualizer Service.
//!
//! The visualizer reads a block and its receipts over JSON-RPC, and reads each
//! transaction's call tree from the node's call tracer. Everything it reports
//! is therefore a measured value: the block fields come from the block, the
//! success flag comes from the receipt, and the internal calls come from the
//! tracer. A node that does not expose the debug API yields a block whose
//! internal calls are empty rather than a block with invented calls.

#![forbid(unsafe_code)]

mod call_graph;
mod export;
mod flow;
mod types;
mod visualization;

pub use call_graph::CallGraph;
pub use export::Exporter;
pub use flow::{FlowAnalyzer, RpcBlock, RpcReceipt, RpcTransaction};
pub use types::{
    decode_quantity, selector_of, BlockVisualization, CallFrame, CallNode, Config, FlowType,
    InternalCall, TransactionFlow, TxVisualization,
};
pub use visualization::Visualizer;

/// The visualizer service, holding the node it reads from.
pub struct VisualizerService {
    visualizer: Visualizer,
    analyzer: FlowAnalyzer,
}

impl VisualizerService {
    pub fn new(config: Config) -> Self {
        Self {
            visualizer: Visualizer::new(config),
            analyzer: FlowAnalyzer,
        }
    }

    /// The visualizer bound to the node.
    pub fn visualizer(&self) -> &Visualizer {
        &self.visualizer
    }

    /// Describe a block.
    pub async fn visualize_block(&self, block_number: u64) -> Result<BlockVisualization, String> {
        self.visualizer.visualize_block(block_number).await
    }

    /// Describe a transaction's call flow.
    pub async fn visualize_transaction(
        &self,
        config: &Config,
        tx_hash: &str,
    ) -> Result<TransactionFlow, String> {
        FlowAnalyzer::analyze(config, tx_hash).await
    }

    /// The current chain head.
    pub async fn latest_block_number(&self) -> Result<u64, String> {
        self.visualizer.latest_block_number().await
    }
}
