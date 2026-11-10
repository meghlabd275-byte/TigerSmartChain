//! Validator set and proposer rotation.

/// An ordered set of active validators.
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
        if !self.validators.contains(&validator) {
            self.validators.push(validator);
        }
    }

    /// Number of validators.
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    /// Get proposer
    ///
    /// Returns `None` for an empty set. An earlier version divided by the set
    /// length without checking, so calling this before any validator was
    /// registered panicked with a divide-by-zero.
    pub fn get_proposer(&self, block_number: u64) -> Option<&str> {
        if self.validators.is_empty() {
            return None;
        }
        let idx = (block_number as usize) % self.validators.len();
        self.validators.get(idx).map(|s| s.as_str())
    }
}

impl Default for ValidatorSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_set_has_no_proposer_and_does_not_panic() {
        let set = ValidatorSet::new();
        assert!(set.is_empty());
        assert_eq!(set.get_proposer(0), None);
        assert_eq!(set.get_proposer(12345), None);
    }

    #[test]
    fn proposers_rotate_in_order() {
        let mut set = ValidatorSet::new();
        set.add("0xa".to_string());
        set.add("0xb".to_string());
        assert_eq!(set.get_proposer(0), Some("0xa"));
        assert_eq!(set.get_proposer(1), Some("0xb"));
        assert_eq!(set.get_proposer(2), Some("0xa"));
    }

    #[test]
    fn adding_a_validator_twice_does_not_duplicate_it() {
        let mut set = ValidatorSet::new();
        set.add("0xa".to_string());
        set.add("0xa".to_string());
        assert_eq!(set.len(), 1);
    }
}
