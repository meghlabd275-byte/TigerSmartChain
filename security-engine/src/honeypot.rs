//! Honeypot detection.
//!
//! The previous version's `analyze_bytecode` lowercased the bytecode and tested
//! `bc.contains("sell") && !bc.contains("transfer")`. Bytecode is hex, so
//! neither `"sell"` nor `"transfer"` can ever appear in it — the function
//! always returned `None` for real input, and its only other path was a
//! hardcoded (empty) set of "known honeypots".
//!
//! Detection here is built on the one thing that actually distinguishes a
//! honeypot: whether a transfer *out* succeeds. [`HoneypotDetector::simulate_sell`]
//! takes the results of real `eth_call` executions of a buy and a sell against
//! the contract, and reports the honeypot when the sell reverts, charges an
//! impossible tax, or only succeeds for the owner. The capability analysis is a
//! static pre-filter that raises suspicion without pretending to be a
//! simulation.

use crate::scam::{decode_hex, normalize_address, Capability};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A honeypot finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HoneypotMatch {
    pub contract: String,
    pub honeypot_type: HoneypotType,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub sell_tax: Option<f64>,
    pub transfer_tax: Option<f64>,
}

/// Kinds of honeypot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoneypotType {
    /// The sell call reverts.
    CannotSell,
    /// The sell succeeds but returns far less than expected.
    HighSellTax,
    /// Transfers only work for an allowlisted sender.
    BlockedTransfer,
    /// Selling is disabled until a timestamp passes.
    TimeLock,
    /// Only the owner can transfer.
    OnlyOwner,
    /// Detected by capability analysis rather than simulation.
    SuspiciousCapabilities,
    Unknown,
}

/// The outcome of one `eth_call`.
#[derive(Debug, Clone, PartialEq)]
pub struct CallOutcome {
    /// Whether the call reverted.
    pub reverted: bool,
    /// Tokens received, as a decimal string.
    pub tokens_received: Option<String>,
    /// The revert reason, when the node returned one.
    pub revert_reason: Option<String>,
}

impl CallOutcome {
    /// A call that succeeded and returned `amount` tokens.
    pub fn success(amount: &str) -> Self {
        Self {
            reverted: false,
            tokens_received: Some(amount.to_string()),
            revert_reason: None,
        }
    }

    /// A call that reverted.
    pub fn revert(reason: &str) -> Self {
        Self {
            reverted: true,
            tokens_received: None,
            revert_reason: Some(reason.to_string()),
        }
    }
}

/// The results of the buy/sell simulation.
#[derive(Debug, Clone)]
pub struct SellSimulation {
    /// The result of buying.
    pub buy: CallOutcome,
    /// The result of selling the tokens just bought.
    pub sell: CallOutcome,
    /// The same sell attempted from an account that is not the owner.
    pub sell_from_non_owner: Option<CallOutcome>,
}

/// The honeypot detector.
pub struct HoneypotDetector {
    known_honeypots: HashSet<String>,
}

impl HoneypotDetector {
    /// A detector with an empty known-honeypot list.
    ///
    /// Nothing is hardcoded: [`HoneypotDetector::report`] is how the list is
    /// populated from confirmed findings.
    pub fn new() -> Self {
        Self {
            known_honeypots: HashSet::new(),
        }
    }

    /// Record a contract as a confirmed honeypot.
    pub fn report(&mut self, address: &str) {
        if let Some(address) = normalize_address(address) {
            self.known_honeypots.insert(address);
        }
    }

    /// Number of confirmed honeypots.
    pub fn known_count(&self) -> usize {
        self.known_honeypots.len()
    }

    /// Check the confirmed list.
    pub fn check_contract(&self, address: &str) -> Option<HoneypotMatch> {
        let address = normalize_address(address)?;
        if !self.known_honeypots.contains(&address) {
            return None;
        }

        Some(HoneypotMatch {
            contract: address,
            honeypot_type: HoneypotType::CannotSell,
            confidence: 1.0,
            evidence: vec!["Confirmed honeypot: sells revert for ordinary holders".to_string()],
            sell_tax: None,
            transfer_tax: None,
        })
    }

