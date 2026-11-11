//! Honeypot and risky-feature detection
//!
//! A honeypot is a token that lets you buy but not sell. That property lives in
//! the contract's *behaviour*, which cannot be read from its bytes alone: the
//! same opcodes appear in honest and malicious contracts alike.
//!
//! So this module does not claim to detect honeypots. It reports the
//! *capabilities* a contract's code contains, by walking real opcodes (skipping
//! `PUSH` data), and it names what each capability permits. That is a
//! reviewable signal, and every field says so.

use crate::validation::ContractValidator;

/// A capability found in a contract's code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    /// Opcode name, e.g. `SELFDESTRUCT`.
    pub opcode: &'static str,
    /// Opcode byte.
    pub byte: u8,
    /// What the capability permits.
    pub permits: &'static str,
    /// How much it should raise attention, 0-100. This is a triage weight, not
    /// a probability.
    pub weight: u8,
}

/// The result of a code scan.
#[derive(Debug, Clone)]
pub struct CodeScan {
    /// Capabilities present in the code, deduplicated.
    pub capabilities: Vec<Capability>,
    /// Sum of the capability weights, capped at 100.
    pub risk_score: u8,
    /// One line per capability, for display.
    pub notes: Vec<String>,
}

impl CodeScan {
    /// Whether the scan found anything worth a look.
    pub fn needs_review(&self) -> bool {
        !self.capabilities.is_empty()
    }
}

/// Scans contract code for capabilities that deserve review.
pub struct HoneypotDetector;

impl HoneypotDetector {
    /// Weight assigned to each watched opcode.
    ///
    /// `SELFDESTRUCT` is weighted highest because it can remove the code
    /// entirely; `DELEGATECALL` and `CALLCODE` next because they can run
    /// foreign code in this contract's storage; `CREATE2` lowest because
    /// deterministic deployment is common and often legitimate.
    fn weight_for(opcode: &str) -> u8 {
        match opcode {
            "SELFDESTRUCT" => 40,
            "DELEGATECALL" => 25,
            "CALLCODE" => 25,
            "CREATE2" => 15,
            _ => 0,
        }
    }

    /// Analyse contract code and report its capabilities.
    ///
    /// `bytecode` may be `0x`-prefixed. This reads opcodes only: it does not
    /// execute the contract, and it cannot tell a legitimate use of a
    /// capability from a malicious one.
    pub fn analyze(bytecode: &str) -> CodeScan {
        let features = ContractValidator::scan_features(bytecode);

        let capabilities: Vec<Capability> = features
            .iter()
            .map(|f| Capability {
                opcode: f.opcode,
                byte: f.byte,
                permits: f.note,
                weight: Self::weight_for(f.opcode),
            })
            .collect();

        let risk_score = capabilities
            .iter()
            .map(|c| c.weight as u16)
            .sum::<u16>()
            .min(100) as u8;

        let notes = capabilities
            .iter()
            .map(|c| format!("{} (0x{:02x}): {}", c.opcode, c.byte, c.permits))
            .collect();

        CodeScan {
            capabilities,
            risk_score,
            notes,
        }
    }

    /// Whether the code contains a specific capability.
    pub fn has_capability(bytecode: &str, opcode: &str) -> bool {
        ContractValidator::scan_features(bytecode)
            .iter()
            .any(|f| f.opcode == opcode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_code_has_no_capabilities() {
        // PUSH1 0x00 PUSH1 0x00 RETURN — no watched opcodes.
        let scan = HoneypotDetector::analyze("0x60006000f3");
        assert!(scan.capabilities.is_empty());
        assert_eq!(scan.risk_score, 0);
        assert!(!scan.needs_review());
    }

    #[test]
    fn test_selfdestruct_is_reported() {
        let scan = HoneypotDetector::analyze("0xff");
        assert_eq!(scan.capabilities.len(), 1);
        assert_eq!(scan.capabilities[0].opcode, "SELFDESTRUCT");
        assert_eq!(scan.risk_score, 40);
        assert!(scan.needs_review());
    }

    #[test]
    fn test_push_data_is_not_a_capability() {
        // PUSH1 0xff: the 0xff is data, so there is no SELFDESTRUCT.
        let scan = HoneypotDetector::analyze("0x60ff");
        assert!(scan.capabilities.is_empty());
    }

    #[test]
    fn test_combined_score_is_capped() {
        // DELEGATECALL + SELFDESTRUCT: 25 + 40 = 65.
        let scan = HoneypotDetector::analyze("0xf4ff");
        assert_eq!(scan.risk_score, 65);
        assert_eq!(scan.capabilities.len(), 2);
    }

    #[test]
    fn test_duplicate_opcodes_counted_once() {
        let scan = HoneypotDetector::analyze("0xf4f4f4");
        assert_eq!(scan.capabilities.len(), 1);
        assert_eq!(scan.risk_score, 25);
    }

    #[test]
    fn test_has_capability() {
        assert!(HoneypotDetector::has_capability("0xf4", "DELEGATECALL"));
        assert!(!HoneypotDetector::has_capability("0x60f4", "DELEGATECALL"));
    }
}
