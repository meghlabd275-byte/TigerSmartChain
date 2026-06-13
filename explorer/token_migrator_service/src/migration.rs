//! Migration Logic

use crate::types::{MigrationType, TokenMigration};

pub struct MigrationService;

impl MigrationService {
    /// Detect migration type
    pub fn detect_type(tx_data: &str) -> Option<MigrationType> {
        if tx_data.contains("swap") {
            Some(MigrationType::Swap)
        } else if tx_data.contains("bridge") {
            Some(MigrationType::Bridge)
        } else if tx_data.contains("migrate") {
            Some(MigrationType::Migrate)
        } else if tx_data.contains("upgrade") {
            Some(MigrationType::Upgrade)
        } else {
            None
        }
    }
    
    /// Calculate swap rate
    pub fn calculate_rate(from_amount: f64, to_amount: f64) -> String {
        if to_amount > 0.0 {
            (from_amount / to_amount).to_string()
        } else {
            "0".to_string()
        }
    }
}