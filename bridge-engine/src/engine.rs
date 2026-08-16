//! Bridge Engine - Main Entry Point

use crate::{Chain, ChainConfig, BridgeConfig, Transfer, TransferStatus, TokenType};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// SQL schema for persisted bridge transfers. Created idempotently on init
/// when a database_url is configured.
const BRIDGE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS bridge_transfers (
    id              TEXT PRIMARY KEY,
    source_chain    TEXT NOT NULL,
    destination_chain TEXT NOT NULL,
    sender          TEXT NOT NULL,
    recipient       TEXT NOT NULL,
    token           TEXT NOT NULL,
    token_type      TEXT NOT NULL,
    amount          TEXT NOT NULL,
    token_id        TEXT,
    status          TEXT NOT NULL,
    source_tx       TEXT NOT NULL,
    destination_tx  TEXT,
    timestamp       BIGINT NOT NULL,
    confirmations   BIGINT NOT NULL DEFAULT 0,
    relayer         TEXT,
    relayer_sig     TEXT
);
CREATE INDEX IF NOT EXISTS idx_bridge_status ON bridge_transfers(status);
CREATE INDEX IF NOT EXISTS idx_bridge_sender ON bridge_transfers(sender);

CREATE TABLE IF NOT EXISTS bridge_locks (
    id              TEXT PRIMARY KEY,
    source_chain    TEXT NOT NULL,
    target_chain    TEXT NOT NULL,
    user_addr       TEXT NOT NULL,
    token           TEXT NOT NULL,
    amount          TEXT NOT NULL,
    source_tx       TEXT NOT NULL,
    recipient       TEXT,
    timestamp       BIGINT NOT NULL,
    status          TEXT NOT NULL,
    relayer_pubkey  TEXT,
    relayer_sig     TEXT,
    mint_tx         TEXT
);
CREATE INDEX IF NOT EXISTS idx_bridge_locks_status ON bridge_locks(status);
CREATE INDEX IF NOT EXISTS idx_bridge_locks_user ON bridge_locks(user_addr);

CREATE TABLE IF NOT EXISTS bridge_burns (
    id              TEXT PRIMARY KEY,
    source_chain    TEXT NOT NULL,
    target_chain    TEXT NOT NULL,
    user_addr       TEXT NOT NULL,
    token           TEXT NOT NULL,
    amount          TEXT NOT NULL,
    burn_tx         TEXT NOT NULL,
    timestamp       BIGINT NOT NULL,
    status          TEXT NOT NULL,
    relayer_pubkey  TEXT,
    relayer_sig     TEXT,
    unlock_tx       TEXT
);
CREATE INDEX IF NOT EXISTS idx_bridge_burns_status ON bridge_burns(status);
CREATE INDEX IF NOT EXISTS idx_bridge_burns_user ON bridge_burns(user_addr);
"#;

/// Lifecycle status of a lock event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockStatus {
    /// Tokens locked on the source chain, awaiting relayer attestation.
    Locked,
    /// Wrapped tokens minted on the target chain.
    Minted,
}

/// Lifecycle status of a burn event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BurnStatus {
    /// Wrapped tokens burned on the target chain, awaiting relayer attestation.
    Burned,
    /// Original tokens unlocked on the source chain.
    Unlocked,
}

/// A lock event: a user locked `amount` of `token` on `source_chain` to be
/// bridged to `target_chain`. `id` is a deterministic hash of the parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LockEvent {
    pub id: String,
    pub source_chain: Chain,
    pub target_chain: Chain,
    pub user: String,
    pub token: String,
    pub amount: u128,
    /// Hash of the on-chain lock transaction on the source chain.
    pub source_tx: String,
    /// Optional recipient for the minted wrapped tokens (defaults to `user`).
    pub recipient: Option<String>,
    pub timestamp: i64,
    pub status: LockStatus,
}

/// A burn event: a user burned `amount` of wrapped `token` on `target_chain`
/// to be unlocked back on `source_chain`. `id` is a deterministic hash of the
/// parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BurnEvent {
    pub id: String,
    pub source_chain: Chain,
    pub target_chain: Chain,
    pub user: String,
    pub token: String,
    pub amount: u128,
    /// Hash of the on-chain burn transaction on the target chain.
    pub burn_tx: String,
    pub timestamp: i64,
    pub status: BurnStatus,
}

/// Result of a successful mint: the lock event id and the (wrapped) balance
/// credited to the recipient on the target chain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MintRecord {
    pub lock_event_id: String,
    pub target_chain: Chain,
    pub recipient: String,
    pub token: String,
    pub amount: u128,
    pub new_balance: u128,
    pub relayer_pubkey: String,
}

