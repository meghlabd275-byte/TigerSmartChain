//! Durable storage for the chain, its receipts and the world state.
//!
//! Earlier versions rewrote the entire chain and the entire account set as a
//! single JSON document after every block. That is O(chain) work per block:
//! producing the millionth block would rewrite a million blocks, and the whole
//! ledger would have to be held in memory to be written at all. It also made
//! the write window proportional to the ledger size, so the exposure to a
//! crash grew with the chain.
//!
//! This module keeps a real embedded key/value store (redb, a copy-on-write
//! B-tree) and writes only what changed:
//!
//!   * each block once, keyed by its hash, when it is first inserted;
//!   * each receipt list once, keyed by block hash;
//!   * the canonical height -> hash map and the cumulative work, which are what
//!     fork choice reads on restart;
//!   * the accounts written since the last flush, keyed by address.
//!
//! Every write for one block lands in a single transaction, so a crash either
//! leaves the store at the previous block or at the new one, never in between.
//! A block is only announced to peers and subscribers after that transaction
//! has committed.
//!
//! An account record carries the storage slots and the contract code behind
//! the roots the trie commits to, not only the roots: a state that kept just
//! the roots could not execute a contract or answer `eth_getStorageAt` after a
//! restart.

use std::path::{Path, PathBuf};

use redb::{
    Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
};
use tiger_blockchain::receipt::Receipt;
use tiger_blockchain::types::Block;
use tiger_state::account::{decode_account_record, encode_account_record, Address, WorldState};

/// The on-disk format version. Bumped when the layout changes incompatibly.
pub const STORAGE_VERSION: u64 = 2;

/// `meta` holds the format version, the chain id and the head marker.
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
/// `blocks` maps a block hash to its encoded block, canonical or not.
const BLOCKS: TableDefinition<&str, &[u8]> = TableDefinition::new("blocks");
/// `receipts` maps a block hash to its encoded receipt list.
const RECEIPTS: TableDefinition<&str, &[u8]> = TableDefinition::new("receipts");
/// `canonical` maps a height to the hash that is canonical at it.
const CANONICAL: TableDefinition<u64, &str> = TableDefinition::new("canonical");
/// `work` maps a block hash to its cumulative difficulty, the fork-choice score.
const WORK: TableDefinition<&str, u128> = TableDefinition::new("work");
/// `accounts` maps a 20-byte address to its encoded account record.
const ACCOUNTS: TableDefinition<[u8; 20], &[u8]> = TableDefinition::new("accounts");

/// A failure while reading or writing the store.
#[derive(Debug)]
pub enum StorageError {
    /// The store could not be opened, or a transaction failed.
    Database(String),
    Encode(String),
    Decode { key: String, reason: String },
    /// The stored format version is not one this build understands.
    UnsupportedVersion(u64),
    /// The stored chain belongs to a different chain id.
    WrongChain { stored: u64, configured: u64 },
    /// A stored record did not match the key it was stored under, or failed
    /// its own integrity check.
    Integrity { key: String, reason: String },
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Database(reason) => write!(f, "storage backend: {reason}"),
            StorageError::Encode(reason) => write!(f, "cannot encode: {reason}"),
            StorageError::Decode { key, reason } => {
                write!(f, "cannot decode the record at {key}: {reason}")
            }
            StorageError::UnsupportedVersion(version) => {
                write!(f, "unsupported storage version {version}")
            }
            StorageError::WrongChain {
                stored,
                configured,
            } => write!(
                f,
                "the stored chain is for chain id {stored}, but this node is configured for {configured}"
            ),
            StorageError::Integrity { key, reason } => {
                write!(f, "the record at {key} failed its integrity check: {reason}")
            }
        }
    }
}

impl std::error::Error for StorageError {}

impl From<redb::DatabaseError> for StorageError {
    fn from(e: redb::DatabaseError) -> Self {
        StorageError::Database(e.to_string())
    }
}

impl From<redb::TransactionError> for StorageError {
    fn from(e: redb::TransactionError) -> Self {
        StorageError::Database(e.to_string())
    }
}

