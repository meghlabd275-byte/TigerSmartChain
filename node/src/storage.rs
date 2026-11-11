//! Durable storage for the chain.
//!
//! The node previously accepted a `--data-dir` and ignored it, so every
//! restart discarded every block. This module writes the chain, its receipts
//! and the world state to disk and reads them back on startup.
//!
//! Each snapshot is written to a temporary file, flushed to the platform, and
//! then renamed over the destination. A rename is atomic, so a crash mid-write
//! leaves the previous snapshot intact rather than a half-written file. The
//! on-disk format carries a version so a future change can be detected rather
//! than misread.
//!
//! Every snapshot records the state root it was taken at. On load the state
//! root is recomputed from the restored accounts and compared; a mismatch
//! means the file was truncated or altered, and the load fails loudly instead
//! of running on a corrupted ledger.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tiger_blockchain::receipt::Receipt;
use tiger_blockchain::types::Block;
use tiger_state::account::{Address, WorldState};

/// The on-disk format version. Bumped when the layout changes incompatibly.
pub const STORAGE_VERSION: u32 = 1;

const CHAIN_FILE: &str = "chain.json";
const STATE_FILE: &str = "state.json";

/// A failure while reading or writing the chain.
#[derive(Debug)]
pub enum StorageError {
    Io { path: PathBuf, source: std::io::Error },
    Encode(String),
    Decode { path: PathBuf, reason: String },
    /// The restored state does not hash to the root the snapshot recorded.
    StateRootMismatch { expected: String, found: String },
    UnsupportedVersion(u32),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Io { path, source } => {
                write!(f, "cannot access {}: {source}", path.display())
            }
            StorageError::Encode(reason) => write!(f, "cannot encode snapshot: {reason}"),
            StorageError::Decode { path, reason } => {
                write!(f, "cannot decode {}: {reason}", path.display())
            }
            StorageError::StateRootMismatch { expected, found } => write!(
                f,
                "restored state root {found} does not match the recorded root {expected}"
            ),
            StorageError::UnsupportedVersion(version) => {
                write!(f, "unsupported storage version {version}")
            }
        }
    }
}

impl std::error::Error for StorageError {}

/// The chain as it is written to disk: every block, its receipts, and the
/// genesis definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSnapshot {
    pub version: u32,
    pub config: tiger_blockchain::types::ChainConfig,
    pub genesis: Option<tiger_blockchain::types::Genesis>,
    /// Blocks by number. Headers are derived from the blocks on load, so they
    /// are not stored twice.
    pub blocks: Vec<Block>,
    /// Receipts keyed by block number.
    pub receipts: Vec<(u64, Vec<Receipt>)>,
}

/// The world state as it is written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub version: u32,
    /// The state root at the time of the snapshot, as a hex string.
    pub state_root: String,
    /// Accounts with their address hex-encoded, because a JSON object key
    /// must be a string and a 20-byte array is not.
    pub accounts: Vec<(String, tiger_state::account::Account)>,
}

