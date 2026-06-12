//! Validator

/// Validator Set
pub struct ValidatorSet {
    validators: Vec<String>,
}

impl ValidatorSet {
    pub fn new() -> Self {
        Self {
            validators: vec![],
        }
    }

    /// Add validator
    pub fn add(&mut self, validator: String) {
        self.validators.push(validator);
    }

    /// Get proposer
    pub fn get_proposer(&self, block_number: u64) -> Option<&str> {
        let idx = (block_number as usize) % self.validators.len();
        self.validators.get(idx).map(|s| s.as_str())
    }
}

impl Default for ValidatorSet {
    fn default() -> Self {
        Self::new()
    }
}