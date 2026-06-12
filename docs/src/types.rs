//! Docs Types

use serde::{Deserialize, Serialize};

// =============================================================================
// DOCS
// =============================================================================

/// Document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub tags: Vec<String>,
    pub updated_at: u64,
}

/// API Reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiReference {
    pub endpoint: String,
    pub method: String,
    pub params: Vec<Param>,
    pub response: String,
    pub example: String,
}

/// Param
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub r#type: String,
    pub required: bool,
    pub description: String,
}

/// Documentation Service
pub struct Service {
    documents: std::collections::HashMap<String, Document>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            documents: std::collections::HashMap::new(),
        }
    }

    /// Add document
    pub fn add(&mut self, doc: Document) {
        self.documents.insert(doc.id.clone(), doc);
    }

    /// Get document
    pub fn get(&self, id: &str) -> Option<&Document> {
        self.documents.get(id)
    }

    /// List by category
    pub fn list_by_category(&self, category: &str) -> Vec<&Document> {
        self.documents
            .values()
            .filter(|d| d.category == category)
            .collect()
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}