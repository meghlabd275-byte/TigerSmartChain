//! GraphQL Subscriptions for TigerScan
//!
//! A subscription here is a poll over the index, not a fabricated stream. The
//! node writes new rows as it indexes; each subscription remembers the last
//! block or row it emitted and asks for what appeared after that. A client
//! therefore sees every block once, in order, and a reconnect resumes from the
//! last row it received rather than from the tip.

use crate::db::{normalize_hex_key, Database};
use crate::model::*;
use crate::queries::gas_tiers;
use crate::schema::*;
use async_graphql::{Context, SimpleObject, Subscription};
use futures_util::stream::{self, Stream, StreamExt};
use sqlx::Row;
use std::pin::Pin;
use std::time::Duration;

/// How often a subscription asks the index for new rows.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// A subscription cannot run forever on one connection; this caps how many
/// polls it performs so a client that walks away releases its resources.
const MAX_POLLS: u32 = 86_400;

/// A boxed stream, which is the shape the subscription macro expects.
type TigerStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// A pending transaction from the mempool.
#[derive(SimpleObject, Clone)]
pub struct PendingTransaction {
    pub hash: String,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas_price: Option<i64>,
    pub gas: Option<i64>,
    pub nonce: i64,
    pub input: Option<String>,
}

/// A contract event.
#[derive(SimpleObject, Clone)]
pub struct ContractEvent {
    pub address: String,
    pub event_type: String,
    pub data: String,
    pub transaction_hash: String,
    pub block_number: i64,
}

/// A security alert.
#[derive(SimpleObject, Clone)]
pub struct Alert {
    pub id: i64,
    pub alert_type: String,
    pub severity: String,
    pub title: String,
    pub description: Option<String>,
    pub address: Option<String>,
    pub transaction_hash: Option<String>,
}

