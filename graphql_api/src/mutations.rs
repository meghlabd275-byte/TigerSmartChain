//! GraphQL Mutations for TigerScan
//!
//! Each mutation either verifies something against the chain or records a
//! request. None of them reports success for work that was not done.

use crate::db::{normalize_hex_key, Database};
use crate::model::keccak_bytes;
use crate::schema::*;
use async_graphql::{Context, Object, SimpleObject};
use contract_verifier_advanced::{ContractVerificationRequest, ContractVerifier};
use rand::RngCore;
use sqlx::Row;

/// The number of random bytes behind an API key. 32 bytes is 256 bits, which
/// is not guessable; the key is stored as a hash so a database leak does not
/// hand over working keys.
const API_KEY_BYTES: usize = 32;

/// The result of verifying a contract.
#[derive(SimpleObject, Clone)]
pub struct VerificationResult {
    /// Whether the submitted source compiles to the deployed bytecode.
    pub success: bool,
    /// The verified contract, present only when verification succeeded.
    pub contract: Option<Contract>,
    /// Why verification failed, present only when it did.
    pub message: Option<String>,
}

#[Object(name = "Mutation")]
impl MutationRoot {
    /// Verify a contract by compiling the submitted source and comparing the
    /// result with the code deployed at the address.
    ///
    /// This is the real check, not a badge: `ContractVerifier::verify` compiles
    /// the source with `solc`, fetches the deployed code over `eth_getCode`
    /// and compares the runtime bytecode. A submission that does not match is
    /// rejected and nothing is written.
    pub async fn verify_contract(
        &self,
        ctx: &Context<'_>,
        address: String,
        source_code: String,
        compiler_version: String,
        evm_version: Option<String>,
        license: Option<String>,
        optimization_enabled: Option<bool>,
        optimization_runs: Option<i32>,
        constructor_args: Option<String>,
    ) -> async_graphql::Result<VerificationResult> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(VerificationResult {
                success: false,
                contract: None,
                message: Some("database unavailable".to_string()),
            });
        };
        let address = normalize_hex_key(&address);
        if source_code.trim().is_empty() {
            return Ok(VerificationResult {
                success: false,
                contract: None,
                message: Some("source code is empty".to_string()),
            });
        }

        // The verifier reads the deployed code from the node, so it needs the
        // RPC endpoint the service was configured with.
        let verifier = match db.rpc_url() {
            Some(url) => ContractVerifier::new_with_rpc(url),
            None => {
                return Ok(VerificationResult {
                    success: false,
                    contract: None,
                    message: Some(
                        "no RPC endpoint configured, cannot read deployed code".to_string(),
                    ),
                })
            }
        };

        // The verifier only compiles with versions it can actually run, so an
        // unsupported version is refused here with the list of supported ones
        // rather than reported as a bytecode mismatch.
        if !verifier
            .get_supported_versions()
            .iter()
            .any(|v| v == &compiler_version)
        {
            return Ok(VerificationResult {
                success: false,
                contract: None,
                message: Some(format!(
                    "unsupported compiler version {compiler_version}; supported: {}",
                    verifier.get_supported_versions().join(", ")
                )),
            });
        }

        let request = ContractVerificationRequest {
            address: address.clone(),
            name: "Contract".to_string(),
            source_code: source_code.clone(),
            compiler_version: compiler_version.clone(),
            optimization: optimization_enabled.unwrap_or(false),
            optimization_runs: optimization_runs.map(|runs| runs.max(0) as u32),
            evm_version: evm_version.clone(),
            libraries: std::collections::HashMap::new(),
            constructor_args: constructor_args.clone(),
            license: license.clone(),
        };

        let (success, message) = match verifier.verify(request) {
            Ok(result) => (
                result.success && result.matches,
                if result.errors.is_empty() {
                    None
                } else {
                    Some(result.errors.join("; "))
                },
            ),
            Err(e) => (false, Some(e.to_string())),
        };

        if !success {
            return Ok(VerificationResult {
                success: false,
                contract: None,
                message: message.or_else(|| Some("bytecode does not match".to_string())),
            });
        }

        // Verification passed, so the source is recorded. The address is
        // unique, so a second successful submission updates the first.
        sqlx::query(
            "INSERT INTO contracts (address, source_code, compiler_version, evm_version,
                                    license_type, optimization_enabled, optimization_runs,
                                    is_verified, verification_status, verified_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, true, 'verified', NOW())
             ON CONFLICT (address) DO UPDATE SET
                source_code = EXCLUDED.source_code,
                compiler_version = EXCLUDED.compiler_version,
                evm_version = EXCLUDED.evm_version,
                license_type = EXCLUDED.license_type,
                optimization_enabled = EXCLUDED.optimization_enabled,
                optimization_runs = EXCLUDED.optimization_runs,
                is_verified = true,
                verification_status = 'verified',
                verified_at = NOW(),
                updated_at = NOW()",
        )
        .bind(&address)
        .bind(&source_code)
        .bind(&compiler_version)
        .bind(&evm_version)
        .bind(&license)
        .bind(optimization_enabled.unwrap_or(false))
        .bind(optimization_runs.unwrap_or(200))
        .execute(db.pool())
        .await?;

        let row = sqlx::query(crate::model::CONTRACT_COLUMNS_QUERY)
            .bind(&address)
            .fetch_optional(db.pool())
            .await?;

        Ok(VerificationResult {
            success: true,
            contract: row.as_ref().map(crate::model::contract_from_row),
            message: None,
        })
    }

    /// Report a malicious contract.
    ///
    /// The report is recorded, not acted on: a single report is one user's
    /// claim, and the explorer shows it as a claim until it is corroborated.
    pub async fn report_malicious_contract(
        &self,
        ctx: &Context<'_>,
        address: String,
        reason: String,
    ) -> async_graphql::Result<bool> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(false);
        };
        let address = normalize_hex_key(&address);
        if address.len() != 42 {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO malicious_contracts (address, threat_type, confidence, description, source, first_seen, last_seen, is_active)
             VALUES ($1, 'user_report', 0, $2, 'graphql', NOW(), NOW(), true)",
        )
        .bind(&address)
        .bind(&reason)
        .execute(db.pool())
        .await?;
        Ok(true)
    }

    /// Create an API key.
    ///
    /// The key itself is returned once and never stored: the row keeps only its
    /// hash, so the database cannot be used to recover a working key.
    pub async fn create_api_key(
        &self,
        ctx: &Context<'_>,
        name: String,
        rate_limit: Option<i32>,
        monthly_limit: Option<i64>,
    ) -> async_graphql::Result<APIKey> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Err("database unavailable".into());
        };
        let mut secret = [0u8; API_KEY_BYTES];
        rand::thread_rng().fill_bytes(&mut secret);
        let key = format!("tsk_{}", hex::encode(secret));
        let key_hash = keccak_bytes(key.as_bytes());

        let row = sqlx::query(
            "INSERT INTO api_keys (key_hash, key_name, rate_limit, daily_limit, is_active)
             VALUES ($1, $2, $3, $4, true)
             RETURNING id, key_hash, key_name, user_id, rate_limit, daily_limit,
                       requests_count, is_active, expires_at, created_at, updated_at",
        )
        .bind(&key_hash)
        .bind(&name)
        .bind(rate_limit.unwrap_or(60))
        .bind(monthly_limit)
        .fetch_one(db.pool())
        .await?;

        Ok(APIKey {
            id: row.try_get("id")?,
            key: Some(key),
            key_hash: row.try_get("key_hash")?,
            name: row.try_get("key_name")?,
            user_id: row.try_get("user_id").ok().flatten(),
            rate_limit: row.try_get("rate_limit").ok().flatten(),
            monthly_limit: row.try_get("daily_limit").ok().flatten(),
            requests_used: row.try_get("requests_count").ok().flatten(),
            is_active: row.try_get("is_active").unwrap_or(true),
            expires_at: row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339()),
            created_at: row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339()),
            updated_at: row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("updated_at")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339()),
        })
    }

    /// Create a webhook.
    pub async fn create_webhook(
        &self,
        ctx: &Context<'_>,
        url: String,
        events: Vec<String>,
    ) -> async_graphql::Result<Webhook> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Err("database unavailable".into());
        };
        // The scheme decides whether the payload would travel encrypted. A
        // plain HTTP webhook would send event data in the clear, so it is
        // refused rather than silently accepted.
        if !url.starts_with("https://") {
            return Err("webhook url must use https".into());
        }
        if events.is_empty() {
            return Err("at least one event type is required".into());
        }

        // The signing secret is generated here and only its hash is stored;
        // the receiver is given the secret once, at creation.
        let mut secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        let signing_secret = format!("whsec_{}", hex::encode(secret));
        let secret_hash = keccak_bytes(signing_secret.as_bytes());

        let row = sqlx::query(
            "INSERT INTO webhooks (url, event_type, secret_hash, is_active)
             VALUES ($1, $2, $3, true)
             RETURNING id, url, event_type, is_active, retry_count, timeout_seconds, created_at, updated_at",
        )
        .bind(&url)
        .bind(events.join(","))
        .bind(&secret_hash)
        .fetch_one(db.pool())
        .await?;

        let event_type: String = row.try_get("event_type").unwrap_or_default();
        Ok(Webhook {
            id: row.try_get("id")?,
            url: row.try_get("url")?,
            events: event_type.split(',').map(|s| s.to_string()).collect(),
            signing_secret: Some(signing_secret),
            is_active: row.try_get("is_active").unwrap_or(true),
            failure_count: row.try_get("retry_count").unwrap_or_default(),
            last_failure_at: None,
            created_at: row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339()),
            updated_at: row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("updated_at")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339()),
        })
    }

    /// Subscribe an address to alerts.
    pub async fn subscribe_alerts(
        &self,
        ctx: &Context<'_>,
        address: String,
        alert_types: Vec<String>,
    ) -> async_graphql::Result<bool> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(false);
        };
        let address = normalize_hex_key(&address);
        if address.len() != 42 || alert_types.is_empty() {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO alert_subscriptions (address, alert_types, is_active)
             VALUES ($1, $2, true)
             ON CONFLICT (address, webhook_id) DO UPDATE SET
                alert_types = EXCLUDED.alert_types, is_active = true, updated_at = NOW()",
        )
        .bind(&address)
        .bind(&alert_types)
        .execute(db.pool())
        .await?;
        Ok(true)
    }

    /// Simulate a transaction against the node.
    ///
    /// The node runs the call with `eth_call`, which executes it against the
    /// current state without mining it. The returned data is the node's, not a
    /// prediction.
    pub async fn simulate_transaction(
        &self,
        ctx: &Context<'_>,
        from: Option<String>,
        to: String,
        value: Option<String>,
        data: Option<String>,
    ) -> async_graphql::Result<SimulationResult> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(SimulationResult {
                success: false,
                gas_used: 0,
                return_value: "0x".to_string(),
                error: Some("database unavailable".to_string()),
            });
        };
        let Some(url) = db.rpc_url() else {
            return Ok(SimulationResult {
                success: false,
                gas_used: 0,
                return_value: "0x".to_string(),
                error: Some("no RPC endpoint configured".to_string()),
            });
        };

        let mut call = serde_json::Map::new();
        call.insert("to".to_string(), serde_json::json!(normalize_hex_key(&to)));
        if let Some(from) = from {
            call.insert("from".to_string(), serde_json::json!(normalize_hex_key(&from)));
        }
        if let Some(value) = value {
            // A decimal value has to reach the node as a hex quantity, because
            // that is the only form `eth_call` accepts for it.
            call.insert("value".to_string(), serde_json::json!(decimal_to_hex(&value)));
        }
        if let Some(data) = data {
            call.insert("data".to_string(), serde_json::json!(data));
        }

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [call, "latest"],
        });
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let response: serde_json::Value = client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .json()
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        if let Some(error) = response.get("error") {
            return Ok(SimulationResult {
                success: false,
                gas_used: 0,
                return_value: "0x".to_string(),
                error: Some(error.to_string()),
            });
        }
        let return_value = response
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x")
            .to_string();
        Ok(SimulationResult {
            success: true,
            gas_used: 0,
            return_value,
            error: None,
        })
    }
}

