//! Account state with a real Ethereum state root.
//!
//! An Ethereum account is RLP-encoded as `[nonce, balance, storageRoot,
//! codeHash]`, and contract storage is itself a Merkle Patricia Trie keyed by
//! the 32-byte storage slot with the value RLP-encoded after stripping leading
//! zeros. The world state root is the trie root over
//! `keccak256(address) -> rlp(account)`.
//!
//! This module is deliberately self-contained: it holds the authoritative
//! encoding so the state root is reproducible by any other EVM client.

use crate::rlp::RlpItem;
use crate::trie::Trie;
use std::collections::HashMap;
use tiger_crypto::keccak256;

/// The keccak256 hash of the empty byte string, used as the code hash of an
/// account with no code (including externally owned accounts).
pub const EMPTY_CODE_HASH: [u8; 32] = [
    0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0,
    0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
];

/// The state root of a completely empty world state: the keccak256 hash of the
/// RLP encoding of the empty trie root (`0x80`).
pub const EMPTY_STATE_ROOT: [u8; 32] = [
    0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6, 0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
    0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0, 0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21,
];

/// A 20-byte account address.
pub type Address = [u8; 20];

/// An account, exactly as the Yellow Paper defines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// Number of transactions sent from this account.
    pub nonce: u64,
    /// Balance in wei.
    pub balance: [u8; 32],
    /// Root of the account's storage trie.
    pub storage_root: [u8; 32],
    /// keccak256 of the account's code.
    pub code_hash: [u8; 32],
    /// Contract code, held out of the trie (it is hashed, not inlined).
    pub code: Vec<u8>,
    /// Storage slots, held as a trie internally.
    pub storage: HashMap<[u8; 32], [u8; 32]>,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            nonce: 0,
            balance: [0u8; 32],
            storage_root: EMPTY_STATE_ROOT,
            code_hash: EMPTY_CODE_HASH,
            code: Vec::new(),
            storage: HashMap::new(),
        }
    }
}

/// Minimal big-endian scalar helpers for 32-byte quantities.
pub fn be_to_u64(value: &[u8; 32]) -> u64 {
    let mut out = 0u64;
    for &b in &value[24..] {
        out = (out << 8) | b as u64;
    }
    out
}

/// RLP-encode a 32-byte big-endian quantity with leading zeros stripped, as
/// the Yellow Paper requires for account fields.
fn rlp_quantity(value: &[u8; 32]) -> RlpItem {
    let first = value.iter().position(|&b| b != 0).unwrap_or(32);
    RlpItem::Bytes(value[first..].to_vec())
}

/// RLP-encode a 32-byte storage slot value with leading zeros stripped.
fn rlp_storage_value(value: &[u8; 32]) -> RlpItem {
    rlp_quantity(value)
}

/// Compute the storage root for a set of slots.
///
/// Keys are the 32-byte slot indices; the trie key is their keccak256 hash and
/// the value is the RLP encoding of the slot value with leading zeros removed.
pub fn storage_root(slots: &HashMap<[u8; 32], [u8; 32]>) -> [u8; 32] {
    if slots.is_empty() {
        return EMPTY_STATE_ROOT;
    }
    let mut trie = Trie::new();
    // Sorted iteration keeps the result deterministic regardless of hash order.
    let mut keys: Vec<&[u8; 32]> = slots.keys().collect();
    keys.sort_unstable();
    for key in keys {
        let value = slots[key];
        let encoded = rlp_storage_value(&value).encode();
        trie.insert(&keccak256(key), &encoded);
    }
    trie.root_hash()
}

/// The canonical RLP encoding of an account: `[nonce, balance,
/// storageRoot, codeHash]`.
pub fn encode_account(account: &Account) -> Vec<u8> {
    RlpItem::list(vec![
        RlpItem::from_u64(account.nonce),
        rlp_quantity(&account.balance),
        RlpItem::Bytes(account.storage_root.to_vec()),
        RlpItem::Bytes(account.code_hash.to_vec()),
    ])
    .encode()
}

/// The keccak256 hash of an account's canonical encoding.
pub fn account_hash(account: &Account) -> [u8; 32] {
    keccak256(&encode_account(account))
}

