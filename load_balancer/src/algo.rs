//! Backend selection algorithms.
//!
//! Each algorithm returns a backend that is available (healthy and not
//! draining), or `None` when none is. Selection state that has to persist
//! between calls — the round-robin cursor, the smooth weighted round-robin
//! currents — lives in [`SelectionState`].

use crate::{Backend, LoadBalancingAlgorithm};
use std::collections::HashMap;

/// State carried between selections.
#[derive(Debug, Default)]
pub struct SelectionState {
    /// Cursor for plain round-robin.
    pub cursor: usize,
    /// Current weight per backend, for smooth weighted round-robin.
    pub currents: HashMap<String, i64>,
    /// Seed for the random algorithm.
    pub rng: u64,
}

impl SelectionState {
    /// A deterministic pseudo-random number.
    ///
    /// This is xorshift64*, which is fast and has no dependency. The random
    /// algorithm spreads load; it is not a security boundary, and nothing that
    /// needs unpredictability uses it.
    pub fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        if x == 0 {
            // Any non-zero seed works; derive one from the clock on first use.
            x = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15);
            x |= 1;
        }
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Select a backend.
pub fn select(
    algorithm: LoadBalancingAlgorithm,
    backends: &HashMap<String, Backend>,
    order: &[String],
    client_ip: Option<&str>,
    state: &mut SelectionState,
) -> Option<Backend> {
    // The candidate set is the available backends in insertion order.
    let available: Vec<&Backend> = order
        .iter()
        .filter_map(|url| backends.get(url))
        .filter(|b| b.is_available())
        .collect();

    if available.is_empty() {
        return None;
    }

    match algorithm {
        LoadBalancingAlgorithm::RoundRobin => round_robin(&available, state),
        LoadBalancingAlgorithm::LeastConnections => least_connections(&available),
        LoadBalancingAlgorithm::WeightedRoundRobin => weighted_round_robin(&available, state),
        LoadBalancingAlgorithm::WeightedLeastConnections => weighted_least_connections(&available),
        LoadBalancingAlgorithm::IpHash => {
            ip_hash(client_ip, &available).or_else(|| round_robin(&available, state))
        }
        LoadBalancingAlgorithm::Random => random(&available, state),
        LoadBalancingAlgorithm::HealthScore => health_score(&available),
    }
}

/// Plain round-robin over the available set.
fn round_robin(available: &[&Backend], state: &mut SelectionState) -> Option<Backend> {
    let index = state.cursor % available.len();
    state.cursor = state.cursor.wrapping_add(1);
    available.get(index).map(|b| (*b).clone())
}

/// Fewest in-flight requests.
fn least_connections(available: &[&Backend]) -> Option<Backend> {
    available
        .iter()
        .min_by_key(|b| b.active_connections)
        .map(|b| (*b).clone())
}

/// Smooth weighted round-robin.
///
/// Each candidate's current weight is increased by its configured weight, the
/// highest current wins, and that winner has the total weight subtracted. Over
/// a full cycle each backend is chosen in proportion to its weight, and the
/// choices are spread out rather than clustered.
fn weighted_round_robin(available: &[&Backend], state: &mut SelectionState) -> Option<Backend> {
    let total: i64 = available.iter().map(|b| b.weight as i64).sum();
    if total <= 0 {
        return None;
    }

    let mut best_index = 0usize;
    let mut best_current = i64::MIN;

    for (index, backend) in available.iter().enumerate() {
        let current = state.currents.entry(backend.url.clone()).or_insert(0);
        *current += backend.weight as i64;

        if *current > best_current {
            best_current = *current;
            best_index = index;
        }
    }

    let winner = available[best_index];
    if let Some(current) = state.currents.get_mut(&winner.url) {
        *current -= total;
    }

    Some(winner.clone())
}