/// The directory the chain is stored in.
#[derive(Debug, Clone)]
pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    /// Open (and create) the storage directory.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|source| StorageError::Io {
            path: dir.clone(),
            source,
        })?;
        Ok(Self { dir })
    }

    pub fn chain_path(&self) -> PathBuf {
        self.dir.join(CHAIN_FILE)
    }

    pub fn state_path(&self) -> PathBuf {
        self.dir.join(STATE_FILE)
    }

    /// Whether a stored chain exists.
    pub fn chain_exists(&self) -> bool {
        self.chain_path().is_file()
    }

    /// Write the chain snapshot atomically.
    pub fn save_chain(&self, snapshot: &ChainSnapshot) -> Result<(), StorageError> {
        let bytes = serde_json::to_vec(snapshot)
            .map_err(|e| StorageError::Encode(e.to_string()))?;
        write_atomic(&self.chain_path(), &bytes)
    }

    /// Read the chain snapshot. Returns `None` when nothing is stored yet.
    pub fn load_chain(&self) -> Result<Option<ChainSnapshot>, StorageError> {
        let path = self.chain_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(StorageError::Io { path, source }),
        };
        let snapshot: ChainSnapshot = serde_json::from_slice(&bytes).map_err(|e| {
            StorageError::Decode {
                path: path.clone(),
                reason: e.to_string(),
            }
        })?;
        if snapshot.version != STORAGE_VERSION {
            return Err(StorageError::UnsupportedVersion(snapshot.version));
        }
        Ok(Some(snapshot))
    }

    /// Write the world state snapshot atomically.
    pub fn save_state(&self, state: &WorldState) -> Result<(), StorageError> {
        let snapshot = StateSnapshot {
            version: STORAGE_VERSION,
            state_root: format!("0x{}", tiger_blockchain::hex_encode(&state.state_root())),
            accounts: state
                .iter_sorted()
                .into_iter()
                .map(|(address, account)| {
                    (
                        format!("0x{}", tiger_blockchain::hex_encode(&address)),
                        account.clone(),
                    )
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|e| StorageError::Encode(e.to_string()))?;
        write_atomic(&self.state_path(), &bytes)
    }

    /// Read the world state. Returns `None` when nothing is stored yet.
    ///
    /// The restored state's root is recomputed and checked against the root
    /// recorded in the snapshot, so a corrupted file is detected here rather
    /// than producing a chain that silently disagrees with its own blocks.
    pub fn load_state(&self) -> Result<Option<WorldState>, StorageError> {
        let path = self.state_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(StorageError::Io { path, source }),
        };
        let snapshot: StateSnapshot = serde_json::from_slice(&bytes).map_err(|e| {
            StorageError::Decode {
                path: path.clone(),
                reason: e.to_string(),
            }
        })?;
        if snapshot.version != STORAGE_VERSION {
            return Err(StorageError::UnsupportedVersion(snapshot.version));
        }

        let mut state = WorldState::new();
        for (encoded_address, account) in snapshot.accounts {
            let bytes = crate::rpc_server::decode_hex(&encoded_address).map_err(|e| {
                StorageError::Decode {
                    path: path.clone(),
                    reason: format!("bad account address {encoded_address}: {e}"),
                }
            })?;
            if bytes.len() != 20 {
                return Err(StorageError::Decode {
                    path: path.clone(),
                    reason: format!(
                        "account address {encoded_address} is {} bytes, expected 20",
                        bytes.len()
                    ),
                });
            }
            let mut address: Address = [0u8; 20];
            address.copy_from_slice(&bytes);
            state.insert_account(address, account);
        }

        let found = format!("0x{}", tiger_blockchain::hex_encode(&state.state_root()));
        if found != snapshot.state_root {
            return Err(StorageError::StateRootMismatch {
                expected: snapshot.state_root,
                found,
            });
        }
        Ok(Some(state))
    }

    /// Build a chain snapshot from a manager's blocks and receipts.
    pub fn snapshot_chain(
        config: tiger_blockchain::types::ChainConfig,
        genesis: Option<tiger_blockchain::types::Genesis>,
        blocks: Vec<Block>,
        receipts: Vec<(u64, Vec<Receipt>)>,
    ) -> ChainSnapshot {
        ChainSnapshot {
            version: STORAGE_VERSION,
            config,
            genesis,
            blocks,
            receipts,
        }
    }
}

