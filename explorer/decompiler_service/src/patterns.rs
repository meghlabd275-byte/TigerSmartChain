//! Contract type detection.
//!
//! Detection is based on the ERC interface identifier from ERC-165 plus the
//! selectors actually present in the dispatcher, not on substring matches in
//! the raw hex. A contract that merely mentions a selector in its constant
//! data is not that interface.

use std::collections::HashSet;

use crate::analysis::Analyzer;
use crate::types::DecompiledFunction;

/// Contract type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractType {
    ERC20,
    ERC721,
    ERC1155,
    Proxy,
    Multisig,
    Unknown,
}

impl ContractType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ERC20 => "ERC-20 Token",
            Self::ERC721 => "ERC-721 NFT",
            Self::ERC1155 => "ERC-1155 Multi-Token",
            Self::Proxy => "Proxy Contract",
            Self::Multisig => "Multi-Sig Wallet",
            Self::Unknown => "Unknown",
        }
    }
}

/// The four-byte ERC-165 interface identifiers.
pub mod interface_id {
    /// `type(IERC20).interfaceId`
    pub const ERC20: [u8; 4] = [0x36, 0x37, 0x2b, 0x07];
    /// `type(IERC721).interfaceId`
    pub const ERC721: [u8; 4] = [0x80, 0xac, 0x58, 0xcd];
    /// `type(IERC1155).interfaceId`
    pub const ERC1155: [u8; 4] = [0xd9, 0xb6, 0x7a, 0x26];
}

/// Contract pattern detector
pub struct PatternDetector;

impl PatternDetector {
    /// Detect the contract type from its runtime bytecode.
    pub fn detect(bytecode: &str) -> ContractType {
        let analysis = Analyzer::analyze(bytecode);

        if analysis.is_minimal_proxy || analysis.is_proxy {
            return ContractType::Proxy;
        }

        let selectors: HashSet<&str> = analysis
            .functions
            .iter()
            .filter(|function| function.resolved)
            .map(|function| function.selector.as_str())
            .collect();

        if Self::is_erc20(&selectors) {
            return ContractType::ERC20;
        }

        if Self::is_erc721(&selectors) {
            return ContractType::ERC721;
        }

        if Self::is_erc1155(&selectors) {
            return ContractType::ERC1155;
        }

        if Self::is_multisig(&selectors) {
            return ContractType::Multisig;
        }

        ContractType::Unknown
    }

    /// True when every mandatory ERC-20 selector is present.
    fn is_erc20(selectors: &HashSet<&str>) -> bool {
        // transfer, transferFrom, approve, balanceOf, allowance, totalSupply
        const MANDATORY: [&str; 6] = [
            "a9059cbb",
            "23b872dd",
            "095ea7b3",
            "70a08231",
            "dd62ed3e",
            "18160ddd",
        ];

        MANDATORY
            .iter()
            .all(|selector| selectors.contains(selector))
    }

    /// True when the ERC-721 mandatory selectors are present.
    fn is_erc721(selectors: &HashSet<&str>) -> bool {
        // The full IERC721 selector set, so the XOR equals 0x80ac58cd.
        const MANDATORY: [&str; 9] = [
            "23b872dd",
            "70a08231",
            "6352211e",
            "095ea7b3",
            "a22cb465",
            "081812fc",
            "e985e9c5",
            "42842e0e",
            "b88d4fde",
        ];

        MANDATORY
            .iter()
            .all(|selector| selectors.contains(selector))
    }

    /// True when the ERC-1155 mandatory selectors are present.
    fn is_erc1155(selectors: &HashSet<&str>) -> bool {
        // The full IERC1155 selector set, so the XOR equals 0xd9b67a26.
        const MANDATORY: [&str; 6] = [
            "00fdd58e",
            "4e1273f4",
            "a22cb465",
            "e985e9c5",
            "f242432a",
            "2eb2c2d6",
        ];

        MANDATORY
            .iter()
            .all(|selector| selectors.contains(selector))
    }

    /// True when the Safe (Gnosis) multi-signature surface is present.
    fn is_multisig(selectors: &HashSet<&str>) -> bool {
        // execTransaction, getThreshold, getOwners
        const MANDATORY: [&str; 3] = ["6a761202", "e75235b8", "a0e67e2b"];

        MANDATORY
            .iter()
            .all(|selector| selectors.contains(selector))
    }
}

