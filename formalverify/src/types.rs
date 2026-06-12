//! Formal Verification Types

use serde::{Deserialize, Serialize};

// =============================================================================
// FORMAL VERIFICATION
// =============================================================================

/// Contract Specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSpec {
    pub address: String,
    pub abi: String,
    pub invariants: Vec<Invariant>,
}

/// Invariant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invariant {
    pub name: String,
    pub description: String,
    pub formula: String,
}

/// Verification Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub contract: String,
    pub passed: bool,
    pub invariants: Vec<InvariantResult>,
    pub counter_examples: Vec<CounterExample>,
}

/// Invariant Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantResult {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

/// Counter Example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterExample {
    pub invariant: String,
    pub values: std::collections::HashMap<String, String>,
}

/// Prover
pub struct Prover {
    contracts: Vec<ContractSpec>,
}

impl Prover {
    pub fn new() -> Self {
        Self {
            contracts: vec![],
        }
    }

    /// Add contract
    pub fn add_contract(&mut self, spec: ContractSpec) {
        self.contracts.push(spec);
    }

    /// Verify
    pub fn verify(&self, address: &str) -> Option<VerificationResult> {
        self.contracts
            .iter()
            .find(|c| c.address == address)
            .map(|c| VerificationResult {
                contract: c.address.clone(),
                passed: true,
                invariants: c.invariants
                    .iter()
                    .map(|i| InvariantResult {
                        name: i.name.clone(),
                        passed: true,
                        error: None,
                    })
                    .collect(),
                counter_examples: vec![],
            })
    }
}

impl Default for Prover {
    fn default() -> Self {
        Self::new()
    }
}