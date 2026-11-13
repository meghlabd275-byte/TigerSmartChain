//! Blockchain Types

use serde::{Deserialize, Serialize};

// =============================================================================
// BLOCK
// =============================================================================

/// Block Header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub parent_hash: String,
    pub sha3_uncles: String,
    pub miner: String,
    pub state_root: String,
    pub transactions_root: String,
    pub receipts_root: String,
    pub logs_bloom: String,
    pub difficulty: String,
    pub number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: String,
    pub mix_hash: String,
    pub nonce: String,
    pub base_fee_per_gas: Option<u64>,
    pub withdrawals_root: Option<String>,
}

/// Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub uncles: Vec<String>,
}

// =============================================================================
// TRANSACTION
// =============================================================================

/// Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub nonce: u64,
    pub block_hash: Option<String>,
    pub block_number: Option<u64>,
    pub transaction_index: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    /// Effective gas price. For a legacy transaction this is the signed
    /// `gasPrice`. For an EIP-1559 transaction there is no such field on the
    /// wire, and this carries the `maxFeePerGas` the sender committed to,
    /// which is the ceiling the fee is actually charged against.
    pub gas_price: u64,
    pub gas: u64,
    pub input: String,
    pub v: String,
    pub r: String,
    pub s: String,
    /// Transaction envelope type: 0 for a legacy transaction, 1 for
    /// EIP-2930 (access list) and 2 for EIP-1559. It is not part of the
    /// legacy field list, so it defaults to 0 when decoding a legacy
    /// transaction or a JSON payload that omits it.
    #[serde(default)]
    pub tx_type: u64,
    /// EIP-2930/1559 access list, RLP-encoded as the list of
    /// `[address, storageKeys]` entries. Held in its wire form because the
    /// signing hash commits to it byte-for-byte.
    #[serde(default)]
    pub access_list: Option<String>,
    /// EIP-1559 maximum priority fee per gas. `None` for a legacy or
    /// EIP-2930 transaction.
    #[serde(default)]
    pub max_priority_fee_per_gas: Option<u64>,
    /// Chain id carried in the payload of a typed transaction. It is a field
    /// of the signed list, so re-encoding the transaction for its hash needs
    /// it. A legacy transaction leaves this `None`; its chain id lives in `v`.
    #[serde(default)]
    pub chain_id: Option<u64>,
}

/// Transaction Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionStatus {
    pub transaction_hash: String,
    pub block_hash: String,
    pub block_number: u64,
    pub contract_address: Option<String>,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub logs: Vec<Log>,
    pub logs_bloom: String,
    pub status: bool,
}

/// Log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub log_index: u64,
    pub transaction_index: u64,
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub removed: bool,
}

// =============================================================================
// CHAIN
// =============================================================================

/// Chain Config
///
/// Field names accept both the Tiger snake_case spelling and the camelCase a
/// standard Ethereum genesis file uses, so either parses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    #[serde(alias = "chainId", default = "default_chain_id")]
    pub chain_id: u64,
    #[serde(alias = "homesteadBlock", default)]
    pub homestead_block: Option<u64>,
    #[serde(alias = "daoForkBlock", default)]
    pub dao_fork_block: Option<u64>,
    #[serde(alias = "eip150Block", default)]
    pub eip150_block: Option<u64>,
    #[serde(alias = "eip150Hash", default)]
    pub eip150_hash: Option<String>,
    #[serde(alias = "eip155Block", default)]
    pub eip155_block: Option<u64>,
    #[serde(alias = "eip158Block", default)]
    pub eip158_block: Option<u64>,
    #[serde(alias = "byzantiumBlock", default)]
    pub byzantium_block: Option<u64>,
    #[serde(alias = "constantinopleBlock", default)]
    pub constantinople_block: Option<u64>,
    #[serde(alias = "petersburgBlock", default)]
    pub petersburg_block: Option<u64>,
    #[serde(alias = "istanbulBlock", default)]
    pub istanbul_block: Option<u64>,
    #[serde(alias = "eip1283Block", default)]
    pub eip1283_block: Option<u64>,
    #[serde(alias = "berlinBlock", default)]
    pub berlin_block: Option<u64>,
    #[serde(alias = "londonBlock", default)]
    pub london_block: Option<u64>,
    #[serde(alias = "mergeForkBlock", alias = "mergeNetsplitBlock", default)]
    pub merge_fork_block: Option<u64>,
    #[serde(alias = "shanghaiBlock", default)]
    pub shanghai_block: Option<u64>,
    #[serde(alias = "cancelDeneb", default)]
    pub cancel_deneb: Option<u64>,
    /// Blocks per reward epoch. When this is set together with `validators`,
    /// the chain mints `block_reward` to each block's proposer as part of
    /// block execution, so every node derives the same state root.
    #[serde(alias = "epochLength", default)]
    pub epoch_length: Option<u64>,
    /// Reward minted to the proposer of each block, in wei, written as a
    /// decimal or `0x`-prefixed hex string.
    #[serde(alias = "blockReward", default)]
    pub block_reward: Option<String>,
    /// The authorities entitled to propose blocks, fixed in genesis. A node
    /// whose genesis pins the set enforces exactly it and learns nothing from
    /// the network.
    #[serde(default)]
    pub validators: Vec<String>,
}

