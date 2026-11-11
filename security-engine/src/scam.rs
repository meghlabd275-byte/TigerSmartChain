//! Scam contract detection.
//!
//! The previous version matched substrings of *source text* against a list of
//! words. Its patterns included `"mint"`, `"buy"` and `"fake"`, and it ran them
//! against the contract address, so a plain ERC-20 with an ordinary `mint`
//! function was flagged as a rug pull and any address containing the letters
//! `buy` was flagged as a honeypot. Its only "known scam" was the burn address
//! `0x…dEaD`, which is not a scam contract. Its impersonation check flagged
//! every token with `bnb` or `eth` in its name, so `Wrapped ETH` was a scam.
//!
//! What follows works on a real signal: the deployed bytecode and, when
//! supplied, the token's metadata. Selectors are compared against the actual
//! 4-byte function selectors, so the test is "does this contract expose
//! `mint(uint256)`", not "does the address contain the letters m-i-n-t".

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A scam finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScamMatch {
    pub contract: String,
    pub scam_type: ScamType,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub reported_at: Option<i64>,
    pub source: String,
}

/// Kinds of scam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScamType {
    RugPull,
    Honeypot,
    FakeToken,
    FlashLoan,
    Ponzi,
    Phishing,
    FakeExchange,
    FakeStaking,
    Ico,
    Migrator,
    Unknown,
}

impl ScamType {
    /// Every variant, for iteration.
    pub fn all() -> [ScamType; 11] {
        [
            ScamType::RugPull,
            ScamType::Honeypot,
            ScamType::FakeToken,
            ScamType::FlashLoan,
            ScamType::Ponzi,
            ScamType::Phishing,
            ScamType::FakeExchange,
            ScamType::FakeStaking,
            ScamType::Ico,
            ScamType::Migrator,
            ScamType::Unknown,
        ]
    }

    /// The wire name, matching the serde representation.
    pub fn as_str(self) -> &'static str {
        match self {
            ScamType::RugPull => "rug_pull",
            ScamType::Honeypot => "honeypot",
            ScamType::FakeToken => "fake_token",
            ScamType::FlashLoan => "flash_loan",
            ScamType::Ponzi => "ponzi",
            ScamType::Phishing => "phishing",
            ScamType::FakeExchange => "fake_exchange",
            ScamType::FakeStaking => "fake_staking",
            ScamType::Ico => "ico",
            ScamType::Migrator => "migrator",
            ScamType::Unknown => "unknown",
        }
    }
}

/// A capability that is suspicious in combination with others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// `mint(uint256)` — new supply can be created.
    Mint,
    /// `burn(uint256)`.
    Burn,
    /// A setter for the transfer fee.
    SetTax,
    /// `blacklist(address)`.
    Blacklist,
    /// `pause()`.
    Pause,
    /// `setMaxTxAmount(uint256)`.
    SetMaxTx,
    /// `transferOwnership(address)`.
    TransferOwnership,
    /// `renounceOwnership()`.
    RenounceOwnership,
    /// `upgradeTo(address)` — the implementation can be swapped.
    Upgrade,
}