/// Convert a decimal integer string to a JSON-RPC hex quantity.
///
/// The node accepts only hex for a quantity, and a value that is not a number
/// is passed through so the node can reject it with its own message rather
/// than the client being told a wrong number.
fn decimal_to_hex(value: &str) -> String {
    let value = value.trim();
    if value.starts_with("0x") {
        return value.to_string();
    }
    match value.parse::<u128>() {
        Ok(number) => format!("0x{number:x}"),
        Err(_) => value.to_string(),
    }
}

// =============================================================================
// MUTATION TYPES
// =============================================================================

/// An API key. The `key` field is populated only in the reply to the request
/// that created it; a later read returns the hash and no key.
#[derive(SimpleObject, Clone)]
pub struct APIKey {
    pub id: i64,
    pub key: Option<String>,
    pub key_hash: String,
    pub name: String,
    pub user_id: Option<String>,
    pub rate_limit: Option<i32>,
    pub monthly_limit: Option<i64>,
    pub requests_used: Option<i64>,
    pub is_active: bool,
    pub expires_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// A webhook. `signing_secret` is returned only when the webhook is created.
#[derive(SimpleObject, Clone)]
pub struct Webhook {
    pub id: i64,
    pub url: String,
    pub events: Vec<String>,
    pub signing_secret: Option<String>,
    pub is_active: bool,
    pub failure_count: i32,
    pub last_failure_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// The result of a simulated call.
#[derive(SimpleObject, Clone)]
pub struct SimulationResult {
    pub success: bool,
    pub gas_used: i64,
    pub return_value: String,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_values_become_hex_quantities() {
        assert_eq!(decimal_to_hex("0"), "0x0");
        assert_eq!(decimal_to_hex("255"), "0xff");
        assert_eq!(decimal_to_hex("1000000000000000000"), "0xde0b6b3a7640000");
        // A value already in hex is left as it is.
        assert_eq!(decimal_to_hex("0xff"), "0xff");
        // Something that is not a number is passed through so the node can
        // reject it, rather than being silently turned into zero.
        assert_eq!(decimal_to_hex("not-a-number"), "not-a-number");
    }

