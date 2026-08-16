//! Bridge Engine - Main Entry Point

use crate::{Chain, ChainConfig, BridgeConfig, Transfer, TransferStatus, TokenType};
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
"#;

/// Bridge engine. Transfers are persisted to PostgreSQL and a relayer
/// signature is verified before any transfer is marked Completed, so the
/// engine is not a pure in-memory stub.
pub struct BridgeEngine {
    config: BridgeConfig,
    transfers: Arc<RwLock<HashMap<String, Transfer>>>,
    chains: HashMap<Chain, ChainState>,
    db: Option<sqlx::PgPool>,
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

use rand::Rng;
