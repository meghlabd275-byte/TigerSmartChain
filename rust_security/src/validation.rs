//! Input validation module
//!
//! Validates the shapes of the values the explorer accepts: hashes, block
//! numbers, quantities, and calldata. Validation is strict, so a value that
//! passes can be used without further guessing.

use crate::Result;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref HASH_REGEX: Regex = Regex::new(r"^0x[a-fA-F0-9]{64}$").unwrap();
    static ref HEX_REGEX: Regex = Regex::new(r"^0x[a-fA-F0-9]*$").unwrap();
}

/// Validates various input types
pub struct InputValidator;

impl InputValidator {
    /// Validate a 32-byte transaction hash.
    pub fn is_valid_tx_hash(hash: &str) -> bool {
        HASH_REGEX.is_match(hash)
    }

    /// Validate a decimal block number.
    ///
    /// Block `0` is the genesis block and is valid; only non-numeric input is
    /// rejected.
    pub fn is_valid_block_number(block: &str) -> bool {
        !block.is_empty() && block.chars().all(|c| c.is_ascii_digit())
    }

    /// Validate a hex string, with or without the `0x` prefix.
    ///
    /// An odd number of digits is rejected: a hex quantity is a whole number of
    /// bytes.
    pub fn is_valid_hex(data: &str) -> bool {
        if !HEX_REGEX.is_match(data) {
            return false;
        }
        (data.len() - 2).is_multiple_of(2)
    }

    /// Validate an unsigned 256-bit quantity.
    ///
    /// Accepts decimal or `0x`-prefixed hex, and rejects any value that would
    /// not fit in 32 bytes.
    pub fn is_valid_uint256(value: &str) -> bool {
        if value.is_empty() {
            return false;
        }

        if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
            if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return false;
            }
            // 64 hex digits is the most a 256-bit value can need; leading zeros
            // are allowed, so anything longer is still fine only if it is all
            // zeros beyond 64 digits.
            let trimmed = hex.trim_start_matches('0');
            return trimmed.len() <= 64;
        }

        if !value.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        // 2^256 - 1 is 78 decimal digits.
        let trimmed = value.trim_start_matches('0');
        if trimmed.len() < 78 {
            return true;
        }
        trimmed.len() == 78
            && trimmed
                <= "115792089237316195423570985008687907853269984665640564039457584007913129639935"
    }

    /// Validate contract bytecode.
    ///
    /// Bytecode is a whole number of bytes; `0x` alone is empty code, which is
    /// a valid (if useless) value.
    pub fn is_valid_bytecode(bytecode: &str) -> bool {
        Self::is_valid_hex(bytecode)
    }

    /// Validate ABI-encoded calldata: hex with a whole number of bytes.
    pub fn is_valid_abi_data(data: &str) -> bool {
        Self::is_valid_hex(data)
    }

    /// Sanitize a search query by keeping only characters that can appear in a
    /// hash, address, or block number.
    pub fn sanitize_search_query(query: &str) -> String {
        query
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == 'x' || *c == '-')
            .take(128)
            .collect::<String>()
            .trim()
            .to_string()
    }
}

/// A bytecode feature detected during a static scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BytecodeFeature {
    /// The opcode name, e.g. `DELEGATECALL`.
    pub opcode: &'static str,
    /// The opcode byte, e.g. `0xf4`.
    pub byte: u8,
    /// Why the feature deserves attention.
    pub note: &'static str,
}

/// Scans contract bytecode for features worth reviewing.
///
/// The scan is deliberately honest about what it can and cannot do. It walks
/// the deployed code as opcodes, skipping `PUSH` immediates, so a byte that
/// merely looks like an opcode inside pushed data is not reported. It does not
/// interpret control flow, so it reports *features present*, not *behaviour*:
/// the output is a starting point for review, never a verdict.
pub struct ContractValidator;

impl ContractValidator {
    /// Opcodes that change who runs code or destroy the contract.
    const WATCHED: [(u8, &'static str, &'static str); 4] = [
        (0xff, "SELFDESTRUCT", "can destroy the contract and move its balance"),
        (0xf4, "DELEGATECALL", "runs another contract's code in this contract's storage"),
        (0xf2, "CALLCODE", "legacy delegate-like call"),
        (0xf5, "CREATE2", "deploys to an address derived from code and salt"),
    ];

