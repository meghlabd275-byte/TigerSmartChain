//! Database access for the TigerScan GraphQL API.
//!
//! Every resolver reads through this module, so the queries in `queries.rs`
//! are the only place a table name appears. The pool is handed to the schema
//! as request data, which is how a resolver reaches it.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// The PostgreSQL pool plus the read-only JSON-RPC endpoint.
///
/// Resolvers that answer from indexed data use `pool`. The few that cannot —
/// a balance, a gas estimate, an account that has never been indexed — use
/// `rpc_url` to ask the node directly rather than inventing an answer.
#[derive(Clone)]
pub struct Database {
    pool: PgPool,
    rpc_url: Option<String>,
}

impl Database {
    /// Connect to PostgreSQL, failing if the pool cannot be established.
    pub async fn connect(database_url: &str, rpc_url: Option<String>) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(32)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(600))
            .test_before_acquire(true)
            .connect(database_url)
            .await?;
        Ok(Self { pool, rpc_url })
    }

    /// Build from an existing pool.
    pub fn from_pool(pool: PgPool, rpc_url: Option<String>) -> Self {
        Self { pool, rpc_url }
    }

    /// The connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The JSON-RPC endpoint, when one is configured.
    pub fn rpc_url(&self) -> Option<&str> {
        self.rpc_url.as_deref()
    }
}

/// Percent-encode a value for use in a PostgreSQL full-text `to_tsquery`.
///
/// The search term reaches the database as a bind parameter, so it can never
/// be executed as SQL. It does reach `to_tsquery` as an operand, though, and
/// that function has its own syntax: a stray `&`, `|`, `!` or `:` in a user's
/// input would be read as an operator and raise a syntax error instead of
/// searching for the text the user typed. Dropping those characters leaves
/// the words, which is what the user meant.
pub fn sanitize_search_term(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    // A `*` suffix makes the last word a prefix, which is what a user typing
    // a partial address expects. `to_tsquery` rejects an empty operand, so an
    // input with no word characters yields no query at all.
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let mut term = words.join(" & ");
    term.push_str(":*");
    term
}

/// A normalised 32-byte hex hash, or the input unchanged when it is not one.
///
/// Addresses and hashes arrive from clients in any case. PostgreSQL compares
/// the stored lowercase form, so a lookup has to lowercase the key, but the
/// key is only an address or a hash when it has the right shape; a name must
/// be left alone or `WHERE address = 'Vitalik'` would match nothing.
pub fn normalize_hex_key(input: &str) -> String {
    let trimmed = input.trim();
    let body = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X"));
    match body {
        Some(b) if (b.len() == 40 || b.len() == 64) && b.chars().all(|c| c.is_ascii_hexdigit()) => {
            format!("0x{}", b.to_ascii_lowercase())
        }
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_drop_query_operators() {
        // The operators `to_tsquery` recognises are replaced by spaces, so the
        // term cannot change the shape of the query.
        assert_eq!(sanitize_search_term("usdt&btc"), "usdt & btc:*");
        assert_eq!(sanitize_search_term("a|b"), "a & b:*");
        assert_eq!(sanitize_search_term("!drop"), "drop:*");
        assert_eq!(sanitize_search_term("a:b"), "a & b:*");
        // An input with nothing searchable produces no term.
        assert_eq!(sanitize_search_term("!!!&&&"), "");
        assert_eq!(sanitize_search_term("   "), "");
    }

    #[test]
    fn search_terms_keep_address_characters() {
        // An address must survive intact so it can be searched as text.
        let term = sanitize_search_term("0xdac17f958d2ee523a2206206994597c13d831ec7");
        assert!(term.starts_with("0xdac17f958d2ee523a2206206994597c13d831ec7"));
        assert!(term.ends_with(":*"));
    }

    #[test]
    fn hex_keys_are_lowercased_but_only_when_they_are_keys() {
        assert_eq!(
            normalize_hex_key("0xDAC17F958D2EE523A2206206994597C13D831EC7"),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
        // A name is not an address, so it is returned untouched.
        assert_eq!(normalize_hex_key("Vitalik"), "Vitalik");
        // A wrong-length hex string is not treated as a key either.
        assert_eq!(normalize_hex_key("0xdeadbeef"), "0xdeadbeef");
    }
}