fn default_chain_id() -> u64 {
    6666
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_id: 1,
            homestead_block: Some(0),
            dao_fork_block: None,
            eip150_block: Some(0),
            eip150_hash: None,
            eip155_block: Some(0),
            eip158_block: Some(0),
            byzantium_block: Some(0),
            constantinople_block: Some(0),
            petersburg_block: Some(0),
            istanbul_block: Some(0),
            eip1283_block: None,
            berlin_block: Some(0),
            london_block: Some(0),
            merge_fork_block: None,
            shanghai_block: None,
            cancel_deneb: None,
            epoch_length: None,
            block_reward: None,
            validators: Vec::new(),
        }
    }
}

// =============================================================================
// GENESIS
// =============================================================================

/// Genesis
///
/// The parser accepts the field shapes real Ethereum-family genesis files use.
/// A standard file writes `nonce`, `gasLimit` and `difficulty` as numbers,
/// omits `mixHash`/`coinbase` for a proof-of-stake chain, and leaves `alloc`
/// accounts with only a balance. Requiring the strict Tiger shape would reject
/// every such file, so each field has a tolerant deserializer and a default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genesis {
    pub config: ChainConfig,
    #[serde(default, deserialize_with = "flexible::u64_or_string")]
    pub nonce: u64,
    #[serde(default = "default_timestamp", deserialize_with = "flexible::string_or_number")]
    pub timestamp: String,
    #[serde(default, alias = "extraData")]
    pub extra_data: String,
    #[serde(
        default = "default_gas_limit",
        alias = "gasLimit",
        deserialize_with = "flexible::string_or_number"
    )]
    pub gas_limit: String,
    #[serde(default = "default_difficulty", deserialize_with = "flexible::string_or_number")]
    pub difficulty: String,
    #[serde(default = "zero_hash", alias = "mixHash")]
    pub mix_hash: String,
    #[serde(default = "zero_address")]
    pub coinbase: String,
    #[serde(default)]
    pub alloc: std::collections::HashMap<String, Account>,
}

fn default_timestamp() -> String {
    "0x0".to_string()
}

fn default_gas_limit() -> String {
    "0x1c9c380".to_string()
}

fn default_difficulty() -> String {
    "0x1".to_string()
}

fn zero_hash() -> String {
    format!("0x{}", "0".repeat(64))
}

fn zero_address() -> String {
    format!("0x{}", "0".repeat(40))
}

/// Deserializers that accept a field written either as a JSON string or as a
/// JSON number, which is what makes a standard genesis file parse.
mod flexible {
    use serde::de::{self, Visitor};
    use serde::{Deserialize, Deserializer};
    use std::fmt;

    struct NumberOrString;

    impl<'de> Visitor<'de> for NumberOrString {
        type Value = String;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or a number")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
            Ok(v)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
            Ok(format!("0x{v:x}"))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
            Ok(format!("0x{v:x}"))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<String, E> {
            Ok(format!("{v}"))
        }
    }

