//! Proof of Stake Authority

/// PoSA
pub struct PoSA {
    validators: Vec<String>,
    min_stake: u64,
}

impl PoSA {
    pub fn new(min_stake: u64) -> Self {
        Self {
            validators: vec![],
            min_stake,
        }
    }

    /// Add validator
    pub fn add_validator(&mut self, validator: String) {
        self.validators.push(validator);
    }

    /// Get validators
    pub fn validators(&self) -> &Vec<String> {
        &self.validators
    }
}