/// Result of a successful unlock: the burn event id and the (original) balance
/// credited back to the user on the source chain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnlockRecord {
    pub burn_event_id: String,
    pub source_chain: Chain,
    pub user: String,
    pub token: String,
    pub amount: u128,
    pub new_balance: u128,
    pub relayer_pubkey: String,
}

/// Bridge engine. Transfers are persisted to PostgreSQL and a relayer
/// signature is verified before any transfer is marked Completed, so the
/// engine is not a pure in-memory stub.
pub struct BridgeEngine {
    config: BridgeConfig,
    transfers: Arc<RwLock<HashMap<String, Transfer>>>,
    chains: HashMap<Chain, ChainState>,
    db: Option<sqlx::PgPool>,
    /// Lock events indexed by their deterministic event id.
    locks: Arc<RwLock<HashMap<String, LockEvent>>>,
    /// Burn events indexed by their deterministic event id.
    burns: Arc<RwLock<HashMap<String, BurnEvent>>>,
    /// Wrapped (minted) token balances on a target chain:
    /// (chain, recipient, token) -> balance.
    wrapped_balances: Arc<RwLock<HashMap<(Chain, String, String), u128>>>,
    /// Unlocked original-token balances on a source chain:
    /// (chain, user, token) -> balance.
    unlocked_balances: Arc<RwLock<HashMap<(Chain, String, String), u128>>>,
}

/// Chain state
struct ChainState {
    config: ChainConfig,
    provider: Option<ethers_providers::Provider<ethers_providers::Http>>,
}

/// Event emitted by bridge
#[derive(Debug, Clone)]
pub enum BridgeEvent {
    TransferInitiated(Transfer),
    TransferCompleted(Transfer),
    TransferFailed(Transfer),
    ValidatorSignature(String, String),
    RelayerMessage(String),
}

