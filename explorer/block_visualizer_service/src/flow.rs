//! Transaction flow analysis over the call tracer output.

use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{Address, H256, U64};
use serde::{Deserialize, Serialize};

use crate::call_graph::CallGraph;
use crate::types::{CallFrame, CallNode, Config, FlowType, TransactionFlow};

/// A block as the JSON-RPC `eth_getBlockByNumber` call returns it.
#[derive(Debug, Deserialize, Serialize)]
pub struct RpcBlock {
    pub number: U64,
    pub hash: H256,
    #[serde(rename = "parentHash")]
    pub parent_hash: H256,
    pub miner: Address,
    pub timestamp: U64,
    #[serde(rename = "gasUsed")]
    pub gas_used: U64,
    #[serde(rename = "gasLimit")]
    pub gas_limit: U64,
    pub size: U64,
    pub transactions: Vec<RpcTransaction>,
}

/// A transaction as the JSON-RPC `eth_getBlockByNumber` call returns it.
#[derive(Debug, Deserialize, Serialize)]
pub struct RpcTransaction {
    pub hash: H256,
    pub from: Address,
    #[serde(default)]
    pub to: Option<Address>,
    #[serde(default)]
    pub value: U64,
    pub gas: U64,
    #[serde(default)]
    pub input: String,
}

/// The receipt fields the visualizer needs.
#[derive(Debug, Deserialize, Serialize)]
pub struct RpcReceipt {
    pub status: Option<U64>,
}

pub struct FlowAnalyzer;

impl FlowAnalyzer {
    /// Fetch a transaction's call frames and build its flow.
    ///
    /// The frames come from `debug_traceTransaction` with the `callTracer`
    /// tracer, which is what a node exposes when the debug API is enabled.
    /// The tracer result is used as-is; nothing is synthesised when the node
    /// does not answer.
    pub async fn analyze(config: &Config, tx_hash: &str) -> Result<TransactionFlow, String> {
        let provider = Provider::<Http>::try_from(config.rpc_url.as_str())
            .map_err(|error| format!("invalid RPC URL {}: {error}", config.rpc_url))?;

        let hash: H256 = tx_hash
            .parse()
            .map_err(|error| format!("invalid transaction hash {tx_hash}: {error}"))?;

        let params = serde_json::json!([format!("{hash:?}"), { "tracer": "callTracer" }]);

        let result: serde_json::Value = provider
            .request("debug_traceTransaction", params)
            .await
            .map_err(|error| format!("debug_traceTransaction failed: {error}"))?;

        let root: CallFrame = serde_json::from_value(result)
            .map_err(|error| format!("call tracer result is not a call frame: {error}"))?;

        Ok(Self::from_frames(tx_hash, &root))
    }

    /// Build a flow from frames the caller already holds.
    pub fn from_frames(tx_hash: &str, root: &CallFrame) -> TransactionFlow {
        let calls = CallGraph::build(root);
        let flow_type = CallGraph::get_flow_type(&calls);

        TransactionFlow {
            tx_hash: tx_hash.to_string(),
            calls,
            flow_type,
        }
    }

    /// The frames of a flow, indexed by id.
    pub fn node(flow: &TransactionFlow, id: u32) -> Option<&CallNode> {
        flow.calls.iter().find(|call| call.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(json: &str) -> CallFrame {
        serde_json::from_str(json).expect("valid frame")
    }

    #[test]
    fn a_flow_is_built_from_a_frame_tree() {
        let root = frame(
            r#"{"type":"CALL","from":"0x1","to":"0x2","input":"0x38ed1739",
                "calls":[{"type":"CALL","from":"0x2","to":"0x3","input":"0xa9059cbb"}]}"#,
        );

        let flow = FlowAnalyzer::from_frames("0xabc", &root);

        assert_eq!(flow.tx_hash, "0xabc");
        assert_eq!(flow.calls.len(), 2);
        assert_eq!(flow.flow_type, FlowType::Swap);
    }

    #[test]
    fn a_node_is_found_by_id() {
        let root = frame(r#"{"type":"CALL","from":"0x1","to":"0x2"}"#);

        let flow = FlowAnalyzer::from_frames("0xabc", &root);

        assert_eq!(FlowAnalyzer::node(&flow, 0).unwrap().to, "0x2");
        assert!(FlowAnalyzer::node(&flow, 99).is_none());
    }

    #[test]
    fn an_invalid_rpc_url_is_rejected() {
        let config = Config {
            rpc_url: "not a url".to_string(),
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(FlowAnalyzer::analyze(&config, "0x00"))
            .unwrap_err();

        assert!(error.contains("invalid RPC URL"));
    }

    #[test]
    fn an_invalid_transaction_hash_is_rejected_before_the_rpc_call() {
        let config = Config {
            rpc_url: "http://localhost:8545".to_string(),
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(FlowAnalyzer::analyze(&config, "0xnothex"))
            .unwrap_err();

        assert!(error.contains("invalid transaction hash"));
    }
}
