//! Call Graph Generation

use crate::types::{CallNode, FlowType};

pub struct CallGraph;

impl CallGraph {
    /// Build call graph from traces
    pub fn build(traces: &[String]) -> Vec<CallNode> {
        let mut nodes = Vec::new();
        for (i, trace) in traces.iter().enumerate() {
            nodes.push(CallNode {
                id: i as u32,
                call_type: trace.clone(),
                from: String::new(),
                to: String::new(),
                value: "0".to_string(),
                depth: 0,
                children: vec![],
            });
        }
        nodes
    }
    
    /// Get flow type from call
    pub fn get_flow_type(call: &str) -> FlowType {
        match call {
            "create" => FlowType::Transfer,
            "call" => FlowType::ContractCall,
            _ => FlowType::ContractCall,
        }
    }
}