//! GraphQL Queries for TigerScan
//!
//! Every resolver reads the indexed database. The few answers the index does
//! not hold — a live balance, a gas estimate — are read from the node over
//! JSON-RPC. No resolver invents a value: when there is no data the result is
//! empty or absent, which is what a client can act on.

use crate::db::{normalize_hex_key, sanitize_search_term, Database};
use crate::model::*;
use crate::schema::*;
use async_graphql::{Context, Object};
use bigdecimal::BigDecimal;
use serde_json::Value as JsonValue;
use sqlx::Row;
use std::str::FromStr;

// =============================================================================
// SHARED LOADERS
//
// These serve both the root queries and the field resolvers on `Block`,
// `Transaction`, `Token` and `NFTToken`, so a nested field and a root query
// cannot disagree about what the data is.
// =============================================================================

/// The transactions of a block, in mining order.
pub async fn load_block_transactions(db: &Database, number: i64) -> Vec<Transaction> {
    let rows = sqlx::query(
        "SELECT hash, nonce, block_hash, block_number, transaction_index, from_address,
                to_address, value, gas_price, gas_limit, input, transaction_type, status
         FROM transactions WHERE block_number = $1 ORDER BY transaction_index",
    )
    .bind(number)
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();
    rows.iter().map(transaction_from_row).collect()
}

/// The uncles of a block.
pub async fn load_block_uncles(db: &Database, number: i64) -> Vec<Uncle> {
    let rows = sqlx::query(
        "SELECT hash, block_number, miner, parent_hash, difficulty, gas_limit, timestamp
         FROM uncles WHERE block_number = $1 ORDER BY id",
    )
    .bind(number)
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();
    rows.iter()
        .map(|row| Uncle {
            hash: row.try_get("hash").unwrap_or_default(),
            block_number: row.try_get("block_number").unwrap_or_default(),
            miner: row.try_get("miner").unwrap_or_default(),
            parent_hash: row.try_get("parent_hash").unwrap_or_default(),
            difficulty: row.try_get("difficulty").unwrap_or_default(),
            gas_limit: row.try_get("gas_limit").ok().flatten(),
            timestamp: row.try_get("timestamp").unwrap_or_default(),
        })
        .collect()
}

/// The receipt for a transaction: the row itself plus the logs it produced.
pub async fn load_receipt(db: &Database, hash: &str) -> Option<Receipt> {
    let row = sqlx::query(
        "SELECT hash, block_number, block_hash, contract_address, cumulative_gas_used,
                gas_used, status
         FROM transactions WHERE hash = $1",
    )
    .bind(normalize_hex_key(hash))
    .fetch_optional(db.pool())
    .await
    .ok()
    .flatten()?;
    let logs = load_logs_for_transaction(db, hash).await;
    Some(receipt_from_row(&row, logs))
}

/// The logs a transaction emitted, in index order.
async fn load_logs_for_transaction(db: &Database, hash: &str) -> Vec<Log> {
    let rows = sqlx::query(
        "SELECT id, address, topics, data, block_number, transaction_hash, log_index
         FROM logs WHERE transaction_hash = $1 ORDER BY log_index",
    )
    .bind(normalize_hex_key(hash))
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();
    rows.iter().map(log_from_row).collect()
}

/// The traces of a transaction, outermost call first.
pub async fn load_traces(db: &Database, hash: &str) -> Vec<Trace> {
    let rows = sqlx::query(
        "SELECT id, transaction_hash, block_number, transaction_index, from_address,
                to_address, call_type, value, gas, input, output, error, depth
         FROM traces WHERE transaction_hash = $1 ORDER BY depth, id",
    )
    .bind(normalize_hex_key(hash))
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();
    rows.iter().map(trace_from_row).collect()
}

/// The largest holders of a token.
pub async fn load_token_holders(db: &Database, token: &str, limit: i64) -> Vec<Holder> {
    let rows = sqlx::query(
        "SELECT address, balance, percent_holdings FROM token_holders
         WHERE token_address = $1 ORDER BY balance DESC NULLS LAST LIMIT $2",
    )
    .bind(normalize_hex_key(token))
    .bind(limit)
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();
    rows.iter()
        .map(|row| Holder {
            address: row.try_get("address").unwrap_or_default(),
            balance: numeric(row, "balance").unwrap_or_else(|| "0".to_string()),
            percentage: float_of(row, "percent_holdings").unwrap_or_default(),
        })
        .collect()
}

