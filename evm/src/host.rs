//! The host interface the interpreter uses to reach world state.
//!
//! The interpreter itself owns no persistent state: balances, code, storage,
//! block hashes and account creation all go through [`Host`]. This keeps the
//! execution engine free of storage concerns and lets the same interpreter run
//! against an in-memory overlay (for `eth_call`) or the real trie-backed world
//! state (for block execution).

use crate::types::{Address, EvmWord};
use std::collections::HashMap;

/// What the interpreter needs from the surrounding world.
pub trait Host {
    /// Account balance in wei.
    fn balance(&self, address: &Address) -> EvmWord;

    /// Whether an account exists. Drives the EIP-161 "new account" gas
    /// surcharges and SELFDESTRUCT semantics.
    fn exists(&self, address: &Address) -> bool;

    /// Contract code deployed at `address`.
    fn code(&self, address: &Address) -> Vec<u8>;

    /// keccak256 of the code at `address`.
    ///
    /// Returns zero for an address with no account and
    /// `keccak256("")` for an account without code, exactly as `EXTCODEHASH`
    /// specifies.
    fn code_hash(&self, address: &Address) -> EvmWord;

    /// Read a storage slot.
    fn storage(&self, address: &Address, slot: &EvmWord) -> EvmWord;

    /// Write a storage slot.
    fn set_storage(&mut self, address: &Address, slot: EvmWord, value: EvmWord);

    /// Block hash for a recent block number, or zero if unavailable.
    fn block_hash(&self, number: u64) -> EvmWord;

    /// Record a log emitted by `address`.
    fn emit_log(&mut self, address: &Address, topics: Vec<EvmWord>, data: Vec<u8>);

    /// Transfer `value` from `from` to `to`.
    ///
    /// Returns false when `from` cannot cover the transfer, in which case no
    /// state must be modified.
    fn transfer(&mut self, from: &Address, to: &Address, value: EvmWord) -> bool;

    /// Deploy `code` at `address`.
    fn set_code(&mut self, address: &Address, code: Vec<u8>);

    /// Increment and return the nonce of `address`, used for CREATE addresses.
    fn bump_nonce(&mut self, address: &Address) -> u64;

    /// Mark `address` for destruction at the end of the transaction.
    fn selfdestruct(&mut self, address: &Address);

    /// Logs recorded so far.
    fn logs(&self) -> &[crate::types::Log];

    /// Take the logs recorded so far, leaving the host empty of logs.
    fn take_logs(&mut self) -> Vec<crate::types::Log>;
}

/// A host backed by in-memory maps.
///
/// Used for `eth_call`, `eth_estimateGas` and tests: it has the same semantics
/// as the real world state but nothing is persisted.
#[derive(Debug, Default, Clone)]
pub struct MemoryHost {
    balances: HashMap<Address, EvmWord>,
    code: HashMap<Address, Vec<u8>>,
    storage: HashMap<(Address, EvmWord), EvmWord>,
    block_hashes: HashMap<u64, EvmWord>,
    logs: Vec<crate::types::Log>,
    nonces: HashMap<Address, u64>,
}

impl MemoryHost {
    /// An empty host.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a balance directly.
    pub fn with_balance(mut self, address: Address, balance: EvmWord) -> Self {
        self.balances.insert(address, balance);
        self
    }

    /// Deploy code directly.
    pub fn with_code(mut self, address: Address, code: Vec<u8>) -> Self {
        self.code.insert(address, code);
        self
    }

    /// Set a storage slot directly.
    pub fn with_storage(mut self, address: Address, slot: EvmWord, value: EvmWord) -> Self {
        self.storage.insert((address, slot), value);
        self
    }

    /// Provide a block hash for a given number.
    pub fn with_block_hash(mut self, number: u64, hash: EvmWord) -> Self {
        self.block_hashes.insert(number, hash);
        self
    }

    /// Set an account nonce directly.
    pub fn with_nonce(mut self, address: Address, nonce: u64) -> Self {
        self.nonces.insert(address, nonce);
        self
    }

    /// Borrow the underlying storage map.
    pub fn storage_map(&self) -> &HashMap<(Address, EvmWord), EvmWord> {
        &self.storage
    }
}

impl Host for MemoryHost {
    fn balance(&self, address: &Address) -> EvmWord {
        self.balances.get(address).cloned().unwrap_or_else(EvmWord::zero)
    }

    fn exists(&self, address: &Address) -> bool {
        self.balances.get(address).map(|b| !b.is_zero()).unwrap_or(false)
            || self.code.contains_key(address)
            || self.nonces.get(address).map(|n| *n > 0).unwrap_or(false)
    }

    fn code(&self, address: &Address) -> Vec<u8> {
        self.code.get(address).cloned().unwrap_or_default()
    }

    fn code_hash(&self, address: &Address) -> EvmWord {
        if !self.exists(address) {
            return EvmWord::zero();
        }
        match self.code.get(address) {
            Some(code) => EvmWord::from_be_bytes(&tiger_crypto::keccak256(code)),
            None => EvmWord::from_be_bytes(&tiger_crypto::keccak256(&[])),
        }
    }