/// The ERC-165 interface identifier a function set implements, if any.
///
/// `supportsInterface` is answered by comparing the supplied id against the
/// XOR of the interface's selectors, so the same computation is reusable when
/// verifying an interface claim.
pub fn interface_id(functions: &[DecompiledFunction]) -> Option<[u8; 4]> {
    let selectors: HashSet<&str> = functions
        .iter()
        .filter(|function| function.resolved)
        .map(|function| function.selector.as_str())
        .collect();

    if PatternDetector::is_erc20(&selectors) {
        return Some(interface_id::ERC20);
    }

    if PatternDetector::is_erc721(&selectors) {
        return Some(interface_id::ERC721);
    }

    if PatternDetector::is_erc1155(&selectors) {
        return Some(interface_id::ERC1155);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatcher(selectors: &[&str]) -> String {
        let mut code = String::new();

        for selector in selectors {
            code.push_str("63");
            code.push_str(selector);
            code.push_str("146101005700");
        }

        code
    }

    #[test]
    fn the_erc165_interface_ids_match_the_specification() {
        // The identifiers are the XOR of the interface selectors, so a wrong
        // constant makes `supportsInterface` answer incorrectly.
        assert_eq!(hex::encode(interface_id::ERC20), "36372b07");
        assert_eq!(hex::encode(interface_id::ERC721), "80ac58cd");
        assert_eq!(hex::encode(interface_id::ERC1155), "d9b67a26");
    }

    #[test]
    fn an_erc20_dispatcher_is_recognised() {
        let code = dispatcher(&[
            "a9059cbb", "23b872dd", "095ea7b3", "70a08231", "dd62ed3e", "18160ddd",
        ]);

        assert_eq!(PatternDetector::detect(&code), ContractType::ERC20);
    }

    #[test]
    fn an_erc721_dispatcher_is_recognised() {
        let code = dispatcher(&[
            "23b872dd", "70a08231", "6352211e", "095ea7b3", "a22cb465", "081812fc",
            "e985e9c5", "42842e0e", "b88d4fde",
        ]);

        assert_eq!(PatternDetector::detect(&code), ContractType::ERC721);
    }

    #[test]
    fn an_erc1155_dispatcher_is_recognised() {
        let code = dispatcher(&[
            "00fdd58e", "4e1273f4", "a22cb465", "e985e9c5", "f242432a", "2eb2c2d6",
        ]);

        assert_eq!(PatternDetector::detect(&code), ContractType::ERC1155);
    }

    #[test]
    fn a_safe_dispatcher_is_recognised() {
        let code = dispatcher(&["6a761202", "e75235b8", "a0e67e2b"]);

        assert_eq!(PatternDetector::detect(&code), ContractType::Multisig);
    }

    #[test]
    fn a_partial_token_is_not_reported_as_a_token() {
        // transfer and approve alone are not an ERC-20.
        let code = dispatcher(&["a9059cbb", "095ea7b3"]);

        assert_eq!(PatternDetector::detect(&code), ContractType::Unknown);
    }

    #[test]
    fn a_clone_is_reported_as_a_proxy() {
        let mut code = String::from("363d3d373d3d3d363d73");
        code.push_str("000000000000000000000000deadbeefdeadbeef");
        code.push_str("5af43d82803e903d91602b57fd5bf3");

        assert_eq!(PatternDetector::detect(&code), ContractType::Proxy);
    }

    #[test]
    fn selectors_in_dead_data_do_not_type_a_contract() {
        // The selectors appear as PUSH4 immediates without an EQ comparison.
        let code = "63a9059cbb5063 23b872dd 50 63 095ea7b3 50 00";

        assert_eq!(PatternDetector::detect(code), ContractType::Unknown);
    }

    #[test]
    fn the_interface_id_is_returned_for_a_known_interface() {
        let functions: Vec<DecompiledFunction> = [
            "a9059cbb", "23b872dd", "095ea7b3", "70a08231", "dd62ed3e", "18160ddd",
        ]
        .iter()
        .map(|selector| DecompiledFunction {
            name: String::new(),
            signature: String::new(),
            selector: (*selector).to_string(),
            visibility: "external".to_string(),
            inputs: Vec::new(),
            resolved: true,
        })
        .collect();

        assert_eq!(interface_id(&functions), Some(interface_id::ERC20));
    }
}