/// Least in-flight requests per unit of weight.
fn weighted_least_connections(available: &[&Backend]) -> Option<Backend> {
    available
        .iter()
        .min_by(|a, b| {
            let left = a.active_connections as f64 / a.weight.max(1) as f64;
            let right = b.active_connections as f64 / b.weight.max(1) as f64;
            left.partial_cmp(&right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|b| (*b).clone())
}

/// Stable hashing of the client address.
///
/// FNV-1a, so that addresses which differ only in byte order land on different
/// backends. The previous version summed the bytes, which made every
/// permutation of an address collide.
fn ip_hash(client_ip: Option<&str>, available: &[&Backend]) -> Option<Backend> {
    let ip = client_ip?;
    if ip.is_empty() {
        return None;
    }

    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for byte in ip.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    let index = (hash as usize) % available.len();
    available.get(index).map(|b| (*b).clone())
}

/// Uniformly random choice.
fn random(available: &[&Backend], state: &mut SelectionState) -> Option<Backend> {
    let index = (state.next_random() as usize) % available.len();
    available.get(index).map(|b| (*b).clone())
}

/// Highest health score.
fn health_score(available: &[&Backend]) -> Option<Backend> {
    available
        .iter()
        .max_by(|a, b| {
            a.health_score()
                .partial_cmp(&b.health_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|b| (*b).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backends(entries: &[(&str, u32, u64)]) -> (HashMap<String, Backend>, Vec<String>) {
        let mut map = HashMap::new();
        let mut order = Vec::new();
        for (url, weight, connections) in entries {
            let mut backend = Backend::with_weight(url, *weight);
            backend.active_connections = *connections;
            map.insert(url.to_string(), backend);
            order.push(url.to_string());
        }
        (map, order)
    }

    #[test]
    fn round_robin_cycles_through_every_backend_in_order() {
        let (map, order) = backends(&[("a", 100, 0), ("b", 100, 0), ("c", 100, 0)]);
        let mut state = SelectionState::default();

        let picked: Vec<String> = (0..6)
            .map(|_| {
                select(
                    LoadBalancingAlgorithm::RoundRobin,
                    &map,
                    &order,
                    None,
                    &mut state,
                )
                .unwrap()
                .url
            })
            .collect();

        assert_eq!(picked, vec!["a", "b", "c", "a", "b", "c"]);
    }

    #[test]
    fn round_robin_does_not_use_the_clock() {
        // Two calls in the same instant must land on different backends; the
        // old implementation used `timestamp % len` and could not do this.
        let (map, order) = backends(&[("a", 100, 0), ("b", 100, 0)]);
        let mut state = SelectionState::default();

        let first = select(LoadBalancingAlgorithm::RoundRobin, &map, &order, None, &mut state)
            .unwrap()
            .url;
        let second = select(LoadBalancingAlgorithm::RoundRobin, &map, &order, None, &mut state)
            .unwrap()
            .url;

        assert_ne!(first, second);
    }

    #[test]
    fn unavailable_backends_are_never_selected() {
        let (mut map, order) = backends(&[("a", 100, 0), ("b", 100, 0)]);
        map.get_mut("a").unwrap().is_healthy = false;
        map.get_mut("b").unwrap().is_draining = true;

        let mut state = SelectionState::default();
        assert!(select(
            LoadBalancingAlgorithm::RoundRobin,
            &map,
            &order,
            None,
            &mut state
        )
        .is_none());
    }

    #[test]
    fn least_connections_picks_the_idlest_backend() {
        let (map, order) = backends(&[("a", 100, 10), ("b", 100, 2), ("c", 100, 7)]);
        let mut state = SelectionState::default();

        let chosen = select(
            LoadBalancingAlgorithm::LeastConnections,
            &map,
            &order,
            None,
            &mut state,
        )
        .unwrap();
        assert_eq!(chosen.url, "b");
    }

    #[test]
    fn weighted_round_robin_honours_the_weights() {
        // Weights 5:1 over six picks must give five to "a" and one to "b".
        let (map, order) = backends(&[("a", 5, 0), ("b", 1, 0)]);
        let mut state = SelectionState::default();

        let mut counts: HashMap<String, usize> = HashMap::new();
        for _ in 0..6 {
            let chosen = select(
                LoadBalancingAlgorithm::WeightedRoundRobin,
                &map,
                &order,
                None,
                &mut state,
            )
            .unwrap();
            *counts.entry(chosen.url).or_default() += 1;
        }

        assert_eq!(counts.get("a"), Some(&5));
        assert_eq!(counts.get("b"), Some(&1));
    }

    #[test]
    fn weighted_round_robin_spreads_rather_than_clusters() {
        let (map, order) = backends(&[("a", 5, 0), ("b", 1, 0)]);
        let mut state = SelectionState::default();

        let picks: Vec<String> = (0..6)
            .map(|_| {
                select(
                    LoadBalancingAlgorithm::WeightedRoundRobin,
                    &map,
                    &order,
                    None,
                    &mut state,
                )
                .unwrap()
                .url
            })
            .collect();

        // Smooth weighted round-robin interleaves: the light backend is served
        // within the first cycle rather than after five consecutive picks of
        // the heavy one.
        assert_eq!(picks[..4].iter().filter(|u| *u == "b").count(), 1);
    }

    #[test]
    fn weighted_least_connections_accounts_for_weight() {
        // "a" has more raw connections but far more weight, so per unit of
        // weight it is the idler of the two.
        let (map, order) = backends(&[("a", 10, 5), ("b", 1, 2)]);
        let mut state = SelectionState::default();

        let chosen = select(
            LoadBalancingAlgorithm::WeightedLeastConnections,
            &map,
            &order,
            None,
            &mut state,
        )
        .unwrap();
        assert_eq!(chosen.url, "a");
    }

    #[test]
    fn ip_hash_is_stable_for_one_address() {
        let (map, order) = backends(&[("a", 100, 0), ("b", 100, 0), ("c", 100, 0)]);
        let mut state = SelectionState::default();

        let first = select(
            LoadBalancingAlgorithm::IpHash,
            &map,
            &order,
            Some("203.0.113.7"),
            &mut state,
        )
        .unwrap()
        .url;

        for _ in 0..10 {
            let again = select(
                LoadBalancingAlgorithm::IpHash,
                &map,
                &order,
                Some("203.0.113.7"),
                &mut state,
            )
            .unwrap()
            .url;
            assert_eq!(again, first);
        }
    }

    #[test]
    fn ip_hash_separates_permutations_of_an_address() {
        // The old sum-of-bytes hash mapped these two to the same backend.
        let (map, order) = backends(&[("a", 100, 0), ("b", 100, 0), ("c", 100, 0)]);
        let mut state = SelectionState::default();

        let left = select(
            LoadBalancingAlgorithm::IpHash,
            &map,
            &order,
            Some("12"),
            &mut state,
        )
        .unwrap()
        .url;
        let right = select(
            LoadBalancingAlgorithm::IpHash,
            &map,
            &order,
            Some("21"),
            &mut state,
        )
        .unwrap()
        .url;

        assert_ne!(left, right);
    }

    #[test]
    fn ip_hash_without_an_address_falls_back_to_round_robin() {
        let (map, order) = backends(&[("a", 100, 0), ("b", 100, 0)]);
        let mut state = SelectionState::default();

        let first = select(LoadBalancingAlgorithm::IpHash, &map, &order, None, &mut state).unwrap();
        let second = select(LoadBalancingAlgorithm::IpHash, &map, &order, None, &mut state).unwrap();

        assert_ne!(first.url, second.url);
    }

    #[test]
    fn random_can_reach_every_backend() {
        let (map, order) = backends(&[("a", 100, 0), ("b", 100, 0), ("c", 100, 0)]);
        let mut state = SelectionState::default();

        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let chosen = select(
                LoadBalancingAlgorithm::Random,
                &map,
                &order,
                None,
                &mut state,
            )
            .unwrap();
            seen.insert(chosen.url);
        }

        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn health_score_prefers_the_faster_backend() {
        let (mut map, order) = backends(&[("a", 100, 0), ("b", 100, 0)]);
        map.get_mut("a").unwrap().avg_latency_ms = 50;
        map.get_mut("b").unwrap().avg_latency_ms = 900;

        let mut state = SelectionState::default();
        let chosen = select(
            LoadBalancingAlgorithm::HealthScore,
            &map,
            &order,
            None,
            &mut state,
        )
        .unwrap();

        assert_eq!(chosen.url, "a");
    }

    #[test]
    fn health_score_penalises_failures() {
        let (mut map, order) = backends(&[("a", 100, 0), ("b", 100, 0)]);
        // "a" is fast but fails half its requests; "b" is slow but reliable.
        {
            let a = map.get_mut("a").unwrap();
            a.avg_latency_ms = 10;
            a.total_requests = 100;
            a.failed_requests = 50;
        }
        map.get_mut("b").unwrap().avg_latency_ms = 300;

        let mut state = SelectionState::default();
        let chosen = select(
            LoadBalancingAlgorithm::HealthScore,
            &map,
            &order,
            None,
            &mut state,
        )
        .unwrap();

        assert_eq!(chosen.url, "b");
    }

    #[test]
    fn a_draining_backend_is_excluded_but_a_healthy_one_is_not() {
        let (mut map, order) = backends(&[("a", 100, 0), ("b", 100, 0)]);
        map.get_mut("a").unwrap().is_draining = true;

        let mut state = SelectionState::default();
        for _ in 0..5 {
            let chosen = select(
                LoadBalancingAlgorithm::RoundRobin,
                &map,
                &order,
                None,
                &mut state,
            )
            .unwrap();
            assert_eq!(chosen.url, "b");
        }
    }

    #[test]
    fn the_random_generator_is_deterministic_for_a_given_seed() {
        let mut a = SelectionState {
            rng: 12345,
            ..Default::default()
        };
        let mut b = SelectionState {
            rng: 12345,
            ..Default::default()
        };

        for _ in 0..20 {
            assert_eq!(a.next_random(), b.next_random());
        }
    }

    #[test]
    fn the_random_generator_never_sticks_at_zero() {
        let mut state = SelectionState::default();
        for _ in 0..50 {
            assert_ne!(state.next_random(), 0);
        }
    }
}
