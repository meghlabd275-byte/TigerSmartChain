//! Block and Staking Rewards

/// Configuration for block rewards
pub struct RewardConfig {
    pub block_reward: u64,
    pub uncle_reward: u64,
    pub fee_burn_ratio: u8, // Percentage of transaction fees to burn
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            block_reward: 5_000_000_000_000_000_000, // 5 TGR
            uncle_reward: 1_000_000_000_000_000_000, // 1 TGR
            fee_burn_ratio: 50,                       // 50% burn
        }
    }
}

/// Rewards Manager
pub struct RewardManager {
    config: RewardConfig,
}

impl RewardManager {
    pub fn new(config: RewardConfig) -> Self {
        Self { config }
    }

    /// Calculate total reward for a block
    pub fn calculate_block_reward(&self, tx_fees: u64) -> (u64, u64) {
        let burn_amount = (tx_fees * self.config.fee_burn_ratio as u64) / 100;
        let distributed_fees = tx_fees - burn_amount;
        let total_reward = self.config.block_reward + distributed_fees;

        (total_reward, burn_amount)
    }

    /// Calculate how a reward is split between validator and delegators
    pub fn split_reward(total_reward: u64, commission_percent: u8) -> (u64, u64) {
        let validator_cut = (total_reward * commission_percent as u64) / 100;
        let delegator_cut = total_reward - validator_cut;

        (validator_cut, delegator_cut)
    }
}
