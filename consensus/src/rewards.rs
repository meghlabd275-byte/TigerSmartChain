//! Rewards

/// Rewards
pub struct Rewards {
    block_reward: u64,
    uncle_reward: u64,
}

impl Rewards {
    pub fn new(block_reward: u64, uncle_reward: u64) -> Self {
        Self {
            block_reward,
            uncle_reward,
        }
    }

    /// Get block reward
    pub fn block_reward(&self) -> u64 {
        self.block_reward
    }

    /// Get uncle reward
    pub fn uncle_reward(&self) -> u64 {
        self.uncle_reward
    }
}