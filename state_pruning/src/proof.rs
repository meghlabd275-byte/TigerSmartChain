//! Account and storage proofs, generated and verified through the real Merkle
//! Patricia Trie in the `tiger-state` crate.
//!
//! The previous version returned `AccountState::default()` and empty proof
//! vectors for every request, and its verifier answered
//! `!proof.account_state.code_hash.as_bytes().is_empty()` — true for any
//! non-empty hash, so every proof "verified", including a proof of nothing.
//!
//! Nothing here re-implements trie logic. A proof is the RLP of each node on
//! the path from the root to the key, exactly as Ethereum defines it, and it is
//! checked by re-deriving the root from those nodes.

use crate::{AccountState, Address, H256, PruningError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tiger_state::account::{account_hash, encode_account, Account as StateAccount};
use tiger_state::trie::Trie;

/// A storage slot proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageProof {
    /// The slot the proof is about.
    pub slot: H256,
    /// The value the proof shows the slot holds.
    pub value: H256,
    /// Whether the proof validly shows the slot is absent.
    pub absent: bool,
    /// RLP of each trie node on the path, root first.
    pub proof: Vec<Vec<u8>>,
}

/// An account proof, plus a proof for each requested storage slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateProof {
    pub address: Address,
    pub account_state: AccountState,
    /// Whether the proof validly shows the account is absent.
    pub absent: bool,
    pub storage_proofs: Vec<StorageProof>,
    pub block_number: u64,
    pub state_root: H256,
    /// RLP of each account-trie node on the path, root first.
    pub account_proof: Vec<Vec<u8>>,
}

/// Verifies account and storage proofs against a state root.
pub struct ProofVerifier;

impl ProofVerifier {
    /// Verify an account proof against the root it claims.
    ///
    /// Returns `Ok(true)` when the proof is valid, `Ok(false)` when it is
    /// internally consistent but does not match, and `Err` when the proof is
    /// malformed. A proof of absence is valid when the trie proves the key is
    /// not present, which is a different statement from "the account has no
    /// code" — the old verifier conflated the two.
    pub fn verify_state_proof(proof: &StateProof) -> Result<bool, PruningError> {
        let key = tiger_crypto::keccak256(proof.address.as_bytes());

        match Trie::verify_proof(&proof.state_root.0, &key, &proof.account_proof) {
            // The proof shows the key is absent, so the account must be absent.
            Some(None) => Ok(proof.absent),
            Some(Some(encoded)) => {
                if proof.absent {
                    return Ok(false);
                }
                Ok(encoded == encode_account(&to_state_account(&proof.account_state)))
            }
            None => Ok(false),
        }
    }

    /// Verify a storage proof against the storage root of the account it
    /// belongs to.
    pub fn verify_storage_proof(
        storage_root: &H256,
        proof: &StorageProof,
    ) -> Result<bool, PruningError> {
        let key = tiger_crypto::keccak256(proof.slot.as_bytes());

        match Trie::verify_proof(&storage_root.0, &key, &proof.proof) {
            Some(None) => Ok(proof.absent),
            Some(Some(value)) => {
                if proof.absent {
                    return Ok(false);
                }
                Ok(value == rlp_encode_slot(&proof.value.0))
            }
            None => Ok(false),
        }
    }
}

/// Generates proofs from a set of accounts.
pub struct ProofGenerator;

/// An account together with its storage slots.
pub type AccountEntry = (Address, AccountState, HashMap<[u8; 32], [u8; 32]>);

impl ProofGenerator {
    /// Build the account trie for `accounts` and prove `address`.
    ///
    /// `accounts` carries each account's storage so the storage trie can be
    /// built and its root embedded in the account, which is what makes a
    /// storage proof verifiable against the account proof.
    pub fn generate_account_proof(
        accounts: &[AccountEntry],
        address: Address,
        storage_slots: Vec<H256>,
    ) -> Result<StateProof, PruningError> {
        let mut trie = Trie::new();
        let mut storage_roots: HashMap<Address, H256> = HashMap::new();

        for (addr, state, storage) in accounts {
            let storage_root = storage_trie_root(storage);
            storage_roots.insert(*addr, storage_root);

            let account = to_state_account_with_root(state, storage_root.0, storage);
            trie.insert(
                &tiger_crypto::keccak256(addr.as_bytes()),
                &encode_account(&account),
            );
        }

        let root = trie.root_hash();
        let key = tiger_crypto::keccak256(address.as_bytes());
        let account_proof = trie.prove(&key);

        let found = accounts.iter().find(|(a, _, _)| *a == address);

        let (account_state, absent) = match found {
            Some((_, state, storage)) => {
                let storage_root = storage_roots
                    .get(&address)
                    .copied()
                    .unwrap_or(H256(tiger_state::account::EMPTY_STATE_ROOT));
                let mut state = state.clone();
                state.storage_root = storage_root;
                let _ = storage;
                (state, false)
            }
            None => (AccountState::default(), true),
        };

        // Storage proofs are generated against the account's own storage trie.
        let mut storage_trie = Trie::new();
        if let Some((_, _, storage)) = found {
            for (slot, value) in storage {
                storage_trie.insert(&tiger_crypto::keccak256(slot), &rlp_encode_slot(value));
            }
        }
        let storage_root = H256(storage_trie.root_hash());

        let storage_proofs: Vec<StorageProof> = storage_slots
            .into_iter()
            .map(|slot| {
                let slot_key = tiger_crypto::keccak256(&slot.0);
                let proof = storage_trie.prove(&slot_key);
                let (value, absent) =
                    match Trie::verify_proof(&storage_root.0, &slot_key, &proof) {
                        Some(Some(_)) => {
                            let raw = found
                                .and_then(|(_, _, storage)| storage.get(&slot.0))
                                .copied()
                                .unwrap_or([0u8; 32]);
                            (H256(raw), false)
                        }
                        _ => (H256::zero(), true),
                    };
                StorageProof {
                    slot,
                    value,
                    absent,
                    proof,
                }
            })
            .collect();

        Ok(StateProof {
            address,
            account_state,
            absent,
            storage_proofs,
            block_number: 0,
            state_root: H256(root),
            account_proof,
        })
    }
}