impl BridgeEngine {
    /// Create new bridge engine
    pub fn new(config: BridgeConfig) -> Self {
        let mut chains = HashMap::new();

        for chain_config in &config.chains {
            chains.insert(
                chain_config.chain,
                ChainState {
                    config: chain_config.clone(),
                    provider: None,
                },
            );
        }

        Self {
            config,
            transfers: Arc::new(RwLock::new(HashMap::new())),
            chains,
            db: None,
            locks: Arc::new(RwLock::new(HashMap::new())),
            burns: Arc::new(RwLock::new(HashMap::new())),
            wrapped_balances: Arc::new(RwLock::new(HashMap::new())),
            unlocked_balances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize chain providers and the PostgreSQL pool. When
    /// `database_url` is configured the schema is ensured on startup.
    pub async fn init(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for (_chain, state) in &mut self.chains {
            let provider = ethers_providers::Provider::<ethers_providers::Http>::try_from(
                state.config.rpc_url.as_str(),
            )?;
            state.provider = Some(provider);
        }

        if !self.config.database_url.is_empty() {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(10)
                .connect(&self.config.database_url)
                .await?;
            sqlx::query(BRIDGE_SCHEMA).execute(&pool).await?;
            self.db = Some(pool);
        }
        Ok(())
    }

    /// Get supported chains
    pub fn supported_chains(&self) -> Vec<Chain> {
        self.chains.keys().copied().collect()
    }

    /// Get chain configuration
    pub fn get_chain(&self, chain: Chain) -> Option<&ChainConfig> {
        self.chains.get(&chain).map(|s| &s.config)
    }

    /// Initiate cross-chain transfer. The transfer is persisted to PostgreSQL
    /// (when configured) in addition to the in-memory map.
    pub async fn initiate_transfer(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        sender: String,
        recipient: String,
        token: String,
        token_type: TokenType,
        amount: String,
        token_id: Option<String>,
    ) -> Result<Transfer, Box<dyn std::error::Error + Send + Sync>> {
        if !self.supported_chains().contains(&source_chain) {
            return Err("Source chain not supported".into());
        }
        if !self.supported_chains().contains(&dest_chain) {
            return Err("Destination chain not supported".into());
        }
        if source_chain == dest_chain {
            return Err("Cannot transfer to same chain".into());
        }

        let transfer = Transfer {
            id: format!("0x{}", hex::encode(rand::random::<[u8; 32]>())),
            source_chain,
            destination_chain: dest_chain,
            sender,
            recipient,
            token,
            token_type,
            amount,
            token_id,
            status: TransferStatus::Pending,
            source_tx: String::new(),
            destination_tx: None,
            timestamp: chrono::Utc::now().timestamp(),
            confirmations: 0,
        };

        // Persist to PostgreSQL when a pool is configured.
        if let Some(pool) = &self.db {
            sqlx::query(
                r#"INSERT INTO bridge_transfers
                   (id, source_chain, destination_chain, sender, recipient, token,
                    token_type, amount, token_id, status, source_tx, destination_tx,
                    timestamp, confirmations)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"#,
            )
            .bind(&transfer.id)
            .bind(format!("{:?}", transfer.source_chain))
            .bind(format!("{:?}", transfer.destination_chain))
            .bind(&transfer.sender)
            .bind(&transfer.recipient)
            .bind(&transfer.token)
            .bind(format!("{:?}", transfer.token_type))
            .bind(&transfer.amount)
            .bind(transfer.token_id.as_ref())
            .bind(format!("{:?}", transfer.status))
            .bind(&transfer.source_tx)
            .bind(transfer.destination_tx.as_ref())
            .bind(transfer.timestamp)
            .bind(transfer.confirmations as i64)
            .execute(pool)
            .await?;
        }

        let mut transfers = self.transfers.write().await;
        transfers.insert(transfer.id.clone(), transfer.clone());

        Ok(transfer)
    }

    /// Complete transfer. The relayer must supply a signature over the
    /// transfer id that recovers to an authorized relayer address; without a
    /// valid signature the transfer is refused. The completed transfer is
    /// persisted to PostgreSQL.
    pub async fn complete_transfer(
        &self,
        transfer_id: &str,
        destination_tx: String,
        relayer_signature: &str,
    ) -> Result<Transfer, Box<dyn std::error::Error + Send + Sync>> {
        let recovered = recover_relayer(transfer_id, relayer_signature)?;
        if !self.config.relayers.iter().any(|r| r.eq_ignore_ascii_case(&recovered)) {
            return Err(format!(
                "signature does not recover to an authorized relayer (got {})",
                recovered
            )
            .into());
        }

        let mut transfers = self.transfers.write().await;
        let transfer = match transfers.get_mut(transfer_id) {
            Some(t) => {
                t.status = TransferStatus::Completed;
                t.destination_tx = Some(destination_tx.clone());
                t.clone()
            }
            None => {
                if let Some(pool) = &self.db {
                    let exists: Option<(String,)> =
                        sqlx::query_as("SELECT id FROM bridge_transfers WHERE id = $1")
                            .bind(transfer_id)
                            .fetch_optional(pool)
                            .await?;
                    if exists.is_none() {
                        return Err("Transfer not found".into());
                    }
                    sqlx::query(
                        r#"UPDATE bridge_transfers
                           SET status = 'Completed', destination_tx = $2,
                               relayer = $3, relayer_sig = $4
                           WHERE id = $1"#,
                    )
                    .bind(transfer_id)
                    .bind(&destination_tx)
                    .bind(&recovered)
                    .bind(relayer_signature)
                    .execute(pool)
                    .await?;
                    return transfers
                        .get(transfer_id)
                        .cloned()
                        .ok_or_else(|| "transfer not in memory after db update".into());
                }
                return Err("Transfer not found".into());
            }
        };

        if let Some(pool) = &self.db {
            sqlx::query(
                r#"UPDATE bridge_transfers
                   SET status = 'Completed', destination_tx = $2,
                       relayer = $3, relayer_sig = $4
                   WHERE id = $1"#,
            )
            .bind(transfer_id)
            .bind(&destination_tx)
            .bind(&recovered)
            .bind(relayer_signature)
            .execute(pool)
            .await?;
        }

        Ok(transfer)
    }

    /// Get transfer status
    pub async fn get_transfer(&self, transfer_id: &str) -> Option<Transfer> {
        let transfers = self.transfers.read().await;
        transfers.get(transfer_id).cloned()
    }

    /// Calculate transfer fee
    pub fn calculate_fee(&self, amount: &str) -> Result<String, Box<dyn std::error::Error>> {
        let amount = amount.parse::<u64>().map_err(|e| e.to_string())?;
        let flat = self.config.fee.flat_fee.parse::<u64>().map_err(|e| e.to_string())?;
        let percentage = (amount as f64 * self.config.fee.percentage_fee) as u64;
        let min = self.config.fee.min_fee.parse::<u64>().map_err(|e| e.to_string())?;
        let max = self.config.fee.max_fee.parse::<u64>().map_err(|e| e.to_string())?;

        let fee = flat + percentage;
        let fee = fee.max(min).min(max);

        Ok(fee.to_string())
    }

    /// Get pending transfers
    pub async fn pending_transfers(&self) -> Vec<Transfer> {
        let transfers = self.transfers.read().await;
        transfers
            .values()
            .filter(|t| t.status == TransferStatus::Pending || t.status == TransferStatus::Minting)
            .cloned()
            .collect()
    }

    // ===== Lock / Mint / Burn / Unlock flows =====
    //
    // These implement a classic lock-and-mint / burn-and-unlock bridge. The
    // source chain holds the original tokens in escrow (recorded as a lock
    // event) and the target chain mints wrapped tokens once a relayer
    // attests to the lock with a valid Ed25519 signature. Burning wrapped
    // tokens on the target chain symmetrically unlocks the originals on the
    // source chain after another relayer attestation.

    /// Record a lock: a user locked `amount` of `token` on `source_chain` to
    /// be bridged to `target_chain`. `source_tx` is the on-chain lock
    /// transaction hash; together with the other parameters it is hashed
    /// (Keccak256) into a deterministic lock event id. The lock is persisted
    /// to PostgreSQL when a pool is configured.
    pub async fn lock(
        &self,
        source_chain: Chain,
        target_chain: Chain,
        user: String,
        token: String,
        amount: u128,
        source_tx: String,
        recipient: Option<String>,
    ) -> Result<LockEvent, Box<dyn std::error::Error + Send + Sync>> {
        if !self.supported_chains().contains(&source_chain) {
            return Err("Source chain not supported".into());
        }
        if !self.supported_chains().contains(&target_chain) {
            return Err("Target chain not supported".into());
        }
        if source_chain == target_chain {
            return Err("Cannot lock to the same chain".into());
        }
        if amount == 0 {
            return Err("Lock amount must be non-zero".into());
        }
        if source_tx.trim().is_empty() {
            return Err("source_tx is required".into());
        }

        let timestamp = chrono::Utc::now().timestamp();
        let id = lock_event_id(
            source_chain,
            target_chain,
            &user,
            &token,
            amount,
            &source_tx,
        );

        let event = LockEvent {
            id: id.clone(),
            source_chain,
            target_chain,
            user: user.clone(),
            token: token.clone(),
            amount,
            source_tx: source_tx.clone(),
            recipient: recipient.clone(),
            timestamp,
            status: LockStatus::Locked,
        };

        if let Some(pool) = &self.db {
            sqlx::query(
                r#"INSERT INTO bridge_locks
                   (id, source_chain, target_chain, user_addr, token, amount,
                    source_tx, recipient, timestamp, status)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                   ON CONFLICT (id) DO NOTHING"#,
            )
            .bind(&id)
            .bind(format!("{:?}", source_chain))
            .bind(format!("{:?}", target_chain))
            .bind(&user)
            .bind(&token)
            .bind(amount.to_string())
            .bind(&source_tx)
            .bind(recipient.as_ref())
            .bind(timestamp)
            .bind("locked")
            .execute(pool)
            .await?;
        }

        let mut locks = self.locks.write().await;
        locks.insert(id.clone(), event.clone());
        Ok(event)
    }

    /// Mint wrapped tokens on the target chain. A relayer submits a signed
    /// proof of the lock event: `relayer_pubkey_hex` (32-byte Ed25519 public
    /// key, hex) and `relayer_sig_hex` (64-byte Ed25519 signature, hex) over
    /// the lock event id. The signature is verified with Ed25519 and the
    /// public key must belong to the authorized relayer set. On success the
    /// recipient's wrapped balance is increased and the lock is marked Minted.
    /// Idempotent: re-submitting an already-minted lock returns the current
    /// wrapped balance without re-minting.
    pub async fn mint(
        &self,
        lock_event_id: &str,
        relayer_pubkey_hex: &str,
        relayer_sig_hex: &str,
    ) -> Result<MintRecord, Box<dyn std::error::Error + Send + Sync>> {
        // Verify the relayer attestation before touching any state.
        let authorized = self.is_authorized_relayer(lock_event_id, relayer_pubkey_hex, relayer_sig_hex)?;
        if !authorized {
            return Err("invalid or unauthorized relayer signature".into());
        }

        let mut locks = self.locks.write().await;
        let event = match locks.get_mut(lock_event_id) {
            Some(e) => e,
            None => {
                if let Some(pool) = &self.db {
                    let exists: Option<(String,)> =
                        sqlx::query_as("SELECT id FROM bridge_locks WHERE id = $1")
                            .bind(lock_event_id)
                            .fetch_optional(pool)
                            .await?;
                    if exists.is_none() {
                        return Err("Lock event not found".into());
                    }
                    // Reconstruct a minimal lock event from the DB to mint.
                    let row: (String, String, String, String, String, Option<String>) =
                        sqlx::query_as(
                            r#"SELECT source_chain, target_chain, user_addr, token, amount,
                                      recipient
                               FROM bridge_locks WHERE id = $1"#,
                        )
                        .bind(lock_event_id)
                        .fetch_one(pool)
                        .await?;
                    let amount: u128 = row.4.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
                    let event = LockEvent {
                        id: lock_event_id.to_string(),
                        source_chain: parse_chain(&row.0)?,
                        target_chain: parse_chain(&row.1)?,
                        user: row.2,
                        token: row.3,
                        amount,
                        source_tx: String::new(),
                        recipient: row.5,
                        timestamp: 0,
                        status: LockStatus::Locked,
                    };
                    locks.insert(lock_event_id.to_string(), event);
                    locks.get_mut(lock_event_id).unwrap()
                } else {
                    return Err("Lock event not found".into());
                }
            }
        };

        let recipient = event
            .recipient
            .clone()
            .unwrap_or_else(|| event.user.clone());
        let target_chain = event.target_chain;
        let token = event.token.clone();
        let amount = event.amount;

        // Idempotent: if already minted, return current balance.
        if event.status == LockStatus::Minted {
            let balances = self.wrapped_balances.read().await;
            let new_balance = balances
                .get(&(target_chain, recipient.clone(), token.clone()))
                .copied()
                .unwrap_or(0);
            drop(balances);
            return Ok(MintRecord {
                lock_event_id: lock_event_id.to_string(),
                target_chain,
                recipient,
                token,
                amount,
                new_balance,
                relayer_pubkey: relayer_pubkey_hex.to_string(),
            });
        }

        let mut balances = self.wrapped_balances.write().await;
        let key = (target_chain, recipient.clone(), token.clone());
        let bal = balances.entry(key).or_insert(0);
        *bal = bal
            .checked_add(amount)
            .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                "wrapped balance overflow".into()
            })?;
        let new_balance = *bal;
        event.status = LockStatus::Minted;

        if let Some(pool) = &self.db {
            sqlx::query(
                r#"UPDATE bridge_locks
                   SET status = 'minted', relayer_pubkey = $2, relayer_sig = $3
                   WHERE id = $1"#,
            )
            .bind(lock_event_id)
            .bind(relayer_pubkey_hex)
            .bind(relayer_sig_hex)
            .execute(pool)
            .await?;
        }

        Ok(MintRecord {
            lock_event_id: lock_event_id.to_string(),
            target_chain,
            recipient,
            token,
            amount,
            new_balance,
            relayer_pubkey: relayer_pubkey_hex.to_string(),
        })
    }

