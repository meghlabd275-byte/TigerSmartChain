//! Tags Types

use serde::{Deserialize, Serialize};

// =============================================================================
// TAGS
// =============================================================================

/// Tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
    pub category: String,
}

/// Address Tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressTag {
    pub address: String,
    pub tag_id: String,
    pub added_by: String,
    pub added_at: u64,
}

/// Contract Tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractTag {
    pub contract_address: String,
    pub tag_id: String,
    pub verified: bool,
}

/// Tag Set
pub struct TagSet {
    tags: std::collections::HashMap<String, Tag>,
}

impl TagSet {
    pub fn new() -> Self {
        Self {
            tags: std::collections::HashMap::new(),
        }
    }

    /// Add tag
    pub fn add(&mut self, tag: Tag) {
        self.tags.insert(tag.id.clone(), tag);
    }

    /// Get tag
    pub fn get(&self, id: &str) -> Option<&Tag> {
        self.tags.get(id)
    }

    /// List tags
    pub fn list(&self) -> Vec<&Tag> {
        self.tags.values().collect()
    }
}

impl Default for TagSet {
    fn default() -> Self {
        Self::new()
    }
}