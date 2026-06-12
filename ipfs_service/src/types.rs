//! IPFS Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// IPFS SERVICE
// =============================================================================

/// IPFS Node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsNode {
    pub id: String,
    pub addresses: Vec<String>,
    pub connected: bool,
}

/// Stored Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredData {
    pub cid: String,
    pub size: u64,
    pub content: Vec<u8>,
    pub uploaded_at: u64,
}

/// IPFS Service
pub struct Service {
    nodes: Vec<IpfsNode>,
    data: std::collections::HashMap<String, StoredData>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            nodes: vec![],
            data: std::collections::HashMap::new(),
        }
    }

    /// Add node
    pub fn add_node(&mut self, node: IpfsNode) {
        self.nodes.push(node);
    }

    /// Store data
    pub fn store(&mut self, cid: String, content: Vec<u8>) {
        let data = StoredData {
            cid: cid.clone(),
            size: content.len() as u64,
            content,
            uploaded_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        self.data.insert(cid, data);
    }

    /// Retrieve data
    pub fn retrieve(&self, cid: &str) -> Option<&StoredData> {
        self.data.get(cid)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}