//! Database Queries for TigerScan.
//!
//! Every query binds its arguments as parameters. The filter values are never
//! interpolated into the SQL text, so a value taken from a request cannot
//! change the statement that runs.

use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::models::*;

/// Errors a query can return.
pub type QueryResult<T> = Result<T, sqlx::Error>;

/// The largest page a query will return.
pub const MAX_PAGE_SIZE: i64 = 100;

/// The default page size.
pub const DEFAULT_PAGE_SIZE: i64 = 25;

/// Normalise a page request so a caller cannot ask for an unbounded page.
pub fn page(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    let offset = offset.unwrap_or(0).max(0);

    (limit, offset)
}

/// Block queries.
pub struct BlockQueries;

impl BlockQueries {
    /// Fetch a block by number.
    pub async fn by_number(pool: &PgPool, number: i64) -> QueryResult<Option<Block>> {
        let row = sqlx::query(
            "SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
                    transactions_root, state_root, receipts_root, miner, difficulty,
                    total_difficulty, size, gas_limit, gas_used, timestamp, extra_data,
                    mix_hash, base_fee_per_gas, created_at, updated_at
             FROM blocks WHERE number = $1",
        )
        .bind(number)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(map_block))
    }

    /// Fetch a block by hash.
    pub async fn by_hash(pool: &PgPool, hash: &str) -> QueryResult<Option<Block>> {
        let row = sqlx::query(
            "SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
                    transactions_root, state_root, receipts_root, miner, difficulty,
                    total_difficulty, size, gas_limit, gas_used, timestamp, extra_data,
                    mix_hash, base_fee_per_gas, created_at, updated_at
             FROM blocks WHERE hash = $1",
        )
        .bind(hash)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(map_block))
    }

    /// Fetch the newest blocks.
    pub async fn latest(pool: &PgPool, limit: Option<i64>) -> QueryResult<Vec<Block>> {
        let (limit, _) = page(limit, None);

        let rows = sqlx::query(
            "SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
                    transactions_root, state_root, receipts_root, miner, difficulty,
                    total_difficulty, size, gas_limit, gas_used, timestamp, extra_data,
                    mix_hash, base_fee_per_gas, created_at, updated_at
             FROM blocks ORDER BY number DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(map_block).collect())
    }

    /// The highest block number stored.
    pub async fn latest_number(pool: &PgPool) -> QueryResult<Option<i64>> {
        let row = sqlx::query("SELECT MAX(number) AS number FROM blocks")
            .fetch_one(pool)
            .await?;

        Ok(row.try_get::<Option<i64>, _>("number")?)
    }
}

/// Transaction queries.
pub struct TransactionQueries;

impl TransactionQueries {
    /// Fetch a transaction by hash.
    pub async fn by_hash(pool: &PgPool, hash: &str) -> QueryResult<Option<Transaction>> {
        let row = sqlx::query(
            "SELECT hash, block_number, block_hash, transaction_index, from_address,
                    to_address, value, gas_price, gas, input, nonce, tx_type, status, created_at
             FROM transactions WHERE hash = $1",
        )
        .bind(hash)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(map_transaction))
    }

    /// Fetch the transactions of a block.
    pub async fn by_block(pool: &PgPool, block_number: i64) -> QueryResult<Vec<Transaction>> {
        let rows = sqlx::query(
            "SELECT hash, block_number, block_hash, transaction_index, from_address,
                    to_address, value, gas_price, gas, input, nonce, tx_type, status, created_at
             FROM transactions WHERE block_number = $1 ORDER BY transaction_index",
        )
        .bind(block_number)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(map_transaction).collect())
    }

    /// Fetch a page of an address's transactions.
    pub async fn by_address(
        pool: &PgPool,
        address: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> QueryResult<Vec<Transaction>> {
        let (limit, offset) = page(limit, offset);

        let rows = sqlx::query(
            "SELECT hash, block_number, block_hash, transaction_index, from_address,
                    to_address, value, gas_price, gas, input, nonce, tx_type, status, created_at
             FROM transactions
             WHERE from_address = $1 OR to_address = $1
             ORDER BY block_number DESC, transaction_index DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(address)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(map_transaction).collect())
    }
}

/// Token queries.
pub struct TokenQueries;

impl TokenQueries {
    /// Fetch a token by address.
    pub async fn by_address(pool: &PgPool, address: &str) -> QueryResult<Option<Token>> {
        let row = sqlx::query(
            "SELECT address, name, symbol, decimals, total_supply, type, price, price_24h_ago,
                    market_cap, volume_24h, holders_count, transfers_count, is_verified, is_spam,
                    price_source, last_updated, created_at, updated_at
             FROM tokens WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(map_token))
    }

    /// Fetch a page of transfers for a token.
    pub async fn transfers(
        pool: &PgPool,
        token_address: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> QueryResult<Vec<TokenTransfer>> {
        let (limit, offset) = page(limit, offset);

        let rows = sqlx::query(
            "SELECT id, token_address, from_address, to_address, value, transaction_hash,
                    block_number, log_index, created_at
             FROM token_transfers
             WHERE token_address = $1
             ORDER BY block_number DESC, log_index DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(token_address)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(map_token_transfer).collect())
    }

    /// Fetch the holders of a token, largest balance first.
    pub async fn holders(
        pool: &PgPool,
        token_address: &str,
        limit: Option<i64>,
    ) -> QueryResult<Vec<TokenHolder>> {
        let (limit, _) = page(limit, None);

        let rows = sqlx::query(
            "SELECT id, token_address, address, balance, block_number, updated_at
             FROM token_holders
             WHERE token_address = $1
             ORDER BY balance::numeric DESC
             LIMIT $2",
        )
        .bind(token_address)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(map_token_holder).collect())
    }
}

