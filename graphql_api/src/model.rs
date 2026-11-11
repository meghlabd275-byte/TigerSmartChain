//! GraphQL Queries for TigerScan
//!
//! Every resolver here reads the indexed database. Where the answer cannot
//! come from the index — a live balance, a gas estimate — it is read from the
//! node over JSON-RPC. No resolver invents a value: when there is no data the
//! result is empty or absent, which is what a client can act on.

//! Row mapping: the one place a database column is turned into a schema type.
//!
//! Every resolver builds its result through these functions, so a value that
//! appears in a nested field and the same value in a root query cannot be
//! mapped differently.

use crate::schema::*;
use bigdecimal::BigDecimal;
use serde_json::Value as JsonValue;
use sqlx::Row;

/// How many rows a query may return when the client does not say.
pub const DEFAULT_PAGE: i64 = 25;
/// The largest page a client may ask for, so one query cannot pull the chain.
pub const MAX_PAGE: i64 = 500;

/// Clamp a requested page size into the allowed range.
pub fn page_size(first: Option<i32>) -> i64 {
    match first {
        Some(n) if n > 0 => (n as i64).min(MAX_PAGE),
        _ => DEFAULT_PAGE,
    }
}

/// Decode an opaque cursor into a row offset.
///
/// The cursor is the offset the previous page ended at, so a client that
/// follows `pageInfo.endCursor` resumes exactly where it stopped. A cursor
/// that does not parse is treated as the start, which is the only useful
/// interpretation: refusing the query would strand the client.
pub fn offset_from_cursor(after: Option<&str>) -> i64 {
    after
        .and_then(|c| c.rsplit(':').next())
        .and_then(|n| n.parse::<i64>().ok())
        .filter(|n| *n >= 0)
        .unwrap_or(0)
}

