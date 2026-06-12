//! Prometheus

use super::types::*;

// =============================================================================
// PROMETHEUS
// =============================================================================

/// Registry
pub struct Registry {
    counters: std::collections::HashMap<String, Counter>,
    gauges: std::collections::HashMap<String, Gauge>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            counters: std::collections::HashMap::new(),
            gauges: std::collections::HashMap::new(),
        }
    }

    pub fn counter(&mut self, name: &str) -> &Counter {
        self.counters.entry(name.to_string()).or_insert_with(Counter::new)
    }

    pub fn gauge(&mut self, name: &str) -> &Gauge {
        self.gauges.entry(name.to_string()).or_insert_with(Gauge::new)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}