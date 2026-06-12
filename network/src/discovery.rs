//! Network Discovery

use crate::types::*;

// =============================================================================
// DISCOVERY
// =============================================================================

/// Discovery Service
pub struct Discovery {
    config: NetworkConfig,
    nodes: std::collections::HashMap<String, Peer>,
    bootstrap_nodes: Vec<String>,
}

impl Discovery {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            nodes: std::collections::HashMap::new(),
            bootstrap_nodes: vec![],
        }
    }

    /// Start discovery
    pub fn start(&mut self) {
        log::info!("Starting discovery service");
    }

    /// Discover peers
    pub fn discover(&mut self) -> Vec<Peer> {
        vec![]
    }

    /// Add bootstrap node
    pub fn add_bootstrap(&mut self, node: &str) {
        self.bootstrap_nodes.push(node.to_string());
    }

    /// Get discovered nodes
    pub fn get_nodes(&self) -> Vec<&Peer> {
        self.nodes.values().collect()
    }
}

// =============================================================================
// DNS DISCOVERY
// =============================================================================

/// DNS Discovery
pub struct DNSDiscovery {
    enrtree_root: Option<String>,
}

impl DNSDiscovery {
    pub fn new() -> Self {
        Self { enrtree_root: None }
    }

    /// Set ENR tree root
    pub fn set_root(&mut self, root: &str) {
        self.enrtree_root = Some(root.to_string());
    }

    /// Sync DNS tree
    pub fn sync(&self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
}

impl Default for DNSDiscovery {
    fn default() -> Self {
        Self::new()
    }
}