    /// Record a burn: a user burned `amount` of wrapped `token` on
    /// `target_chain` to be unlocked back on `source_chain`. `burn_tx` is the
    /// on-chain burn transaction hash; together with the other parameters it
    /// is hashed (Keccak256) into a deterministic burn event id. The user must
    /// hold a sufficient wrapped balance, which is decremented. Persisted to
    /// PostgreSQL when a pool is configured.
    pub async fn burn(
        &self,
        source_chain: Chain,
        target_chain: Chain,
        user: String,
        token: String,
        amount: u128,
        burn_tx: String,
    ) -> Result<BurnEvent, Box<dyn std::error::Error + Send + Sync>> {
        if !self.supported_chains().contains(&source_chain) {
            return Err("Source chain not supported".into());
        }
        if !self.supported_chains().contains(&target_chain) {
            return Err("Target chain not supported".into());
        }
        if source_chain == target_chain {
            return Err("Cannot burn to the same chain".into());
        }
        if amount == 0 {
            return Err("Burn amount must be non-zero".into());
        }
        if burn_tx.trim().is_empty() {
            return Err("burn_tx is required".into());
        }

        // Decrement the wrapped balance (must be sufficient).
        {
            let mut balances = self.wrapped_balances.write().await;
            let key = (target_chain, user.clone(), token.clone());
            let balance = balances.entry(key).or_insert(0);
            if *balance < amount {
                return Err(format!(
                    "insufficient wrapped balance: have {}, need {}",
                    balance, amount
                )
                .into());
            }
            *balance -= amount;
        }

        let timestamp = chrono::Utc::now().timestamp();
        let id = burn_event_id(
            source_chain,
            target_chain,
            &user,
            &token,
            amount,
            &burn_tx,
        );

        let event = BurnEvent {
            id: id.clone(),
            source_chain,
            target_chain,
            user: user.clone(),
            token: token.clone(),
            amount,
            burn_tx: burn_tx.clone(),
            timestamp,
            status: BurnStatus::Burned,
        };

        if let Some(pool) = &self.db {
            sqlx::query(
                r#"INSERT INTO bridge_burns
                   (id, source_chain, target_chain, user_addr, token, amount,
                    burn_tx, timestamp, status)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                   ON CONFLICT (id) DO NOTHING"#,
            )
            .bind(&id)
            .bind(format!("{:?}", source_chain))
            .bind(format!("{:?}", target_chain))
            .bind(&user)
            .bind(&token)
            .bind(amount.to_string())
            .bind(&burn_tx)
            .bind(timestamp)
            .bind("burned")
            .execute(pool)
            .await?;
        }

        let mut burns = self.burns.write().await;
        burns.insert(id.clone(), event.clone());
        Ok(event)
    }