    fn storage(&self, address: &Address, slot: &EvmWord) -> EvmWord {
        self.storage
            .get(&(*address, slot.clone()))
            .cloned()
            .unwrap_or_else(EvmWord::zero)
    }

    fn set_storage(&mut self, address: &Address, slot: EvmWord, value: EvmWord) {
        if value.is_zero() {
            self.storage.remove(&(*address, slot));
        } else {
            self.storage.insert((*address, slot), value);
        }
    }

    fn block_hash(&self, number: u64) -> EvmWord {
        self.block_hashes
            .get(&number)
            .cloned()
            .unwrap_or_else(EvmWord::zero)
    }

    fn emit_log(&mut self, address: &Address, topics: Vec<EvmWord>, data: Vec<u8>) {
        self.logs.push(crate::types::Log {
            address: address.clone(),
            topics,
            data,
        });
    }

    fn transfer(&mut self, from: &Address, to: &Address, value: EvmWord) -> bool {
        if value.is_zero() {
            return true;
        }
        let from_balance = self.balance(from);
        if from_balance.lt(&value).as_u64() == 1 {
            return false;
        }
        let new_from = from_balance.sub(&value);
        let new_to = self.balance(to).add(&value);
        self.balances.insert(*from, new_from);
        self.balances.insert(*to, new_to);
        true
    }

    fn set_code(&mut self, address: &Address, code: Vec<u8>) {
        self.code.insert(*address, code);
    }

    fn bump_nonce(&mut self, address: &Address) -> u64 {
        let entry = self.nonces.entry(*address).or_insert(0);
        let current = *entry;
        *entry = current + 1;
        current
    }

    fn selfdestruct(&mut self, address: &Address) {
        self.code.remove(address);
        self.storage.retain(|(a, _), _| a != address);
    }

    fn logs(&self) -> &[crate::types::Log] {
        &self.logs
    }

    fn take_logs(&mut self) -> Vec<crate::types::Log> {
        std::mem::take(&mut self.logs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    #[test]
    fn missing_account_has_zero_balance_and_zero_code_hash() {
        let host = MemoryHost::new();
        assert_eq!(host.balance(&addr(1)), EvmWord::zero());
        // EXTCODEHASH must be zero for a non-existent account.
        assert_eq!(host.code_hash(&addr(1)), EvmWord::zero());
        assert!(!host.exists(&addr(1)));
    }

    #[test]
    fn account_without_code_hashes_to_empty_keccak() {
        let mut host = MemoryHost::new();
        host.balances.insert(addr(1), EvmWord::from_u64(5));
        assert!(host.exists(&addr(1)));
        let expected = EvmWord::from_be_bytes(&tiger_crypto::keccak256(&[]));
        assert_eq!(host.code_hash(&addr(1)), expected);
    }

    #[test]
    fn code_hash_is_keccak_of_deployed_code() {
        let mut host = MemoryHost::new();
        let code = vec![0x60, 0x01, 0x60, 0x01, 0x01];
        host.set_code(&addr(2), code.clone());
        assert_eq!(
            host.code_hash(&addr(2)),
            EvmWord::from_be_bytes(&tiger_crypto::keccak256(&code))
        );
    }

    #[test]
    fn transfer_moves_value_and_rejects_insufficient_funds() {
        let mut host = MemoryHost::new();
        host.balances.insert(addr(1), EvmWord::from_u64(100));
        assert!(host.transfer(&addr(1), &addr(2), EvmWord::from_u64(40)));
        assert_eq!(host.balance(&addr(1)).as_u64(), 60);
        assert_eq!(host.balance(&addr(2)).as_u64(), 40);
        // Overdraft must fail and leave balances untouched.
        assert!(!host.transfer(&addr(1), &addr(2), EvmWord::from_u64(1000)));
        assert_eq!(host.balance(&addr(1)).as_u64(), 60);
    }

    #[test]
    fn zero_value_storage_write_clears_the_slot() {
        let mut host = MemoryHost::new();
        host.set_storage(&addr(3), EvmWord::from_u64(1), EvmWord::from_u64(9));
        assert_eq!(host.storage(&addr(3), &EvmWord::from_u64(1)).as_u64(), 9);
        host.set_storage(&addr(3), EvmWord::from_u64(1), EvmWord::zero());
        assert_eq!(host.storage(&addr(3), &EvmWord::from_u64(1)), EvmWord::zero());
        assert!(host.storage_map().is_empty());
    }

    #[test]
    fn nonces_increment_monotonically() {
        let mut host = MemoryHost::new();
        assert_eq!(host.bump_nonce(&addr(4)), 0);
        assert_eq!(host.bump_nonce(&addr(4)), 1);
        assert_eq!(host.bump_nonce(&addr(4)), 2);
    }

    #[test]
    fn selfdestruct_clears_code_and_storage() {
        let mut host = MemoryHost::new();
        host.set_code(&addr(5), vec![0x00]);
        host.set_storage(&addr(5), EvmWord::from_u64(1), EvmWord::from_u64(1));
        host.selfdestruct(&addr(5));
        assert!(host.code(&addr(5)).is_empty());
        assert_eq!(host.storage(&addr(5), &EvmWord::from_u64(1)), EvmWord::zero());
    }
}
