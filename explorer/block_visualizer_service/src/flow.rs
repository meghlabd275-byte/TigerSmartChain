//! Transaction Flow Analysis

use crate::types::{CallNode, FlowType, TransactionFlow};

pub struct FlowAnalyzer;

impl FlowAnalyzer {
    /// Analyze transaction flow
    pub fn analyze(tx_hash: &str, traces: Vec<String>) -> TransactionFlow {
        let calls: Vec<CallNode> = traces
            .iter()
            .enumerate()
            .map(|(i, t)| CallNode {
                id: i as u32,
                call_type: t.clone(),
                from: String::new(),
                to: String::new(),
                value: "0".to_string(),
                depth: 0,
                children: vec![],
            })
            .collect();
        
        let flow_type = if calls.iter().any(|c| c.call_type == "create") {
            FlowType::Transfer
        } else {
            FlowType::ContractCall
        };
        
        TransactionFlow {
            tx_hash: tx_hash.to_string(),
            calls,
            flow_type,
        }
    }
}