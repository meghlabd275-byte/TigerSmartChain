//! Address blacklist.
//!
//! The previous version stored whatever string it was given as the key, so
//! `0xAbC…` and `0xabc…` were different entries and a caller could not know
//! whether a lookup had missed because the address was clean or because the
//! case differed. `add` also pushed the address onto the per-category list
//! every time, so a re-report produced a duplicate, and the count it reported
//! was of the map rather than of the categories.
//!
//! This version validates and lowercases on the way in, keeps the category
//! index consistent with the map, and supports removal.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A blacklist hit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlacklistMatch {
    pub address: String,
    pub category: BlacklistCategory,
    pub reason: String,
    pub reported_at: i64,
    pub source: String,
    pub risk_score: f64,
}

/// Why an address is blacklisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlacklistCategory {
    Scam,
    Phishing,
    Hack,
    Mixer,
    Exploit,
    Ransomware,
    Fake,
    Malicious,
    Sanctioned,
    Unknown,
}

impl BlacklistCategory {
    /// Every category, for iteration.
    pub fn all() -> [BlacklistCategory; 10] {
        [
            BlacklistCategory::Scam,
            BlacklistCategory::Phishing,
            BlacklistCategory::Hack,
            BlacklistCategory::Mixer,
            BlacklistCategory::Exploit,
            BlacklistCategory::Ransomware,
            BlacklistCategory::Fake,
            BlacklistCategory::Malicious,
            BlacklistCategory::Sanctioned,
            BlacklistCategory::Unknown,
        ]
    }
}

impl std::fmt::Display for BlacklistCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            BlacklistCategory::Scam => "scam",
            BlacklistCategory::Phishing => "phishing",
            BlacklistCategory::Hack => "hack",
            BlacklistCategory::Mixer => "mixer",
            BlacklistCategory::Exploit => "exploit",
            BlacklistCategory::Ransomware => "ransomware",
            BlacklistCategory::Fake => "fake",
            BlacklistCategory::Malicious => "malicious",
            BlacklistCategory::Sanctioned => "sanctioned",
            BlacklistCategory::Unknown => "unknown",
        };
        f.write_str(name)
    }
}

/// A blacklist entry.
#[derive(Debug, Clone)]
struct BlacklistEntry {
    category: BlacklistCategory,
    reason: String,
    reported_at: i64,
    source: String,
    risk_score: f64,
}

/// The blacklist.
pub struct Blacklist {
    entries: HashMap<String, BlacklistEntry>,
    by_category: HashMap<BlacklistCategory, HashSet<String>>,
}

impl Blacklist {
    /// An empty blacklist.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            by_category: HashMap::new(),
        }
    }

    /// Look up an address.
    ///
    /// Returns `None` for an address that is not listed, and also for one that
    /// is not a valid address: an unparseable string cannot be a hit, and
    /// saying so is better than a silent miss.
    pub fn check(&self, address: &str) -> Option<BlacklistMatch> {
        let address = crate::scam::normalize_address(address)?;
        let entry = self.entries.get(&address)?;

        Some(BlacklistMatch {
            address,
            category: entry.category,
            reason: entry.reason.clone(),
            reported_at: entry.reported_at,
            source: entry.source.clone(),
            risk_score: entry.risk_score,
        })
    }

    /// Whether an address is listed.
    pub fn contains(&self, address: &str) -> bool {
        crate::scam::normalize_address(address)
            .map(|address| self.entries.contains_key(&address))
            .unwrap_or(false)
    }

    /// Add an address.
    ///
    /// Returns `true` when the address was newly listed and `false` when it was
    /// already present or is not a valid address. Re-reporting an address
    /// updates its entry rather than duplicating it.
    pub fn add(
        &mut self,
        address: &str,
        category: BlacklistCategory,
        reason: &str,
        risk_score: f64,
    ) -> bool {
        let Some(address) = crate::scam::normalize_address(address) else {
            return false;
        };

        let risk_score = risk_score.clamp(0.0, 1.0);
        let is_new = !self.entries.contains_key(&address);

        // A category change has to move the address between the index buckets,
        // or the index stops agreeing with the map.
        if let Some(previous) = self.entries.get(&address) {
            if previous.category != category {
                if let Some(bucket) = self.by_category.get_mut(&previous.category) {
                    bucket.remove(&address);
                }
            }
        }

        self.entries.insert(
            address.clone(),
            BlacklistEntry {
                category,
                reason: reason.to_string(),
                reported_at: now_unix(),
                source: "manual".to_string(),
                risk_score,
            },
        );

        self.by_category
            .entry(category)
            .or_default()
            .insert(address);

        is_new
    }

    /// Add an address with an explicit source and timestamp.
    pub fn add_with_source(
        &mut self,
        address: &str,
        category: BlacklistCategory,
        reason: &str,
        risk_score: f64,
        source: &str,
        reported_at: i64,
    ) -> bool {
        let added = self.add(address, category, reason, risk_score);
        if added || self.contains(address) {
            if let Some(normalized) = crate::scam::normalize_address(address) {
                if let Some(entry) = self.entries.get_mut(&normalized) {
                    entry.source = source.to_string();
                    entry.reported_at = reported_at;
                }
            }
        }
        added
    }

    /// Remove an address. Returns whether it was listed.
    pub fn remove(&mut self, address: &str) -> bool {
        let Some(address) = crate::scam::normalize_address(address) else {
            return false;
        };

        let Some(entry) = self.entries.remove(&address) else {
            return false;
        };

        if let Some(bucket) = self.by_category.get_mut(&entry.category) {
            bucket.remove(&address);
        }

        true
    }

    /// The addresses in a category.
    pub fn by_category(&self, category: BlacklistCategory) -> Vec<String> {
        let mut addresses: Vec<String> = self
            .by_category
            .get(&category)
            .map(|bucket| bucket.iter().cloned().collect())
            .unwrap_or_default();
        addresses.sort();
        addresses
    }

    /// Number of listed addresses.
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// The highest risk score among the given addresses, for scoring a
    /// transaction whose participants are known.
    pub fn max_risk_among(&self, addresses: &[String]) -> Option<f64> {
        addresses
            .iter()
            .filter_map(|address| self.check(address))
            .map(|entry| entry.risk_score)
            .reduce(f64::max)
    }
}

