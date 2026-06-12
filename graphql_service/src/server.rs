//! GraphQL Server

use super::types::*;
use super::resolver::Resolver;

// =============================================================================
// SERVER
// =============================================================================

/// Server
pub struct Server {
    resolver: Resolver,
    port: u16,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Self {
            resolver: Resolver::new(),
            port,
        }
    }

    /// Execute query
    pub fn execute(&self, query: &Query) -> Result<String, String> {
        Ok(format!(r#"{{"data": {{}}}}"#))
    }

    /// Start
    pub fn start(&self) -> Result<(), String> {
        log::info!("GraphQL server listening on port {}", self.port);
        Ok(())
    }
}