    /// Walk `bytecode` and report every watched opcode that actually executes.
    ///
    /// `bytecode` may be `0x`-prefixed. Non-hex input yields an empty result.
    pub fn scan_features(bytecode: &str) -> Vec<BytecodeFeature> {
        let hex = bytecode
            .strip_prefix("0x")
            .or_else(|| bytecode.strip_prefix("0X"))
            .unwrap_or(bytecode);

        if hex.len() % 2 != 0 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Vec::new();
        }

        let Ok(code) = hex::decode(hex) else {
            return Vec::new();
        };

        let mut found = Vec::new();
        let mut pc = 0usize;
        while pc < code.len() {
            let opcode = code[pc];
            pc += 1;

            // PUSH1..PUSH32 (0x60..0x7f) carry `n` immediate bytes that are
            // data, not code; step over them.
            if (0x60..=0x7f).contains(&opcode) {
                let n = (opcode - 0x5f) as usize;
                pc = pc.saturating_add(n);
                continue;
            }

            if let Some((byte, opcode_name, note)) =
                Self::WATCHED.iter().find(|(b, _, _)| *b == opcode)
            {
                let feature = BytecodeFeature {
                    opcode: opcode_name,
                    byte: *byte,
                    note,
                };
                if !found.contains(&feature) {
                    found.push(feature);
                }
            }
        }

        found
    }

    /// Whether the scan found any watched opcode.
    pub fn has_reviewable_features(bytecode: &str) -> bool {
        !Self::scan_features(bytecode).is_empty()
    }
}

/// Convenience wrapper matching the historical `Result`-returning shape.
impl ContractValidator {
    /// Validate that `bytecode` is well-formed hex.
    pub fn validate_bytecode(bytecode: &str) -> Result<()> {
        if Self::scan_features(bytecode).is_empty() && !InputValidator::is_valid_hex(bytecode) {
            return Err(crate::SecurityError::InvalidInput(
                "bytecode is not valid hex".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_hash_validation() {
        // Exactly 64 hex digits.
        assert!(InputValidator::is_valid_tx_hash(
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        ));
        assert!(!InputValidator::is_valid_tx_hash("not-a-hash"));
        // 63 hex digits: one short.
        assert!(!InputValidator::is_valid_tx_hash(
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcde"
        ));
    }

    #[test]
    fn test_block_number_accepts_genesis() {
        assert!(InputValidator::is_valid_block_number("0"));
        assert!(InputValidator::is_valid_block_number("12345"));
        assert!(!InputValidator::is_valid_block_number(""));
        assert!(!InputValidator::is_valid_block_number("-1"));
        assert!(!InputValidator::is_valid_block_number("0x10"));
    }

    #[test]
    fn test_hex_requires_whole_bytes() {
        assert!(InputValidator::is_valid_hex("0x"));
        assert!(InputValidator::is_valid_hex("0xabcd"));
        assert!(!InputValidator::is_valid_hex("0xabc"));
        assert!(!InputValidator::is_valid_hex("abcd"));
    }

    #[test]
    fn test_uint256_bounds() {
        assert!(InputValidator::is_valid_uint256("0"));
        assert!(InputValidator::is_valid_uint256("255"));
        assert!(InputValidator::is_valid_uint256("0xff"));
        // 2^256 - 1, the maximum.
        assert!(InputValidator::is_valid_uint256(
            "115792089237316195423570985008687907853269984665640564039457584007913129639935"
        ));
        // 2^256, one too large.
        assert!(!InputValidator::is_valid_uint256(
            "115792089237316195423570985008687907853269984665640564039457584007913129639936"
        ));
        assert!(!InputValidator::is_valid_uint256("12a3"));
    }

    #[test]
    fn test_scan_skips_push_data() {
        // PUSH1 0xff then STOP: the 0xff is data, not SELFDESTRUCT.
        let code = "0x60ff00";
        assert!(ContractValidator::scan_features(code).is_empty());

        // A bare SELFDESTRUCT does execute.
        let code = "0xff";
        let found = ContractValidator::scan_features(code);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].opcode, "SELFDESTRUCT");
    }

    #[test]
    fn test_scan_finds_delegatecall() {
        let code = "0x600060006000600060006000f4";
        let found = ContractValidator::scan_features(code);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].opcode, "DELEGATECALL");
    }

    #[test]
    fn test_scan_rejects_bad_hex() {
        assert!(ContractValidator::scan_features("0xzz").is_empty());
        assert!(ContractValidator::scan_features("0xabc").is_empty());
    }
}