impl From<redb::TableError> for StorageError {
    fn from(e: redb::TableError) -> Self {
        StorageError::Database(e.to_string())
    }
}

impl From<redb::StorageError> for StorageError {
    fn from(e: redb::StorageError) -> Self {
        StorageError::Database(e.to_string())
    }
}

impl From<redb::SetDurabilityError> for StorageError {
    fn from(e: redb::SetDurabilityError) -> Self {
        StorageError::Database(e.to_string())
    }
}

impl From<redb::CommitError> for StorageError {
    fn from(e: redb::CommitError) -> Self {
        StorageError::Database(e.to_string())
    }
}

/// A change to the canonical branch.
///
/// Expressed as a delta rather than as the whole map: on a normal extension
/// only one height changes, and rewriting every height would make a flush cost
/// as much as the chain is long.
#[derive(Debug, Default)]
pub struct CanonicalUpdate {
    /// Heights whose canonical hash is being set.
    pub set: Vec<(u64, String)>,
    /// Heights that no longer name a block, because a reorg shortened the
    /// branch or replaced the block at them.
    pub remove: Vec<u64>,
}

impl CanonicalUpdate {
    /// Whether this update would change anything.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty() && self.remove.is_empty()
    }
}

/// One block's worth of writes, applied as a single transaction.
pub struct BlockWrite {
    /// Hash of the block, the key everything else is indexed by.
    pub hash: String,
    /// The block itself.
    pub block: Block,
    /// Its receipts, in transaction order.
    pub receipts: Vec<Receipt>,
    /// Its cumulative work.
    pub total_difficulty: u128,
    /// The canonical height this block now occupies, when it became canonical.
    pub canonical_height: Option<u64>,
}

/// The durable chain store.
pub struct Storage {
    db: Database,
    path: PathBuf,
}