    /// Unlock the original tokens on the source chain. A relayer submits a
    /// signed proof of the burn event: `relayer_pubkey_hex` and
    /// `relayer_sig_hex` over the burn event id. The Ed25519 signature is
    /// verified and the public key must be authorized. On success the user's
    /// original-token balance on the source chain is increased and the burn is
    /// marked Unlocked. Idempotent: re-submitting an already-unlocked burn
    /// returns the current unlocked balance without re-unlocking.
    pub async fn unlock(
        &self,
        burn_event_id: &str,
        relayer_pubkey_hex: &str,
        relayer_sig_hex: &str,
    ) -> Result<UnlockRecord, Box<dyn std::error::Error + Send + Sync>> {
        // Verify the relayer attestation before touching any state.
        let authorized = self.is_authorized_relayer(burn_event_id, relayer_pubkey_hex, relayer_sig_hex)?;
        if !authorized {
            return Err("invalid or unauthorized relayer signature".into());
        }

        let mut burns = self.burns.write().await;
        let event = match burns.get_mut(burn_event_id) {
            Some(e) => e,
            None => {
                if let Some(pool) = &self.db {
                    let exists: Option<(String,)> =
                        sqlx::query_as("SELECT id FROM bridge_burns WHERE id = $1")
                            .bind(burn_event_id)
                            .fetch_optional(pool)
                            .await?;
                    if exists.is_none() {
                        return Err("Burn event not found".into());
                    }
                    let row: (String, String, String, String, String) = sqlx::query_as(
                        r#"SELECT source_chain, target_chain, user_addr, token, amount
                           FROM bridge_burns WHERE id = $1"#,
                    )
                    .bind(burn_event_id)
                    .fetch_one(pool)
                    .await?;
                    let amount: u128 = row.4.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
                    let event = BurnEvent {
                        id: burn_event_id.to_string(),
                        source_chain: parse_chain(&row.0)?,
                        target_chain: parse_chain(&row.1)?,
                        user: row.2,
                        token: row.3,
                        amount,
                        burn_tx: String::new(),
                        timestamp: 0,
                        status: BurnStatus::Burned,
                    };
                    burns.insert(burn_event_id.to_string(), event);
                    burns.get_mut(burn_event_id).unwrap()
                } else {
                    return Err("Burn event not found".into());
                }
            }
        };

        let source_chain = event.source_chain;
        let user = event.user.clone();
        let token = event.token.clone();
        let amount = event.amount;

        // Idempotent: if already unlocked, return current balance.
        if event.status == BurnStatus::Unlocked {
            let balances = self.unlocked_balances.read().await;
            let new_balance = balances
                .get(&(source_chain, user.clone(), token.clone()))
                .copied()
                .unwrap_or(0);
            drop(balances);
            return Ok(UnlockRecord {
                burn_event_id: burn_event_id.to_string(),
                source_chain,
                user,
                token,
                amount,
                new_balance,
                relayer_pubkey: relayer_pubkey_hex.to_string(),
            });
        }

        let mut balances = self.unlocked_balances.write().await;
        let key = (source_chain, user.clone(), token.clone());
        let bal = balances.entry(key).or_insert(0);
        *bal = bal
            .checked_add(amount)
            .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                "unlocked balance overflow".into()
            })?;
        let new_balance = *bal;
        event.status = BurnStatus::Unlocked;

        if let Some(pool) = &self.db {
            sqlx::query(
                r#"UPDATE bridge_burns
                   SET status = 'unlocked', relayer_pubkey = $2, relayer_sig = $3
                   WHERE id = $1"#,
            )
            .bind(burn_event_id)
            .bind(relayer_pubkey_hex)
            .bind(relayer_sig_hex)
            .execute(pool)
            .await?;
        }

        Ok(UnlockRecord {
            burn_event_id: burn_event_id.to_string(),
            source_chain,
            user,
            token,
            amount,
            new_balance,
            relayer_pubkey: relayer_pubkey_hex.to_string(),
        })
    }

    /// Look up a lock event by id (memory first, then PostgreSQL).
    pub async fn get_lock(&self, id: &str) -> Option<LockEvent> {
        if let Some(e) = self.locks.read().await.get(id).cloned() {
            return Some(e);
        }
        if let Some(pool) = &self.db {
            let row: Option<(String, String, String, String, String, String, Option<String>, i64, String)> =
                sqlx::query_as(
                    r#"SELECT source_chain, target_chain, user_addr, token, amount,
                              source_tx, recipient, timestamp, status
                       FROM bridge_locks WHERE id = $1"#,
                )
                .bind(id)
                .fetch_optional(pool)
                .await
                .ok()?;
            row.and_then(|r| {
                let amount: u128 = r.4.parse().ok()?;
                Some(LockEvent {
                    id: id.to_string(),
                    source_chain: parse_chain(&r.0).ok()?,
                    target_chain: parse_chain(&r.1).ok()?,
                    user: r.2,
                    token: r.3,
                    amount,
                    source_tx: r.5,
                    recipient: r.6,
                    timestamp: r.7,
                    status: match r.8.as_str() {
                        "minted" => LockStatus::Minted,
                        _ => LockStatus::Locked,
                    },
                })
            })
        } else {
            None
        }
    }

    /// Look up a burn event by id (memory first, then PostgreSQL).
    pub async fn get_burn(&self, id: &str) -> Option<BurnEvent> {
        if let Some(e) = self.burns.read().await.get(id).cloned() {
            return Some(e);
        }
        if let Some(pool) = &self.db {
            let row: Option<(String, String, String, String, String, String, i64, String)> =
                sqlx::query_as(
                    r#"SELECT source_chain, target_chain, user_addr, token, amount,
                              burn_tx, timestamp, status
                       FROM bridge_burns WHERE id = $1"#,
                )
                .bind(id)
                .fetch_optional(pool)
                .await
                .ok()?;
            row.and_then(|r| {
                let amount: u128 = r.4.parse().ok()?;
                Some(BurnEvent {
                    id: id.to_string(),
                    source_chain: parse_chain(&r.0).ok()?,
                    target_chain: parse_chain(&r.1).ok()?,
                    user: r.2,
                    token: r.3,
                    amount,
                    burn_tx: r.5,
                    timestamp: r.6,
                    status: match r.7.as_str() {
                        "unlocked" => BurnStatus::Unlocked,
                        _ => BurnStatus::Burned,
                    },
                })
            })
        } else {
            None
        }
    }

    /// Wrapped (minted) balance of `recipient` for `token` on `chain`.
    pub async fn wrapped_balance(&self, chain: Chain, recipient: &str, token: &str) -> u128 {
        self.wrapped_balances
            .read()
            .await
            .get(&(chain, recipient.to_string(), token.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Unlocked original balance of `user` for `token` on `chain`.
    pub async fn unlocked_balance(&self, chain: Chain, user: &str, token: &str) -> u128 {
        self.unlocked_balances
            .read()
            .await
            .get(&(chain, user.to_string(), token.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Verify a relayer Ed25519 attestation over `event_id`: the public key
    /// must be in the authorized relayer set and the signature must verify.
    fn is_authorized_relayer(
        &self,
        event_id: &str,
        relayer_pubkey_hex: &str,
        relayer_sig_hex: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let pubkey_clean = relayer_pubkey_hex.trim_start_matches("0x").to_ascii_lowercase();
        let authorized = self
            .config
            .relayers_pubkeys
            .iter()
            .any(|k| k.trim_start_matches("0x").eq_ignore_ascii_case(&pubkey_clean));
        if !authorized {
            return Ok(false);
        }
        Ok(verify_relayer_signature(event_id, relayer_pubkey_hex, relayer_sig_hex)?)
    }
}

/// Compute a deterministic lock event id as Keccak256 over the canonical
/// encoding of the lock parameters.
fn lock_event_id(
    source_chain: Chain,
    target_chain: Chain,
    user: &str,
    token: &str,
    amount: u128,
    source_tx: &str,
) -> String {
    let mut hasher = Keccak256::new();
    hasher.update(b"LOCK");
    hasher.update(format!("{:?}", source_chain).as_bytes());
    hasher.update(format!("{:?}", target_chain).as_bytes());
    hasher.update(user.as_bytes());
    hasher.update(token.as_bytes());
    hasher.update(amount.to_le_bytes());
    hasher.update(source_tx.as_bytes());
    format!("0x{}", hex::encode(hasher.finalize()))
}

/// Compute a deterministic burn event id as Keccak256 over the canonical
/// encoding of the burn parameters.
fn burn_event_id(
    source_chain: Chain,
    target_chain: Chain,
    user: &str,
    token: &str,
    amount: u128,
    burn_tx: &str,
) -> String {
    let mut hasher = Keccak256::new();
    hasher.update(b"BURN");
    hasher.update(format!("{:?}", source_chain).as_bytes());
    hasher.update(format!("{:?}", target_chain).as_bytes());
    hasher.update(user.as_bytes());
    hasher.update(token.as_bytes());
    hasher.update(amount.to_le_bytes());
    hasher.update(burn_tx.as_bytes());
    format!("0x{}", hex::encode(hasher.finalize()))
}

/// Parse a `Chain` from its `Debug` representation (as written to the DB).
fn parse_chain(s: &str) -> Result<Chain, Box<dyn std::error::Error + Send + Sync>> {
    match s {
        "TigerSmartChain" => Ok(Chain::TigerSmartChain),
        "Ethereum" => Ok(Chain::Ethereum),
        "Polygon" => Ok(Chain::Polygon),
        "Arbitrum" => Ok(Chain::Arbitrum),
        "Optimism" => Ok(Chain::Optimism),
        "Base" => Ok(Chain::Base),
        other => Err(format!("unknown chain: {}", other).into()),
    }
}

/// Verify an Ed25519 relayer attestation: `pubkey_hex` is a 32-byte public
/// key (hex, optional 0x-prefix) and `sig_hex` is a 64-byte signature (hex,
/// optional 0x-prefix) over `event_id.as_bytes()`.
pub fn verify_relayer_signature(
    event_id: &str,
    pubkey_hex: &str,
    sig_hex: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let pubkey_bytes = hex::decode(pubkey_hex.trim_start_matches("0x"))?;
    if pubkey_bytes.len() != 32 {
        return Err(format!(
            "invalid relayer public key length: expected 32 bytes, got {}",
            pubkey_bytes.len()
        )
        .into());
    }
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&pubkey_bytes);
    let verifying_key = VerifyingKey::from_bytes(&pk)
        .map_err(|e| format!("invalid relayer public key: {}", e))?;

    let sig_bytes = hex::decode(sig_hex.trim_start_matches("0x"))?;
    if sig_bytes.len() != 64 {
        return Err(format!(
            "invalid signature length: expected 64 bytes, got {}",
            sig_bytes.len()
        )
        .into());
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_arr);

    Ok(verifying_key
        .verify(event_id.as_bytes(), &signature)
        .is_ok())
}

/// Sign an event id with an Ed25519 signing key. Returns the 64-byte
/// signature as a 0x-prefixed hex string. Useful for relayers and tests.
pub fn sign_event_id(signing_key: &SigningKey, event_id: &str) -> String {
    let signature: Signature = signing_key.sign(event_id.as_bytes());
    format!("0x{}", hex::encode(signature.to_bytes()))
}

/// Generate a fresh Ed25519 signing key using the OS CSPRNG.
pub fn generate_signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

/// Recover the signer address from a relayer signature over `transfer_id`.
/// The signed message is the transfer id itself, prefixed with the Ethereum
/// signed-message envelope. The signature is expected as a 0x-prefixed
/// 65-byte r||s||v hex string (secp256k1).
fn recover_relayer(
    transfer_id: &str,
    signature_hex: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use ethers_core::types::{Signature, RecoveryMessage};

    let sig_bytes = hex::decode(signature_hex.trim_start_matches("0x"))?;
    if sig_bytes.len() != 65 {
        return Err(format!(
            "invalid signature length: expected 65 bytes, got {}",
            sig_bytes.len()
        )
        .into());
    }
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&sig_bytes[0..32]);
    s.copy_from_slice(&sig_bytes[32..64]);
    let v = sig_bytes[64];

    let signature = Signature {
        r: ethers_core::types::U256::from_big_endian(&r),
        s: ethers_core::types::U256::from_big_endian(&s),
        v: v as u64,
    };

    // recover() applies the Ethereum signed-message prefix automatically when
    // given the raw message bytes.
    let address = signature
        .recover(RecoveryMessage::Data(transfer_id.as_bytes().to_vec()))
        .map_err(|e| format!("signature recovery failed: {}", e))?;
    Ok(format!("{:?}", address))
}