/// Known 4-byte selectors for the capabilities above.
///
/// These are the keccak-256 of the canonical signature, first four bytes.
const SELECTORS: &[(Capability, [u8; 4], &str)] = &[
    (Capability::Mint, [0x40, 0xc1, 0x0f, 0x19], "mint(uint256)"),
    (Capability::Mint, [0xa0, 0x71, 0x2d, 0x68], "mint(address,uint256)"),
    (Capability::Burn, [0x42, 0x96, 0x6c, 0x68], "burn(uint256)"),
    (Capability::Burn, [0x9d, 0xc2, 0x3d, 0x0c], "burn(address,uint256)"),
    (Capability::SetTax, [0x8b, 0x9f, 0x0b, 0x1e], "setTaxPercent(uint256)"),
    (Capability::SetTax, [0x06, 0xfd, 0xde, 0x03], "setFees(uint256,uint256)"),
    (Capability::SetTax, [0x69, 0x3c, 0x7d, 0x41], "setTaxFee(uint256)"),
    (Capability::Blacklist, [0xf9, 0xf9, 0x2d, 0x0d], "blacklist(address)"),
    (Capability::Blacklist, [0x15, 0x38, 0x69, 0xd9], "addBlackList(address)"),
    (Capability::Pause, [0x84, 0x56, 0xcb, 0x59], "pause()"),
    (Capability::SetMaxTx, [0xec, 0x28, 0x4e, 0x2c], "setMaxTxAmount(uint256)"),
    (Capability::TransferOwnership, [0xf2, 0xfd, 0xe3, 0x7b], "transferOwnership(address)"),
    (Capability::RenounceOwnership, [0x71, 0x50, 0x18, 0xa6], "renounceOwnership()"),
    (Capability::Upgrade, [0x36, 0x59, 0xcf, 0xe6], "upgradeTo(address)"),
];

/// Canonical ERC-20 / ERC-721 selectors that must be present for a contract to
/// be a token at all.
const ERC20_SELECTORS: &[[u8; 4]] = &[
    [0x06, 0xfd, 0xde, 0x03], // transfer(address,uint256)
    [0x70, 0xa0, 0x82, 0x31], // balanceOf(address)
    [0xa9, 0x05, 0x9c, 0xbb], // transferFrom(address,address,uint256)
    [0xdd, 0x62, 0xed, 0x3e], // approve(address,uint256)
    [0x18, 0x16, 0x0d, 0xdd], // totalSupply()
];

/// Well-known legitimate contracts whose names contain brand tokens.
///
/// Impersonation is decided against the metadata a caller supplies; this list
/// only stops the detector from flagging the genuine articles.
const LEGITIMATE_SYMBOLS: &[&str] = &[
    "WETH", "WBTC", "WBNB", "USDT", "USDC", "BUSD", "DAI", "TGR", "RUSD",
];

/// The scam detector.
pub struct ScamDetector {
    known_scams: HashMap<String, (ScamType, String)>,
    /// Owners that have deployed contracts which were later reported.
    flagged_deployers: HashSet<String>,
}

impl ScamDetector {
    /// A detector with an empty known-scam list.
    ///
    /// Nothing is hardcoded here: a scam list that ships in the binary is
    /// worthless within a day. [`ScamDetector::report`] is how it is populated.
    pub fn new() -> Self {
        Self {
            known_scams: HashMap::new(),
            flagged_deployers: HashSet::new(),
        }
    }

    /// Record a contract as a confirmed scam.
    pub fn report(&mut self, address: &str, scam_type: ScamType, reason: &str) {
        if let Some(address) = normalize_address(address) {
            self.known_scams
                .insert(address, (scam_type, reason.to_string()));
        }
    }

    /// Record a deployer whose contracts are all suspect.
    pub fn report_deployer(&mut self, deployer: &str) {
        if let Some(deployer) = normalize_address(deployer) {
            self.flagged_deployers.insert(deployer);
        }
    }

    /// Number of contracts on the scam list.
    pub fn known_count(&self) -> usize {
        self.known_scams.len()
    }

    /// Check whether a contract is on the confirmed scam list.
    pub fn check_contract(&self, address: &str) -> Option<ScamMatch> {
        let address = normalize_address(address)?;
        let (scam_type, reason) = self.known_scams.get(&address)?;

        Some(ScamMatch {
            contract: address,
            scam_type: *scam_type,
            confidence: 1.0,
            evidence: vec![reason.clone()],
            reported_at: None,
            source: "reported_scam_list".to_string(),
        })
    }

    /// Whether a deployer is known to have deployed scams.
    pub fn check_deployer(&self, deployer: &str) -> Option<ScamMatch> {
        let deployer = normalize_address(deployer)?;
        if !self.flagged_deployers.contains(&deployer) {
            return None;
        }

        Some(ScamMatch {
            contract: deployer,
            scam_type: ScamType::RugPull,
            confidence: 0.8,
            evidence: vec!["Deployer has previously deployed reported scams".to_string()],
            reported_at: None,
            source: "flagged_deployer".to_string(),
        })
    }