impl Storage {
    /// Open (and create) the store in `dir`.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)
            .map_err(|e| StorageError::Database(format!("cannot create {}: {e}", dir.display())))?;
        let path = dir.join("chain.redb");
        let db = Database::create(&path)
            .map_err(|e| StorageError::Database(format!("cannot open {}: {e}", path.display())))?;
        Ok(Self { db, path })
    }

    /// The file the store lives in.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether anything has been stored yet.
    pub fn is_empty(&self) -> Result<bool, StorageError> {
        let read = self.db.begin_read()?;
        match read.open_table(BLOCKS) {
            Ok(table) => Ok(table.len()? == 0),
            // A table that does not exist has never been written to.
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }

    /// Record the format version and chain id on first use, and refuse to serve
    /// a store written by a different chain.
    pub fn initialise(&self, chain_id: u64) -> Result<(), StorageError> {
        // `Durability::Immediate` is the default: a commit is on disk before
        // `commit` returns, which is what lets a block be announced only after
        // it is safe.
        let write = self.db.begin_write()?;
        {
            let mut meta = write.open_table(META)?;
            // Read the existing values out first: the guards borrow the table,
            // and the table has to be borrowed mutably to insert.
            let existing_version = meta
                .get("version")?
                .map(|value| read_u64(value.value()))
                .transpose_opt("meta/version")?;
            let existing_chain = meta
                .get("chain_id")?
                .map(|value| read_u64(value.value()))
                .transpose_opt("meta/chain_id")?;
            match existing_version {
                Some(stored) => {
                    if stored != STORAGE_VERSION {
                        return Err(StorageError::UnsupportedVersion(stored));
                    }
                    if let Some(stored_chain) = existing_chain {
                        if stored_chain != chain_id {
                            return Err(StorageError::WrongChain {
                                stored: stored_chain,
                                configured: chain_id,
                            });
                        }
                    }
                }
                None => {
                    meta.insert("version", STORAGE_VERSION.to_be_bytes().as_slice())?;
                    meta.insert("chain_id", chain_id.to_be_bytes().as_slice())?;
                }
            }
        }
        write.commit()?;
        Ok(())
    }

    /// Write one block and everything derived from it, in a single transaction.
    pub fn write_block(
        &self,
        entry: &BlockWrite,
        state: &WorldState,
        accounts: &[Address],
    ) -> Result<(), StorageError> {
        self.write_batch(std::slice::from_ref(entry), state, accounts, None)
    }

    /// Write a batch of blocks, the accounts their execution wrote, and — when
    /// it changed — the canonical map, all in one transaction.
    ///
    /// One transaction means a crash cannot leave the store holding a block
    /// without the state that block produced, or a canonical map that names a
    /// block the store does not have. Either all of it is there or none of it.
    pub fn write_batch(
        &self,
        entries: &[BlockWrite],
        state: &WorldState,
        accounts: &[Address],
        canonical: Option<&CanonicalUpdate>,
    ) -> Result<(), StorageError> {
        let canonical_changed = canonical.map(|c| !c.is_empty()).unwrap_or(false);
        if entries.is_empty() && !canonical_changed && accounts.is_empty() {
            return Ok(());
        }

        // `Durability::Immediate` is the default: a commit is on disk before
        // `commit` returns, which is what lets a block be announced only after
        // it is safe.
        let write = self.db.begin_write()?;
        {
            let mut blocks_table = write.open_table(BLOCKS)?;
            let mut receipts_table = write.open_table(RECEIPTS)?;
            let mut work_table = write.open_table(WORK)?;
            let mut canonical_table = write.open_table(CANONICAL)?;
            let mut accounts_table = write.open_table(ACCOUNTS)?;
            let mut meta = write.open_table(META)?;

            for entry in entries {
                let key = entry.hash.to_ascii_lowercase();
                let encoded_block = serde_json::to_vec(&entry.block)
                    .map_err(|e| StorageError::Encode(e.to_string()))?;
                let encoded_receipts = serde_json::to_vec(&entry.receipts)
                    .map_err(|e| StorageError::Encode(e.to_string()))?;

                blocks_table.insert(key.as_str(), encoded_block.as_slice())?;
                receipts_table.insert(key.as_str(), encoded_receipts.as_slice())?;
                work_table.insert(key.as_str(), entry.total_difficulty)?;
                if let Some(height) = entry.canonical_height {
                    canonical_table.insert(height, key.as_str())?;
                }
            }

            if let Some(update) = canonical {
                for height in &update.remove {
                    canonical_table.remove(height)?;
                }
                for (height, hash) in &update.set {
                    canonical_table.insert(*height, hash.to_ascii_lowercase().as_str())?;
                }
            }

            for address in accounts {
                match state.account(address) {
                    // An account that no longer exists, or that is empty under
                    // EIP-161, is removed rather than stored as a blank record.
                    Some(account) if !tiger_state::is_empty_account(account) => {
                        let record = encode_account_record(account);
                        accounts_table.insert(address, record.as_slice())?;
                    }
                    _ => {
                        accounts_table.remove(address)?;
                    }
                }
            }

            // The head marker is the highest block this batch made
            // canonical. It is written from the canonical map rather than
            // from the block list, because a batch may contain side-branch
            // blocks that are not the head.
            let head = entries
                .iter()
                .filter_map(|entry| entry.canonical_height.map(|height| (height, &entry.hash)))
                .max_by_key(|(height, _)| *height)
                .map(|(height, hash)| (height, hash.to_ascii_lowercase()))
                .or_else(|| {
                    canonical.and_then(|update| {
                        update
                            .set
                            .iter()
                            .max_by_key(|(height, _)| *height)
                            .map(|(height, hash)| (*height, hash.to_ascii_lowercase()))
                    })
                });
            if let Some((height, hash)) = head {
                meta.insert("head", hash.as_bytes())?;
                meta.insert("height", height.to_be_bytes().as_slice())?;
            }
        }
        write.commit()?;
        Ok(())
    }

    /// Replace the canonical map with `entries`, dropping the marker from every
    /// other height.
    ///
    /// A reorg rewrites a suffix of the canonical branch. The heights above the
    /// new head must stop naming a block, or a restart would rebuild a
    /// canonical branch that no longer exists.
    pub fn write_canonical(&self, entries: &[(u64, String)]) -> Result<(), StorageError> {
        let stored: std::collections::HashMap<u64, String> =
            self.load_canonical()?.into_iter().collect();
        let keep: std::collections::HashSet<u64> = entries.iter().map(|(h, _)| *h).collect();
        let mut update = CanonicalUpdate {
            set: entries
                .iter()
                .filter(|(height, hash)| {
                    stored
                        .get(height)
                        .map(|existing| !existing.eq_ignore_ascii_case(hash))
                        .unwrap_or(true)
                })
                .cloned()
                .collect(),
            remove: stored.keys().copied().filter(|h| !keep.contains(h)).collect(),
        };
        update.set.sort_unstable();
        update.remove.sort_unstable();
        self.write_batch(&[], &WorldState::new(), &[], Some(&update))
    }

    /// The head hash recorded by the last write, if any.
    pub fn head_hash(&self) -> Result<Option<String>, StorageError> {
        let read = self.db.begin_read()?;
        let meta = match read.open_table(META) {
            Ok(meta) => meta,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        match meta.get("head")? {
            Some(value) => Ok(Some(String::from_utf8_lossy(value.value()).to_string())),
            None => Ok(None),
        }
    }

    /// Every block in the store, keyed by hash.
    ///
    /// Each block is verified against its key: the hash of its header must
    /// equal the hash it was stored under. A record that fails is reported
    /// rather than loaded, so a corrupted store cannot present a block under a
    /// hash it does not have.
    pub fn load_blocks(&self) -> Result<Vec<(String, Block)>, StorageError> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(BLOCKS) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let mut out = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            let key = key.value().to_string();
            let block: Block =
                serde_json::from_slice(value.value()).map_err(|e| StorageError::Decode {
                    key: key.clone(),
                    reason: e.to_string(),
                })?;
            let computed = tiger_blockchain::block_hash(&block.header).map_err(|e| {
                StorageError::Integrity {
                    key: key.clone(),
                    reason: format!("header cannot be hashed: {e}"),
                }
            })?;
            if !computed.eq_ignore_ascii_case(&key) {
                return Err(StorageError::Integrity {
                    key: key.clone(),
                    reason: format!("stored under {key} but hashes to {computed}"),
                });
            }
            out.push((key, block));
        }
        Ok(out)
    }

    /// The receipts of every stored block, keyed by hash.
    pub fn load_receipts(&self) -> Result<Vec<(String, Vec<Receipt>)>, StorageError> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(RECEIPTS) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            let receipts: Vec<Receipt> =
                serde_json::from_slice(value.value()).map_err(|e| StorageError::Decode {
                    key: key.value().to_string(),
                    reason: e.to_string(),
                })?;
            out.push((key.value().to_string(), receipts));
        }
        Ok(out)
    }

    /// The canonical branch, ascending by height.
    pub fn load_canonical(&self) -> Result<Vec<(u64, String)>, StorageError> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(CANONICAL) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (height, hash) = entry?;
            out.push((height.value(), hash.value().to_string()));
        }
        Ok(out)
    }

    /// The cumulative work of every stored block, keyed by hash.
    pub fn load_work(&self) -> Result<Vec<(String, u128)>, StorageError> {
        let read = self.db.begin_read()?;
        let table = match read.open_table(WORK) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (hash, work) = entry?;
            out.push((hash.value().to_string(), work.value()));
        }
        Ok(out)
    }

    /// Load the whole world state.
    ///
    /// Each record is checked as it is decoded: its storage slots must hash to
    /// the storage root it carries, and its code to its code hash. The state as
    /// a whole is then checked against the head block's `stateRoot`, so a store
    /// that was truncated or altered is reported rather than serving a ledger
    /// that disagrees with its own blocks.
    pub fn load_state(&self, expected_root: Option<&str>) -> Result<WorldState, StorageError> {
        let read = self.db.begin_read()?;
        let mut state = WorldState::new();
        if let Ok(table) = read.open_table(ACCOUNTS) {
            for entry in table.iter()? {
                let (key, value) = entry?;
                let address = key.value();
                let account = decode_account_record(value.value()).ok_or_else(|| {
                    StorageError::Integrity {
                        key: format!("0x{}", tiger_blockchain::hex_encode(&address)),
                        reason: "the record does not match its own roots".to_string(),
                    }
                })?;
                state.insert_account(address, account);
            }
        }

        if let Some(expected) = expected_root {
            let found = format!("0x{}", tiger_blockchain::hex_encode(&state.state_root()));
            if !found.eq_ignore_ascii_case(expected) {
                return Err(StorageError::Integrity {
                    key: "world state".to_string(),
                    reason: format!(
                        "restored root {found} does not match the head block's stateRoot {expected}"
                    ),
                });
            }
        }

        // Everything just read came from disk, so nothing is pending a write.
        state.clear_dirty_up_to(u64::MAX);
        Ok(state)
    }

    /// The stored format version, if a store exists.
    pub fn version(&self) -> Result<Option<u64>, StorageError> {
        let read = self.db.begin_read()?;
        let meta = match read.open_table(META) {
            Ok(meta) => meta,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        match meta.get("version")? {
            Some(value) => Ok(read_u64(value.value())),
            None => Ok(None),
        }
    }
}