    /// A quantity written as either a number or a `0x`-prefixed/decimal string.
    pub fn string_or_number<'de, D>(d: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        d.deserialize_any(NumberOrString)
    }

    struct IntOrString;

    impl<'de> Visitor<'de> for IntOrString {
        type Value = u64;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a number or a numeric string")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> {
            Ok(v as u64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            match v.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16).map_err(de::Error::custom),
                None => v.parse::<u64>().map_err(de::Error::custom),
            }
        }
    }

    /// A `u64` written as either a number or a numeric string.
    pub fn u64_or_string<'de, D>(d: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        d.deserialize_any(IntOrString)
    }

    /// An optional `u64` written as a number, a numeric string, or `null`.
    pub fn opt_u64_or_string<'de, D>(d: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<serde_json::Value>::deserialize(d)?;
        match value {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::Number(n)) => {
                n.as_u64().map(Some).ok_or_else(|| de::Error::custom("nonce out of range"))
            }
            Some(serde_json::Value::String(s)) => match s.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16).map(Some).map_err(de::Error::custom),
                None => s.parse::<u64>().map(Some).map_err(de::Error::custom),
            },
            Some(other) => Err(de::Error::custom(format!("unexpected nonce value: {other}"))),
        }
    }
}

/// Account
///
/// Only `balance` is required. A genesis file written for any Ethereum-family
/// chain omits `nonce`, `code` and `storage` for ordinary funded accounts, so
/// those default rather than failing the parse: a required field here would
/// reject every standard genesis file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    #[serde(deserialize_with = "flexible::string_or_number")]
    pub balance: String,
    #[serde(default, deserialize_with = "flexible::opt_u64_or_string")]
    pub nonce: Option<u64>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub storage: std::collections::HashMap<String, String>,
}

// =============================================================================
// HEADER
// =============================================================================

/// Block Id
#[derive(Debug, Clone)]
pub enum BlockId {
    Number(u64),
    Hash(String),
    Latest,
    Earliest,
    Pending,
}
#[cfg(test)]
mod genesis_parse_tests {
    use super::*;

    /// A standard Ethereum-family genesis file writes quantities as numbers and
    /// omits the proof-of-work-only fields. It must parse, or a chain built
    /// from a real genesis spec could never start.
    #[test]
    fn a_standard_ethereum_genesis_file_parses() {
        let json = r#"{
            "config": {
                "chainId": 6666,
                "homesteadBlock": 0,
                "eip155Block": 0,
                "londonBlock": 0,
                "epochLength": 200,
                "blockReward": "0x1bc16d674ec80000",
                "validators": ["0xaa00000000000000000000000000000000000000"]
            },
            "nonce": 0,
            "timestamp": 0,
            "extraData": "0x",
            "gasLimit": 30000000,
            "difficulty": 1,
            "alloc": {
                "0x70997970C51812dc3A010C7d01b50e0d17dc79C8": { "balance": "1000000000000000000000" }
            }
        }"#;
        let genesis: Genesis = serde_json::from_str(json).expect("standard genesis must parse");
        assert_eq!(genesis.config.chain_id, 6666);
        assert_eq!(genesis.config.epoch_length, Some(200));
        assert_eq!(genesis.config.validators.len(), 1);
        assert_eq!(genesis.nonce, 0);
        // A numeric gasLimit is normalised to a hex quantity.
        assert_eq!(genesis.gas_limit, "0x1c9c380");
        // A missing coinbase/mixHash defaults rather than failing.
        assert!(genesis.coinbase.starts_with("0x"));
        assert_eq!(genesis.alloc.len(), 1);
        let account = genesis
            .alloc
            .get("0x70997970C51812dc3A010C7d01b50e0d17dc79C8")
            .unwrap();
        assert_eq!(account.balance, "1000000000000000000000");
        assert_eq!(account.nonce, None);
    }

    /// A string-form genesis (the Tiger native shape) still parses.
    #[test]
    fn a_string_form_genesis_still_parses() {
        let json = r#"{
            "config": { "chainId": 6666, "londonBlock": 0 },
            "nonce": 0,
            "timestamp": "2024-01-01T00:00:00Z",
            "extraData": "TigerSmartChain Genesis",
            "gasLimit": "0x1c9c380",
            "difficulty": "0x1",
            "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "coinbase": "0x0000000000000000000000000000000000000000",
            "alloc": {}
        }"#;
        let genesis: Genesis = serde_json::from_str(json).expect("string-form genesis must parse");
        assert_eq!(genesis.gas_limit, "0x1c9c380");
        assert_eq!(genesis.difficulty, "0x1");
    }
}