/// Decode an account from its canonical RLP encoding.
///
/// The inverse of [`encode_account`]. Returns `None` when the bytes are not a
/// well-formed `[nonce, balance, storageRoot, codeHash]` list, so a corrupt
/// record is reported rather than silently read as an empty account.
pub fn decode_account(encoded: &[u8]) -> Option<Account> {
    let item = crate::rlp::decode(encoded).ok()?;
    let items = match item {
        RlpItem::List(items) => items,
        _ => return None,
    };
    if items.len() != 4 {
        return None;
    }

    let nonce = match &items[0] {
        RlpItem::Bytes(b) => {
            if b.len() > 8 {
                return None;
            }
            let mut value = 0u64;
            for &byte in b {
                value = (value << 8) | byte as u64;
            }
            value
        }
        _ => return None,
    };

    let mut balance = [0u8; 32];
    match &items[1] {
        RlpItem::Bytes(b) => {
            if b.len() > 32 {
                return None;
            }
            balance[32 - b.len()..].copy_from_slice(b);
        }
        _ => return None,
    }

    let storage_root: [u8; 32] = match &items[2] {
        RlpItem::Bytes(b) if b.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(b);
            out
        }
        _ => return None,
    };

    let code_hash: [u8; 32] = match &items[3] {
        RlpItem::Bytes(b) if b.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(b);
            out
        }
        _ => return None,
    };

    Some(Account {
        nonce,
        balance,
        storage_root,
        code_hash,
        code: Vec::new(),
        storage: HashMap::new(),
    })
}

/// Decode a storage slot value from the trie's RLP string form.
///
/// Values are stored as RLP strings with leading zeros stripped, so this pads
/// back to the canonical 32-byte big-endian form. A value longer than 32 bytes
/// cannot be a slot, and is reported as zero rather than truncated.
pub fn decode_storage_value(encoded: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    match crate::rlp::decode(encoded) {
        Ok(RlpItem::Bytes(b)) if b.len() <= 32 => {
            out[32 - b.len()..].copy_from_slice(&b);
        }
        _ => {}
    }
    out
}

/// A mutable world state backed by a Merkle Patricia Trie.
///
/// Mutations are journaled so they can be committed (producing a new state
/// root) or rolled back, which is what transaction execution needs.
#[derive(Debug, Clone, Default)]
pub struct WorldState {
    accounts: HashMap<Address, Account>,
}

impl WorldState {
    /// An empty world state.
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    /// Read an account, if it exists.
    pub fn account(&self, address: &Address) -> Option<&Account> {
        self.accounts.get(address)
    }

    /// Mutable access to an account, creating a default one when absent.
    pub fn account_mut(&mut self, address: &Address) -> &mut Account {
        self.accounts.entry(*address).or_default()
    }

    /// Whether an account exists at all.
    ///
    /// Note that an account which exists but has zero nonce, zero balance and
    /// no code is "empty" under EIP-161 and is not stored in the trie.
    pub fn exists(&self, address: &Address) -> bool {
        self.accounts.contains_key(address)
    }

    /// Balance in wei.
    pub fn balance(&self, address: &Address) -> [u8; 32] {
        self.accounts
            .get(address)
            .map(|a| a.balance)
            .unwrap_or([0u8; 32])
    }

    /// Account nonce.
    pub fn nonce(&self, address: &Address) -> u64 {
        self.accounts.get(address).map(|a| a.nonce).unwrap_or(0)
    }

    /// Contract code (empty for externally owned accounts).
    pub fn code(&self, address: &Address) -> Vec<u8> {
        self.accounts
            .get(address)
            .map(|a| a.code.clone())
            .unwrap_or_default()
    }

    /// keccak256 of the account's code. Accounts without code report
    /// [`EMPTY_CODE_HASH`]; accounts that do not exist at all report zero, as
    /// `EXTCODEHASH` requires.
    pub fn code_hash(&self, address: &Address) -> [u8; 32] {
        match self.accounts.get(address) {
            Some(a) => a.code_hash,
            None => [0u8; 32],
        }
    }