impl Default for Blacklist {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0x1111111111111111111111111111111111111111";
    const B: &str = "0x2222222222222222222222222222222222222222";

    #[test]
    fn an_added_address_is_found() {
        let mut blacklist = Blacklist::new();
        assert!(blacklist.add(A, BlacklistCategory::Scam, "rug pull", 0.9));

        let found = blacklist.check(A).unwrap();
        assert_eq!(found.category, BlacklistCategory::Scam);
        assert_eq!(found.reason, "rug pull");
        assert_eq!(found.risk_score, 0.9);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let mut blacklist = Blacklist::new();
        blacklist.add(A, BlacklistCategory::Scam, "x", 0.9);

        // The old implementation keyed on the raw string, so this missed.
        assert!(blacklist.check("0x1111111111111111111111111111111111111111").is_some());
        assert!(blacklist.contains("0X1111111111111111111111111111111111111111"));
    }

    #[test]
    fn an_invalid_address_is_never_a_hit() {
        let mut blacklist = Blacklist::new();
        assert!(!blacklist.add("not-an-address", BlacklistCategory::Scam, "x", 0.9));

        assert!(blacklist.check("not-an-address").is_none());
        assert!(blacklist.check("0xdead").is_none());
        assert!(!blacklist.contains("not-an-address"));
        assert_eq!(blacklist.count(), 0);
    }

    #[test]
    fn re_adding_does_not_duplicate() {
        let mut blacklist = Blacklist::new();
        assert!(blacklist.add(A, BlacklistCategory::Scam, "first", 0.5));
        assert!(!blacklist.add(A, BlacklistCategory::Scam, "second", 0.6));

        assert_eq!(blacklist.count(), 1);
        assert_eq!(blacklist.by_category(BlacklistCategory::Scam).len(), 1);
        // The latest report wins.
        assert_eq!(blacklist.check(A).unwrap().reason, "second");
    }

    #[test]
    fn changing_category_moves_the_address_between_buckets() {
        let mut blacklist = Blacklist::new();
        blacklist.add(A, BlacklistCategory::Scam, "x", 0.5);
        blacklist.add(A, BlacklistCategory::Phishing, "y", 0.5);

        assert!(blacklist.by_category(BlacklistCategory::Scam).is_empty());
        assert_eq!(blacklist.by_category(BlacklistCategory::Phishing), vec![A.to_string()]);
        assert_eq!(blacklist.count(), 1);
    }

    #[test]
    fn removing_clears_both_the_map_and_the_index() {
        let mut blacklist = Blacklist::new();
        blacklist.add(A, BlacklistCategory::Scam, "x", 0.5);

        assert!(blacklist.remove(A));
        assert!(!blacklist.remove(A));
        assert_eq!(blacklist.count(), 0);
        assert!(blacklist.by_category(BlacklistCategory::Scam).is_empty());
        assert!(blacklist.check(A).is_none());
    }

    #[test]
    fn the_category_index_always_agrees_with_the_map() {
        let mut blacklist = Blacklist::new();
        blacklist.add(A, BlacklistCategory::Hack, "x", 0.5);
        blacklist.add(B, BlacklistCategory::Hack, "y", 0.5);
        blacklist.remove(A);

        let indexed: usize = BlacklistCategory::all()
            .iter()
            .map(|category| blacklist.by_category(*category).len())
            .sum();

        assert_eq!(indexed, blacklist.count());
    }

    #[test]
    fn risk_scores_are_clamped_to_the_unit_interval() {
        let mut blacklist = Blacklist::new();
        blacklist.add(A, BlacklistCategory::Scam, "x", 5.0);
        blacklist.add(B, BlacklistCategory::Scam, "y", -3.0);

        assert_eq!(blacklist.check(A).unwrap().risk_score, 1.0);
        assert_eq!(blacklist.check(B).unwrap().risk_score, 0.0);
    }

    #[test]
    fn the_highest_risk_among_a_set_is_reported() {
        let mut blacklist = Blacklist::new();
        blacklist.add(A, BlacklistCategory::Scam, "x", 0.4);
        blacklist.add(B, BlacklistCategory::Hack, "y", 0.95);

        let addresses = vec![A.to_string(), B.to_string()];
        assert_eq!(blacklist.max_risk_among(&addresses), Some(0.95));
        assert_eq!(blacklist.max_risk_among(&[]), None);
    }

    #[test]
    fn an_explicit_source_is_recorded() {
        let mut blacklist = Blacklist::new();
        blacklist.add_with_source(
            A,
            BlacklistCategory::Sanctioned,
            "OFAC",
            1.0,
            "ofac-sdn",
            1_700_000_000,
        );

        let found = blacklist.check(A).unwrap();
        assert_eq!(found.source, "ofac-sdn");
        assert_eq!(found.reported_at, 1_700_000_000);
    }

    #[test]
    fn an_empty_blacklist_contains_nothing() {
        let blacklist = Blacklist::new();
        assert_eq!(blacklist.count(), 0);
        assert!(blacklist.check(A).is_none());
    }
}
