//! Export a flow or a block as a diagram.

use crate::types::{BlockVisualization, TransactionFlow};

pub struct Exporter;

impl Exporter {
    /// Render a transaction flow as Graphviz DOT.
    ///
    /// Every node carries its call kind and value, and every parent-child link
    /// becomes an edge, so the graph is the call tree that was executed.
    pub fn to_dot(flow: &TransactionFlow) -> String {
        let mut dot = String::from("digraph tx_flow {\n  rankdir=LR;\n");

        for call in &flow.calls {
            let label = format!(
                "{}\\n{}\\nvalue {}",
                escape(&call.call_type),
                escape(&call.to),
                escape(&call.value)
            );

            dot.push_str(&format!("  node{} [label=\"{}\"];\n", call.id, label));
        }

        for call in &flow.calls {
            for child in &call.children {
                dot.push_str(&format!("  node{} -> node{};\n", call.id, child));
            }
        }

        dot.push_str("}\n");
        dot
    }

    /// Render a transaction flow as JSON.
    pub fn to_json(flow: &TransactionFlow) -> String {
        serde_json::to_string(flow).unwrap_or_default()
    }

    /// Render a block as a Mermaid graph.
    ///
    /// Each transaction becomes a node labelled with its hash, and the sender
    /// and recipient become nodes of their own, so the diagram shows which
    /// accounts a block touched.
    pub fn to_mermaid(block: &BlockVisualization) -> String {
        let mut mermaid = String::from("graph LR\n");

        for transaction in &block.transactions {
            let tx = escape(&transaction.hash);
            let from = escape(&transaction.from);
            let to = escape(&transaction.to);

            mermaid.push_str(&format!("  {from} -->|{tx}| {to}\n"));
        }

        mermaid
    }

    /// Render a block as JSON.
    pub fn block_to_json(block: &BlockVisualization) -> String {
        serde_json::to_string(block).unwrap_or_default()
    }
}

/// Escape a label for DOT and Mermaid, both of which treat quotes and
/// backslashes as syntax.
fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CallNode, FlowType, TxVisualization};

    fn flow() -> TransactionFlow {
        TransactionFlow {
            tx_hash: "0xabc".to_string(),
            calls: vec![
                CallNode {
                    id: 0,
                    call_type: "CALL".to_string(),
                    from: "0x1".to_string(),
                    to: "0x2".to_string(),
                    value: "10".to_string(),
                    depth: 0,
                    children: vec![1],
                    selector: None,
                    succeeded: true,
                },
                CallNode {
                    id: 1,
                    call_type: "STATICCALL".to_string(),
                    from: "0x2".to_string(),
                    to: "0x3".to_string(),
                    value: "0".to_string(),
                    depth: 1,
                    children: Vec::new(),
                    selector: None,
                    succeeded: true,
                },
            ],
            flow_type: FlowType::ContractCall,
        }
    }

    #[test]
    fn dot_carries_every_node_and_edge() {
        let dot = Exporter::to_dot(&flow());

        assert!(dot.contains("node0 [label=\"CALL"));
        assert!(dot.contains("node1 [label=\"STATICCALL"));
        assert!(dot.contains("node0 -> node1;"));
        assert!(dot.ends_with("}\n"));
    }

    #[test]
    fn mermaid_shows_the_transfer_direction() {
        let block = BlockVisualization {
            number: 1,
            hash: "0xblock".to_string(),
            timestamp: 0,
            transactions: vec![TxVisualization {
                hash: "0xtx".to_string(),
                from: "0x1".to_string(),
                to: "0x2".to_string(),
                value: "0".to_string(),
                gas: 21000,
                status: true,
                call_type: "CALL".to_string(),
                internal_calls: Vec::new(),
            }],
            gas_used: 21000,
            gas_limit: 30000000,
            miner: "0xminer".to_string(),
            size: 100,
        };

        let mermaid = Exporter::to_mermaid(&block);

        assert!(mermaid.contains("0x1 -->|0xtx| 0x2"));
    }

    #[test]
    fn a_quote_in_a_label_does_not_break_the_dot_syntax() {
        let mut flow = flow();
        flow.calls[0].to = "0x\"evil".to_string();

        let dot = Exporter::to_dot(&flow);

        assert!(dot.contains("\\\"evil"));
    }

    #[test]
    fn json_round_trips() {
        let flow = flow();
        let json = Exporter::to_json(&flow);

        let parsed: TransactionFlow = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.calls.len(), 2);
        assert_eq!(parsed.tx_hash, "0xabc");
    }
}
