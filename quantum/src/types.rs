//! Quantum Types

use serde::{Deserialize, Serialize};

// =============================================================================
// POST-QUANTUM
// =============================================================================

/// Post-Quantum Key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    pub public: Vec<u8>,
    pub secret: Vec<u8>,
}

/// Post-Quantum Signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub sigma: Vec<u8>,
}

/// Quantum Engine
pub struct QuantumEngine {
    pub enabled: bool,
    pub algorithm: String,
}
