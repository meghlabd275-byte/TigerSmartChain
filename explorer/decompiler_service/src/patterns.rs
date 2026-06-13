//! Contract Pattern Detection
//! Detect common contract patterns in bytecode

use std::collections::HashMap;

/// Contract pattern detector
pub struct PatternDetector;

impl PatternDetector {
    /// Detect contract type from bytecode
    pub fn detect(bytecode: &str) -> ContractType {
        let bytes = match hex::decode(bytecode.trim_start_matches("0x")) {
            Ok(b) => b,
            Err(_) => return ContractType::Unknown,
        };
        
        // Check for ERC-20 pattern
        if Self::is_erc20(&bytes) {
            return ContractType::ERC20;
        }
        
        // Check for ERC-721 pattern
        if Self::is_erc721(&bytes) {
            return ContractType::ERC721;
        }
        
        // Check for ERC-1155 pattern
        if Self::is_erc1155(&bytes) {
            return ContractType::ERC1155;
        }
        
        // Check for proxy pattern
        if Self::is_proxy(&bytes) {
            return ContractType::Proxy;
        }
        
        // Check for multisig
        if Self::is_multisig(&bytes) {
            return ContractType::Multisig;
        }
        
        ContractType::Unknown
    }
    
    fn is_erc20(bytes: &[u8]) -> bool {
        let selectors = ["a9059cbb", "095ea7b3", "23b872dd"];
        Self::has_selectors(bytes, &selectors)
    }
    
    fn is_erc721(bytes: &[u8]) -> bool {
        let selectors = ["a22cb465", "42842e0e", "b88d4fde"];
        Self::has_selectors(bytes, &selectors)
    }
    
    fn is_erc1155(bytes: &[u8]) -> bool {
        let selectors = ["4e71d92d", "f242432a"];
        Self::has_selectors(bytes, &selectors)
    }
    
    fn is_proxy(bytes: &[u8]) -> bool {
        // EIP-1967 proxy pattern
        let patterns = ["363d3d373d3d3d363d3054", "3d3d3d3d363d3d376034"];
        let bytecode_hex = hex::encode(bytes);
        patterns.iter().any(|p| bytecode_hex.contains(p))
    }
    
    fn is_multisig(bytes: &[u8]) -> bool {
        // Gnosis Safe detection
        let patterns = ["dafecc80", "ce11ed6f"];
        let bytecode_hex = hex::encode(bytes);
        patterns.iter().any(|p| bytecode_hex.contains(p))
    }
    
    fn has_selectors(bytes: &[u8], selectors: &[&str]) -> bool {
        let bytes_hex = hex::encode(bytes);
        selectors.iter().filter(|s| bytes_hex.contains(*s)).count() >= 2
    }
}

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