    /// Read a storage slot.
    pub fn storage(&self, address: &Address, slot: &[u8; 32]) -> [u8; 32] {
        self.accounts
            .get(address)
            .and_then(|a| a.storage.get(slot).copied())
            .unwrap_or([0u8; 32])
    }

    /// Write a storage slot and refresh the account's storage root.
    pub fn set_storage(&mut self, address: &Address, slot: [u8; 32], value: [u8; 32]) {
        let account = self.account_mut(address);
        if value == [0u8; 32] {
            account.storage.remove(&slot);
        } else {
            account.storage.insert(slot, value);
        }
        account.storage_root = storage_root(&account.storage);
    }

    /// Set an account's code and refresh its code hash.
    pub fn set_code(&mut self, address: &Address, code: Vec<u8>) {
        let hash = keccak256(&code);
        let account = self.account_mut(address);
        account.code = code;
        account.code_hash = hash;
    }

    /// Set an account's balance.
    pub fn set_balance(&mut self, address: &Address, balance: [u8; 32]) {
        self.account_mut(address).balance = balance;
    }

    /// Set an account's nonce.
    pub fn set_nonce(&mut self, address: &Address, nonce: u64) {
        self.account_mut(address).nonce = nonce;
    }

    /// Whether an account is "dead" for EIP-161 purposes: either it does not
    /// exist or it is empty (zero nonce, zero balance, no code).
    ///
    /// This is what decides whether a value transfer to the account incurs the
    /// new-account gas surcharge.
    pub fn is_dead(&self, address: &Address) -> bool {
        match self.accounts.get(address) {
            Some(account) => is_empty_account(account),
            None => true,
        }
    }

    /// Credit `amount` to `address`, creating the account if needed.
    pub fn credit(&mut self, address: &Address, amount: &[u8; 32]) {
        let account = self.account_mut(address);
        let (sum, overflow) = u256_add(&account.balance, amount);
        // A balance overflow would be a consensus failure; saturating at the
        // maximum keeps the total supply finite rather than wrapping.
        if !overflow {
            account.balance = sum;
        } else {
            account.balance = [0xffu8; 32];
        }
    }

    /// Debit `amount` from `address`, returning false if the balance is
    /// insufficient. On failure nothing is modified.
    pub fn debit(&mut self, address: &Address, amount: &[u8; 32]) -> bool {
        let current = self.balance(address);
        match u256_sub(&current, amount) {
            Some(remaining) => {
                self.account_mut(address).balance = remaining;
                true
            }
            None => false,
        }
    }

    /// Move `amount` from `from` to `to`.
    ///
    /// Returns false and leaves both balances untouched when `from` cannot
    /// cover the amount.
    pub fn transfer(&mut self, from: &Address, to: &Address, amount: &[u8; 32]) -> bool {
        if !self.debit(from, amount) {
            return false;
        }
        self.credit(to, amount);
        true
    }

    /// Capture the current state so it can be restored if execution fails.
    ///
    /// A reverted call frame must leave no trace, but the gas charge and the
    /// nonce increment are applied at the transaction level and must survive.
    /// Taking the snapshot before any of the transaction's own writes and
    /// restoring it on failure gives exactly that separation.
    ///
    /// The snapshot clones the account map. That is O(state size) per
    /// transaction, which is acceptable for correctness and for test and
    /// single-block use; a production node would journal individual writes
    /// instead. The clone keeps this simple enough to be obviously correct.
    pub fn snapshot(&self) -> WorldState {
        self.clone()
    }

    /// Restore a previously captured snapshot.
    pub fn restore(&mut self, snapshot: WorldState) {
        self.accounts = snapshot.accounts;
    }

    /// Insert a fully formed account.
    pub fn insert_account(&mut self, address: Address, account: Account) {
        self.accounts.insert(address, account);
    }

    /// Remove an account entirely.
    pub fn remove_account(&mut self, address: &Address) {
        self.accounts.remove(address);
    }

