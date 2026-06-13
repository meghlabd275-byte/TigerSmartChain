//! Heartbeat Management
//! Connection health monitoring

use crate::config::Config;
use crate::connection::ConnectionManager;

pub struct Heartbeat {
    config: Config,
}

impl Heartbeat {
    /// Create new heartbeat
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    /// Start heartbeat task
    pub async fn start(&self, manager: ConnectionManager) {
        let interval = self.config.heartbeat_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(interval)
            );
            
            loop {
                interval.tick().await;
                // Check connections and remove stale ones
                let connections = manager.get_connections();
                for conn in connections {
                    if !conn.is_active() {
                        manager.remove_connection(conn.id());
                    }
                }
            }
        });
    }
}

impl Clone for Heartbeat {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}