    /// Judge a buy/sell simulation.
    ///
    /// A tax above 90% is treated as a honeypot in practice: the holder cannot
    /// exit at a meaningful price. The threshold is high on purpose, because a
    /// legitimate token can charge several percent.
    pub fn simulate_sell(&self, address: &str, simulation: &SellSimulation) -> Option<HoneypotMatch> {
        let contract = normalize_address(address).unwrap_or_else(|| address.to_string());

        if simulation.buy.reverted {
            // Nothing to conclude: the buy itself failed, so this is not a
            // honeypot, it is a contract that cannot be bought either.
            return None;
        }

        if simulation.sell.reverted {
            let reason = simulation
                .sell
                .revert_reason
                .clone()
                .unwrap_or_else(|| "sell reverted".to_string());

            // A revert that names the owner is a different, more specific shape.
            let honeypot_type = if reason.to_lowercase().contains("owner") {
                HoneypotType::OnlyOwner
            } else if reason.to_lowercase().contains("time") || reason.to_lowercase().contains("lock")
            {
                HoneypotType::TimeLock
            } else {
                HoneypotType::CannotSell
            };

            return Some(HoneypotMatch {
                contract,
                honeypot_type,
                confidence: 0.95,
                evidence: vec![format!("Selling the token reverts: {reason}")],
                sell_tax: None,
                transfer_tax: None,
            });
        }

        // The owner can sell but nobody else can: an allowlist honeypot.
        if let Some(non_owner) = &simulation.sell_from_non_owner {
            if non_owner.reverted && !simulation.sell.reverted {
                return Some(HoneypotMatch {
                    contract,
                    honeypot_type: HoneypotType::BlockedTransfer,
                    confidence: 0.9,
                    evidence: vec![
                        "The deployer can sell but another account cannot".to_string(),
                    ],
                    sell_tax: None,
                    transfer_tax: None,
                });
            }
        }

        // Compare what was bought with what the sell returned.
        let bought = parse_decimal(simulation.buy.tokens_received.as_deref()?)?;
        let returned = parse_decimal(simulation.sell.tokens_received.as_deref()?)?;

        if bought == 0.0 {
            return None;
        }

        let tax = (1.0 - (returned / bought)).clamp(0.0, 1.0);

        if tax > 0.9 {
            return Some(HoneypotMatch {
                contract,
                honeypot_type: HoneypotType::HighSellTax,
                confidence: 0.9,
                evidence: vec![format!(
                    "Selling returns {:.1}% of the amount bought, so the position cannot be exited",
                    (returned / bought) * 100.0
                )],
                sell_tax: Some(tax),
                transfer_tax: None,
            });
        }

        None
    }

    /// Static capability analysis, used before a simulation is available.
    ///
    /// This only raises suspicion; it does not prove anything, and the
    /// confidence reflects that.
    pub fn analyze_bytecode(&self, bytecode: &str) -> Option<HoneypotMatch> {
        let code = decode_hex(bytecode)?;
        if code.is_empty() {
            return None;
        }

        let detector = crate::scam::ScamDetector::new();
        let capabilities = detector.detect_capabilities(&code);

        let has = |cap: Capability| capabilities.contains(&cap);

        if has(Capability::Blacklist) && has(Capability::Pause) {
            return Some(HoneypotMatch {
                contract: "unknown".to_string(),
                honeypot_type: HoneypotType::SuspiciousCapabilities,
                confidence: 0.4,
                evidence: vec![
                    "Contract can blacklist addresses and pause transfers; simulate a sell to confirm"
                        .to_string(),
                ],
                sell_tax: None,
                transfer_tax: None,
            });
        }

        None
    }
}

