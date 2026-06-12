//! Mobile Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// MOBILE SERVICE
// =============================================================================

/// Mobile App
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileApp {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
}

/// Push Notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotification {
    pub device_token: String,
    pub title: String,
    pub body: String,
    pub data: std::collections::HashMap<String, String>,
}

/// Mobile Service
pub struct Service {
    apps: std::collections::HashMap<String, MobileApp>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            apps: std::collections::HashMap::new(),
        }
    }

    /// Add app
    pub fn add_app(&mut self, app: MobileApp) {
        self.apps.insert(app.id.clone(), app);
    }

    /// Get app
    pub fn get_app(&self, id: &str) -> Option<&MobileApp> {
        self.apps.get(id)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}