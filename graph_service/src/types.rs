//! Graph Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// GRAPH SERVICE
// =============================================================================

/// Node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub data: std::collections::HashMap<String, String>,
}

/// Edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub weight: f64,
}

/// Graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Graph Service
pub struct Service {
    graphs: std::collections::HashMap<String, Graph>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            graphs: std::collections::HashMap::new(),
        }
    }

    /// Add graph
    pub fn add_graph(&mut self, id: String, graph: Graph) {
        self.graphs.insert(id, graph);
    }

    /// Get graph
    pub fn get_graph(&self, id: &str) -> Option<&Graph> {
        self.graphs.get(id)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}