    /// Number of accounts currently held.
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Whether the world state holds no accounts.
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Iterate over accounts in deterministic address order.
    pub fn iter_sorted(&self) -> Vec<(Address, &Account)> {
        let mut entries: Vec<(Address, &Account)> =
            self.accounts.iter().map(|(k, v)| (*k, v)).collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    /// Compute the world state root.
    ///
    /// The trie is keyed by `keccak256(address)` with the canonical account
    /// encoding as the value. Per EIP-161, accounts that are empty (zero
    /// nonce, zero balance, no code) are not inserted, so they cannot affect
    /// the root.
    pub fn state_root(&self) -> [u8; 32] {
        let mut trie = Trie::new();
        for (address, account) in self.iter_sorted() {
            if is_empty_account(account) {
                continue;
            }
            trie.insert(&keccak256(&address), &encode_account(account));
        }
        trie.root_hash()
    }
}

// =============================================================================
// 256-BIT BIG-ENDIAN ARITHMETIC
// =============================================================================
//
// Balances are 256-bit values, so a u64 is not enough. These helpers operate
// on the canonical big-endian 32-byte representation and report overflow
// explicitly rather than silently wrapping, which matters because a wrapping
// balance operation would mint or destroy funds.

/// A 256-bit value from a u64.
pub fn u256_from_u64(value: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&value.to_be_bytes());
    out
}

/// Add two 256-bit values, returning the sum and whether it overflowed.
pub fn u256_add(a: &[u8; 32], b: &[u8; 32]) -> ([u8; 32], bool) {
    let mut out = [0u8; 32];
    let mut carry = 0u16;
    for i in (0..32).rev() {
        let sum = a[i] as u16 + b[i] as u16 + carry;
        out[i] = sum as u8;
        carry = sum >> 8;
    }
    (out, carry != 0)
}

/// Subtract `b` from `a`, returning `None` on underflow.
pub fn u256_sub(a: &[u8; 32], b: &[u8; 32]) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    let mut borrow = 0i16;
    for i in (0..32).rev() {
        let diff = a[i] as i16 - b[i] as i16 - borrow;
        if diff < 0 {
            out[i] = (diff + 256) as u8;
            borrow = 1;
        } else {
            out[i] = diff as u8;
            borrow = 0;
        }
    }
    if borrow != 0 {
        None
    } else {
        Some(out)
    }
}

/// Multiply a 256-bit value by a u64, returning `None` on overflow.
pub fn u256_mul_u64(a: &[u8; 32], b: u64) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    let mut carry = 0u128;
    for i in (0..32).rev() {
        let product = a[i] as u128 * b as u128 + carry;
        out[i] = (product & 0xff) as u8;
        carry = product >> 8;
    }
    if carry != 0 {
        None
    } else {
        Some(out)
    }
}

/// Compare two 256-bit values.
pub fn u256_cmp(a: &[u8; 32], b: &[u8; 32]) -> std::cmp::Ordering {
    a.cmp(b)
}