/// Write `bytes` to `path` atomically: write a temporary file in the same
/// directory, flush it, then rename it over the destination.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("snapshot")
    ));

    {
        let mut file = fs::File::create(&tmp).map_err(|source| StorageError::Io {
            path: tmp.clone(),
            source,
        })?;
        file.write_all(bytes).map_err(|source| StorageError::Io {
            path: tmp.clone(),
            source,
        })?;
        // Flush to the operating system before the rename, so the rename
        // cannot be observed before the contents.
        file.sync_all().map_err(|source| StorageError::Io {
            path: tmp.clone(),
            source,
        })?;
    }

    fs::rename(&tmp, path).map_err(|source| StorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiger_blockchain::types::BlockHeader;
    use tiger_state::account::u256_from_u64;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tiger-storage-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn state_round_trips_through_disk() {
        let dir = temp_dir("state-roundtrip");
        let storage = Storage::open(&dir).unwrap();

        let mut state = WorldState::new();
        let alice = [0x11u8; 20];
        let contract = [0x22u8; 20];
        state.set_balance(&alice, u256_from_u64(1_234_567));
        state.set_nonce(&alice, 7);
        state.set_code(&contract, vec![0x60, 0x00, 0x60, 0x00, 0xf3]);
        state.set_storage(&contract, [0x01u8; 32], [0x42u8; 32]);
        let root_before = state.state_root();

        storage.save_state(&state).unwrap();
        let restored = storage.load_state().unwrap().expect("state must exist");

        assert_eq!(restored.state_root(), root_before);
        assert_eq!(restored.balance(&alice), u256_from_u64(1_234_567));
        assert_eq!(restored.nonce(&alice), 7);
        assert_eq!(restored.code(&contract), vec![0x60, 0x00, 0x60, 0x00, 0xf3]);
        assert_eq!(restored.storage(&contract, &[0x01u8; 32]), [0x42u8; 32]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_state_file_is_rejected() {
        let dir = temp_dir("state-tamper");
        let storage = Storage::open(&dir).unwrap();

        let mut state = WorldState::new();
        state.set_balance(&[0x11u8; 20], u256_from_u64(1_000));
        storage.save_state(&state).unwrap();

        // Rewrite the file with a different balance but the old root.
        let mut snapshot: StateSnapshot =
            serde_json::from_slice(&fs::read(storage.state_path()).unwrap()).unwrap();
        snapshot.accounts[0].1.balance = u256_from_u64(9_999_999);
        fs::write(
            storage.state_path(),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();

        let err = storage.load_state().unwrap_err();
        assert!(
            matches!(err, StorageError::StateRootMismatch { .. }),
            "a tampered state must be rejected, got {err}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_files_load_as_none() {
        let dir = temp_dir("state-missing");
        let storage = Storage::open(&dir).unwrap();
        assert!(storage.load_state().unwrap().is_none());
        assert!(storage.load_chain().unwrap().is_none());
        assert!(!storage.chain_exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_chain_snapshot_round_trips() {
        let dir = temp_dir("chain-roundtrip");
        let storage = Storage::open(&dir).unwrap();

        let block = Block {
            header: BlockHeader {
                parent_hash: "0x00".to_string(),
                sha3_uncles: "0x00".to_string(),
                miner: "0xaa".to_string(),
                state_root: "0x00".to_string(),
                transactions_root: "0x00".to_string(),
                receipts_root: "0x00".to_string(),
                logs_bloom: "0x00".to_string(),
                difficulty: "1".to_string(),
                number: 0,
                gas_limit: 30_000_000,
                gas_used: 0,
                timestamp: 1,
                extra_data: "0x".to_string(),
                mix_hash: "0x00".to_string(),
                nonce: "0x0".to_string(),
                base_fee_per_gas: None,
                withdrawals_root: None,
            },
            transactions: vec![],
            uncles: vec![],
        };

        let snapshot = Storage::snapshot_chain(
            tiger_blockchain::types::ChainConfig {
                chain_id: 6666,
                ..Default::default()
            },
            None,
            vec![block],
            vec![(0, vec![])],
        );
        storage.save_chain(&snapshot).unwrap();
        assert!(storage.chain_exists());

        let loaded = storage.load_chain().unwrap().expect("chain must exist");
        assert_eq!(loaded.config.chain_id, 6666);
        assert_eq!(loaded.blocks.len(), 1);
        assert_eq!(loaded.blocks[0].header.number, 0);
        assert_eq!(loaded.receipts.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_version_is_rejected() {
        let dir = temp_dir("chain-version");
        let storage = Storage::open(&dir).unwrap();
        fs::write(
            storage.chain_path(),
            br#"{"version":999,"config":{"chain_id":1},"genesis":null,"blocks":[],"receipts":[]}"#,
        )
        .unwrap();
        let err = storage.load_chain().unwrap_err();
        assert!(matches!(err, StorageError::UnsupportedVersion(999)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_is_reported_not_ignored() {
        let dir = temp_dir("chain-corrupt");
        let storage = Storage::open(&dir).unwrap();
        fs::write(storage.chain_path(), b"this is not json").unwrap();
        assert!(matches!(
            storage.load_chain().unwrap_err(),
            StorageError::Decode { .. }
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}
