//! Call graph construction from a `debug_traceTransaction` call frame tree.

use crate::types::{selector_of, CallFrame, CallNode, FlowType};

pub struct CallGraph;

impl CallGraph {
    /// Flatten a nested call frame tree into nodes with parent/child links.
    ///
    /// The tracer reports one frame per call, so a node's depth is its distance
    /// from the root and its id is its position in a pre-order walk.
    pub fn build(root: &CallFrame) -> Vec<CallNode> {
        let mut nodes = Vec::new();
        let mut next_id = 0;

        Self::visit(root, 0, &mut next_id, &mut nodes);

        nodes
    }

    fn visit(frame: &CallFrame, depth: u32, next_id: &mut u32, nodes: &mut Vec<CallNode>) -> u32 {
        let id = *next_id;
        *next_id += 1;

        let mut children = Vec::with_capacity(frame.calls.len());

        for child in &frame.calls {
            children.push(Self::visit(child, depth + 1, next_id, nodes));
        }

        nodes.push(CallNode {
            id,
            call_type: frame.call_type.clone(),
            from: frame.from.clone(),
            to: frame.target(),
            value: frame.value_decimal(),
            depth,
            children,
            selector: selector_of(frame.input.as_deref()),
            succeeded: frame.succeeded(),
        });

        id
    }

    /// Classify a transaction from the selectors its frames were called with.
    ///
    /// The selector is the callee's entry point, so a swap on a router, a
    /// stake on a staking pool and a vote on a governor are distinguishable
    /// without guessing from the addresses.
    pub fn get_flow_type(nodes: &[CallNode]) -> FlowType {
        const SWAP_SELECTORS: [&str; 6] = [
            "38ed1739", // swapExactTokensForTokens
            "8803dbee", // swapTokensForExactTokens
            "18cbafe5", // swapExactETHForTokens
            "fb3bdb41", // swapETHForExactTokens
            "7ff36ab5", // swapExactETHForTokensSupportingFeeOnTransferTokens
            "02751cec", // swapExactTokensForETH
        ];

        const STAKE_SELECTORS: [&str; 2] = [
            "a694fc3a", // stake(uint256)
            "2e17de78", // unstake(uint256)
        ];

        const GOVERNANCE_SELECTORS: [&str; 2] = [
            "56781388", // castVote(uint256,uint8)
            "15373e3d", // propose(address[],uint256[],bytes[],string)
        ];

        const NFT_TRANSFER_SELECTORS: [&str; 2] = [
            "42842e0e", // safeTransferFrom(address,address,uint256)
            "b88d4fde", // safeTransferFrom(address,address,uint256,bytes)
        ];

        const TOKEN_TRANSFER_SELECTORS: [&str; 2] = [
            "a9059cbb", // transfer(address,uint256)
            "23b872dd", // transferFrom(address,address,uint256)
        ];

        let selectors: Vec<&str> = nodes
            .iter()
            .filter_map(|node| node.selector.as_deref())
            .collect();

        if selectors.iter().any(|s| SWAP_SELECTORS.contains(s)) {
            return FlowType::Swap;
        }

        if selectors.iter().any(|s| STAKE_SELECTORS.contains(s)) {
            return FlowType::Staking;
        }

        if selectors.iter().any(|s| GOVERNANCE_SELECTORS.contains(s)) {
            return FlowType::Governance;
        }

        if selectors.iter().any(|s| NFT_TRANSFER_SELECTORS.contains(s)) {
            return FlowType::NFTTransfer;
        }

        if selectors.iter().any(|s| TOKEN_TRANSFER_SELECTORS.contains(s)) {
            return FlowType::TokenTransfer;
        }

        // A plain value transfer carries no calldata, so it is the absence of
        // any selector that identifies it.
        if selectors.is_empty() {
            return FlowType::Transfer;
        }

        FlowType::ContractCall
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(json: &str) -> CallFrame {
        serde_json::from_str(json).expect("valid frame")
    }

    #[test]
    fn a_tree_is_flattened_with_depths_and_child_links() {
        let root = frame(
            r#"{"type":"CALL","from":"0x1","to":"0x2",
                "calls":[{"type":"CALL","from":"0x2","to":"0x3",
                          "calls":[{"type":"STATICCALL","from":"0x3","to":"0x4"}]},
                         {"type":"DELEGATECALL","from":"0x2","to":"0x5"}]}"#,
        );

        let nodes = CallGraph::build(&root);

        assert_eq!(nodes.len(), 4);

        let root_node = nodes.iter().find(|node| node.depth == 0).unwrap();
        assert_eq!(root_node.children.len(), 2);

        let deepest = nodes.iter().find(|node| node.depth == 2).unwrap();
        assert_eq!(deepest.call_type, "STATICCALL");
        assert!(deepest.children.is_empty());
    }

    #[test]
    fn a_swap_router_call_is_classified_as_a_swap() {
        let root = frame(
            r#"{"type":"CALL","from":"0x1","to":"0x2","input":"0x38ed1739"}"#,
        );

        let nodes = CallGraph::build(&root);

        assert_eq!(CallGraph::get_flow_type(&nodes), FlowType::Swap);
    }

    #[test]
    fn a_bare_value_transfer_is_classified_as_a_transfer() {
        let root = frame(r#"{"type":"CALL","from":"0x1","to":"0x2","value":"0x10"}"#);

        let nodes = CallGraph::build(&root);

        assert_eq!(CallGraph::get_flow_type(&nodes), FlowType::Transfer);
    }

    #[test]
    fn a_token_transfer_is_classified_as_a_token_transfer() {
        let root = frame(
            r#"{"type":"CALL","from":"0x1","to":"0x2","input":"0xa9059cbb"}"#,
        );

        let nodes = CallGraph::build(&root);

        assert_eq!(CallGraph::get_flow_type(&nodes), FlowType::TokenTransfer);
    }

    #[test]
    fn an_unknown_selector_is_classified_as_a_contract_call() {
        let root = frame(
            r#"{"type":"CALL","from":"0x1","to":"0x2","input":"0xdeadbeef"}"#,
        );

        let nodes = CallGraph::build(&root);

        assert_eq!(CallGraph::get_flow_type(&nodes), FlowType::ContractCall);
    }

    #[test]
    fn a_reverted_frame_is_marked_as_failed() {
        let root = frame(
            r#"{"type":"CALL","from":"0x1","to":"0x2","error":"execution reverted"}"#,
        );

        let nodes = CallGraph::build(&root);

        assert!(!nodes[0].succeeded);
    }
}