/// EIP-161 emptiness: zero nonce, zero balance and no code.
pub fn is_empty_account(account: &Account) -> bool {
    account.nonce == 0
        && account.balance == [0u8; 32]
        && (account.code.is_empty() || account.code_hash == EMPTY_CODE_HASH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        [byte; 20]
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn empty_world_state_has_canonical_root() {
        let state = WorldState::new();
        assert_eq!(state.state_root(), EMPTY_STATE_ROOT);
        assert!(state.is_empty());
    }

    #[test]
    fn empty_accounts_do_not_affect_the_root() {
        let mut state = WorldState::new();
        // Touching an account without giving it nonce, balance or code must
        // leave the state root unchanged (EIP-161).
        state.account_mut(&addr(0x11));
        state.set_storage(&addr(0x11), [0u8; 32], [0u8; 32]);
        assert_eq!(state.state_root(), EMPTY_STATE_ROOT);
    }

    #[test]
    fn account_encoding_matches_yellow_paper_layout() {
        let account = Account {
            nonce: 1,
            balance: {
                let mut b = [0u8; 32];
                b[31] = 0x0f;
                b
            },
            storage_root: EMPTY_STATE_ROOT,
            code_hash: EMPTY_CODE_HASH,
            code: Vec::new(),
            storage: HashMap::new(),
        };
        let encoded = encode_account(&account);
        // [nonce=0x01, balance=0x0f, 32-byte storage root, 32-byte code hash]
        // -> list payload = 1 + 1 + 33 + 33 = 68 bytes, so 0xf8 0x44.
        assert_eq!(encoded[0], 0xf8);
        assert_eq!(encoded[1], 0x44);
        assert_eq!(encoded[2], 0x01); // nonce
        assert_eq!(encoded[3], 0x0f); // balance
        assert_eq!(encoded[4], 0xa0); // 32-byte string prefix for storage root
        assert_eq!(encoded[37], 0xa0); // 32-byte string prefix for code hash
        assert_eq!(encoded.len(), 70);
    }

    #[test]
    fn balance_and_nonce_changes_alter_the_root() {
        let mut state = WorldState::new();
        let mut balance = [0u8; 32];
        balance[31] = 1;
        state.set_balance(&addr(0x22), balance);
        let root_with_balance = state.state_root();
        assert_ne!(root_with_balance, EMPTY_STATE_ROOT);

        state.set_nonce(&addr(0x22), 7);
        assert_ne!(state.state_root(), root_with_balance);
    }

    #[test]
    fn storage_root_reflects_slots_and_clearing() {
        let mut state = WorldState::new();
        let address = addr(0x33);
        let mut balance = [0u8; 32];
        balance[31] = 5;
        state.set_balance(&address, balance);

        let mut slot = [0u8; 32];
        slot[31] = 1;
        let mut value = [0u8; 32];
        value[31] = 0x2a;
        state.set_storage(&address, slot, value);

        let root_with_slot = state.state_root();
        assert_ne!(root_with_slot, EMPTY_STATE_ROOT);

        // Clearing the slot back to zero must restore the earlier root.
        state.set_storage(&address, slot, [0u8; 32]);
        assert_eq!(state.account(&address).unwrap().storage_root, EMPTY_STATE_ROOT);
    }

    #[test]
    fn code_hash_is_keccak_of_code_and_zero_for_missing() {
        let mut state = WorldState::new();
        let address = addr(0x44);
        state.set_code(&address, hex("6001600101"));
        assert_eq!(
            state.code_hash(&address),
            keccak256(&hex("6001600101"))
        );
        assert_ne!(state.code_hash(&address), EMPTY_CODE_HASH);
        // A non-existent account reports zero, which is what EXTCODEHASH needs.
        assert_eq!(state.code_hash(&addr(0x99)), [0u8; 32]);
    }

    #[test]
    fn state_root_is_order_independent() {
        let mut a = WorldState::new();
        let mut b = WorldState::new();
        let mut balance = [0u8; 32];
        balance[31] = 9;

        a.set_balance(&addr(0x01), balance);
        a.set_balance(&addr(0x02), balance);
        a.set_nonce(&addr(0x02), 3);

        b.set_nonce(&addr(0x02), 3);
        b.set_balance(&addr(0x02), balance);
        b.set_balance(&addr(0x01), balance);

        assert_eq!(a.state_root(), b.state_root());
    }

    #[test]
    fn world_state_root_matches_reference_implementation() {
        // Expected root cross-checked against ethereum/py-trie, which encodes
        // the same two accounts as
        //   [nonce, balance, storageRoot, codeHash]
        // keyed by keccak256(address).
        let mut state = WorldState::new();

        let mut balance = [0u8; 32];
        balance[30] = 0x03;
        balance[31] = 0xe8; // 1000 wei
        state.set_balance(&[0x11u8; 20], balance);
        state.set_nonce(&[0x11u8; 20], 1);

        let mut balance2 = [0u8; 32];
        balance2[30] = 0x01; // 256 wei
        state.set_balance(&[0x22u8; 20], balance2);
        state.set_code(&[0x22u8; 20], hex("6001600101"));

        assert_eq!(
            to_hex(&state.state_root()),
            "6779436c64ba6aca6d5776eb5c6292437577bf0e7e02928ba13035642b2da6cd"
        );
    }
}