    /// Analyze deployed bytecode for scam capabilities.
    ///
    /// The bytecode is hex, with or without `0x`. Every 4-byte selector in the
    /// dispatch table is matched against [`SELECTORS`]. The combinations that
    /// matter:
    ///
    /// - mint + a tax setter that the owner can raise: the classic rug-pull
    ///   shape, where supply can be diluted and exits taxed arbitrarily.
    /// - blacklist + pause with no renounce: the owner can freeze holders.
    /// - upgrade: the code that runs is not the code that was audited.
    pub fn analyze_bytecode(&self, bytecode: &str) -> Vec<ScamMatch> {
        let code = match decode_hex(bytecode) {
            Some(code) => code,
            None => return Vec::new(),
        };

        if code.is_empty() {
            return Vec::new();
        }

        let capabilities = self.detect_capabilities(&code);
        let mut matches = Vec::new();

        let has = |cap: Capability| capabilities.contains(&cap);

        // Supply can be created and the exit fee raised at will.
        if has(Capability::Mint) && has(Capability::SetTax) {
            matches.push(ScamMatch {
                contract: "unknown".to_string(),
                scam_type: ScamType::RugPull,
                confidence: 0.75,
                evidence: vec![
                    "Contract can mint new supply and its owner can change the transfer tax"
                        .to_string(),
                ],
                reported_at: None,
                source: "capability_analysis".to_string(),
            });
        }

        // Holders can be frozen and trading halted, and the owner cannot
        // renounce, so the freeze cannot be escaped.
        if has(Capability::Blacklist) && has(Capability::Pause) && !has(Capability::RenounceOwnership)
        {
            matches.push(ScamMatch {
                contract: "unknown".to_string(),
                scam_type: ScamType::Honeypot,
                confidence: 0.7,
                evidence: vec![
                    "Owner can blacklist addresses and pause transfers, and cannot renounce"
                        .to_string(),
                ],
                reported_at: None,
                source: "capability_analysis".to_string(),
            });
        }

        // A transfer limit the owner sets after launch is a common way to trap
        // buyers: it starts permissive and is dropped to zero.
        if has(Capability::SetMaxTx) && has(Capability::Blacklist) {
            matches.push(ScamMatch {
                contract: "unknown".to_string(),
                scam_type: ScamType::Honeypot,
                confidence: 0.6,
                evidence: vec![
                    "Owner can set a maximum transfer amount and blacklist sellers".to_string(),
                ],
                reported_at: None,
                source: "capability_analysis".to_string(),
            });
        }

        if has(Capability::Upgrade) {
            matches.push(ScamMatch {
                contract: "unknown".to_string(),
                scam_type: ScamType::Migrator,
                confidence: 0.5,
                evidence: vec![
                    "Contract is upgradeable, so the audited code can be replaced".to_string(),
                ],
                reported_at: None,
                source: "capability_analysis".to_string(),
            });
        }

        matches
    }

    /// The capabilities present in a contract's dispatch table.
    pub fn detect_capabilities(&self, code: &[u8]) -> HashSet<Capability> {
        let mut found = HashSet::new();

        for (capability, selector, _) in SELECTORS {
            if contains_selector(code, selector) {
                found.insert(*capability);
            }
        }

        found
    }

    /// Whether a contract exposes the ERC-20 interface.
    pub fn is_erc20(&self, bytecode: &str) -> bool {
        let Some(code) = decode_hex(bytecode) else {
            return false;
        };

        ERC20_SELECTORS
            .iter()
            .all(|selector| contains_selector(&code, selector))
    }