/// A PostgreSQL `NUMERIC` as a decimal string, which is what the schema
/// exposes. Integers and text columns are accepted too, so a column that is
/// declared `BIGINT` in one deployment and `NUMERIC` in another still reads.
pub fn numeric(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    if let Ok(v) = row.try_get::<Option<BigDecimal>, _>(column) {
        return v.map(|d| d.to_string());
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(column) {
        return v.map(|d| d.to_string());
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(column) {
        return v;
    }
    None
}

/// A floating point column, accepted as `FLOAT`, `NUMERIC` or text.
pub fn float_of(row: &sqlx::postgres::PgRow, column: &str) -> Option<f64> {
    if let Ok(v) = row.try_get::<Option<f64>, _>(column) {
        return v;
    }
    if let Ok(v) = row.try_get::<Option<BigDecimal>, _>(column) {
        return v.and_then(|d| d.to_string().parse().ok());
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(column) {
        return v.map(|d| d as f64);
    }
    None
}

/// A JSONB column, tolerating a NULL.
pub fn json_of(row: &sqlx::postgres::PgRow, column: &str) -> Option<JsonValue> {
    row.try_get::<Option<JsonValue>, _>(column).ok().flatten()
}

/// A text array, tolerating a NULL.
pub fn text_array(row: &sqlx::postgres::PgRow, column: &str) -> Vec<String> {
    row.try_get::<Option<Vec<String>>, _>(column)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Render a JSON value as the string the schema declares, if it is one.
pub fn json_string(value: Option<&JsonValue>) -> Option<String> {
    match value? {
        JsonValue::String(s) => Some(s.clone()),
        JsonValue::Null => None,
        other => Some(other.to_string()),
    }
}

// =============================================================================
// ROW MAPPING
// =============================================================================

/// Build a block from a `blocks` row.
pub fn block_from_row(row: &sqlx::postgres::PgRow) -> Block {
    Block {
        number: row.try_get("number").unwrap_or_default(),
        hash: row.try_get("hash").unwrap_or_default(),
        parent_hash: row.try_get("parent_hash").unwrap_or_default(),
        nonce: row.try_get("nonce").ok().flatten(),
        sha3_uncles: row.try_get("sha3_uncles").ok().flatten(),
        logs_bloom: row.try_get("logs_bloom").ok().flatten(),
        transactions_root: row.try_get("transactions_root").ok().flatten(),
        state_root: row.try_get("state_root").ok().flatten(),
        receipts_root: row.try_get("receipts_root").ok().flatten(),
        miner: row.try_get("miner").ok().flatten(),
        difficulty: row.try_get("difficulty").ok().flatten(),
        total_difficulty: row.try_get("total_difficulty").ok().flatten(),
        size: row.try_get("size").ok().flatten(),
        gas_limit: row.try_get("gas_limit").unwrap_or_default(),
        gas_used: row.try_get("gas_used").unwrap_or_default(),
        timestamp: row.try_get("timestamp").unwrap_or_default(),
        extra_data: row.try_get("extra_data").ok().flatten(),
        base_fee_per_gas: row.try_get("base_fee_per_gas").ok().flatten(),
    }
}

/// Build a transaction from a `transactions` row.
pub fn transaction_from_row(row: &sqlx::postgres::PgRow) -> Transaction {
    let tx_type = row
        .try_get::<Option<String>, _>("transaction_type")
        .ok()
        .flatten()
        .and_then(|t| t.parse::<i32>().ok());
    Transaction {
        hash: row.try_get("hash").unwrap_or_default(),
        block_number: row.try_get("block_number").ok().flatten(),
        block_hash: row.try_get("block_hash").ok().flatten(),
        transaction_index: row
            .try_get::<Option<i32>, _>("transaction_index")
            .ok()
            .flatten()
            .unwrap_or_default() as i64,
        from_address: row.try_get("from_address").unwrap_or_default(),
        to_address: row.try_get("to_address").ok().flatten(),
        value: numeric(row, "value").unwrap_or_else(|| "0".to_string()),
        gas_price: row.try_get("gas_price").ok().flatten(),
        gas: row.try_get("gas_limit").ok().flatten(),
        input: row.try_get("input").ok().flatten(),
        nonce: row.try_get("nonce").unwrap_or_default(),
        tx_type,
        status: row.try_get("status").ok().flatten(),
    }
}

/// Build a token from a `tokens` row.
pub fn token_from_row(row: &sqlx::postgres::PgRow) -> Token {
    // The schema's `contract_type` column is the enum; the token standard is
    // carried there for tokens that are contracts.
    let contract_type: Option<String> = row
        .try_get::<Option<String>, _>("contract_type")
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<serde_json::Value>, _>("contract_type")
                .ok()
                .flatten()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        });
    let token_type = match contract_type.as_deref() {
        Some("ERC721") | Some("erc721") => TokenType::ERC721,
        Some("ERC1155") | Some("erc1155") => TokenType::ERC1155,
        Some("ERC20") | Some("erc20") | Some("EIP20") => TokenType::ERC20,
        _ => TokenType::Unknown,
    };
    Token {
        address: row
            .try_get::<Option<String>, _>("address")
            .ok()
            .flatten()
            .or_else(|| row.try_get::<Option<String>, _>("contract_address").ok().flatten())
            .unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        symbol: row.try_get("symbol").unwrap_or_default(),
        decimals: row.try_get("decimals").unwrap_or_default(),
        total_supply: numeric(row, "total_supply"),
        token_type,
        price: float_of(row, "price_usd"),
        price_24h_ago: {
            let price = float_of(row, "price_usd");
            let change = float_of(row, "price_change_24h");
            match (price, change) {
                // The stored column is a percent change, so the earlier price
                // is recovered from it rather than stored twice.
                (Some(p), Some(c)) if c != -100.0 => Some(p / (1.0 + c / 100.0)),
                _ => None,
            }
        },
        market_cap: float_of(row, "market_cap"),
        volume_24h: float_of(row, "volume_24h"),
        holders_count: row.try_get("holders_count").ok().flatten(),
        transfers_count: row.try_get("transfers_count").ok().flatten(),
        is_verified: row.try_get("is_verified").unwrap_or(false),
        is_spam: row.try_get("is_spam").unwrap_or(false),
    }
}

/// Build a contract from a `contracts` row.
pub fn contract_from_row(row: &sqlx::postgres::PgRow) -> Contract {
    Contract {
        address: row.try_get("address").unwrap_or_default(),
        bytecode: row.try_get("runtime_bytecode").ok().flatten(),
        bytecode_hash: None,
        is_verified: row.try_get("is_verified").unwrap_or(false),
        is_contract: true,
        contract_type: row.try_get("contract_type").ok().flatten(),
        source_code: row.try_get("source_code").ok().flatten(),
        compiler_version: row.try_get("compiler_version").ok().flatten(),
        evm_version: row.try_get("evm_version").ok().flatten(),
        license: row.try_get("license_type").ok().flatten(),
    }
}

/// Build an NFT collection from an `nft_collections` row.
pub fn nft_collection_from_row(row: &sqlx::postgres::PgRow) -> NFTCollection {
    let contract_type = row
        .try_get::<Option<String>, _>("contract_type")
        .ok()
        .flatten()
        .unwrap_or_else(|| "ERC721".to_string());
    let social = json_of(row, "social");
    NFTCollection {
        address: row.try_get("address").unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        symbol: row.try_get("symbol").ok().flatten(),
        contract_type: if contract_type.eq_ignore_ascii_case("ERC1155") {
            NFTContractType::ERC1155
        } else {
            NFTContractType::ERC721
        },
        total_supply: row.try_get("total_supply").ok().flatten(),
        minted_count: None,
        owner_count: row.try_get("holders_count").ok().flatten(),
        floor_price: numeric(row, "floor_price"),
        average_price: numeric(row, "average_price_24h"),
        volume_24h: numeric(row, "volume_24h"),
        volume_7d: None,
        image_url: row.try_get("image_url").ok().flatten(),
        description: row.try_get("description").ok().flatten(),
        twitter: social
            .as_ref()
            .and_then(|s| s.get("twitter"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        discord: social
            .as_ref()
            .and_then(|s| s.get("discord"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_verified: row.try_get("is_verified").unwrap_or(false),
        is_spam: row.try_get("is_spam").unwrap_or(false),
    }
}

/// Build a log from a `logs` row.
pub fn log_from_row(row: &sqlx::postgres::PgRow) -> Log {
    Log {
        id: row.try_get("id").unwrap_or_default(),
        address: row.try_get("address").unwrap_or_default(),
        topics: text_array(row, "topics"),
        data: row.try_get("data").unwrap_or_default(),
        block_number: row.try_get("block_number").unwrap_or_default(),
        transaction_hash: row.try_get("transaction_hash").unwrap_or_default(),
        log_index: row
            .try_get::<Option<i32>, _>("log_index")
            .ok()
            .flatten()
            .unwrap_or_default() as i64,
    }
}

/// Build a trace from a `traces` row.
pub fn trace_from_row(row: &sqlx::postgres::PgRow) -> Trace {
    Trace {
        id: row.try_get("id").unwrap_or_default(),
        transaction_hash: row.try_get("transaction_hash").unwrap_or_default(),
        block_number: row.try_get("block_number").unwrap_or_default(),
        subtrace_index: row
            .try_get::<Option<i32>, _>("transaction_index")
            .ok()
            .flatten()
            .unwrap_or_default(),
        call_type: row.try_get("call_type").unwrap_or_default(),
        from_address: row.try_get("from_address").unwrap_or_default(),
        to_address: row
            .try_get::<Option<String>, _>("to_address")
            .ok()
            .flatten()
            .unwrap_or_default(),
        value: numeric(row, "value"),
        gas: row.try_get("gas").ok().flatten(),
        gas_used: None,
        input: row.try_get("input").ok().flatten(),
        output: row.try_get("output").ok().flatten(),
        error: row.try_get("error").ok().flatten(),
        depth: row.try_get("depth").unwrap_or_default(),
        trace_type: row.try_get("call_type").unwrap_or_default(),
    }
}

/// Build a receipt from a `transactions` row and its `logs` rows.
pub fn receipt_from_row(row: &sqlx::postgres::PgRow, logs: Vec<Log>) -> Receipt {
    Receipt {
        transaction_hash: row.try_get("hash").unwrap_or_default(),
        block_number: row.try_get::<Option<i64>, _>("block_number").ok().flatten().unwrap_or_default(),
        block_hash: row.try_get::<Option<String>, _>("block_hash").ok().flatten().unwrap_or_default(),
        contract_address: row.try_get("contract_address").ok().flatten(),
        cumulative_gas_used: row.try_get::<Option<i64>, _>("cumulative_gas_used").ok().flatten().unwrap_or_default(),
        gas_used: row.try_get::<Option<i64>, _>("gas_used").ok().flatten().unwrap_or_default(),
        logs_bloom: String::new(),
        status: row.try_get("status").ok().flatten(),
        logs,
    }
}

/// The columns every block query selects.
pub const BLOCK_COLUMNS: &str = "number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
    transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty,
    size, gas_limit, gas_used, timestamp, extra_data, base_fee_per_gas";

/// The columns every transaction query selects.
pub const TRANSACTION_COLUMNS: &str = "hash, nonce, block_hash, block_number, transaction_index,
    from_address, to_address, value, gas_price, gas_limit, input, transaction_type, status";

/// The columns every token query selects.
pub const TOKEN_COLUMNS: &str = "address, name, symbol, decimals, total_supply, holders_count,
    transfers_count, price_usd, price_change_24h, market_cap, volume_24h, is_verified, is_spam,
    contract_address";

/// The columns every contract query selects.
pub const CONTRACT_COLUMNS: &str = "address, runtime_bytecode, is_verified, contract_type,
    source_code, compiler_version, evm_version, license_type";

/// The columns every NFT collection query selects.
pub const NFT_COLLECTION_COLUMNS: &str = "address, name, symbol, contract_type, total_supply,
    holders_count, floor_price, average_price_24h, volume_24h, description, image_url, social,
    is_verified, is_spam";

/// The columns an NFT token query selects.
pub const NFT_TOKEN_COLUMNS: &str = "id, token_address, token_id, owner, uri, image_url,
    external_url, attributes";

/// A block looked up by its hash.
pub const BLOCK_COLUMNS_QUERY_HASH: &str = "SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit, gas_used, timestamp, extra_data, base_fee_per_gas FROM blocks WHERE hash = $1";
/// A block looked up by its number.
pub const BLOCK_COLUMNS_QUERY_NUMBER: &str = "SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit, gas_used, timestamp, extra_data, base_fee_per_gas FROM blocks WHERE number = $1";
/// The highest block.
pub const BLOCK_COLUMNS_QUERY_LATEST: &str = "SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit, gas_used, timestamp, extra_data, base_fee_per_gas FROM blocks WHERE is_uncle = false ORDER BY number DESC LIMIT 1";
/// A transaction looked up by its hash.
pub const TRANSACTION_COLUMNS_QUERY: &str = "SELECT hash, nonce, block_hash, block_number, transaction_index, from_address, to_address, value, gas_price, gas_limit, input, transaction_type, status FROM transactions WHERE hash = $1";
/// A token looked up by its address.
pub const TOKEN_COLUMNS_QUERY: &str = "SELECT address, name, symbol, decimals, total_supply, holders_count, transfers_count, price_usd, price_change_24h, market_cap, volume_24h, is_verified, is_spam, contract_address FROM tokens WHERE address = $1";
/// A contract looked up by its address.
pub const CONTRACT_COLUMNS_QUERY: &str = "SELECT address, runtime_bytecode, is_verified, contract_type, source_code, compiler_version, evm_version, license_type FROM contracts WHERE address = $1";
/// An NFT collection looked up by its address.
pub const NFT_COLLECTION_COLUMNS_QUERY: &str = "SELECT address, name, symbol, contract_type, total_supply, holders_count, floor_price, average_price_24h, volume_24h, description, image_url, social, is_verified, is_spam FROM nft_collections WHERE address = $1";

/// Build an NFT token from an `nfts` row.
pub fn nft_token_from_row(row: &sqlx::postgres::PgRow) -> NFTToken {
    let attributes = match row.try_get::<Option<JsonValue>, _>("attributes").ok().flatten() {
        Some(JsonValue::Array(items)) => Some(
            items
                .iter()
                .filter_map(|item| {
                    let object = item.as_object()?;
                    Some(NFTAttribute {
                        trait_type: object
                            .get("trait_type")
                            .or_else(|| object.get("traitType"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        value: json_string(object.get("value")).unwrap_or_default(),
                        display_type: object
                            .get("display_type")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    })
                })
                .collect(),
        ),
        _ => None,
    };
    NFTToken {
        id: row.try_get("id").unwrap_or_default(),
        collection_address: row.try_get("token_address").unwrap_or_default(),
        token_id: numeric(row, "token_id").unwrap_or_else(|| "0".to_string()),
        owner: row.try_get("owner").ok().flatten(),
        uri: row.try_get("uri").ok().flatten(),
        image_url: row.try_get("image_url").ok().flatten(),
        animation_url: row.try_get("external_url").ok().flatten(),
        attributes,
    }
}

/// The timestamp of a block, used to date a transfer that only stores a block
/// number. A missing block gives zero, which the client reads as unknown.
pub async fn block_timestamp(db: &crate::db::Database, number: Option<i64>) -> i64 {
    let Some(number) = number else { return 0 };
    sqlx::query("SELECT timestamp FROM blocks WHERE number = $1")
        .bind(number)
        .fetch_optional(db.pool())
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get::<i64, _>("timestamp").ok())
        .unwrap_or_default()
}

/// keccak256 of the bytes a hex string denotes, as a `0x`-prefixed hash.
///
/// The argument must be hex. Text that is not hex has no bytes to hash, so it
/// is rejected rather than silently hashed as the empty string, which would
/// make every distinct non-hex input collide on one digest.
pub fn keccak_hex(hex: &str) -> String {
    match hex::decode(hex.trim_start_matches("0x")) {
        Ok(bytes) => keccak_bytes(&bytes),
        Err(_) => String::new(),
    }
}

/// keccak256 of arbitrary bytes, as a `0x`-prefixed hash.
pub fn keccak_bytes(bytes: &[u8]) -> String {
    use sha3::{Digest, Keccak256};
    format!("0x{}", hex::encode(Keccak256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_round_trip_to_offsets() {
        assert_eq!(offset_from_cursor(Some("cursor:0")), 0);
        assert_eq!(offset_from_cursor(Some("cursor:25")), 25);
        // A cursor that cannot be read means the client starts over, which is
        // the only interpretation that cannot skip rows.
        assert_eq!(offset_from_cursor(Some("garbage")), 0);
        assert_eq!(offset_from_cursor(None), 0);
    }

    #[test]
    fn page_sizes_are_bounded() {
        assert_eq!(page_size(None), DEFAULT_PAGE);
        assert_eq!(page_size(Some(0)), DEFAULT_PAGE);
        assert_eq!(page_size(Some(-5)), DEFAULT_PAGE);
        assert_eq!(page_size(Some(10)), 10);
        // A client cannot ask for more than the cap, so one query cannot pull
        // the whole table.
        assert_eq!(page_size(Some(1_000_000)), MAX_PAGE);
    }

    #[test]
    fn keccak_matches_a_known_digest() {
        // keccak256("") is a published constant.
        assert_eq!(
            keccak_hex("0x"),
            "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
        // keccak256("abc") is also published.
        assert_eq!(
            keccak_bytes(b"abc"),
            "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }

    #[test]
    fn keccak_hex_rejects_non_hex_instead_of_colliding() {
        // Text that is not hex must not be hashed as an empty input, because
        // every such input would then share one digest.
        assert_eq!(keccak_hex("tsk_01"), "");
        assert_eq!(keccak_hex("not-hex"), "");
        assert_ne!(keccak_bytes(b"tsk_01"), keccak_bytes(b"tsk_02"));
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::schema::build_schema;

    /// Whether the SDL declares `field` on the given type.
    fn declares(sdl: &str, type_name: &str, field: &str) -> bool {
        let mut in_type = false;
        for line in sdl.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(&format!("type {type_name} ")) || trimmed == format!("type {type_name} {{") {
                in_type = true;
                continue;
            }
            if in_type {
                if trimmed == "}" {
                    return false;
                }
                // A field line is `name`, `name(` or `name:`.
                let name = trimmed.split(['(', ':', ' ']).next().unwrap_or_default();
                if name == field {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn schema_builds_and_registers_every_root_type() {
        // Building the schema runs the same registration the server does at
        // start-up, so a type that cannot be registered fails the test rather
        // than the running service.
        let schema = build_schema();
        let sdl = schema.sdl();

        for field in [
            "block", "blocks", "transaction", "transactions", "contract",
            "searchContracts", "token", "tokens", "tokenTransfers", "tokenHolders",
            "priceHistory", "nftCollection", "nftCollections", "nftToken", "nftTokens",
            "nftTransfers", "address", "addresses", "resolveEns", "analytics",
            "blockStats", "gasOracle", "traces", "logs", "search",
        ] {
            assert!(declares(&sdl, "Query", field), "Query.{field} is missing");
        }
        for field in [
            "verifyContract", "reportMaliciousContract", "createApiKey",
            "createWebhook", "subscribeAlerts", "simulateTransaction",
        ] {
            assert!(declares(&sdl, "Mutation", field), "Mutation.{field} is missing");
        }
        for field in [
            "newBlocks", "newTransactions", "pendingTransactions", "tokenTransfers",
            "nftTransfers", "logs", "gasPrice", "contractEvents", "alerts",
        ] {
            assert!(
                declares(&sdl, "Subscription", field),
                "Subscription.{field} is missing"
            );
        }

        // The search union must expose its variants, including the empty case.
        assert!(sdl.contains("union SearchResult"));
        assert!(sdl.contains("Block") && sdl.contains("EmptyResult"));
        // The pagination wrapper must be generated for the paginated types.
        assert!(sdl.contains("type BlockConnection"));
        assert!(sdl.contains("type TokenConnection"));
        assert!(sdl.contains("type NFTTransferConnection"));
    }
}