/// Log queries.
pub struct LogQueries;

impl LogQueries {
    /// Fetch the logs of a block, optionally filtered by address.
    pub async fn by_block(
        pool: &PgPool,
        block_number: i64,
        address: Option<&str>,
    ) -> QueryResult<Vec<Log>> {
        let rows = match address {
            Some(address) => {
                sqlx::query(
                    "SELECT id, address, topics, data, block_number, transaction_hash,
                            log_index, created_at
                     FROM logs WHERE block_number = $1 AND address = $2
                     ORDER BY log_index",
                )
                .bind(block_number)
                .bind(address)
                .fetch_all(pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, address, topics, data, block_number, transaction_hash,
                            log_index, created_at
                     FROM logs WHERE block_number = $1
                     ORDER BY log_index",
                )
                .bind(block_number)
                .fetch_all(pool)
                .await?
            }
        };

        Ok(rows.into_iter().map(map_log).collect())
    }
}

fn map_block(row: sqlx::postgres::PgRow) -> Block {
    Block {
        number: row.get("number"),
        hash: row.get("hash"),
        parent_hash: row.get("parent_hash"),
        nonce: row.get("nonce"),
        sha3_uncles: row.get("sha3_uncles"),
        logs_bloom: row.get("logs_bloom"),
        transactions_root: row.get("transactions_root"),
        state_root: row.get("state_root"),
        receipts_root: row.get("receipts_root"),
        miner: row.get("miner"),
        difficulty: row.get("difficulty"),
        total_difficulty: row.get("total_difficulty"),
        size: row.get("size"),
        gas_limit: row.get("gas_limit"),
        gas_used: row.get("gas_used"),
        timestamp: row.get("timestamp"),
        extra_data: row.get("extra_data"),
        mix_hash: row.get("mix_hash"),
        base_fee_per_gas: row.get("base_fee_per_gas"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_transaction(row: sqlx::postgres::PgRow) -> Transaction {
    Transaction {
        hash: row.get("hash"),
        block_number: row.get("block_number"),
        block_hash: row.get("block_hash"),
        transaction_index: row.get("transaction_index"),
        from_address: row.get("from_address"),
        to_address: row.get("to_address"),
        value: row.get("value"),
        gas_price: row.get("gas_price"),
        gas: row.get("gas"),
        input: row.get("input"),
        nonce: row.get("nonce"),
        tx_type: row.get("tx_type"),
        status: row.get("status"),
        created_at: row.get("created_at"),
    }
}

fn map_token(row: sqlx::postgres::PgRow) -> Token {
    Token {
        address: row.get("address"),
        name: row.get("name"),
        symbol: row.get("symbol"),
        decimals: row.get("decimals"),
        total_supply: row.get("total_supply"),
        token_type: row.get("type"),
        price: row.get("price"),
        price_24h_ago: row.get("price_24h_ago"),
        market_cap: row.get("market_cap"),
        volume_24h: row.get("volume_24h"),
        holders_count: row.get("holders_count"),
        transfers_count: row.get("transfers_count"),
        is_verified: row.get("is_verified"),
        is_spam: row.get("is_spam"),
        price_source: row.get("price_source"),
        last_updated: row.get("last_updated"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_token_transfer(row: sqlx::postgres::PgRow) -> TokenTransfer {
    TokenTransfer {
        id: row.get("id"),
        token_address: row.get("token_address"),
        from_address: row.get("from_address"),
        to_address: row.get("to_address"),
        value: row.get("value"),
        transaction_hash: row.get("transaction_hash"),
        block_number: row.get("block_number"),
        log_index: row.get("log_index"),
        created_at: row.get("created_at"),
    }
}

fn map_token_holder(row: sqlx::postgres::PgRow) -> TokenHolder {
    TokenHolder {
        id: row.get("id"),
        token_address: row.get("token_address"),
        address: row.get("address"),
        balance: row.get("balance"),
        block_number: row.get("block_number"),
        updated_at: row.get("updated_at"),
    }
}

fn map_log(row: sqlx::postgres::PgRow) -> Log {
    Log {
        id: row.get("id"),
        address: row.get("address"),
        topics: row.get("topics"),
        data: row.get("data"),
        block_number: row.get("block_number"),
        transaction_hash: row.get("transaction_hash"),
        log_index: row.get("log_index"),
        created_at: row.get("created_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_size_is_clamped_to_the_maximum() {
        assert_eq!(page(Some(10_000), None).0, MAX_PAGE_SIZE);
    }

    #[test]
    fn a_page_size_of_zero_becomes_one() {
        assert_eq!(page(Some(0), None).0, 1);
    }

    #[test]
    fn a_negative_offset_becomes_zero() {
        assert_eq!(page(None, Some(-5)).1, 0);
    }

    #[test]
    fn a_missing_page_size_uses_the_default() {
        assert_eq!(page(None, None).0, DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn the_page_size_and_offset_are_independent() {
        let (limit, offset) = page(Some(50), Some(100));

        assert_eq!(limit, 50);
        assert_eq!(offset, 100);
    }
}