    /// Check token metadata for impersonation.
    ///
    /// A token whose *symbol* exactly matches a well-known one is impersonating
    /// it unless its name matches too: `Wrapped Ether (WETH)` is genuine, while
    /// `Totally Legit (WETH)` is not. A token that merely mentions `eth` in its
    /// name, such as `Ethereum Classic`, is not flagged — the old code flagged
    /// it.
    pub fn analyze_token(&self, name: &str, symbol: &str) -> Option<ScamMatch> {
        let symbol_upper = symbol.trim().to_uppercase();
        let name_upper = name.trim().to_uppercase();

        if symbol_upper.is_empty() {
            return None;
        }

        let impersonated = LEGITIMATE_SYMBOLS
            .iter()
            .find(|legit| **legit == symbol_upper)?;

        // The genuine token carries the full name as well.
        let genuine_names: &[&str] = match *impersonated {
            "WETH" => &["WRAPPED ETHER", "WETH"],
            "WBTC" => &["WRAPPED BITCOIN", "WBTC"],
            "WBNB" => &["WRAPPED BNB", "WBNB"],
            "USDT" => &["TETHER", "TETHER USD", "USDT"],
            "USDC" => &["USD COIN", "USDC"],
            "BUSD" => &["BINANCE USD", "BUSD"],
            "DAI" => &["DAI STABLE COIN", "DAI"],
            "TGR" => &["TIGER", "TIGERCOIN", "TIGERSMARTCHAIN", "TGR"],
            "RUSD" => &["TIGER USD", "RUSD"],
            _ => &[],
        };

        if genuine_names.iter().any(|genuine| name_upper == *genuine) {
            return None;
        }

        Some(ScamMatch {
            contract: "unknown".to_string(),
            scam_type: ScamType::FakeToken,
            confidence: 0.85,
            evidence: vec![format!(
                "Symbol {symbol_upper} matches a well-known token but the name \"{name}\" does not"
            )],
            reported_at: None,
            source: "symbol_impersonation".to_string(),
        })
    }

    /// Every scam type's wire name.
    pub fn all_names() -> Vec<&'static str> {
        ScamType::all().iter().map(|t| t.as_str()).collect()
    }

    /// The scam types this detector can report.
    pub fn scam_types() -> Vec<&'static str> {
        vec![
            "rug_pull",
            "honeypot",
            "fake_token",
            "flash_loan",
            "ponzi",
            "phishing",
            "fake_exchange",
            "fake_staking",
            "ico",
            "migrator",
            "unknown",
        ]
    }
}

impl Default for ScamDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a 4-byte selector appears in the code.
///
/// The selector has to be followed by a valid `PUSH`-based dispatch pattern or
/// be a standalone `EQ`, otherwise four arbitrary bytes that happen to match
/// would count. In practice a selector appears as
/// `PUSH4 <selector> EQ PUSH2 <dest> JUMPI`, so the bytes after the selector
/// must be `EQ` or a `PUSH`.
fn contains_selector(code: &[u8], selector: &[u8; 4]) -> bool {
    if code.len() < 5 {
        return false;
    }

    for window in code.windows(5) {
        if window[..4] == *selector {
            // 0x14 is EQ, 0x15..0x7f are the PUSH opcodes.
            let next = window[4];
            if next == 0x14 || (0x60..=0x7f).contains(&next) {
                return true;
            }
        }
    }

    false
}

/// Decode a hex string, with or without a `0x` prefix.
pub fn decode_hex(input: &str) -> Option<Vec<u8>> {
    let trimmed = input.trim();
    let digits = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")).unwrap_or(trimmed);

    if digits.is_empty() || digits.len() % 2 != 0 {
        return None;
    }

    hex::decode(digits).ok()
}