/// The storage root of a set of slots.
fn storage_trie_root(storage: &HashMap<[u8; 32], [u8; 32]>) -> H256 {
    let mut trie = Trie::new();
    for (slot, value) in storage {
        trie.insert(&tiger_crypto::keccak256(slot), &rlp_encode_slot(value));
    }
    H256(trie.root_hash())
}

/// Convert the crate's `AccountState` into the state layer's `Account`.
fn to_state_account(state: &AccountState) -> StateAccount {
    StateAccount {
        nonce: state.nonce,
        balance: state.balance.to_be_bytes(),
        storage_root: state.storage_root.0,
        code_hash: state.code_hash.0,
        code: state.code.clone().unwrap_or_default(),
        storage: HashMap::new(),
    }
}

fn to_state_account_with_root(
    state: &AccountState,
    storage_root: [u8; 32],
    storage: &HashMap<[u8; 32], [u8; 32]>,
) -> StateAccount {
    StateAccount {
        nonce: state.nonce,
        balance: state.balance.to_be_bytes(),
        storage_root,
        code_hash: state.code_hash.0,
        code: state.code.clone().unwrap_or_default(),
        storage: storage.clone(),
    }
}

/// RLP-encode a 32-byte storage value the way the trie stores it.
///
/// The trie stores each slot value as an RLP string, so the bytes that come
/// back out of a proof are the RLP encoding, not the raw 32 bytes.
fn rlp_encode_slot(value: &[u8; 32]) -> Vec<u8> {
    let first_nonzero = value.iter().position(|&b| b != 0);
    match first_nonzero {
        // A zero value encodes as the empty string.
        None => vec![0x80],
        Some(offset) => {
            let significant = &value[offset..];
            if significant.len() == 1 && significant[0] < 0x80 {
                vec![significant[0]]
            } else {
                let mut out = vec![0x80 + significant.len() as u8];
                out.extend_from_slice(significant);
                out
            }
        }
    }
}

/// Hash of an account, exposed so callers can compute the same value the trie
/// stores.
pub fn state_account_hash(state: &AccountState) -> H256 {
    H256(account_hash(&to_state_account(state)))
}