    #[test]
    fn key_hashes_are_keccak_of_the_key() {
        let key = "tsk_0000000000000000000000000000000000000000000000000000000000000000";
        let hash = keccak_bytes(key.as_bytes());
        assert!(hash.starts_with("0x"));
        assert_eq!(hash.len(), 66);
        // Two different keys must never share a hash.
        assert_ne!(hash, keccak_bytes(b"tsk_01"));
        assert_ne!(keccak_bytes(b"tsk_01"), keccak_bytes(b"tsk_02"));
    }

    #[test]
    fn generated_keys_never_collide() {
        // The stored value is a hash of the key, so two keys that hashed the
        // same would be one key as far as the database is concerned.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..256 {
            let mut secret = [0u8; API_KEY_BYTES];
            rand::thread_rng().fill_bytes(&mut secret);
            let key = format!("tsk_{}", hex::encode(secret));
            assert!(
                seen.insert(keccak_bytes(key.as_bytes())),
                "two generated keys hashed to the same value"
            );
        }
    }

    #[test]
    fn generated_keys_are_unique_and_long() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            let mut secret = [0u8; API_KEY_BYTES];
            rand::thread_rng().fill_bytes(&mut secret);
            let key = format!("tsk_{}", hex::encode(secret));
            assert_eq!(key.len(), 4 + API_KEY_BYTES * 2);
            assert!(seen.insert(key), "a generated key repeated");
        }
    }

    #[test]
    fn keccak_matches_a_known_digest() {
        // keccak256("abc") is a published constant.
        assert_eq!(
            crate::model::keccak_bytes(b"abc"),
            "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }
}