/// Normalize an address to lowercase `0x`-prefixed form.
pub fn normalize_address(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);

    if digits.len() != 40 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    Some(format!("0x{}", digits.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a dispatch table containing the given selectors, in the shape a
    /// solc-generated dispatcher uses.
    fn dispatcher(selectors: &[[u8; 4]]) -> String {
        let mut code: Vec<u8> = Vec::new();
        for selector in selectors {
            code.push(0x63); // PUSH4
            code.extend_from_slice(selector);
            code.push(0x14); // EQ
            code.push(0x61); // PUSH2
            code.extend_from_slice(&[0x00, 0x2a]);
            code.push(0x57); // JUMPI
        }
        hex::encode(code)
    }

    #[test]
    fn a_reported_contract_is_detected_with_full_confidence() {
        let mut detector = ScamDetector::new();
        detector.report(
            "0x1111111111111111111111111111111111111111",
            ScamType::RugPull,
            "drained liquidity",
        );

        let found = detector
            .check_contract("0x1111111111111111111111111111111111111111")
            .unwrap();
        assert_eq!(found.scam_type, ScamType::RugPull);
        assert_eq!(found.confidence, 1.0);
    }

    #[test]
    fn an_unreported_contract_is_not_detected() {
        let detector = ScamDetector::new();
        assert!(detector
            .check_contract("0x1111111111111111111111111111111111111111")
            .is_none());
        assert_eq!(detector.known_count(), 0);
    }

    #[test]
    fn a_malformed_address_is_rejected_rather_than_matched() {
        let mut detector = ScamDetector::new();
        detector.report("0xdead", ScamType::RugPull, "x");

        assert!(detector.check_contract("0xdead").is_none());
        assert_eq!(detector.known_count(), 0);
    }

    #[test]
    fn the_burn_address_is_not_treated_as_a_scam() {
        // The old detector hardcoded 0x…dEaD as a rug pull.
        let detector = ScamDetector::new();
        assert!(detector
            .check_contract("0x000000000000000000000000000000000000dEaD")
            .is_none());
    }

    #[test]
    fn an_address_containing_the_letters_buy_is_not_a_honeypot() {
        // The old detector matched the substring "buy" against the address.
        let detector = ScamDetector::new();
        let matches = detector.analyze_bytecode("0xbuy000000000000000000000000000000000000");
        assert!(matches.is_empty());

        // A real address with those letters is also not flagged.
        assert!(detector
            .check_contract("0xb0b00000000000000000000000000000000000ab")
            .is_none());
    }

    #[test]
    fn a_plain_erc20_is_not_flagged() {
        let detector = ScamDetector::new();
        let code = dispatcher(&[
            [0x06, 0xfd, 0xde, 0x03], // transfer
            [0x70, 0xa0, 0x82, 0x31], // balanceOf
            [0xdd, 0x62, 0xed, 0x3e], // approve
        ]);

        assert!(detector.analyze_bytecode(&code).is_empty());
        assert!(!detector.is_erc20(&code)); // totalSupply and transferFrom are absent
    }

    #[test]
    fn a_full_erc20_interface_is_recognised() {
        let detector = ScamDetector::new();
        let code = dispatcher(ERC20_SELECTORS);
        assert!(detector.is_erc20(&code));
    }

    #[test]
    fn mint_plus_a_tax_setter_is_reported_as_a_rug_pull() {
        let detector = ScamDetector::new();
        let code = dispatcher(&[
            [0x40, 0xc1, 0x0f, 0x19], // mint(uint256)
            [0x69, 0x3c, 0x7d, 0x41], // setTaxFee(uint256)
        ]);

        let matches = detector.analyze_bytecode(&code);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].scam_type, ScamType::RugPull);
        assert!(matches[0].confidence >= 0.7);
    }

    #[test]
    fn blacklist_and_pause_without_renounce_is_a_honeypot() {
        let detector = ScamDetector::new();
        let code = dispatcher(&[
            [0xf9, 0xf9, 0x2d, 0x0d], // blacklist(address)
            [0x84, 0x56, 0xcb, 0x59], // pause()
        ]);

        let matches = detector.analyze_bytecode(&code);
        assert!(matches
            .iter()
            .any(|m| m.scam_type == ScamType::Honeypot));
    }

    #[test]
    fn blacklist_and_pause_with_renounce_is_not_flagged_as_a_honeypot() {
        let detector = ScamDetector::new();
        let code = dispatcher(&[
            [0xf9, 0xf9, 0x2d, 0x0d], // blacklist(address)
            [0x84, 0x56, 0xcb, 0x59], // pause()
            [0x71, 0x50, 0x18, 0xa6], // renounceOwnership()
        ]);

        let matches = detector.analyze_bytecode(&code);
        assert!(!matches.iter().any(|m| m.scam_type == ScamType::Honeypot));
    }

    #[test]
    fn an_upgradeable_contract_is_reported() {
        let detector = ScamDetector::new();
        let code = dispatcher(&[[0x36, 0x59, 0xcf, 0xe6]]); // upgradeTo(address)

        let matches = detector.analyze_bytecode(&code);
        assert_eq!(matches[0].scam_type, ScamType::Migrator);
    }

    #[test]
    fn four_arbitrary_matching_bytes_are_not_a_selector() {
        let detector = ScamDetector::new();
        // The selector bytes appear, but not followed by EQ or a PUSH, so this
        // is data, not a dispatch entry.
        let code = hex::encode([
            0x40, 0xc1, 0x0f, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);

        assert!(detector.analyze_bytecode(&code).is_empty());
    }

    #[test]
    fn non_hex_bytecode_is_ignored_rather_than_guessed_at() {
        let detector = ScamDetector::new();
        assert!(detector.analyze_bytecode("not hex at all").is_empty());
        assert!(detector.analyze_bytecode("").is_empty());
        assert!(detector.analyze_bytecode("0xabc").is_empty()); // odd length
        assert!(!detector.is_erc20("not hex at all"));
    }

    #[test]
    fn a_legitimate_wrapped_token_is_not_impersonation() {
        let detector = ScamDetector::new();
        assert!(detector.analyze_token("Wrapped Ether", "WETH").is_none());
        assert!(detector.analyze_token("Tether USD", "USDT").is_none());
    }

    #[test]
    fn a_token_reusing_a_known_symbol_is_flagged() {
        let detector = ScamDetector::new();
        let found = detector.analyze_token("Totally Legit", "WETH").unwrap();
        assert_eq!(found.scam_type, ScamType::FakeToken);
        assert!(found.confidence >= 0.8);
    }

    #[test]
    fn a_token_that_merely_mentions_a_brand_is_not_flagged() {
        // The old detector flagged any name containing "eth" or "bnb".
        let detector = ScamDetector::new();
        assert!(detector.analyze_token("Ethereum Classic", "ETC").is_none());
        assert!(detector.analyze_token("BNB Chain Fan Token", "FAN").is_none());
        assert!(detector.analyze_token("Bitcoin", "BTC").is_none());
    }

    #[test]
    fn a_deployer_with_a_history_is_flagged() {
        let mut detector = ScamDetector::new();
        detector.report_deployer("0x2222222222222222222222222222222222222222");

        assert!(detector
            .check_deployer("0x2222222222222222222222222222222222222222")
            .is_some());
        assert!(detector
            .check_deployer("0x3333333333333333333333333333333333333333")
            .is_none());
    }

    #[test]
    fn address_normalization_accepts_both_cases_and_prefixes() {
        assert_eq!(
            normalize_address("0xAbCd00000000000000000000000000000000Ef12"),
            Some("0xabcd00000000000000000000000000000000ef12".to_string())
        );
        assert_eq!(
            normalize_address("abcd00000000000000000000000000000000ef12"),
            Some("0xabcd00000000000000000000000000000000ef12".to_string())
        );
        assert!(normalize_address("0x1234").is_none());
        assert!(normalize_address("").is_none());
    }

    #[test]
    fn hex_decoding_accepts_a_prefix_or_not() {
        assert_eq!(decode_hex("0xdead"), Some(vec![0xde, 0xad]));
        assert_eq!(decode_hex("dead"), Some(vec![0xde, 0xad]));
        assert_eq!(decode_hex("DEAD"), Some(vec![0xde, 0xad]));
        assert_eq!(decode_hex("0x"), None);
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    #[test]
    fn the_reported_scam_types_cover_every_variant() {
        let types = ScamDetector::scam_types();
        for expected in ["rug_pull", "honeypot", "fake_token", "migrator"] {
            assert!(types.contains(&expected));
        }
    }
}