/// The metadata document of an NFT.
pub async fn load_nft_metadata(
    db: &Database,
    collection: &str,
    token_id: &str,
) -> Option<NFTMetadata> {
    let id = BigDecimal::from_str(token_id).ok()?;
    let row = sqlx::query(
        "SELECT metadata, name, description, image_url, external_url, attributes
         FROM nfts WHERE token_address = $1 AND token_id = $2",
    )
    .bind(normalize_hex_key(collection))
    .bind(id)
    .fetch_optional(db.pool())
    .await
    .ok()
    .flatten()?;

    // The JSONB document is authoritative for the fields it has; the columns
    // fill in whatever the document omits.
    let document = json_of(&row, "metadata");
    let pick = |key: &str| -> Option<String> {
        document
            .as_ref()
            .and_then(|d| d.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let attributes = match json_of(&row, "attributes") {
        Some(JsonValue::Array(items)) => items
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
        _ => Vec::new(),
    };

    Some(NFTMetadata {
        name: pick("name").or_else(|| row.try_get("name").ok().flatten()),
        description: pick("description").or_else(|| row.try_get("description").ok().flatten()),
        image: pick("image").or_else(|| row.try_get("image_url").ok().flatten()),
        external_url: pick("external_url")
            .or_else(|| row.try_get("external_url").ok().flatten()),
        background_color: pick("background_color"),
        attributes,
    })
}

/// A gas oracle built from the fee history the node has recorded.
///
/// The three tiers are the 25th, 50th and 90th percentile of the gas price of
/// recent blocks. Deriving them from what was actually paid means the answer
/// moves with the chain instead of being a number someone chose.
pub async fn gas_tiers(db: &Database) -> GasPrice {
    let rows = sqlx::query(
        "SELECT gas_price FROM gas_prices WHERE gas_price IS NOT NULL ORDER BY id DESC LIMIT 200",
    )
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();

    let mut prices: Vec<i64> = rows
        .iter()
        .filter_map(|row| row.try_get::<Option<i64>, _>("gas_price").ok().flatten())
        .collect();
    if prices.is_empty() {
        // Nothing recorded yet, so the node's own base fee is the only
        // defensible answer.
        let base = rpc_quantity(db, "eth_gasPrice", &[]).await.unwrap_or(0);
        return GasPrice {
            slow: base,
            standard: base,
            fast: base,
        };
    }
    prices.sort_unstable();
    let percentile = |p: f64| -> i64 {
        let index = ((p / 100.0) * (prices.len() as f64 - 1.0)).round() as usize;
        prices[index.min(prices.len() - 1)]
    };
    GasPrice {
        slow: percentile(25.0),
        standard: percentile(50.0),
        fast: percentile(90.0),
    }
}

// =============================================================================
// JSON-RPC
// =============================================================================

/// Call a JSON-RPC method and return the raw result.
///
/// A node that cannot be reached produces `None`, which the caller reports as
/// an absent field rather than a zero the client would believe.
async fn rpc_call(db: &Database, method: &str, params: &[JsonValue]) -> Option<JsonValue> {
    let url = db.rpc_url()?;
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let response = client.post(url).json(&body).send().await.ok()?;
    let value: JsonValue = response.json().await.ok()?;
    value.get("result").cloned()
}

/// A JSON-RPC quantity decoded as an integer.
async fn rpc_quantity(db: &Database, method: &str, params: &[JsonValue]) -> Option<i64> {
    let result = rpc_call(db, method, params).await?;
    let text = result.as_str()?;
    i64::from_str_radix(text.trim_start_matches("0x"), 16).ok()
}

/// A JSON-RPC string result.
async fn rpc_string(db: &Database, method: &str, params: &[JsonValue]) -> Option<String> {
    rpc_call(db, method, params).await?.as_str().map(|s| s.to_string())
}

// =============================================================================
// QUERY ROOT
// =============================================================================

#[Object(name = "Query")]
impl QueryRoot {
    /// Get block by number or hash
    pub async fn block(
        &self,
        ctx: &Context<'_>,
        number: Option<i64>,
        hash: Option<String>,
    ) -> async_graphql::Result<Option<Block>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        let row = match (number, hash) {
            (_, Some(hash)) => {
                sqlx::query(BLOCK_COLUMNS_QUERY_HASH)
                    .bind(normalize_hex_key(&hash))
                    .fetch_optional(db.pool())
                    .await?
            }
            (Some(number), None) => {
                sqlx::query(BLOCK_COLUMNS_QUERY_NUMBER)
                    .bind(number)
                    .fetch_optional(db.pool())
                    .await?
            }
            (None, None) => {
                sqlx::query(BLOCK_COLUMNS_QUERY_LATEST)
                    .fetch_optional(db.pool())
                    .await?
            }
        };
        Ok(row.as_ref().map(block_from_row))
    }

    /// Get blocks with filtering and pagination
    pub async fn blocks(
        &self,
        ctx: &Context<'_>,
        filter: Option<BlockFilter>,
        first: Option<i32>,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<Block>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Connection::new(false, Vec::new(), 0));
        };
        let limit = page_size(first);
        let offset = offset_from_cursor(after.as_deref());

        // The filter is applied in SQL rather than after the fact, so a page
        // is a page of matching blocks and not of whatever happened to be in
        // the window.
        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {BLOCK_COLUMNS} FROM blocks WHERE is_uncle = false"
        ));
        if let Some(filter) = filter {
            if let Some(from) = filter.from_block {
                builder.push(" AND number >= ").push_bind(from);
            }
            if let Some(to) = filter.to_block {
                builder.push(" AND number <= ").push_bind(to);
            }
            if let Some(miner) = filter.miner {
                builder.push(" AND miner = ").push_bind(normalize_hex_key(&miner));
            }
            if let Some(hash) = filter.hash {
                builder.push(" AND hash = ").push_bind(normalize_hex_key(&hash));
            }
        }
        builder.push(" ORDER BY number DESC LIMIT ");
        builder.push_bind(limit + 1);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder.build().fetch_all(db.pool()).await?;
        let has_next = rows.len() as i64 > limit;
        let nodes: Vec<Block> = rows.iter().take(limit as usize).map(block_from_row).collect();
        Ok(Connection::new(has_next, nodes, offset))
    }

    /// Get transaction by hash
    pub async fn transaction(
        &self,
        ctx: &Context<'_>,
        hash: String,
    ) -> async_graphql::Result<Option<Transaction>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        let row = sqlx::query(TRANSACTION_COLUMNS_QUERY)
            .bind(normalize_hex_key(&hash))
            .fetch_optional(db.pool())
            .await?;
        Ok(row.as_ref().map(transaction_from_row))
    }

    /// Get transactions with filtering
    pub async fn transactions(
        &self,
        ctx: &Context<'_>,
        filter: Option<TransactionFilter>,
        first: Option<i32>,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<Transaction>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Connection::new(false, Vec::new(), 0));
        };
        let limit = page_size(first);
        let offset = offset_from_cursor(after.as_deref());

        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {TRANSACTION_COLUMNS} FROM transactions WHERE TRUE"
        ));
        if let Some(filter) = filter {
            if let Some(from) = filter.from {
                builder.push(" AND from_address = ").push_bind(normalize_hex_key(&from));
            }
            if let Some(to) = filter.to {
                builder.push(" AND to_address = ").push_bind(normalize_hex_key(&to));
            }
            if let Some(block) = filter.block {
                builder.push(" AND block_number = ").push_bind(block);
            }
            if let Some(from) = filter.from_block {
                builder.push(" AND block_number >= ").push_bind(from);
            }
            if let Some(to) = filter.to_block {
                builder.push(" AND block_number <= ").push_bind(to);
            }
        }
        builder.push(
            " ORDER BY block_number DESC NULLS LAST, transaction_index DESC NULLS LAST LIMIT ",
        );
        builder.push_bind(limit + 1);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder.build().fetch_all(db.pool()).await?;
        let has_next = rows.len() as i64 > limit;
        let nodes: Vec<Transaction> = rows
            .iter()
            .take(limit as usize)
            .map(transaction_from_row)
            .collect();
        Ok(Connection::new(has_next, nodes, offset))
    }

    /// Get contract by address
    pub async fn contract(
        &self,
        ctx: &Context<'_>,
        address: String,
    ) -> async_graphql::Result<Option<Contract>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        let key = normalize_hex_key(&address);

        if let Some(row) = sqlx::query(CONTRACT_COLUMNS_QUERY)
            .bind(&key)
            .fetch_optional(db.pool())
            .await?
        {
            return Ok(Some(contract_from_row(&row)));
        }

        // A contract that has never been submitted for verification is still a
        // contract, so when the `contracts` table has no row the code is read
        // from the node. Reporting `None` would tell the client the address is
        // not a contract, which would be wrong.
        let latest = JsonValue::String("latest".to_string());
        let code = rpc_string(
            db,
            "eth_getCode",
            &[JsonValue::String(key.clone()), latest],
        )
        .await;
        let Some(code) = code else {
            return Ok(None);
        };
        if code.is_empty() || code == "0x" {
            return Ok(Some(Contract {
                address: key,
                bytecode: None,
                bytecode_hash: None,
                is_verified: false,
                is_contract: false,
                contract_type: None,
                source_code: None,
                compiler_version: None,
                evm_version: None,
                license: None,
            }));
        }
        let hash = keccak_hex(&code);
        Ok(Some(Contract {
            address: key,
            bytecode: Some(code),
            bytecode_hash: Some(hash),
            is_verified: false,
            is_contract: true,
            contract_type: None,
            source_code: None,
            compiler_version: None,
            evm_version: None,
            license: None,
        }))
    }

    /// Search contracts by bytecode
    pub async fn search_contracts(
        &self,
        ctx: &Context<'_>,
        bytecode: String,
    ) -> async_graphql::Result<Vec<Contract>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Vec::new());
        };
        let needle = bytecode.trim().trim_start_matches("0x").to_ascii_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        // A substring of the runtime code is the only thing a user can supply
        // without knowing the whole program, and it is how a clone is found.
        let rows = sqlx::query(
            "SELECT address, runtime_bytecode, is_verified, contract_type, source_code,
                    compiler_version, evm_version, license_type
             FROM contracts WHERE runtime_bytecode ILIKE '%' || $1 || '%'
             ORDER BY is_verified DESC, id DESC LIMIT 100",
        )
        .bind(&needle)
        .fetch_all(db.pool())
        .await?;
        Ok(rows.iter().map(contract_from_row).collect())
    }

    /// Get token by address
    pub async fn token(
        &self,
        ctx: &Context<'_>,
        address: String,
    ) -> async_graphql::Result<Option<Token>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        let row = sqlx::query(TOKEN_COLUMNS_QUERY)
            .bind(normalize_hex_key(&address))
            .fetch_optional(db.pool())
            .await?;
        Ok(row.as_ref().map(token_from_row))
    }

    /// Get tokens with filtering
    pub async fn tokens(
        &self,
        ctx: &Context<'_>,
        token_type: Option<TokenType>,
        search: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<Token>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Connection::new(false, Vec::new(), 0));
        };
        let limit = page_size(first);
        let offset = offset_from_cursor(after.as_deref());

        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {TOKEN_COLUMNS} FROM tokens WHERE TRUE"
        ));
        if let Some(kind) = token_type {
            let name = match kind {
                TokenType::ERC20 => "ERC20",
                TokenType::ERC721 => "ERC721",
                TokenType::ERC1155 => "ERC1155",
                TokenType::Unknown => "Unknown",
            };
            builder.push(" AND contract_type = ").push_bind(name);
        }
        if let Some(term) = search.filter(|s| !s.trim().is_empty()) {
            let pattern = format!("%{}%", term.trim());
            builder
                .push(" AND (name ILIKE ")
                .push_bind(pattern.clone())
                .push(" OR symbol ILIKE ")
                .push_bind(pattern)
                .push(")");
        }
        builder.push(" ORDER BY holders_count DESC NULLS LAST, id DESC LIMIT ");
        builder.push_bind(limit + 1);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder.build().fetch_all(db.pool()).await?;
        let has_next = rows.len() as i64 > limit;
        let nodes: Vec<Token> = rows.iter().take(limit as usize).map(token_from_row).collect();
        Ok(Connection::new(has_next, nodes, offset))
    }

    /// Get token transfers
    pub async fn token_transfers(
        &self,
        ctx: &Context<'_>,
        token: String,
        from: Option<String>,
        to: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<TokenTransfer>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Connection::new(false, Vec::new(), 0));
        };
        let limit = page_size(first);
        let offset = offset_from_cursor(after.as_deref());

        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, token_address, from_address, to_address, value, transaction_hash, block_number
             FROM token_transfers WHERE token_address = ",
        );
        builder.push_bind(normalize_hex_key(&token));
        if let Some(from) = from {
            builder.push(" AND from_address = ").push_bind(normalize_hex_key(&from));
        }
        if let Some(to) = to {
            builder.push(" AND to_address = ").push_bind(normalize_hex_key(&to));
        }
        builder.push(" ORDER BY block_number DESC, log_index DESC LIMIT ");
        builder.push_bind(limit + 1);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder.build().fetch_all(db.pool()).await?;
        let has_next = rows.len() as i64 > limit;
        let mut nodes = Vec::with_capacity(rows.len().min(limit as usize));
        for row in rows.iter().take(limit as usize) {
            let block_number: i64 = row.try_get("block_number").unwrap_or_default();
            nodes.push(TokenTransfer {
                id: row.try_get("id").unwrap_or_default(),
                token_address: row.try_get("token_address").unwrap_or_default(),
                from_address: row.try_get("from_address").unwrap_or_default(),
                to_address: row.try_get("to_address").unwrap_or_default(),
                value: numeric(row, "value").unwrap_or_else(|| "0".to_string()),
                transaction_hash: row.try_get("transaction_hash").unwrap_or_default(),
                block_number,
                timestamp: block_timestamp(db, Some(block_number)).await,
            });
        }
        Ok(Connection::new(has_next, nodes, offset))
    }

    /// Get token holders
    pub async fn token_holders(
        &self,
        ctx: &Context<'_>,
        address: String,
        first: Option<i32>,
    ) -> async_graphql::Result<Vec<Holder>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Vec::new());
        };
        Ok(load_token_holders(db, &address, page_size(first)).await)
    }

    /// Get price history
    pub async fn price_history(
        &self,
        ctx: &Context<'_>,
        address: String,
        days: i32,
    ) -> async_graphql::Result<Vec<PriceHistoryPoint>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Vec::new());
        };
        // A negative or zero window would ask for a range that ends before it
        // begins, so it is treated as the default day.
        let days = if days > 0 { days as i64 } else { 1 };
        let rows = sqlx::query(
            "SELECT timestamp, price_usd FROM token_prices
             WHERE token_address = $1
               AND timestamp >= EXTRACT(EPOCH FROM (NOW() - ($2 || ' days')::interval))
             ORDER BY timestamp",
        )
        .bind(normalize_hex_key(&address))
        .bind(days)
        .fetch_all(db.pool())
        .await?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                Some(PriceHistoryPoint {
                    timestamp: row.try_get("timestamp").ok()?,
                    price: float_of(row, "price_usd")?,
                })
            })
            .collect())
    }

    /// Get NFT collection
    pub async fn nft_collection(
        &self,
        ctx: &Context<'_>,
        address: String,
    ) -> async_graphql::Result<Option<NFTCollection>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        let row = sqlx::query(NFT_COLLECTION_COLUMNS_QUERY)
            .bind(normalize_hex_key(&address))
            .fetch_optional(db.pool())
            .await?;
        Ok(row.as_ref().map(nft_collection_from_row))
    }

    /// Get NFT collections
    pub async fn nft_collections(
        &self,
        ctx: &Context<'_>,
        search: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<NFTCollection>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Connection::new(false, Vec::new(), 0));
        };
        let limit = page_size(first);
        let offset = offset_from_cursor(after.as_deref());

        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {NFT_COLLECTION_COLUMNS} FROM nft_collections WHERE TRUE"
        ));
        if let Some(term) = search.filter(|s| !s.trim().is_empty()) {
            let pattern = format!("%{}%", term.trim());
            builder
                .push(" AND (name ILIKE ")
                .push_bind(pattern.clone())
                .push(" OR symbol ILIKE ")
                .push_bind(pattern)
                .push(")");
        }
        builder.push(" ORDER BY volume_24h DESC NULLS LAST, id DESC LIMIT ");
        builder.push_bind(limit + 1);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder.build().fetch_all(db.pool()).await?;
        let has_next = rows.len() as i64 > limit;
        let nodes: Vec<NFTCollection> = rows
            .iter()
            .take(limit as usize)
            .map(nft_collection_from_row)
            .collect();
        Ok(Connection::new(has_next, nodes, offset))
    }

    /// Get NFT token
    pub async fn nft_token(
        &self,
        ctx: &Context<'_>,
        collection: String,
        token_id: String,
    ) -> async_graphql::Result<Option<NFTToken>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        let Some(id) = BigDecimal::from_str(&token_id).ok() else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT id, token_address, token_id, owner, uri, image_url, external_url, attributes
             FROM nfts WHERE token_address = $1 AND token_id = $2",
        )
        .bind(normalize_hex_key(&collection))
        .bind(id)
        .fetch_optional(db.pool())
        .await?;
        Ok(row.as_ref().map(nft_token_from_row))
    }

    /// Get NFT tokens in collection
    pub async fn nft_tokens(
        &self,
        ctx: &Context<'_>,
        collection: String,
        owner: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<NFTToken>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Connection::new(false, Vec::new(), 0));
        };
        let limit = page_size(first);
        let offset = offset_from_cursor(after.as_deref());

        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, token_address, token_id, owner, uri, image_url, external_url, attributes
             FROM nfts WHERE is_burned = false AND token_address = ",
        );
        builder.push_bind(normalize_hex_key(&collection));
        if let Some(owner) = owner {
            builder.push(" AND owner = ").push_bind(normalize_hex_key(&owner));
        }
        builder.push(" ORDER BY token_id LIMIT ");
        builder.push_bind(limit + 1);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder.build().fetch_all(db.pool()).await?;
        let has_next = rows.len() as i64 > limit;
        let nodes: Vec<NFTToken> = rows
            .iter()
            .take(limit as usize)
            .map(nft_token_from_row)
            .collect();
        Ok(Connection::new(has_next, nodes, offset))
    }

    /// Get NFT transfers
    pub async fn nft_transfers(
        &self,
        ctx: &Context<'_>,
        collection: String,
        from: Option<String>,
        to: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> async_graphql::Result<Connection<NFTTransfer>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Connection::new(false, Vec::new(), 0));
        };
        let limit = page_size(first);
        let offset = offset_from_cursor(after.as_deref());

        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, token_address, token_id, from_address, to_address, transaction_hash, block_number
             FROM nft_transfers WHERE token_address = ",
        );
        builder.push_bind(normalize_hex_key(&collection));
        if let Some(from) = from {
            builder.push(" AND from_address = ").push_bind(normalize_hex_key(&from));
        }
        if let Some(to) = to {
            builder.push(" AND to_address = ").push_bind(normalize_hex_key(&to));
        }
        builder.push(" ORDER BY block_number DESC, log_index DESC LIMIT ");
        builder.push_bind(limit + 1);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder.build().fetch_all(db.pool()).await?;
        let has_next = rows.len() as i64 > limit;
        let mut nodes = Vec::with_capacity(rows.len().min(limit as usize));
        for row in rows.iter().take(limit as usize) {
            let block_number: i64 = row.try_get("block_number").unwrap_or_default();
            nodes.push(NFTTransfer {
                id: row.try_get("id").unwrap_or_default(),
                collection_address: row.try_get("token_address").unwrap_or_default(),
                token_id: numeric(row, "token_id").unwrap_or_else(|| "0".to_string()),
                from_address: row.try_get("from_address").unwrap_or_default(),
                to_address: row.try_get("to_address").unwrap_or_default(),
                transaction_hash: row.try_get("transaction_hash").unwrap_or_default(),
                block_number,
                timestamp: block_timestamp(db, Some(block_number)).await,
            });
        }
        Ok(Connection::new(has_next, nodes, offset))
    }

    /// Get address info
    pub async fn address(
        &self,
        ctx: &Context<'_>,
        address: String,
    ) -> async_graphql::Result<Option<Address>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        let key = normalize_hex_key(&address);
        let latest = JsonValue::String("latest".to_string());

        // The indexed row gives the counts and the first and last sighting;
        // the balance and the nonce are live values that only the node has.
        let row = sqlx::query(
            "SELECT address, is_contract, name, ens_name, is_scammer, balance,
                    transaction_count, first_seen_block, last_seen_block
             FROM accounts WHERE address = $1",
        )
        .bind(&key)
        .fetch_optional(db.pool())
        .await?;

        let (balance, transaction_count, is_contract) = match &row {
            Some(row) => (
                numeric(row, "balance"),
                row.try_get::<Option<i64>, _>("transaction_count")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                row.try_get::<Option<bool>, _>("is_contract")
                    .ok()
                    .flatten()
                    .unwrap_or(false),
            ),
            None => {
                let balance = rpc_quantity(
                    db,
                    "eth_getBalance",
                    &[JsonValue::String(key.clone()), latest.clone()],
                )
                .await
                .map(|v| v.to_string());
                let nonce = rpc_quantity(
                    db,
                    "eth_getTransactionCount",
                    &[JsonValue::String(key.clone()), latest.clone()],
                )
                .await
                .unwrap_or_default();
                let code = rpc_string(
                    db,
                    "eth_getCode",
                    &[JsonValue::String(key.clone()), latest],
                )
                .await;
                let is_contract = code.map(|c| c != "0x" && !c.is_empty()).unwrap_or(false);
                // A balance is reported only when the node answered. A node
                // that could not be reached must not look like an empty
                // account.
                (balance, nonce, is_contract)
            }
        };

        Ok(Some(match &row {
            Some(row) => Address {
                address: row.try_get("address").unwrap_or_else(|_| key.clone()),
                is_contract,
                is_multisig: false,
                multisig_owners: None,
                name: row.try_get("name").ok().flatten(),
                ens_name: row.try_get("ens_name").ok().flatten(),
                is_scammer: row.try_get("is_scammer").unwrap_or(false),
                balance,
                transaction_count,
                first_seen_block: row.try_get("first_seen_block").ok().flatten(),
                last_seen_block: row.try_get("last_seen_block").ok().flatten(),
            },
            None => Address {
                address: key,
                is_contract,
                is_multisig: false,
                multisig_owners: None,
                name: None,
                ens_name: None,
                is_scammer: false,
                balance,
                transaction_count,
                first_seen_block: None,
                last_seen_block: None,
            },
        }))
    }

    /// Get addresses
    pub async fn addresses(
        &self,
        ctx: &Context<'_>,
        addresses: Vec<String>,
    ) -> async_graphql::Result<Vec<Address>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Vec::new());
        };
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        // One query for the whole list, so a batch is not one round trip per
        // address.
        let keys: Vec<String> = addresses.iter().map(|a| normalize_hex_key(a)).collect();
        let rows = sqlx::query(
            "SELECT address, is_contract, name, ens_name, is_scammer, balance,
                    transaction_count, first_seen_block, last_seen_block
             FROM accounts WHERE address = ANY($1)",
        )
        .bind(&keys)
        .fetch_all(db.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|row| Address {
                address: row.try_get("address").unwrap_or_default(),
                is_contract: row.try_get("is_contract").unwrap_or(false),
                is_multisig: false,
                multisig_owners: None,
                name: row.try_get("name").ok().flatten(),
                ens_name: row.try_get("ens_name").ok().flatten(),
                is_scammer: row.try_get("is_scammer").unwrap_or(false),
                balance: numeric(row, "balance"),
                transaction_count: row
                    .try_get::<Option<i64>, _>("transaction_count")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                first_seen_block: row.try_get("first_seen_block").ok().flatten(),
                last_seen_block: row.try_get("last_seen_block").ok().flatten(),
            })
            .collect())
    }

    /// Search by name
    pub async fn resolve_ens(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<Option<String>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(None);
        };
        // The name registry is part of the index: `accounts` carries the name
        // attached to the address that owns it.
        let row = sqlx::query("SELECT address FROM accounts WHERE ens_name = $1 LIMIT 1")
            .bind(name.trim().to_ascii_lowercase())
            .fetch_optional(db.pool())
            .await?;
        Ok(row.and_then(|r| r.try_get::<String, _>("address").ok()))
    }

    /// Get analytics
    pub async fn analytics(&self, ctx: &Context<'_>) -> async_graphql::Result<AnalyticsMetric> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(AnalyticsMetric {
                name: "total_transactions".to_string(),
                value: "0".to_string(),
                timestamp: 0,
                change_24h: None,
            });
        };
        // Two days are read so the change is computed from the data instead of
        // being a guess.
        let rows = sqlx::query(
            "SELECT date, total_transactions FROM analytics_daily ORDER BY date DESC LIMIT 2",
        )
        .fetch_all(db.pool())
        .await?;
        let latest = rows.first();
        let value = latest
            .and_then(|r| r.try_get::<Option<i64>, _>("total_transactions").ok().flatten())
            .unwrap_or_default();
        let timestamp = latest
            .and_then(|r| r.try_get::<Option<chrono::NaiveDate>, _>("date").ok().flatten())
            .map(|d| {
                d.and_hms_opt(0, 0, 0)
                    .map(|t| t.and_utc().timestamp())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let change_24h = match (rows.first(), rows.get(1)) {
            (Some(current), Some(previous)) => {
                let current = current
                    .try_get::<Option<i64>, _>("total_transactions")
                    .ok()
                    .flatten();
                let previous = previous
                    .try_get::<Option<i64>, _>("total_transactions")
                    .ok()
                    .flatten();
                match (current, previous) {
                    (Some(c), Some(p)) if p > 0 => Some(((c - p) as f64 / p as f64) * 100.0),
                    _ => None,
                }
            }
            _ => None,
        };
        Ok(AnalyticsMetric {
            name: "total_transactions".to_string(),
            value: value.to_string(),
            timestamp,
            change_24h,
        })
    }

    /// Get block stats
    pub async fn block_stats(&self, ctx: &Context<'_>) -> async_graphql::Result<BlockStats> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(BlockStats {
                total_blocks: 0,
                total_transactions: 0,
                total_addresses: 0,
                total_contracts: 0,
                gas_price: GasPrice { slow: 0, standard: 0, fast: 0 },
                tps: 0.0,
            });
        };
        // One round trip: each count is a scalar subquery, so the numbers are
        // read from one snapshot and cannot disagree with each other.
        let row = sqlx::query(
            "SELECT
                (SELECT count(*) FROM blocks WHERE is_uncle = false) AS total_blocks,
                (SELECT count(*) FROM transactions) AS total_transactions,
                (SELECT count(*) FROM accounts) AS total_addresses,
                (SELECT count(*) FROM contracts) AS total_contracts,
                (SELECT min(timestamp) FROM blocks WHERE is_uncle = false) AS first_timestamp,
                (SELECT max(timestamp) FROM blocks WHERE is_uncle = false) AS last_timestamp",
        )
        .fetch_one(db.pool())
        .await?;

        let total_transactions: i64 = row.try_get("total_transactions").unwrap_or_default();
        let first: Option<i64> = row.try_get("first_timestamp").ok().flatten();
        let last: Option<i64> = row.try_get("last_timestamp").ok().flatten();
        // Transactions per second over the span the chain covers. An unknown
        // span gives zero rather than a rate computed from nothing.
        let tps = match (first, last) {
            (Some(first), Some(last)) if last > first => {
                total_transactions as f64 / (last - first) as f64
            }
            _ => 0.0,
        };

        Ok(BlockStats {
            total_blocks: row.try_get("total_blocks").unwrap_or_default(),
            total_transactions,
            total_addresses: row.try_get("total_addresses").unwrap_or_default(),
            total_contracts: row.try_get("total_contracts").unwrap_or_default(),
            gas_price: gas_tiers(db).await,
            tps,
        })
    }

    /// Get gas oracle
    pub async fn gas_oracle(&self, ctx: &Context<'_>) -> async_graphql::Result<GasPrice> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(GasPrice { slow: 0, standard: 0, fast: 0 });
        };
        Ok(gas_tiers(db).await)
    }

    /// Get traces for transaction
    pub async fn traces(
        &self,
        ctx: &Context<'_>,
        transaction_hash: String,
    ) -> async_graphql::Result<Vec<Trace>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Vec::new());
        };
        Ok(load_traces(db, &transaction_hash).await)
    }

    /// Get logs
    pub async fn logs(
        &self,
        ctx: &Context<'_>,
        address: Option<String>,
        topics: Option<Vec<String>>,
        from_block: Option<i64>,
        to_block: Option<i64>,
        first: Option<i32>,
    ) -> async_graphql::Result<Vec<Log>> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(Vec::new());
        };
        let limit = page_size(first);

        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, address, topics, data, block_number, transaction_hash, log_index
             FROM logs WHERE TRUE",
        );
        if let Some(address) = address {
            builder.push(" AND address = ").push_bind(normalize_hex_key(&address));
        }
        if let Some(from) = from_block {
            builder.push(" AND block_number >= ").push_bind(from);
        }
        if let Some(to) = to_block {
            builder.push(" AND block_number <= ").push_bind(to);
        }
        if let Some(topics) = topics {
            // Topic N matches a log only when the log's Nth topic equals it.
            // Checking each position is what makes a filter precise; a filter
            // that ignored position would match a log whose arguments are in a
            // different order.
            for (index, topic) in topics.iter().enumerate() {
                let topic = topic.trim();
                if topic.is_empty() || topic == "*" || topic == "null" {
                    continue;
                }
                builder
                    .push(format!(" AND topics->>{index} = "))
                    .push_bind(topic.to_ascii_lowercase());
            }
        }
        builder.push(" ORDER BY block_number DESC, log_index DESC LIMIT ");
        builder.push_bind(limit);

        let rows = builder.build().fetch_all(db.pool()).await?;
        Ok(rows.iter().map(log_from_row).collect())
    }

    /// Search full text
    pub async fn search(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<SearchResult> {
        let Some(db) = ctx.data_opt::<Database>() else {
            return Ok(SearchResult::default());
        };
        let term = query.trim();
        if term.is_empty() {
            return Ok(SearchResult::default());
        }

        // A search box accepts three things: a block number, a hash or an
        // address. Recognising the shape means the exact entity is returned
        // rather than a text match that happens to contain it.
        if let Ok(number) = term.parse::<i64>() {
            if let Some(row) = sqlx::query(BLOCK_COLUMNS_QUERY_NUMBER)
                .bind(number)
                .fetch_optional(db.pool())
                .await?
            {
                return Ok(SearchResult::Block(block_from_row(&row)));
            }
        }

        let key = normalize_hex_key(term);
        let body = key.trim_start_matches("0x");
        let is_hex = body.chars().all(|c| c.is_ascii_hexdigit());
        if body.len() == 64 && is_hex {
            if let Some(row) = sqlx::query(TRANSACTION_COLUMNS_QUERY)
                .bind(&key)
                .fetch_optional(db.pool())
                .await?
            {
                return Ok(SearchResult::Transaction(transaction_from_row(&row)));
            }
            if let Some(row) = sqlx::query(BLOCK_COLUMNS_QUERY_HASH)
                .bind(&key)
                .fetch_optional(db.pool())
                .await?
            {
                return Ok(SearchResult::Block(block_from_row(&row)));
            }
        }
        if body.len() == 40 && is_hex {
            if let Some(row) = sqlx::query(TOKEN_COLUMNS_QUERY)
                .bind(&key)
                .fetch_optional(db.pool())
                .await?
            {
                return Ok(SearchResult::Token(token_from_row(&row)));
            }
            if let Some(row) = sqlx::query(CONTRACT_COLUMNS_QUERY)
                .bind(&key)
                .fetch_optional(db.pool())
                .await?
            {
                return Ok(SearchResult::Contract(contract_from_row(&row)));
            }
            if let Some(row) = sqlx::query(NFT_COLLECTION_COLUMNS_QUERY)
                .bind(&key)
                .fetch_optional(db.pool())
                .await?
            {
                return Ok(SearchResult::NFTCollection(nft_collection_from_row(&row)));
            }
        }

        // Nothing matched by shape, so the indexed names are searched. The term
        // is sanitised because `to_tsquery` reads `&`, `|` and `!` as
        // operators, and a user typing them means them as text.
        let tsquery = sanitize_search_term(term);
        if tsquery.is_empty() {
            return Ok(SearchResult::default());
        }
        let row = sqlx::query(
            "SELECT search_type, address, hash, number FROM search_index
             WHERE content_tsv @@ to_tsquery('simple', $1)
                OR name ILIKE '%' || $2 || '%'
             ORDER BY id DESC LIMIT 1",
        )
        .bind(&tsquery)
        .bind(term)
        .fetch_optional(db.pool())
        .await?;
        let Some(row) = row else {
            return Ok(SearchResult::default());
        };
        let kind: String = row.try_get("search_type").unwrap_or_default();
        let address: Option<String> = row.try_get("address").ok().flatten();
        let hash: Option<String> = row.try_get("hash").ok().flatten();
        let number: Option<i64> = row.try_get("number").ok().flatten();

        // The index row names an entity; the entity itself is then loaded, so
        // the caller gets the same shape an exact lookup would return.
        match kind.as_str() {
            "token" => {
                if let Some(address) = address {
                    if let Some(row) = sqlx::query(TOKEN_COLUMNS_QUERY)
                        .bind(address)
                        .fetch_optional(db.pool())
                        .await?
                    {
                        return Ok(SearchResult::Token(token_from_row(&row)));
                    }
                }
            }
            "contract" => {
                if let Some(address) = address {
                    if let Some(row) = sqlx::query(CONTRACT_COLUMNS_QUERY)
                        .bind(address)
                        .fetch_optional(db.pool())
                        .await?
                    {
                        return Ok(SearchResult::Contract(contract_from_row(&row)));
                    }
                }
            }
            "nft_collection" => {
                if let Some(address) = address {
                    if let Some(row) = sqlx::query(NFT_COLLECTION_COLUMNS_QUERY)
                        .bind(address)
                        .fetch_optional(db.pool())
                        .await?
                    {
                        return Ok(SearchResult::NFTCollection(nft_collection_from_row(&row)));
                    }
                }
            }
            "transaction" => {
                if let Some(hash) = hash {
                    if let Some(row) = sqlx::query(TRANSACTION_COLUMNS_QUERY)
                        .bind(hash)
                        .fetch_optional(db.pool())
                        .await?
                    {
                        return Ok(SearchResult::Transaction(transaction_from_row(&row)));
                    }
                }
            }
            _ => {
                if let Some(number) = number {
                    if let Some(row) = sqlx::query(BLOCK_COLUMNS_QUERY_NUMBER)
                        .bind(number)
                        .fetch_optional(db.pool())
                        .await?
                    {
                        return Ok(SearchResult::Block(block_from_row(&row)));
                    }
                }
            }
        }
        Ok(SearchResult::default())
    }
}