/// Turn a `Some(None)` — a key that exists but whose value is not eight bytes
/// — into a decode error, leaving `None` (key absent) alone.
trait TransposeOpt<T> {
    fn transpose_opt(self, key: &str) -> Result<Option<T>, StorageError>;
}

impl TransposeOpt<u64> for Option<Option<u64>> {
    fn transpose_opt(self, key: &str) -> Result<Option<u64>, StorageError> {
        match self {
            Some(Some(value)) => Ok(Some(value)),
            Some(None) => Err(StorageError::Decode {
                key: key.to_string(),
                reason: "not eight bytes".to_string(),
            }),
            None => Ok(None),
        }
    }
}

/// Read a big-endian `u64` from a fixed eight-byte value.
fn read_u64(bytes: &[u8]) -> Option<u64> {
    let slice: [u8; 8] = bytes.try_into().ok()?;
    Some(u64::from_be_bytes(slice))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiger_blockchain::types::BlockHeader;
    use tiger_state::account::{is_empty_account, u256_from_u64};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tiger-storage-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn hex(bytes: usize, fill: u8) -> String {
        format!("0x{}", tiger_blockchain::hex_encode(&vec![fill; bytes]))
    }

    fn block(number: u64, parent: &str) -> Block {
        Block {
            header: BlockHeader {
                parent_hash: parent.to_string(),
                sha3_uncles: hex(32, 0),
                miner: hex(20, 0xaa),
                state_root: hex(32, 0),
                transactions_root: hex(32, 0),
                receipts_root: hex(32, 0),
                logs_bloom: hex(256, 0),
                difficulty: "1".to_string(),
                number,
                gas_limit: 30_000_000,
                gas_used: 0,
                timestamp: number + 1,
                extra_data: "0x".to_string(),
                mix_hash: hex(32, 0),
                nonce: "0x0000000000000000".to_string(),
                base_fee_per_gas: Some(1_000_000_000),
                withdrawals_root: None,
            },
            transactions: vec![],
            uncles: vec![],
        }
    }

    fn hash_of(b: &Block) -> String {
        tiger_blockchain::block_hash(&b.header).unwrap()
    }

    fn write_one(
        storage: &Storage,
        b: &Block,
        height: u64,
        accounts: &[Address],
        state: &WorldState,
    ) {
        storage
            .write_block(
                &BlockWrite {
                    hash: hash_of(b),
                    block: b.clone(),
                    receipts: vec![],
                    total_difficulty: height as u128,
                    canonical_height: Some(height),
                },
                state,
                accounts,
            )
            .unwrap();
    }

    #[test]
    fn blocks_round_trip_and_are_verified_against_their_key() {
        let dir = temp_dir("blocks");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let genesis = block(0, &format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])));
        let hash = hash_of(&genesis);
        write_one(&storage, &genesis, 0, &[], &WorldState::new());

        let loaded = storage.load_blocks().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, hash);
        assert_eq!(loaded[0].1.header.number, 0);
        assert_eq!(storage.head_hash().unwrap().as_deref(), Some(hash.as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_block_stored_under_the_wrong_hash_is_rejected() {
        let dir = temp_dir("wrong-hash");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        storage
            .write_block(
                &BlockWrite {
                    hash: "0xdeadbeef".to_string(),
                    block: block(1, &hex(32, 0)),
                    receipts: vec![],
                    total_difficulty: 1,
                    canonical_height: Some(1),
                },
                &WorldState::new(),
                &[],
            )
            .unwrap();

        let err = storage.load_blocks().unwrap_err();
        assert!(
            matches!(err, StorageError::Integrity { .. }),
            "a block stored under a hash it does not have must be refused, got {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accounts_round_trip_with_code_and_storage() {
        let dir = temp_dir("accounts");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let alice = [0x11u8; 20];
        let contract = [0x22u8; 20];
        let mut state = WorldState::new();
        state.set_balance(&alice, u256_from_u64(1_234_567));
        state.set_nonce(&alice, 7);
        state.set_code(&contract, vec![0x60, 0x00, 0x60, 0x00, 0xf3]);
        state.set_storage(&contract, [0x01u8; 32], [0x42u8; 32]);
        let root = format!("0x{}", tiger_blockchain::hex_encode(&state.state_root()));

        let b = block(1, &hex(32, 0));
        let dirty: Vec<Address> = state.dirty_entries().into_iter().map(|(a, _)| a).collect();
        write_one(&storage, &b, 1, &dirty, &state);

        let restored = storage.load_state(Some(&root)).unwrap();
        assert_eq!(restored.state_root(), state.state_root());
        assert_eq!(restored.balance(&alice), u256_from_u64(1_234_567));
        assert_eq!(restored.nonce(&alice), 7);
        assert_eq!(restored.code(&contract), vec![0x60, 0x00, 0x60, 0x00, 0xf3]);
        assert_eq!(restored.storage(&contract, &[0x01u8; 32]), [0x42u8; 32]);
        assert!(!restored.has_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_written_accounts_are_stored() {
        let dir = temp_dir("incremental");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let first = [0x01u8; 20];
        let second = [0x02u8; 20];
        let mut state = WorldState::new();
        state.set_balance(&first, u256_from_u64(10));
        state.set_balance(&second, u256_from_u64(20));

        let b = block(1, &hex(32, 0));
        write_one(&storage, &b, 1, &[first], &state);
        let b2 = block(2, &hash_of(&b));
        write_one(&storage, &b2, 2, &[second], &state);

        let root = format!("0x{}", tiger_blockchain::hex_encode(&state.state_root()));
        let restored = storage.load_state(Some(&root)).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.balance(&first), u256_from_u64(10));
        assert_eq!(restored.balance(&second), u256_from_u64(20));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_removed_account_is_deleted_from_the_store() {
        let dir = temp_dir("delete");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let addr = [0x07u8; 20];
        let mut state = WorldState::new();
        state.set_balance(&addr, u256_from_u64(5));
        let b = block(1, &hex(32, 0));
        write_one(&storage, &b, 1, &[addr], &state);

        state.remove_account(&addr);
        let b2 = block(2, &hash_of(&b));
        write_one(&storage, &b2, 2, &[addr], &state);

        let restored = storage.load_state(None).unwrap();
        assert!(!restored.exists(&addr), "a deleted account must not come back");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_state_that_disagrees_with_the_head_root_is_rejected() {
        let dir = temp_dir("root-mismatch");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let addr = [0x09u8; 20];
        let mut state = WorldState::new();
        state.set_balance(&addr, u256_from_u64(1));
        let b = block(1, &hex(32, 0));
        write_one(&storage, &b, 1, &[addr], &state);

        let wrong = format!("0x{}", tiger_blockchain::hex_encode(&[0xab; 32]));
        let err = storage.load_state(Some(&wrong)).unwrap_err();
        assert!(
            matches!(err, StorageError::Integrity { .. }),
            "a state that does not match the head root must be refused, got {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reorg_removes_the_abandoned_heights() {
        let dir = temp_dir("reorg");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        storage
            .write_canonical(&[
                (0, "0xa".to_string()),
                (1, "0xb".to_string()),
                (2, "0xc".to_string()),
            ])
            .unwrap();
        assert_eq!(storage.load_canonical().unwrap().len(), 3);

        storage
            .write_canonical(&[
                (0, "0xa".to_string()),
                (1, "0xd".to_string()),
                (2, "0xe".to_string()),
                (3, "0xf".to_string()),
            ])
            .unwrap();
        assert_eq!(
            storage.load_canonical().unwrap(),
            vec![
                (0, "0xa".to_string()),
                (1, "0xd".to_string()),
                (2, "0xe".to_string()),
                (3, "0xf".to_string())
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_from_another_chain_is_refused() {
        let dir = temp_dir("chain-id");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();
        assert_eq!(storage.version().unwrap(), Some(STORAGE_VERSION));

        let err = storage.initialise(1).unwrap_err();
        assert!(
            matches!(err, StorageError::WrongChain { stored: 6666, configured: 1 }),
            "a store written by another chain must be refused, got {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_version_is_rejected() {
        let dir = temp_dir("version");
        let storage = Storage::open(&dir).unwrap();
        let write = storage.db.begin_write().unwrap();
        {
            let mut meta = write.open_table(META).unwrap();
            meta.insert("version", 999u64.to_be_bytes().as_slice()).unwrap();
        }
        write.commit().unwrap();

        let err = storage.initialise(6666).unwrap_err();
        assert!(matches!(err, StorageError::UnsupportedVersion(999)), "got {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_store_reports_nothing_stored() {
        let dir = temp_dir("empty");
        let storage = Storage::open(&dir).unwrap();
        assert!(storage.is_empty().unwrap());
        assert!(storage.head_hash().unwrap().is_none());
        assert!(storage.load_blocks().unwrap().is_empty());
        assert!(storage.load_canonical().unwrap().is_empty());
        assert!(storage.load_receipts().unwrap().is_empty());
        assert_eq!(storage.load_state(None).unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_accounts_are_not_stored() {
        let dir = temp_dir("empty-account");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let addr = [0x03u8; 20];
        let mut state = WorldState::new();
        state.set_nonce(&addr, 0);
        assert!(is_empty_account(state.account(&addr).unwrap()));

        let b = block(1, &hex(32, 0));
        write_one(&storage, &b, 1, &[addr], &state);

        assert!(!storage.load_state(None).unwrap().exists(&addr));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn receipts_round_trip() {
        let dir = temp_dir("receipts");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let b = block(1, &hex(32, 0));
        let hash = hash_of(&b);
        let receipts = vec![Receipt {
            status: true,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs: vec![],
        }];
        storage
            .write_block(
                &BlockWrite {
                    hash: hash.clone(),
                    block: b,
                    receipts: receipts.clone(),
                    total_difficulty: 1,
                    canonical_height: Some(1),
                },
                &WorldState::new(),
                &[],
            )
            .unwrap();

        assert_eq!(storage.load_receipts().unwrap(), vec![(hash, receipts)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cumulative_work_round_trips() {
        let dir = temp_dir("work");
        let storage = Storage::open(&dir).unwrap();
        storage.initialise(6666).unwrap();

        let b = block(1, &hex(32, 0));
        let hash = hash_of(&b);
        storage
            .write_block(
                &BlockWrite {
                    hash: hash.clone(),
                    block: b,
                    receipts: vec![],
                    total_difficulty: 9_999,
                    canonical_height: Some(1),
                },
                &WorldState::new(),
                &[],
            )
            .unwrap();

        assert_eq!(storage.load_work().unwrap(), vec![(hash, 9_999)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
