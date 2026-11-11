//! State for the filter methods: `eth_newFilter`, `eth_newBlockFilter`,
//! `eth_newPendingTransactionFilter`, and their `getFilterChanges` /
//! `getFilterLogs` / `uninstallFilter` counterparts.
//!
//! A filter is a server-side cursor. The client installs one and then polls it
//! for whatever has appeared since the previous poll, so the node has to
//! remember where each filter has read up to. Nothing is pushed to the client;
//! `getFilterChanges` is a pull, which is what makes these methods work over
//! plain HTTP where the node has no way to reach back to the client.

use std::collections::{HashMap, HashSet};

use crate::rpc_server::TopicFilter;

/// The range and matching rules of a log filter.
#[derive(Debug, Clone)]
pub struct LogFilter {
    /// First block the filter covers. `None` means the range was given as
    /// `latest` at install time, so it follows the head.
    pub from_block: Option<u64>,
    /// Last block the filter covers. `None` means open-ended.
    pub to_block: Option<u64>,
    /// Restrict to these addresses; `None` matches any.
    pub address: Option<Vec<[u8; 20]>>,
    /// Restrict by topic position; `None` matches any.
    pub topics: Option<TopicFilter>,
    /// A filter pinned to one block by hash.
    pub block_hash: Option<String>,
}

/// What a filter tracks.
#[derive(Debug, Clone)]
pub enum FilterKind {
    /// Logs matching a range and set of rules.
    Logs(LogFilter),
    /// Block hashes, from the height the filter had already reached.
    Blocks,
    /// Transaction hashes seen in the pool.
    PendingTransactions,
}

/// A filter and its cursor.
#[derive(Debug, Clone)]
pub struct Filter {
    pub kind: FilterKind,
    /// Highest block already returned to the client. A log filter starts one
    /// below its `from_block` so the first poll includes the starting block,
    /// which is what clients expect.
    pub last_block: u64,
    /// Transaction hashes already reported. A set rather than a count, so the
    /// pool's internal ordering cannot cause a transaction to be reported
    /// twice or missed.
    pub seen: HashSet<String>,
}

/// The node's installed filters.
pub struct FilterRegistry {
    filters: HashMap<u64, Filter>,
    next_id: u64,
    /// Bound on how many filters one node will hold. Each filter costs a small
    /// amount of memory and is never removed by a client that disappears, so
    /// the count has to be capped.
    max_filters: usize,
}

impl FilterRegistry {
    /// Default cap. Generous for a real client, small enough that a client
    /// installing filters in a loop cannot exhaust the node.
    pub const DEFAULT_MAX_FILTERS: usize = 1024;

    pub fn new() -> Self {
        Self {
            filters: HashMap::new(),
            next_id: 1,
            max_filters: Self::DEFAULT_MAX_FILTERS,
        }
    }

    /// Install a filter and return its id.
    pub fn install(&mut self, kind: FilterKind, head: u64) -> Result<u64, String> {
        if self.filters.len() >= self.max_filters {
            return Err(format!(
                "too many filters installed (limit {})",
                self.max_filters
            ));
        }

        let last_block = match &kind {
            // Start one below the first block of interest so that block is
            // included in the first `getFilterChanges`.
            FilterKind::Logs(filter) => match &filter.block_hash {
                // A single-block filter reports that block once and then has
                // nothing more to report.
                Some(_) => head,
                None => filter.from_block.unwrap_or(head).saturating_sub(1),
            },
            FilterKind::Blocks => head,
            FilterKind::PendingTransactions => head,
        };

        let id = self.next_id;
        self.next_id += 1;
        self.filters.insert(
            id,
            Filter {
                kind,
                last_block,
                seen: HashSet::new(),
            },
        );
        Ok(id)
    }

    /// The filter with this id, if it is installed.
    pub fn get(&self, id: u64) -> Option<&Filter> {
        self.filters.get(&id)
    }

    /// The filter with this id, for updating its cursor.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Filter> {
        self.filters.get_mut(&id)
    }

    /// Remove a filter. Returns whether it existed.
    pub fn uninstall(&mut self, id: u64) -> bool {
        self.filters.remove(&id).is_some()
    }

    /// How many filters are installed.
    pub fn len(&self) -> usize {
        self.filters.len()
    }
}

impl Default for FilterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_filter(from: Option<u64>, to: Option<u64>) -> LogFilter {
        LogFilter {
            from_block: from,
            to_block: to,
            address: None,
            topics: None,
            block_hash: None,
        }
    }

    #[test]
    fn a_log_filter_includes_its_starting_block_on_the_first_poll() {
        let mut registry = FilterRegistry::new();
        let id = registry
            .install(FilterKind::Logs(log_filter(Some(10), None)), 50)
            .unwrap();

        // The cursor starts one below `fromBlock` so the first poll covers it.
        assert_eq!(registry.get(id).unwrap().last_block, 9);
    }

    #[test]
    fn a_block_filter_starts_at_the_current_head() {
        let mut registry = FilterRegistry::new();
        let id = registry.install(FilterKind::Blocks, 50).unwrap();

        // Starting at the head means the first poll reports only blocks
        // produced after the filter was installed, not the whole chain.
        assert_eq!(registry.get(id).unwrap().last_block, 50);
    }

    #[test]
    fn filters_are_removed_when_uninstalled() {
        let mut registry = FilterRegistry::new();
        let id = registry.install(FilterKind::Blocks, 0).unwrap();
        assert!(registry.get(id).is_some());

        assert!(registry.uninstall(id));
        assert!(registry.get(id).is_none());
        // A second uninstall reports that there was nothing to remove.
        assert!(!registry.uninstall(id));
    }

    #[test]
    fn filter_ids_are_not_reused_after_uninstall() {
        let mut registry = FilterRegistry::new();
        let first = registry.install(FilterKind::Blocks, 0).unwrap();
        registry.uninstall(first);
        let second = registry.install(FilterKind::Blocks, 0).unwrap();

        // Reusing an id would let a stale client poll a filter it no longer
        // owns, so ids only move forward.
        assert_ne!(first, second);
    }

    #[test]
    fn installing_filters_is_capped() {
        let mut registry = FilterRegistry::new();
        for _ in 0..FilterRegistry::DEFAULT_MAX_FILTERS {
            registry.install(FilterKind::Blocks, 0).unwrap();
        }
        // A client that installs filters in a loop must not be able to grow
        // the node's memory without bound.
        assert!(registry.install(FilterKind::Blocks, 0).is_err());
        assert_eq!(registry.len(), FilterRegistry::DEFAULT_MAX_FILTERS);
    }
}
