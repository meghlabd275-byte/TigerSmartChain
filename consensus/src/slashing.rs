//! Slashing implementation for PoSA

use crate::posa::PoSA;

/// Reasons for slashing a validator
pub enum SlashingReason {
    /// Validator signed two different blocks for the same slot
    DoubleSigning,
    /// Validator was offline for too many slots
    Downtime,
    /// Malicious behavior detected by other validators
    MaliciousBehavior,
}

/// Slashing Engine
pub struct SlashingManager;

impl SlashingManager {
    /// Calculate penalty based on the reason
    pub fn calculate_penalty(reason: SlashingReason, total_stake: u64) -> u64 {
        match reason {
            SlashingReason::DoubleSigning => total_stake / 10, // 10% penalty
            SlashingReason::Downtime => total_stake / 100,      // 1% penalty
            SlashingReason::MaliciousBehavior => total_stake / 5, // 20% penalty
        }
    }

    /// Apply slashing to a validator in the PoSA engine
    pub fn slash(posa: &mut PoSA, validator_addr: &str, reason: SlashingReason, total_stake: u64) {
        let penalty = Self::calculate_penalty(reason, total_stake);
        posa.slash_validator(validator_addr, penalty);
    }
}
