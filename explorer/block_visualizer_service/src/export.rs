//! Export Formats

use crate::types::{BlockVisualization, TransactionFlow};
use serde_json::json;

pub struct Exporter;

impl Exporter {
    /// Export to DOT format (Graphviz)
    pub fn to_dot(flow: &TransactionFlow) -> String {
        let mut dot = String::from("digraph tx_flow {\n");
        for call in &flow.calls {
            dot.push_str(&format!("  node{} [label=\"{}\\n{}\"];\n", call.id, call.call_type, call.to));
        }
        dot.push_str("}\n");
        dot
    }
    
    /// Export to JSON
    pub fn to_json(flow: &TransactionFlow) -> String {
        serde_json::to_string(flow).unwrap_or_default()
    }
    
    /// Export block to Mermaid
    pub fn to_mermaid(block: &BlockVisualization) -> String {
        let mut md = String::from("graph TD\n");
        for tx in &block.transactions {
            md.push_str(&format!("  {} [\"{} -> {}\"]\n", &tx.hash[..8], &tx.from[..8], &tx.to[..8]));
        }
        md
    }
}