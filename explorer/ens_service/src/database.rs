//! ENS persistence.
//!
//! Two tables back the service:
//!
//! * `ens_records` stores resolved records so a repeat lookup avoids a round
//!   trip. The primary key is the name.
//! * `ens_domains` stores registration metadata (owner, expiry) that is not
//!   derivable from the registry alone.
//!
//! Every statement is parameterised, so no value is ever interpolated into SQL.

use ethers::core::types::Address;
use sqlx::{PgPool, Row};

use crate::errors::{Error, Result};
use crate::types::{ENSDomain, ENSRecord};

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create the schema if it is absent.
    pub async fn init(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ens_records (
                name            VARCHAR(255) PRIMARY KEY,
                address         VARCHAR(42),
                resolver        VARCHAR(42),
                owner           VARCHAR(42),
                ttl             BIGINT,
                content_hash    TEXT,
                avatar          TEXT,
                description     TEXT,
                email           TEXT,
                url             TEXT,
                version         INTEGER NOT NULL DEFAULT 0,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| Error::database(format!("cannot create ens_records: {error}")))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ens_domains (
                name                VARCHAR(255) PRIMARY KEY,
                owner               VARCHAR(42),
                resolver            VARCHAR(42),
                ttl                 BIGINT,
                registration_date   TIMESTAMPTZ,
                expiry_date         TIMESTAMPTZ,
                updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| Error::database(format!("cannot create ens_domains: {error}")))?;

        // Reverse lookups read by the claimed address, and the forward-confirm
        // step reads by name, so both columns need an index.
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_ens_records_address ON ens_records(address)")
            .execute(&self.pool)
            .await
            .map_err(|error| Error::database(format!("cannot index ens_records: {error}")))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_ens_domains_owner ON ens_domains(owner)")
            .execute(&self.pool)
            .await
            .map_err(|error| Error::database(format!("cannot index ens_domains: {error}")))?;

        Ok(())
    }

    /// Insert or update a record.
    pub async fn save_record(&self, record: &ENSRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ens_records (
                name, address, resolver, owner, ttl, content_hash,
                avatar, description, email, url, version, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW())
            ON CONFLICT (name) DO UPDATE SET
                address      = EXCLUDED.address,
                resolver     = EXCLUDED.resolver,
                owner        = EXCLUDED.owner,
                ttl          = EXCLUDED.ttl,
                content_hash = EXCLUDED.content_hash,
                avatar       = EXCLUDED.avatar,
                description  = EXCLUDED.description,
                email        = EXCLUDED.email,
                url          = EXCLUDED.url,
                version      = EXCLUDED.version,
                updated_at   = NOW()
            "#,
        )
        .bind(&record.name)
        .bind(record.address.as_deref())
        .bind(record.resolver.as_deref())
        .bind(record.owner.as_deref())
        .bind(record.ttl.map(|ttl| ttl as i64))
        .bind(record.content_hash.as_deref())
        .bind(record.avatar.as_deref())
        .bind(record.description.as_deref())
        .bind(record.email.as_deref())
        .bind(record.url.as_deref())
        .bind(record.version.map(|version| version as i32).unwrap_or(0))
        .execute(&self.pool)
        .await
        .map_err(|error| Error::database(format!("cannot save record: {error}")))?;

        Ok(())
    }

    /// Read a record by name.
    pub async fn get_record(&self, name: &str) -> Result<Option<ENSRecord>> {
        let row = sqlx::query(
            r#"
            SELECT name, address, resolver, owner, ttl, content_hash,
                   avatar, description, email, url, version, created_at, updated_at
            FROM ens_records
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| Error::database(format!("cannot read record: {error}")))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let ttl: Option<i64> = row.get("ttl");
        let version: Option<i32> = row.get("version");

        Ok(Some(ENSRecord {
            name: row.get("name"),
            address: row.get("address"),
            resolver: row.get("resolver"),
            owner: row.get("owner"),
            ttl: ttl.and_then(|ttl| u64::try_from(ttl).ok()),
            content_hash: row.get("content_hash"),
            text_records: None,
            coin_addresses: None,
            interface: None,
            abi: None,
            avatar: row.get("avatar"),
            email: row.get("email"),
            description: row.get("description"),
            notice: None,
            keywords: None,
            url: row.get("url"),
            version: version.and_then(|version| u32::try_from(version).ok()),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }

    /// Read domain metadata by name.
    pub async fn get_domain(&self, name: &str) -> Result<Option<ENSDomain>> {
        let row = sqlx::query(
            r#"
            SELECT name, owner, resolver, ttl, registration_date, expiry_date
            FROM ens_domains
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| Error::database(format!("cannot read domain: {error}")))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let ttl: Option<i64> = row.get("ttl");
        let owner: Option<String> = row.get("owner");

        Ok(Some(ENSDomain {
            name: row.get("name"),
            label_hash: None,
            name_hash: None,
            owner: owner.clone(),
            resolver: row.get("resolver"),
            ttl: ttl.and_then(|ttl| u64::try_from(ttl).ok()),
            is_eth_2ld: name.split('.').count() == 2,
            registration_date: row.get("registration_date"),
            expiry_date: row.get("expiry_date"),
            is_available: owner.is_none(),
        }))
    }

    /// Record that `name` claims `address`, under the reverse node.
    pub async fn set_reverse(&self, address: Address, name: &str) -> Result<()> {
        let reverse = crate::crypto::reverse_name(&format!("{address:?}"));

        let mut record = ENSRecord::new(reverse);
        record.address = Some(name.to_string());

        self.save_record(&record).await
    }

    /// Every name whose reverse record points at `address`.
    pub async fn names_claiming(&self, address: Address) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"
            SELECT address
            FROM ens_records
            WHERE name LIKE $1
            "#,
        )
        .bind(format!("%.{}", reverse_suffix(address)))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| Error::database(format!("cannot read reverse records: {error}")))?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.get::<Option<String>, _>("address"))
            .collect())
    }
}

/// The literal suffix shared by every reverse node of one address.
fn reverse_suffix(address: Address) -> String {
    format!(
        "{}.addr.reverse",
        format!("{address:?}").trim_start_matches("0x")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reverse_suffix_drops_the_prefix_and_lowercases() {
        let address: Address = "0xAbC0000000000000000000000000000000000001".parse().unwrap();
        assert_eq!(
            reverse_suffix(address),
            "abc0000000000000000000000000000000000001.addr.reverse"
        );
    }
}
