//! Telegram Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// TELEGRAM SERVICE
// =============================================================================

/// Bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bot {
    pub token: String,
    pub chat_id: String,
}

/// Message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub chat_id: String,
    pub text: String,
    pub parse_mode: String,
}

/// Telegram Service
pub struct Service {
    bots: std::collections::HashMap<String, Bot>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            bots: std::collections::HashMap::new(),
        }
    }

    /// Add bot
    pub fn add_bot(&mut self, name: String, bot: Bot) {
        self.bots.insert(name, bot);
    }

    /// Get bot
    pub fn get_bot(&self, name: &str) -> Option<&Bot> {
        self.bots.get(name)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}