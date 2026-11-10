//! Proof of Stake Authority (PoSA) Implementation
//!
//! TigerSmartChain uses PoSA to achieve high speed and low latency.
//! Validators are elected based on their stake and reputation.

use std::collections::HashMap;
use crate::types::Validator;

/// Epoch length in blocks
pub const DEFAULT_EPOCH_LENGTH: u64 = 200;
/// Minimum stake required to be a validator (in Wei)
pub const MIN_STAKE: u64 = 1_000_000_000_000_000_000; // 1 TGR

/// PoSA Consensus Engine
pub struct PoSA {
    /// Current chain ID
    pub chain_id: u64,
    /// Epoch length
    pub epoch_length: u64,
    /// Slot duration in seconds
    pub slot_duration: u64,
    /// Active validators for the current epoch
    validators: Vec<String>,
    /// All registered validators and their status
    validator_registry: HashMap<String, Validator>,
    /// Last epoch updated
    last_epoch: u64,
    /// Block rewards for current epoch
    pending_rewards: HashMap<String, u64>,
}

impl PoSA {
    /// Create a new PoSA instance
    pub fn new(chain_id: u64, epoch_length: u64, slot_duration: u64) -> Self {
        Self {
            chain_id,
            epoch_length,
            slot_duration,
            validators: Vec::new(),
            validator_registry: HashMap::new(),
            last_epoch: 0,
            pending_rewards: HashMap::new(),
        }
    }

    /// Register a new validator.
    ///
    /// Returns false when the stake is below the minimum, or when the address
    /// is already registered: overwriting an existing validator would silently
    /// discard its accumulated delegated stake.
    pub fn register_validator(&mut self, address: String, stake: u64) -> bool {
        if stake < MIN_STAKE {
            return false;
        }
        if self.validator_registry.contains_key(&address) {
            return false;
        }

        let validator = Validator {
            address: address.clone(),
            stake,
            delegated: 0,
            commission: 10, // 10% default commission
            active: false,
        };

        self.validator_registry.insert(address, validator);
        true
    }

    /// Update the validator set for a new epoch.
    ///
    /// Returns the rewards accrued during the epoch that just ended, so the
    /// caller can credit them to the real world state. The map is empty when
    /// no new epoch began.
    pub fn update_epoch(&mut self, block_number: u64) -> HashMap<String, u64> {
        let current_epoch = block_number / self.epoch_length;
        if current_epoch <= self.last_epoch && !self.validators.is_empty() {
            return HashMap::new();
        }

        // Settle the rewards accrued during the epoch that just ended.
        let settled = self.distribute_rewards();

        // Elect new validators based on total stake (stake + delegated)
        let mut candidates: Vec<_> = self.validator_registry.values()
            .filter(|v| v.stake >= MIN_STAKE)
            .collect();

        // Sort by total stake descending, then by address for deterministic tie-breaking
        candidates.sort_by(|a, b| {
            let stake_a = a.stake + a.delegated;
            let stake_b = b.stake + b.delegated;
            stake_b.cmp(&stake_a).then_with(|| a.address.cmp(&b.address))
        });

        // Take top 21 validators (standard for BSC-like chains)
        self.validators = candidates.iter()
            .take(21)
            .map(|v| v.address.clone())
            .collect();

        // Mark them as active in registry
        for v in self.validator_registry.values_mut() {
            v.active = self.validators.contains(&v.address);
        }

        self.last_epoch = current_epoch;
        settled
    }

    /// Get the proposer for a given block number and timestamp
    pub fn get_proposer(&self, block_number: u64) -> Option<String> {
        if self.validators.is_empty() {
            return None;
        }
        let index = (block_number as usize) % self.validators.len();
        Some(self.validators[index].clone())
    }

    /// Process block reward for the proposer
    pub fn record_block_reward(&mut self, proposer: String, reward: u64) {
        *self.pending_rewards.entry(proposer).or_insert(0) += reward;
    }

    /// Take the rewards accrued since the last epoch, leaving the ledger empty.
    ///
    /// The consensus engine does not hold account balances, so it cannot pay
    /// the rewards itself. Returning them lets the caller credit the real
    /// world state; discarding them (as an earlier version did by clearing the
    /// map) would silently destroy every block reward ever earned.
    pub fn take_pending_rewards(&mut self) -> HashMap<String, u64> {
        std::mem::take(&mut self.pending_rewards)
    }

    /// The rewards currently owed, without clearing them.
    pub fn pending_rewards(&self) -> &HashMap<String, u64> {
        &self.pending_rewards
    }

    /// Distribute rewards to validators.
    ///
    /// The rewards are handed to the caller through
    /// [`PoSA::take_pending_rewards`]; this method is called at an epoch
    /// boundary so the caller can settle them before the new epoch begins.
    fn distribute_rewards(&mut self) -> HashMap<String, u64> {
        self.take_pending_rewards()
    }

    /// Get current active validators
    pub fn active_validators(&self) -> &Vec<String> {
        &self.validators
    }

    /// Check if an address is an active validator
    pub fn is_validator(&self, address: &str) -> bool {
        self.validators.iter().any(|v| v == address)
    }