/// The state root of a set of accounts.
pub fn accounts_state_root(accounts: &[AccountEntry]) -> H256 {
    let mut trie = Trie::new();
    for (addr, state, storage) in accounts {
        let storage_root = storage_trie_root(storage);
        let account = to_state_account_with_root(state, storage_root.0, storage);
        trie.insert(
            &tiger_crypto::keccak256(addr.as_bytes()),
            &encode_account(&account),
        );
    }
    H256(trie.root_hash())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(nonce: u64, balance: u64) -> AccountState {
        use crate::U256;
        AccountState {
            nonce,
            balance: U256::from_u64(balance),
            storage_root: H256(tiger_state::account::EMPTY_STATE_ROOT),
            code_hash: H256(tiger_state::account::EMPTY_CODE_HASH),
            code: None,
        }
    }

    fn addr(n: u8) -> Address {
        let mut a = [0u8; 20];
        a[19] = n;
        Address(a)
    }

    fn entry(n: u8, nonce: u64, balance: u64) -> AccountEntry {
        (addr(n), account(nonce, balance), HashMap::new())
    }

    #[test]
    fn an_account_proof_verifies_against_its_root() {
        let accounts = vec![entry(1, 1, 1000), entry(2, 2, 2000), entry(3, 3, 3000)];

        let proof = ProofGenerator::generate_account_proof(&accounts, addr(2), vec![]).unwrap();

        assert!(!proof.absent);
        assert_eq!(proof.account_state.nonce, 2);
        assert!(ProofVerifier::verify_state_proof(&proof).unwrap());
    }

    #[test]
    fn a_proof_for_a_missing_account_shows_absence() {
        let accounts = vec![entry(1, 1, 1000)];

        let proof = ProofGenerator::generate_account_proof(&accounts, addr(9), vec![]).unwrap();

        assert!(proof.absent);
        assert!(ProofVerifier::verify_state_proof(&proof).unwrap());
    }

    #[test]
    fn a_tampered_account_proof_is_rejected() {
        let accounts = vec![entry(1, 1, 1000), entry(2, 2, 2000)];

        let mut proof = ProofGenerator::generate_account_proof(&accounts, addr(1), vec![]).unwrap();

        // Change the claimed balance without changing the proof.
        proof.account_state.balance = crate::U256::from_u64(999_999);

        assert!(!ProofVerifier::verify_state_proof(&proof).unwrap());
    }

    #[test]
    fn a_proof_checked_against_the_wrong_root_is_rejected() {
        let accounts = vec![entry(1, 1, 1000)];
        let mut proof = ProofGenerator::generate_account_proof(&accounts, addr(1), vec![]).unwrap();

        proof.state_root = H256([0xabu8; 32]);
        assert!(!ProofVerifier::verify_state_proof(&proof).unwrap());
    }

    #[test]
    fn an_absent_proof_claiming_presence_is_rejected() {
        let accounts = vec![entry(1, 1, 1000)];
        let mut proof = ProofGenerator::generate_account_proof(&accounts, addr(9), vec![]).unwrap();

        proof.absent = false;
        assert!(!ProofVerifier::verify_state_proof(&proof).unwrap());
    }

    #[test]
    fn a_storage_proof_verifies_against_the_account_storage_root() {
        let mut storage = HashMap::new();
        let mut slot = [0u8; 32];
        slot[31] = 7;
        let mut value = [0u8; 32];
        value[31] = 42;
        storage.insert(slot, value);

        let accounts = vec![(addr(1), account(1, 1000), storage)];
        let proof =
            ProofGenerator::generate_account_proof(&accounts, addr(1), vec![H256(slot)]).unwrap();

        assert_eq!(proof.storage_proofs.len(), 1);
        let storage_proof = &proof.storage_proofs[0];
        assert!(!storage_proof.absent);
        assert_eq!(storage_proof.value.0, value);

        assert!(ProofVerifier::verify_storage_proof(
            &proof.account_state.storage_root,
            storage_proof
        )
        .unwrap());
    }

    #[test]
    fn a_storage_proof_for_an_absent_slot_shows_absence() {
        let accounts = vec![entry(1, 1, 1000)];
        let mut slot = [0u8; 32];
        slot[31] = 9;

        let proof =
            ProofGenerator::generate_account_proof(&accounts, addr(1), vec![H256(slot)]).unwrap();

        assert!(proof.storage_proofs[0].absent);
        assert!(ProofVerifier::verify_storage_proof(
            &proof.account_state.storage_root,
            &proof.storage_proofs[0]
        )
        .unwrap());
    }

    #[test]
    fn a_tampered_storage_value_is_rejected() {
        let mut storage = HashMap::new();
        let mut slot = [0u8; 32];
        slot[31] = 7;
        let mut value = [0u8; 32];
        value[31] = 42;
        storage.insert(slot, value);

        let accounts = vec![(addr(1), account(1, 1000), storage)];
        let mut proof =
            ProofGenerator::generate_account_proof(&accounts, addr(1), vec![H256(slot)]).unwrap();

        proof.storage_proofs[0].value = H256([0xffu8; 32]);

        assert!(!ProofVerifier::verify_storage_proof(
            &proof.account_state.storage_root,
            &proof.storage_proofs[0]
        )
        .unwrap());
    }

    #[test]
    fn the_state_root_changes_with_the_accounts() {
        let one = vec![entry(1, 1, 1000)];
        let two = vec![entry(1, 1, 1000), entry(2, 2, 2000)];

        assert_ne!(accounts_state_root(&one), accounts_state_root(&two));
    }

    #[test]
    fn the_state_root_is_order_independent() {
        let a = vec![entry(1, 1, 1000), entry(2, 2, 2000), entry(3, 3, 3000)];
        let mut b = a.clone();
        b.reverse();

        assert_eq!(accounts_state_root(&a), accounts_state_root(&b));
    }

    #[test]
    fn an_empty_account_set_has_the_empty_trie_root() {
        assert_eq!(accounts_state_root(&[]).0, Trie::new().root_hash());
    }

    #[test]
    fn slot_values_encode_with_leading_zeros_stripped() {
        assert_eq!(rlp_encode_slot(&[0u8; 32]), vec![0x80]);

        let mut one = [0u8; 32];
        one[31] = 1;
        assert_eq!(rlp_encode_slot(&one), vec![0x01]);

        let mut large = [0u8; 32];
        large[30] = 0x01;
        large[31] = 0x00;
        assert_eq!(rlp_encode_slot(&large), vec![0x82, 0x01, 0x00]);
    }
}
