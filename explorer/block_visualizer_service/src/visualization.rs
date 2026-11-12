//! Block visualization over JSON-RPC.

use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{BlockId, H256, U64};
use tracing::warn;

use crate::flow::{RpcBlock, RpcReceipt};
use crate::types::{
    decode_quantity, BlockVisualization, Config, InternalCall, TxVisualization,
};

pub struct Visualizer {
    config: Config,
}

impl Visualizer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Fetch a block and describe it.
    ///
    /// The transaction list and the gas figures come from the block itself.
    /// Each transaction's success flag comes from its receipt, and its internal
    /// calls come from the call tracer. A node without the debug API still
    /// produces a block; the internal calls are then empty rather than invented.
    pub async fn visualize_block(&self, block_number: u64) -> Result<BlockVisualization, String> {
        let provider = Provider::<Http>::try_from(self.config.rpc_url.as_str())
            .map_err(|error| format!("invalid RPC URL {}: {error}", self.config.rpc_url))?;

        let block: RpcBlock = provider
            .request(
                "eth_getBlockByNumber",
                (BlockId::Number(block_number.into()), true),
            )
            .await
            .map_err(|error| format!("eth_getBlockByNumber failed: {error}"))?;

        let mut transactions = Vec::with_capacity(block.transactions.len());

        for transaction in &block.transactions {
            let receipt: Option<RpcReceipt> = provider
                .request(
                    "eth_getTransactionReceipt",
                    (format!("{:?}", transaction.hash),),
                )
                .await
                .ok();

            let status = receipt
                .as_ref()
                .and_then(|receipt| receipt.status)
                .map(|status| status.as_u64() == 1)
                // A pre-Byzantium receipt carries no status field; the call
                // tracer result is then the only success signal available.
                .unwrap_or(true);

            let internal_calls = self.internal_calls(transaction.hash).await;

            transactions.push(TxVisualization {
                hash: format!("{:?}", transaction.hash),
                from: format!("{:?}", transaction.from),
                to: transaction
                    .to
                    .map(|address| format!("{address:?}"))
                    .unwrap_or_default(),
                value: transaction.value.to_string(),
                gas: transaction.gas.as_u64(),
                status,
                call_type: if transaction.to.is_some() {
                    "CALL".to_string()
                } else {
                    "CREATE".to_string()
                },
                internal_calls,
            });
        }

        Ok(BlockVisualization {
            number: block.number.as_u64(),
            hash: format!("{:?}", block.hash),
            timestamp: block.timestamp.as_u64() as i64,
            transactions,
            gas_used: block.gas_used.as_u64(),
            gas_limit: block.gas_limit.as_u64(),
            miner: format!("{:?}", block.miner),
            size: block.size.as_u64() as usize,
        })
    }

    /// The internal calls of a transaction, from the call tracer.
    async fn internal_calls(&self, hash: H256) -> Vec<InternalCall> {
        let provider = match Provider::<Http>::try_from(self.config.rpc_url.as_str()) {
            Ok(provider) => provider,
            Err(_) => return Vec::new(),
        };

        let params = serde_json::json!([format!("{hash:?}"), { "tracer": "callTracer" }]);

        let result: serde_json::Value = match provider
            .request("debug_traceTransaction", params)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                // A node without the debug API is a normal deployment; the
                // block view degrades to the transaction list.
                warn!(%error, "call tracer unavailable; reporting no internal calls");
                return Vec::new();
            }
        };

        let root: crate::types::CallFrame = match serde_json::from_value(result) {
            Ok(root) => root,
            Err(error) => {
                warn!(%error, "call tracer result is not a call frame");
                return Vec::new();
            }
        };

        flatten(&root, 1)
    }

    /// Fetch the number of the latest block.
    pub async fn latest_block_number(&self) -> Result<u64, String> {
        let provider = Provider::<Http>::try_from(self.config.rpc_url.as_str())
            .map_err(|error| format!("invalid RPC URL {}: {error}", self.config.rpc_url))?;

        provider
            .get_block_number()
            .await
            .map(|number| number.as_u64())
            .map_err(|error| format!("eth_blockNumber failed: {error}"))
    }
}

/// Flatten the nested frames below the root into internal calls.
fn flatten(frame: &crate::types::CallFrame, depth: u32) -> Vec<InternalCall> {
    let mut calls = Vec::new();

    for child in &frame.calls {
        calls.push(InternalCall {
            from: child.from.clone(),
            to: child.target(),
            value: decode_quantity(child.value.as_deref().unwrap_or("0x0"))
                .unwrap_or(0)
                .to_string(),
            call_type: child.call_type.clone(),
            depth,
        });

        calls.extend(flatten(child, depth + 1));
    }

    calls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flatten_of_a_leaf_frame_is_empty() {
        let frame: crate::types::CallFrame =
            serde_json::from_str(r#"{"type":"CALL","from":"0x1","to":"0x2"}"#).unwrap();

        assert!(flatten(&frame, 1).is_empty());
    }

    #[test]
    fn nested_frames_are_flattened_with_increasing_depth() {
        let frame: crate::types::CallFrame = serde_json::from_str(
            r#"{"type":"CALL","from":"0x1","to":"0x2",
                "calls":[{"type":"CALL","from":"0x2","to":"0x3",
                          "calls":[{"type":"STATICCALL","from":"0x3","to":"0x4"}]}]}"#,
        )
        .unwrap();

        let calls = flatten(&frame, 1);

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].depth, 1);
        assert_eq!(calls[1].depth, 2);
        assert_eq!(calls[1].call_type, "STATICCALL");
    }

    #[test]
    fn an_invalid_rpc_url_is_rejected() {
        let visualizer = Visualizer::new(Config {
            rpc_url: "not a url".to_string(),
        });

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime.block_on(visualizer.visualize_block(1)).unwrap_err();

        assert!(error.contains("invalid RPC URL"));
    }
}