impl Default for HoneypotDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a decimal integer or floating-point amount.
///
/// Values on chain are integers, but a node may hand back a decimal string; both
/// are accepted, and anything else is rejected rather than silently becoming
/// zero.
fn parse_decimal(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    trimmed.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confirmed_honeypot_is_detected() {
        let mut detector = HoneypotDetector::new();
        detector.report("0x4444444444444444444444444444444444444444");

        let found = detector
            .check_contract("0x4444444444444444444444444444444444444444")
            .unwrap();
        assert_eq!(found.confidence, 1.0);
        assert_eq!(found.honeypot_type, HoneypotType::CannotSell);
    }

    #[test]
    fn an_unreported_contract_is_not_detected() {
        let detector = HoneypotDetector::new();
        assert!(detector
            .check_contract("0x4444444444444444444444444444444444444444")
            .is_none());
    }

    #[test]
    fn a_reverting_sell_is_a_honeypot() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            sell: CallOutcome::revert("execution reverted"),
            sell_from_non_owner: None,
        };

        let found = detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .unwrap();
        assert_eq!(found.honeypot_type, HoneypotType::CannotSell);
        assert!(found.confidence >= 0.9);
    }

    #[test]
    fn a_high_sell_tax_is_a_honeypot() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            // Only 5% comes back.
            sell: CallOutcome::success("50"),
            sell_from_non_owner: None,
        };

        let found = detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .unwrap();
        assert_eq!(found.honeypot_type, HoneypotType::HighSellTax);
        assert!(found.sell_tax.unwrap() > 0.9);
    }

    #[test]
    fn an_ordinary_tax_is_not_a_honeypot() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            // A 10% tax is high but tradeable.
            sell: CallOutcome::success("900"),
            sell_from_non_owner: None,
        };

        assert!(detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .is_none());
    }

    #[test]
    fn a_clean_round_trip_is_not_a_honeypot() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            sell: CallOutcome::success("1000"),
            sell_from_non_owner: Some(CallOutcome::success("1000")),
        };

        assert!(detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .is_none());
    }

    #[test]
    fn the_owner_can_sell_but_others_cannot_is_an_allowlist_honeypot() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            sell: CallOutcome::success("1000"),
            sell_from_non_owner: Some(CallOutcome::revert("not allowed")),
        };

        let found = detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .unwrap();
        assert_eq!(found.honeypot_type, HoneypotType::BlockedTransfer);
    }

    #[test]
    fn a_failed_buy_is_not_a_honeypot_finding() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::revert("no liquidity"),
            sell: CallOutcome::revert("no balance"),
            sell_from_non_owner: None,
        };

        // Nothing can be concluded about selling when the buy never happened.
        assert!(detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .is_none());
    }

    #[test]
    fn a_revert_that_names_the_owner_is_reported_as_only_owner() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            sell: CallOutcome::revert("only owner can transfer"),
            sell_from_non_owner: None,
        };

        let found = detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .unwrap();
        assert_eq!(found.honeypot_type, HoneypotType::OnlyOwner);
    }

    #[test]
    fn a_revert_that_names_a_lock_is_reported_as_a_timelock() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            sell: CallOutcome::revert("trading not open: time lock active"),
            sell_from_non_owner: None,
        };

        let found = detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .unwrap();
        assert_eq!(found.honeypot_type, HoneypotType::TimeLock);
    }

    #[test]
    fn a_missing_token_amount_is_not_treated_as_zero() {
        let detector = HoneypotDetector::new();
        let simulation = SellSimulation {
            buy: CallOutcome {
                reverted: false,
                tokens_received: None,
                revert_reason: None,
            },
            sell: CallOutcome::success("1000"),
            sell_from_non_owner: None,
        };

        assert!(detector
            .simulate_sell("0x4444444444444444444444444444444444444444", &simulation)
            .is_none());
    }

    #[test]
    fn bytecode_is_not_matched_on_text_substrings() {
        let detector = HoneypotDetector::new();

        // The old detector tested for the literal words "sell" and "transfer"
        // inside a hex string. Those cannot occur, so neither can this.
        assert!(detector.analyze_bytecode("0x73656c6c").is_none());
        assert!(detector.analyze_bytecode("not hex").is_none());
    }

    #[test]
    fn blacklist_and_pause_in_bytecode_raises_suspicion() {
        let detector = HoneypotDetector::new();

        let mut code: Vec<u8> = Vec::new();
        for selector in [[0xf9, 0xf9, 0x2d, 0x0d], [0x84, 0x56, 0xcb, 0x59]] {
            code.push(0x63);
            code.extend_from_slice(&selector);
            code.push(0x14);
            code.push(0x61);
            code.extend_from_slice(&[0x00, 0x2a]);
            code.push(0x57);
        }

        let found = detector.analyze_bytecode(&hex::encode(code)).unwrap();
        assert_eq!(found.honeypot_type, HoneypotType::SuspiciousCapabilities);
        // The confidence is deliberately low: this is a pre-filter.
        assert!(found.confidence < 0.5);
    }

    #[test]
    fn a_plain_contract_raises_no_suspicion() {
        let detector = HoneypotDetector::new();
        let mut code: Vec<u8> = Vec::new();
        code.push(0x63);
        code.extend_from_slice(&[0x70, 0xa0, 0x82, 0x31]); // balanceOf
        code.push(0x14);
        code.push(0x61);
        code.extend_from_slice(&[0x00, 0x2a]);
        code.push(0x57);

        assert!(detector.analyze_bytecode(&hex::encode(code)).is_none());
    }

    #[test]
    fn decimal_parsing_rejects_junk() {
        assert_eq!(parse_decimal("1000"), Some(1000.0));
        assert_eq!(parse_decimal(" 12.5 "), Some(12.5));
        assert_eq!(parse_decimal(""), None);
        assert_eq!(parse_decimal("abc"), None);
        assert_eq!(parse_decimal("inf"), None);
    }
}