    /// Slash a validator for double signing or downtime
    pub fn slash_validator(&mut self, address: &str, penalty: u64) {
        if let Some(v) = self.validator_registry.get_mut(address) {
            v.stake = v.stake.saturating_sub(penalty);
            if v.stake < MIN_STAKE {
                v.active = false;
                // Remove from active validators if present
                self.validators.retain(|addr| addr != address);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_election() {
        let mut posa = PoSA::new(1, 10, 3);

        // Register some validators
        posa.register_validator("0x1".to_string(), MIN_STAKE * 2);
        posa.register_validator("0x2".to_string(), MIN_STAKE * 5);
        posa.register_validator("0x3".to_string(), MIN_STAKE);

        // Update epoch at block 0
        posa.update_epoch(0);

        assert_eq!(posa.active_validators().len(), 3);
        // Sorted by stake
        assert_eq!(posa.active_validators()[0], "0x2");
        assert_eq!(posa.active_validators()[1], "0x1");
        assert_eq!(posa.active_validators()[2], "0x3");
    }

    #[test]
    fn test_proposer_rotation() {
        let mut posa = PoSA::new(1, 10, 3);
        posa.register_validator("0x1".to_string(), MIN_STAKE);
        posa.register_validator("0x2".to_string(), MIN_STAKE);
        posa.update_epoch(0);

        assert_eq!(posa.get_proposer(0).unwrap(), "0x1");
        assert_eq!(posa.get_proposer(1).unwrap(), "0x2");
        assert_eq!(posa.get_proposer(2).unwrap(), "0x1");
    }

    #[test]
    fn epoch_change_settles_rewards_instead_of_discarding_them() {
        let mut posa = PoSA::new(1, 10, 3);
        posa.register_validator("0x1".to_string(), MIN_STAKE);
        posa.update_epoch(0);

        posa.record_block_reward("0x1".to_string(), 100);
        posa.record_block_reward("0x1".to_string(), 50);
        assert_eq!(posa.pending_rewards().get("0x1"), Some(&150));

        // Crossing into a new epoch must hand the rewards back, not drop them.
        let settled = posa.update_epoch(10);
        assert_eq!(settled.get("0x1"), Some(&150));
        // The ledger is empty afterwards.
        assert!(posa.pending_rewards().is_empty());
    }

    #[test]
    fn rewards_accumulate_within_an_epoch() {
        let mut posa = PoSA::new(1, 10, 3);
        posa.register_validator("0x1".to_string(), MIN_STAKE);
        posa.update_epoch(0);
        posa.record_block_reward("0x1".to_string(), 7);
        // Still in the same epoch: nothing is settled.
        assert!(posa.update_epoch(5).is_empty());
        assert_eq!(posa.pending_rewards().get("0x1"), Some(&7));
    }

    #[test]
    fn duplicate_registration_is_refused() {
        let mut posa = PoSA::new(1, 10, 3);
        assert!(posa.register_validator("0x1".to_string(), MIN_STAKE));
        assert!(!posa.register_validator("0x1".to_string(), MIN_STAKE * 10));
    }

    #[test]
    fn below_minimum_stake_is_refused() {
        let mut posa = PoSA::new(1, 10, 3);
        assert!(!posa.register_validator("0x1".to_string(), MIN_STAKE - 1));
    }

    #[test]
    fn only_the_top_staked_validators_are_elected() {
        let mut posa = PoSA::new(1, 10, 3);
        // Register 25 validators with increasing stake. Stakes stay well
        // below u64::MAX / 25 so the multiplication cannot overflow.
        for i in 0..25u64 {
            posa.register_validator(format!("0x{i}"), MIN_STAKE + i * 1_000_000);
        }
        posa.update_epoch(0);
        // The cap is 21, so the four lowest-staked are left out.
        assert_eq!(posa.active_validators().len(), 21);
        assert!(!posa.is_validator("0x0"));
        assert!(posa.is_validator("0x24"));
    }

    #[test]
    fn slashing_below_the_minimum_deactivates_the_validator() {
        let mut posa = PoSA::new(1, 10, 3);
        // Register with headroom above the minimum so a small penalty does
        // not immediately drop the validator below it.
        posa.register_validator("0x1".to_string(), MIN_STAKE * 2);
        posa.update_epoch(0);
        assert!(posa.is_validator("0x1"));

        // A penalty smaller than the headroom leaves the stake above the
        // minimum, so the validator stays active.
        posa.slash_validator("0x1", MIN_STAKE / 2);
        assert_eq!(posa.active_validators().len(), 1, "still above the minimum");
        assert!(posa.is_validator("0x1"));

        // Slashing past the minimum deactivates and removes it.
        posa.slash_validator("0x1", MIN_STAKE);
        assert!(!posa.is_validator("0x1"), "now below the minimum");
        assert!(posa.active_validators().is_empty());
    }

    #[test]
    fn a_validator_at_exactly_the_minimum_is_deactivated_by_any_penalty() {
        let mut posa = PoSA::new(1, 10, 3);
        posa.register_validator("0x1".to_string(), MIN_STAKE);
        posa.update_epoch(0);
        // The stake sits exactly on the threshold, so the smallest slash takes
        // it below and the validator must be removed.
        posa.slash_validator("0x1", 1);
        assert!(!posa.is_validator("0x1"));
    }

    #[test]
    fn an_empty_validator_set_has_no_proposer() {
        let posa = PoSA::new(1, 10, 3);
        assert_eq!(posa.get_proposer(0), None);
    }

    #[test]
    fn test_slashing() {
        let mut posa = PoSA::new(1, 10, 3);
        posa.register_validator("0x1".to_string(), MIN_STAKE);
        posa.update_epoch(0);

        assert!(posa.is_validator("0x1"));

        posa.slash_validator("0x1", MIN_STAKE / 2);
        // Still above 0 but below MIN_STAKE if we slash more
        posa.slash_validator("0x1", MIN_STAKE);

        assert!(!posa.is_validator("0x1"));
    }
}