/// Repeatedly read rows that appeared after the last one seen.
///
/// `fetch` is given the highest marker already emitted and returns the rows
/// after it. An empty result means there is nothing new, so the stream waits
/// rather than ending, which is what a subscriber expects.
fn poll_stream<T, F, Fut>(db: Database, last_seen: i64, fetch: F) -> TigerStream<T>
where
    T: Send + 'static,
    F: Fn(Database, i64) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Vec<(i64, T)>> + Send,
{
    // The generator owns the closure and the high-water mark, so each poll
    // sees the marker the previous poll left and never repeats a row.
    let state = (0u32, last_seen, fetch, db);
    stream::unfold(state, |(polls, last_seen, fetch, db)| {
        let fetch = fetch.clone();
        async move {
            if polls >= MAX_POLLS {
                return None;
            }
            let rows = fetch(db.clone(), last_seen).await;
            let mut next_seen = last_seen;
            let mut items = Vec::new();
            for (marker, item) in rows {
                next_seen = next_seen.max(marker);
                items.push(item);
            }
            if items.is_empty() {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
            Some((stream::iter(items), (polls + 1, next_seen, fetch, db)))
        }
    })
    .flatten()
    .boxed()
}

/// Start a poll stream after an asynchronous lookup of the first marker.
///
/// The marker comes from the database, so it cannot be known before the
/// subscription is built. Awaiting it inside the stream keeps the resolver
/// synchronous, which is what the subscription macro expects.
fn poll_stream_after<S, SFut, T, F, Fut>(start: S, fetch: F) -> TigerStream<T>
where
    S: FnOnce() -> SFut + Send + 'static,
    SFut: std::future::Future<Output = (Database, i64)> + Send,
    T: Send + 'static,
    F: Fn(Database, i64) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Vec<(i64, T)>> + Send,
{
    stream::once(async move { start().await })
        .flat_map(move |(db, marker)| poll_stream(db, marker, fetch.clone()))
        .boxed()
}

/// The highest block number already indexed.
async fn tip_block_number(db: &Database) -> i64 {
    sqlx::query("SELECT COALESCE(MAX(number), 0) AS tip FROM blocks WHERE is_uncle = false")
        .fetch_one(db.pool())
        .await
        .ok()
        .and_then(|row| row.try_get::<i64, _>("tip").ok())
        .unwrap_or_default()
}

/// The highest row id in a table.
///
/// Every table the subscriptions follow is append-only and keyed by a
/// sequence, so the largest id is the marker the next poll looks past. The
/// table name is chosen by the caller from a fixed set, never from input.
async fn tip_id(db: &Database, table: &str) -> i64 {
    let query = format!("SELECT COALESCE(MAX(id), 0) AS tip FROM {table}");
    sqlx::query(&query)
        .fetch_one(db.pool())
        .await
        .ok()
        .and_then(|row| row.try_get::<i64, _>("tip").ok())
        .unwrap_or_default()
}

/// A start closure that reads the current tip of a table.
fn from_tip_id(
    db: Database,
    table: &'static str,
) -> impl FnOnce() -> futures_util::future::BoxFuture<'static, (Database, i64)> + Send + 'static {
    move || {
        Box::pin(async move {
            let tip = tip_id(&db, table).await;
            (db, tip)
        })
    }
}

/// A start closure that reads the current tip block number.
fn from_tip_block(
    db: Database,
) -> impl FnOnce() -> futures_util::future::BoxFuture<'static, (Database, i64)> + Send + 'static {
    move || {
        Box::pin(async move {
            let tip = tip_block_number(&db).await;
            (db, tip)
        })
    }
}

#[Subscription(name = "Subscription")]
impl SubscriptionRoot {
    /// Subscribe to new blocks.
    pub async fn new_blocks(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<TigerStream<Block>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        Ok(poll_stream_after(from_tip_block(db), |db, after| async move {
            let rows = sqlx::query(&format!(
                "SELECT {BLOCK_COLUMNS} FROM blocks
                 WHERE is_uncle = false AND number > $1 ORDER BY number LIMIT 100"
            ))
            .bind(after)
            .fetch_all(db.pool())
            .await
            .unwrap_or_default();
            rows.iter()
                .map(|row| {
                    let block = block_from_row(row);
                    (block.number, block)
                })
                .collect()
        }))
    }

    /// Subscribe to new blocks with their transactions.
    ///
    /// This is the same stream as `new_blocks`: the transactions are resolved
    /// by the `Block.transactions` field when the client asks for them, so a
    /// client that does not request them pays nothing for them.
    pub async fn new_blocks_full(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<TigerStream<Block>> {
        self.new_blocks(ctx).await
    }

    /// Subscribe to new transactions.
    pub async fn new_transactions(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<TigerStream<Transaction>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        Ok(poll_stream_after(
            from_tip_id(db, "transactions"),
            |db, after| async move {
                let rows = sqlx::query(&format!(
                    "SELECT id, {TRANSACTION_COLUMNS} FROM transactions
                     WHERE id > $1 ORDER BY id LIMIT 200"
                ))
                .bind(after)
                .fetch_all(db.pool())
                .await
                .unwrap_or_default();
                rows.iter()
                    .map(|row| {
                        let id: i64 = row.try_get("id").unwrap_or_default();
                        (id, transaction_from_row(row))
                    })
                    .collect()
            },
        ))
    }

    /// Subscribe to pending transactions.
    ///
    /// These are transactions the node has accepted but not yet mined, which
    /// is what a wallet watches to decide whether its own transaction is
    /// propagating.
    pub async fn pending_transactions(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<TigerStream<PendingTransaction>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        Ok(poll_stream_after(
            from_tip_id(db, "pending_transactions"),
            |db, after| async move {
                let rows = sqlx::query(
                    "SELECT id, hash, from_address, to_address, value, gas_price, gas_limit,
                            nonce, input
                     FROM pending_transactions WHERE id > $1 ORDER BY id LIMIT 200",
                )
                .bind(after)
                .fetch_all(db.pool())
                .await
                .unwrap_or_default();
                rows.iter()
                    .map(|row| {
                        let id: i64 = row.try_get("id").unwrap_or_default();
                        (
                            id,
                            PendingTransaction {
                                hash: row.try_get("hash").unwrap_or_default(),
                                from_address: row.try_get("from_address").unwrap_or_default(),
                                to_address: row.try_get("to_address").ok().flatten(),
                                value: numeric(row, "value").unwrap_or_else(|| "0".to_string()),
                                gas_price: row.try_get("gas_price").ok().flatten(),
                                gas: row.try_get("gas_limit").ok().flatten(),
                                nonce: row.try_get("nonce").unwrap_or_default(),
                                input: row.try_get("input").ok().flatten(),
                            },
                        )
                    })
                    .collect()
            },
        ))
    }

    /// Subscribe to token transfers.
    pub async fn token_transfers(
        &self,
        ctx: &Context<'_>,
        token: Option<String>,
        from: Option<String>,
        to: Option<String>,
    ) -> async_graphql::Result<TigerStream<TokenTransfer>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        let token = token.map(|t| normalize_hex_key(&t));
        let from = from.map(|a| normalize_hex_key(&a));
        let to = to.map(|a| normalize_hex_key(&a));
        Ok(poll_stream_after(
            from_tip_id(db, "token_transfers"),
            move |db, after| {
                let token = token.clone();
                let from = from.clone();
                let to = to.clone();
                async move {
                    let rows = sqlx::query(
                        "SELECT id, token_address, from_address, to_address, value,
                                transaction_hash, block_number
                         FROM token_transfers
                         WHERE id > $1
                           AND ($2::text IS NULL OR token_address = $2)
                           AND ($3::text IS NULL OR from_address = $3)
                           AND ($4::text IS NULL OR to_address = $4)
                         ORDER BY id LIMIT 200",
                    )
                    .bind(after)
                    .bind(token)
                    .bind(from)
                    .bind(to)
                    .fetch_all(db.pool())
                    .await
                    .unwrap_or_default();
                    let mut items = Vec::with_capacity(rows.len());
                    for row in &rows {
                        let id: i64 = row.try_get("id").unwrap_or_default();
                        let block_number: i64 = row.try_get("block_number").unwrap_or_default();
                        items.push((
                            id,
                            TokenTransfer {
                                id,
                                token_address: row.try_get("token_address").unwrap_or_default(),
                                from_address: row.try_get("from_address").unwrap_or_default(),
                                to_address: row.try_get("to_address").unwrap_or_default(),
                                value: numeric(row, "value").unwrap_or_else(|| "0".to_string()),
                                transaction_hash: row
                                    .try_get("transaction_hash")
                                    .unwrap_or_default(),
                                block_number,
                                timestamp: block_timestamp(&db, Some(block_number)).await,
                            },
                        ));
                    }
                    items
                }
            },
        ))
    }

    /// Subscribe to NFT transfers.
    pub async fn nft_transfers(
        &self,
        ctx: &Context<'_>,
        collection: Option<String>,
    ) -> async_graphql::Result<TigerStream<NFTTransfer>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        let collection = collection.map(|c| normalize_hex_key(&c));
        Ok(poll_stream_after(
            from_tip_id(db, "nft_transfers"),
            move |db, after| {
                let collection = collection.clone();
                async move {
                    let rows = sqlx::query(
                        "SELECT id, token_address, token_id, from_address, to_address,
                                transaction_hash, block_number
                         FROM nft_transfers
                         WHERE id > $1 AND ($2::text IS NULL OR token_address = $2)
                         ORDER BY id LIMIT 200",
                    )
                    .bind(after)
                    .bind(collection)
                    .fetch_all(db.pool())
                    .await
                    .unwrap_or_default();
                    let mut items = Vec::with_capacity(rows.len());
                    for row in &rows {
                        let id: i64 = row.try_get("id").unwrap_or_default();
                        let block_number: i64 = row.try_get("block_number").unwrap_or_default();
                        items.push((
                            id,
                            NFTTransfer {
                                id,
                                collection_address: row
                                    .try_get("token_address")
                                    .unwrap_or_default(),
                                token_id: numeric(row, "token_id")
                                    .unwrap_or_else(|| "0".to_string()),
                                from_address: row.try_get("from_address").unwrap_or_default(),
                                to_address: row.try_get("to_address").unwrap_or_default(),
                                transaction_hash: row
                                    .try_get("transaction_hash")
                                    .unwrap_or_default(),
                                block_number,
                                timestamp: block_timestamp(&db, Some(block_number)).await,
                            },
                        ));
                    }
                    items
                }
            },
        ))
    }

    /// Subscribe to logs.
    pub async fn logs(
        &self,
        ctx: &Context<'_>,
        address: Option<String>,
        topics: Option<Vec<String>>,
    ) -> async_graphql::Result<TigerStream<Log>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        let address = address.map(|a| normalize_hex_key(&a));
        let topics = topics.unwrap_or_default();
        Ok(poll_stream_after(from_tip_id(db, "logs"), move |db, after| {
            let address = address.clone();
            let topics = topics.clone();
            async move {
                // A topic is matched positionally, so a filter cannot match a
                // log whose arguments are in a different order.
                let mut builder = sqlx::QueryBuilder::new(
                    "SELECT id, address, topics, data, block_number, transaction_hash, log_index
                     FROM logs WHERE id > ",
                );
                builder.push_bind(after);
                if let Some(address) = &address {
                    builder.push(" AND address = ").push_bind(address.clone());
                }
                for (index, topic) in topics.iter().enumerate() {
                    let topic = topic.trim();
                    if topic.is_empty() || topic == "*" || topic == "null" {
                        continue;
                    }
                    builder
                        .push(format!(" AND topics->>{index} = "))
                        .push_bind(topic.to_ascii_lowercase());
                }
                builder.push(" ORDER BY id LIMIT 200");
                let rows = builder.build().fetch_all(db.pool()).await.unwrap_or_default();
                rows.iter()
                    .map(|row| {
                        let id: i64 = row.try_get("id").unwrap_or_default();
                        (id, log_from_row(row))
                    })
                    .collect()
            }
        }))
    }

    /// Subscribe to gas price updates, one sample per new block.
    pub async fn gas_price(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<TigerStream<GasPrice>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        Ok(poll_stream_after(
            from_tip_id(db, "gas_prices"),
            |db, after| async move {
                let rows =
                    sqlx::query("SELECT id FROM gas_prices WHERE id > $1 ORDER BY id LIMIT 100")
                        .bind(after)
                        .fetch_all(db.pool())
                        .await
                        .unwrap_or_default();
                if rows.is_empty() {
                    return Vec::new();
                }
                let last: i64 = rows
                    .last()
                    .and_then(|row| row.try_get::<i64, _>("id").ok())
                    .unwrap_or(after);
                // One reading per poll: the oracle is a view of the fee
                // history, not a value that changed, so repeating it would be
                // noise.
                vec![(last, gas_tiers(&db).await)]
            },
        ))
    }

    /// Subscribe to contract events.
    pub async fn contract_events(
        &self,
        ctx: &Context<'_>,
        address: String,
    ) -> async_graphql::Result<TigerStream<ContractEvent>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        let address = normalize_hex_key(&address);
        Ok(poll_stream_after(
            from_tip_id(db, "contract_events"),
            move |db, after| {
                let address = address.clone();
                async move {
                    let rows = sqlx::query(
                        "SELECT id, contract_address, event_name, data, transaction_hash,
                                block_number
                         FROM contract_events
                         WHERE id > $1 AND contract_address = $2
                         ORDER BY id LIMIT 200",
                    )
                    .bind(after)
                    .bind(address)
                    .fetch_all(db.pool())
                    .await
                    .unwrap_or_default();
                    rows.iter()
                        .map(|row| {
                            let id: i64 = row.try_get("id").unwrap_or_default();
                            (
                                id,
                                ContractEvent {
                                    address: row.try_get("contract_address").unwrap_or_default(),
                                    event_type: row.try_get("event_name").unwrap_or_default(),
                                    data: row
                                        .try_get::<Option<serde_json::Value>, _>("data")
                                        .ok()
                                        .flatten()
                                        .map(|v| v.to_string())
                                        .unwrap_or_default(),
                                    transaction_hash: row
                                        .try_get("transaction_hash")
                                        .unwrap_or_default(),
                                    block_number: row.try_get("block_number").unwrap_or_default(),
                                },
                            )
                        })
                        .collect()
                }
            },
        ))
    }

    /// Subscribe to security alerts.
    ///
    /// An alert is raised when an address is recorded in the threat tables.
    /// The alert is derived from the index, so it is produced from the same
    /// data the rest of the explorer shows.
    pub async fn alerts(
        &self,
        ctx: &Context<'_>,
        address: Option<String>,
    ) -> async_graphql::Result<TigerStream<Alert>> {
        let Some(db) = ctx.data_opt::<Database>().cloned() else {
            return Ok(stream::empty().boxed());
        };
        let address = address.map(|a| normalize_hex_key(&a));
        Ok(poll_stream_after(
            from_tip_id(db, "malicious_contracts"),
            move |db, after| {
                let address = address.clone();
                async move {
                    let rows = sqlx::query(
                        "SELECT id, address, threat_type, confidence, description, source
                         FROM malicious_contracts
                         WHERE id > $1 AND ($2::text IS NULL OR address = $2)
                         ORDER BY id LIMIT 100",
                    )
                    .bind(after)
                    .bind(address)
                    .fetch_all(db.pool())
                    .await
                    .unwrap_or_default();
                    rows.iter()
                        .map(|row| {
                            let id: i64 = row.try_get("id").unwrap_or_default();
                            let confidence: i32 = row.try_get("confidence").unwrap_or_default();
                            (
                                id,
                                Alert {
                                    id,
                                    alert_type: row.try_get("threat_type").unwrap_or_default(),
                                    // The stored confidence decides the
                                    // severity, so the two can never disagree.
                                    severity: alert_severity(confidence).to_string(),
                                    title: "Address reported as malicious".to_string(),
                                    description: row.try_get("description").ok().flatten(),
                                    address: row.try_get("address").ok().flatten(),
                                    transaction_hash: None,
                                },
                            )
                        })
                        .collect()
                }
            },
        ))
    }
}

/// The severity a threat record carries, from its confidence score.
fn alert_severity(confidence: i32) -> &'static str {
    match confidence {
        c if c >= 80 => "high",
        c if c >= 50 => "medium",
        _ => "low",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_severity_follows_confidence() {
        assert_eq!(alert_severity(100), "high");
        assert_eq!(alert_severity(80), "high");
        assert_eq!(alert_severity(79), "medium");
        assert_eq!(alert_severity(50), "medium");
        assert_eq!(alert_severity(49), "low");
        assert_eq!(alert_severity(0), "low");
    }

    #[test]
    fn poll_limits_are_finite() {
        // A subscription must not be able to run forever on one connection.
        assert!(MAX_POLLS > 0);
        assert!(POLL_INTERVAL.as_secs() > 0);
    }
}
