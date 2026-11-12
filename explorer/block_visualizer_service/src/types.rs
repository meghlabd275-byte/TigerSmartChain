//! Visualizer Types

use serde::{Deserialize, Serialize};

/// Block Visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockVisualization {
    pub number: u64,
    pub hash: String,
    pub timestamp: i64,
    pub transactions: Vec<TxVisualization>,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub miner: String,
    pub size: usize,
}

/// Transaction Visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxVisualization {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: u64,
    pub status: bool,
    pub call_type: String,
    pub internal_calls: Vec<InternalCall>,
}

/// Internal Call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalCall {
    pub from: String,
    pub to: String,
    pub value: String,
    pub call_type: String,
    pub depth: u32,
}

/// Transaction Flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionFlow {
    pub tx_hash: String,
    pub calls: Vec<CallNode>,
    pub flow_type: FlowType,
}

/// Call Node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallNode {
    pub id: u32,
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub depth: u32,
    pub children: Vec<u32>,
    /// The callee's four-byte entry point, when the frame carried calldata.
    pub selector: Option<String>,
    /// False when the frame reverted.
    pub succeeded: bool,
}

/// Flow Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowType {
    Transfer,
    ContractCall,
    TokenTransfer,
    NFTTransfer,
    Swap,
    Staking,
    Governance,
}

/// Config
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
        }
    }
}
/// A call frame as returned by `debug_traceTransaction` with the call tracer.
///
/// The tracer nests the frames it produced under `calls`, and reports the
/// fields below. Parsing this shape rather than the raw opcode trace is what
/// makes the tree the interpreter actually executed, including depth.
#[derive(Debug, Clone, Deserialize)]
pub struct CallFrame {
    #[serde(rename = "type", default)]
    pub call_type: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub gas: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub calls: Vec<CallFrame>,
}

impl CallFrame {
    /// The callee address, or the created contract for a CREATE frame.
    pub fn target(&self) -> String {
        self.to.clone().unwrap_or_default()
    }

    /// True when the frame did not revert.
    pub fn succeeded(&self) -> bool {
        self.error.is_none()
    }

    /// The value transferred, normalised to a decimal string.
    pub fn value_decimal(&self) -> String {
        let Some(value) = self.value.as_deref() else {
            return "0".to_string();
        };

        decode_quantity(value).unwrap_or(0).to_string()
    }
}

/// Decode a JSON-RPC quantity (`0x`-prefixed, no leading zeros).
pub fn decode_quantity(value: &str) -> Option<u128> {
    let trimmed = value.trim_start_matches("0x");

    if trimmed.is_empty() {
        return Some(0);
    }

    u128::from_str_radix(trimmed, 16).ok()
}

/// A four-byte function selector, or `None` when the input is too short.
pub fn selector_of(input: Option<&str>) -> Option<String> {
    let input = input?.trim_start_matches("0x");

    (input.len() >= 8).then(|| input[..8].to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities_are_decoded_from_hex() {
        assert_eq!(decode_quantity("0x0"), Some(0));
        assert_eq!(decode_quantity("0x10"), Some(16));
        assert_eq!(decode_quantity("0x"), Some(0));
        assert_eq!(decode_quantity("0xzz"), None);
    }

    #[test]
    fn a_selector_is_the_first_four_bytes_of_the_input() {
        assert_eq!(
            selector_of(Some("0xa9059cbb000000000000000000000000deadbeef")),
            Some("a9059cbb".to_string())
        );
        assert_eq!(selector_of(Some("0x1234")), None);
        assert_eq!(selector_of(None), None);
    }

    #[test]
    fn a_frame_without_a_value_transfers_nothing() {
        let frame: CallFrame = serde_json::from_str(r#"{"type":"CALL","from":"0x1","to":"0x2"}"#)
            .expect("the tracer always emits type, from and to");

        assert_eq!(frame.value_decimal(), "0");
        assert_eq!(frame.target(), "0x2");
        assert!(frame.succeeded());
    }

    #[test]
    fn a_reverted_frame_is_reported_as_failed() {
        let frame: CallFrame = serde_json::from_str(
            r#"{"type":"CALL","from":"0x1","to":"0x2","error":"execution reverted"}"#,
        )
        .expect("valid frame");

        assert!(!frame.succeeded());
    }

    #[test]
    fn nested_frames_are_parsed() {
        let frame: CallFrame = serde_json::from_str(
            r#"{"type":"CALL","from":"0x1","to":"0x2","value":"0x5",
                "calls":[{"type":"STATICCALL","from":"0x2","to":"0x3"}]}"#,
        )
        .expect("valid frame");

        assert_eq!(frame.calls.len(), 1);
        assert_eq!(frame.calls[0].call_type, "STATICCALL");
        assert_eq!(frame.value_decimal(), "5");
    }
}
