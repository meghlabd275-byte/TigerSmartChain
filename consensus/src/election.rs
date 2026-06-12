//! Election

/// Election
pub struct Election {
    pub block_number: u64,
    pub candidates: Vec<String>,
    pub votes: std::collections::HashMap<String, u64>,
}

impl Election {
    pub fn new(block_number: u64) -> Self {
        Self {
            block_number,
            candidates: vec![],
            votes: std::collections::HashMap::new(),
        }
    }

    /// Add candidate
    pub fn add_candidate(&mut self, candidate: String) {
        self.candidates.push(candidate);
    }

    /// Vote
    pub fn vote(&mut self, candidate: String, amount: u64) {
        *self.votes.entry(candidate).or_insert(0) += amount;
    }

    /// Get winner
    pub fn winner(&self) -> Option<&str> {
        self.votes
            .iter()
            .max_by_key(|(_, v)| *v)
            .map(|(k, _)| k.as_str())
    }
}