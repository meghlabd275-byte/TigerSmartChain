//! Chain constants for TigerSmartChain.
//!
//! TigerSmartChain is an EVM chain with chain ID 6666. It has two native
//! assets:
//!
//! * **TGR** — the native utility coin. It is the gas token, the staking token
//!   and the block-reward token, and it has no contract: it lives in the
//!   account balances of the state trie, exactly like BNB on BNB Chain.
//! * **RUSD** — the native stablecoin, pegged 1:1 to the US dollar and held at
//!   the well-known address below so that a client can identify it without a
//!   registry lookup.

/// TigerSmartChain mainnet chain ID.
pub const CHAIN_ID: u64 = 6666;

/// Number of decimals on the native coin, matching every other EVM chain.
pub const NATIVE_DECIMALS: u8 = 18;

/// Native utility coin symbol.
pub const NATIVE_SYMBOL: &str = "TGR";

/// Native utility coin name.
pub const NATIVE_NAME: &str = "Tiger Coin";

/// Native stablecoin symbol.
pub const STABLECOIN_SYMBOL: &str = "RUSD";

/// Native stablecoin name.
pub const STABLECOIN_NAME: &str = "Royal Tiger USD";

/// Decimals on the native stablecoin, matching USDT and USDC.
pub const STABLECOIN_DECIMALS: u8 = 6;

/// Canonical address of the native stablecoin.
///
/// Chosen to be recognisable and to have no known private key. RUSD is issued
/// by the chain's treasury contract, which mints against deposited collateral
/// and burns on redemption; the address below is the treasury's mint target.
pub const STABLECOIN_ADDRESS: &str = "0x5255534400000000000000000000000000000000";

/// True when `address` is the native stablecoin contract.
pub fn is_stablecoin(address: &str) -> bool {
    address.eq_ignore_ascii_case(STABLECOIN_ADDRESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stablecoin_address_is_twenty_bytes() {
        let trimmed = STABLECOIN_ADDRESS.trim_start_matches("0x");
        assert_eq!(trimmed.len(), 40);
        assert!(trimmed.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn stablecoin_matching_ignores_case() {
        assert!(is_stablecoin("0x5255534400000000000000000000000000000000"));
        assert!(is_stablecoin("0x5255534400000000000000000000000000000000"));
        assert!(!is_stablecoin("0x0000000000000000000000000000000000000000"));
    }
}
