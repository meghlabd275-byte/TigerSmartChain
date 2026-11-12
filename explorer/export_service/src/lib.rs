//! Bulk Export Service — CSV, Excel, JSON and Parquet.
//!
//! Every export reads the rows it writes out of PostgreSQL. The table, the
//! filter columns and the selected columns are all allow-listed per export
//! type, so a caller cannot reach a table or a column the service does not
//! publish, and the filter values are always bound as parameters.

use std::collections::HashMap;

use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub export_type: ExportType,
    pub format: ExportFormat,
    pub filters: ExportFilters,
    pub columns: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportType {
    Transactions,
    Blocks,
    Tokens,
    Nfts,
    TokenTransfers,
    NftTransfers,
    Contracts,
    Logs,
}

impl ExportType {
    /// The table a request reads from.
    pub fn table(&self) -> &'static str {
        match self {
            Self::Transactions => "transactions",
            Self::Blocks => "blocks",
            Self::Tokens => "tokens",
            Self::Nfts => "nfts",
            Self::TokenTransfers => "token_transfers",
            Self::NftTransfers => "nft_transfers",
            Self::Contracts => "contracts",
            Self::Logs => "logs",
        }
    }

    /// The columns a request may select. Anything outside this list is refused
    /// so that a caller cannot read a column the service does not publish.
    pub fn columns(&self) -> &'static [&'static str] {
        match self {
            Self::Transactions => &[
                "hash",
                "block_number",
                "block_timestamp",
                "transaction_index",
                "from_address",
                "to_address",
                "value",
                "gas_price",
                "gas_limit",
                "gas_used",
                "nonce",
                "transaction_type",
                "status",
                "contract_address",
            ],
            Self::Blocks => &[
                "number",
                "hash",
                "parent_hash",
                "timestamp",
                "miner",
                "difficulty",
                "gas_limit",
                "gas_used",
                "base_fee_per_gas",
                "size",
                "tx_count",
                "reward",
            ],
            Self::Tokens => &[
                "address",
                "name",
                "symbol",
                "decimals",
                "total_supply",
                "holders_count",
                "transfers_count",
                "price_usd",
                "market_cap",
                "volume_24h",
                "is_verified",
            ],
            Self::Nfts => &[
                "token_address",
                "token_id",
                "owner",
                "name",
                "description",
                "image_url",
                "uri",
                "is_burned",
                "last_transfer_block",
            ],
            Self::TokenTransfers => &[
                "token_address",
                "from_address",
                "to_address",
                "value",
                "transaction_hash",
                "block_number",
                "log_index",
            ],
            Self::NftTransfers => &[
                "token_address",
                "from_address",
                "to_address",
                "token_id",
                "value",
                "transaction_hash",
                "block_number",
                "log_index",
            ],
            Self::Contracts => &[
                "address",
                "contract_name",
                "compiler_version",
                "evm_version",
                "license_type",
                "contract_type",
                "is_verified",
                "verification_status",
                "verified_at",
            ],
            Self::Logs => &[
                "transaction_hash",
                "block_number",
                "address",
                "topics",
                "data",
                "log_index",
                "timestamp",
            ],
        }
    }

    /// The default column set, used when a request selects none.
    pub fn default_columns(&self) -> Vec<String> {
        self.columns()
            .iter()
            .take(8)
            .map(|column| (*column).to_string())
            .collect()
    }

    /// The column that carries a block range, if the table has one.
    fn block_column(&self) -> &'static str {
        match self {
            Self::Blocks => "number",
            _ => "block_number",
        }
    }

    /// The column that carries the sender, if the table has one.
    fn from_column(&self) -> &'static str {
        match self {
            Self::Tokens | Self::Nfts | Self::Contracts | Self::Blocks => "address",
            _ => "from_address",
        }
    }

    /// The column that carries the recipient, if the table has one.
    fn to_column(&self) -> &'static str {
        match self {
            Self::TokenTransfers | Self::NftTransfers | Self::Transactions => "to_address",
            _ => "address",
        }
    }

    /// The column that carries a token address, if the table has one.
    fn token_column(&self) -> &'static str {
        match self {
            Self::Nfts | Self::NftTransfers | Self::TokenTransfers => "token_address",
            _ => "address",
        }
    }

    /// The column that carries a timestamp, if the table has one.
    fn time_column(&self) -> &'static str {
        match self {
            Self::Blocks => "timestamp",
            Self::Transactions => "block_timestamp",
            Self::Logs => "timestamp",
            _ => "created_at",
        }
    }

    /// The column that carries a status, if the table has one.
    fn status_column(&self) -> &'static str {
        match self {
            Self::Transactions => "status",
            Self::Contracts => "verification_status",
            _ => "is_verified",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Excel,
    Json,
    Parquet,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportFilters {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub from_address: Option<String>,
    pub to_address: Option<String>,
    pub token_address: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportJob {
    pub id: String,
    pub request: ExportRequest,
    pub status: JobStatus,
    pub file_url: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub row_count: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// The largest export a single request may produce.
pub const MAX_EXPORT_ROWS: usize = 1_000_000;

/// The largest limit a caller may ask for.
pub const MAX_LIMIT: usize = 1_000_000;

/// Export a row set in one of the supported formats.
pub struct Exporter;

impl Exporter {
    /// Render a header and rows as CSV.
    pub fn to_csv(columns: &[String], rows: &[Vec<String>]) -> Result<Vec<u8>, String> {
        let mut writer = csv::Writer::from_writer(Vec::new());

        writer
            .write_record(columns)
            .map_err(|error| format!("cannot write CSV header: {error}"))?;

        for row in rows {
            writer
                .write_record(row)
                .map_err(|error| format!("cannot write CSV row: {error}"))?;
        }

        writer
            .into_inner()
            .map_err(|error| format!("cannot finish CSV: {error}"))
    }

    /// Render a header and rows as JSON objects.
    pub fn to_json(columns: &[String], rows: &[Vec<String>]) -> Result<Vec<u8>, String> {
        let records: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let object: serde_json::Map<String, serde_json::Value> = columns
                    .iter()
                    .zip(row.iter())
                    .map(|(column, value)| {
                        (column.clone(), serde_json::Value::String(value.clone()))
                    })
                    .collect();

                serde_json::Value::Object(object)
            })
            .collect();

        serde_json::to_vec(&records).map_err(|error| format!("cannot serialise JSON: {error}"))
    }

    /// Render a header and rows as an XLSX workbook.
    ///
    /// Values are written as text: a decimal string such as a 78-digit token
    /// supply does not fit in a spreadsheet number, and a hash is not a number
    /// at all, so converting them would lose data.
    pub fn to_excel(
        columns: &[String],
        rows: &[Vec<String>],
        sheet_name: &str,
    ) -> Result<Vec<u8>, String> {
        use rust_xlsxwriter::{Format, Workbook};

        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();

        sheet
            .set_name(sheet_name)
            .map_err(|error| format!("cannot name sheet: {error}"))?;

        let header = Format::new().set_bold();
        let text = Format::new().set_num_format("@");

        for (column_index, column) in columns.iter().enumerate() {
            sheet
                .write_string_with_format(0, column_index as u16, column, &header)
                .map_err(|error| format!("cannot write header: {error}"))?;
        }

        for (row_index, row) in rows.iter().enumerate() {
            for (column_index, value) in row.iter().enumerate() {
                sheet
                    .write_string_with_format(
                        (row_index + 1) as u32,
                        column_index as u16,
                        value,
                        &text,
                    )
                    .map_err(|error| format!("cannot write cell: {error}"))?;
            }
        }

        workbook
            .save_to_buffer()
            .map_err(|error| format!("cannot save workbook: {error}"))
    }

    /// Render a header and rows as a Parquet file.
    ///
    /// Every column is written as UTF-8. The source values are decimal strings
    /// and hex strings, so a narrower physical type would silently truncate a
    /// 256-bit value.
    pub fn to_parquet(columns: &[String], rows: &[Vec<String>]) -> Result<Vec<u8>, String> {
        use arrow::array::StringArray;
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;

        let fields: Vec<Field> = columns
            .iter()
            .map(|column| Field::new(column, DataType::Utf8, true))
            .collect();

        let schema = Arc::new(Schema::new(fields));

        let arrays: Vec<Arc<dyn arrow::array::Array>> = (0..columns.len())
            .map(|column_index| {
                let values: Vec<Option<&str>> = rows
                    .iter()
                    .map(|row| row.get(column_index).map(String::as_str))
                    .collect();

                Arc::new(StringArray::from(values)) as Arc<dyn arrow::array::Array>
            })
            .collect();

        let batch = RecordBatch::try_new(Arc::clone(&schema), arrays)
            .map_err(|error| format!("cannot build a record batch: {error}"))?;

        let mut buffer = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, schema, None)
            .map_err(|error| format!("cannot create the parquet writer: {error}"))?;

        writer
            .write(&batch)
            .map_err(|error| format!("cannot write the parquet batch: {error}"))?;

        writer
            .close()
            .map_err(|error| format!("cannot finish the parquet file: {error}"))?;

        Ok(buffer)
    }

    /// The file extension for a format.
    pub fn extension(format: ExportFormat) -> &'static str {
        match format {
            ExportFormat::Csv => "csv",
            ExportFormat::Excel => "xlsx",
            ExportFormat::Json => "json",
            ExportFormat::Parquet => "parquet",
        }
    }

    /// The media type for a format.
    pub fn content_type(format: ExportFormat) -> &'static str {
        match format {
            ExportFormat::Csv => "text/csv",
            ExportFormat::Excel => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
            ExportFormat::Json => "application/json",
            ExportFormat::Parquet => "application/vnd.apache.parquet",
        }
    }

    /// Render rows in the requested format.
    pub fn render(
        format: ExportFormat,
        columns: &[String],
        rows: &[Vec<String>],
        sheet_name: &str,
    ) -> Result<Vec<u8>, String> {
        match format {
            ExportFormat::Csv => Self::to_csv(columns, rows),
            ExportFormat::Json => Self::to_json(columns, rows),
            ExportFormat::Excel => Self::to_excel(columns, rows, sheet_name),
            ExportFormat::Parquet => Self::to_parquet(columns, rows),
        }
    }
}

/// Read export rows out of PostgreSQL.
pub struct ExportService {
    pool: PgPool,
}

impl ExportService {
    pub async fn new(database_url: &str) -> Result<Self, String> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await
            .map_err(|error| format!("cannot connect to PostgreSQL: {error}"))?;

        Ok(Self { pool })
    }

    /// Validate a request and return the columns it will emit.
    pub fn resolve_columns(request: &ExportRequest) -> Result<Vec<String>, String> {
        let allowed = request.export_type.columns();

        if request.columns.is_empty() {
            return Ok(request.export_type.default_columns());
        }

        if request.columns.len() > 64 {
            return Err("too many columns requested".to_string());
        }

        let mut resolved = Vec::with_capacity(request.columns.len());

        for column in &request.columns {
            if !allowed.contains(&column.as_str()) {
                return Err(format!(
                    "column {column} is not exportable for {:?}",
                    request.export_type
                ));
            }

            resolved.push(column.clone());
        }

        Ok(resolved)
    }

    /// Run an export and return the encoded file.
    pub async fn export(
        &self,
        request: &ExportRequest,
    ) -> Result<(Vec<u8>, u64, Vec<String>), String> {
        let columns = Self::resolve_columns(request)?;
        let limit = request
            .limit
            .unwrap_or(MAX_EXPORT_ROWS)
            .min(MAX_LIMIT)
            .min(MAX_EXPORT_ROWS);

        let mut conditions: Vec<String> = Vec::new();
        let mut bindings: Vec<String> = Vec::new();

        // Each filter binds its own parameter; the index is the position in the
        // binding list, so a filter cannot shift another filter's value.
        if let Some(from_block) = request.filters.from_block {
            bindings.push(from_block.to_string());
            conditions.push(format!(
                "{} >= ${}::bigint",
                request.export_type.block_column(),
                bindings.len()
            ));
        }

        if let Some(to_block) = request.filters.to_block {
            bindings.push(to_block.to_string());
            conditions.push(format!(
                "{} <= ${}::bigint",
                request.export_type.block_column(),
                bindings.len()
            ));
        }

        if let Some(address) = &request.filters.from_address {
            validate_address(address)?;
            bindings.push(address.to_lowercase());
            conditions.push(format!(
                "lower({}) = ${}",
                request.export_type.from_column(),
                bindings.len()
            ));
        }

        if let Some(address) = &request.filters.to_address {
            validate_address(address)?;
            bindings.push(address.to_lowercase());
            conditions.push(format!(
                "lower({}) = ${}",
                request.export_type.to_column(),
                bindings.len()
            ));
        }

        if let Some(address) = &request.filters.token_address {
            validate_address(address)?;
            bindings.push(address.to_lowercase());
            conditions.push(format!(
                "lower({}) = ${}",
                request.export_type.token_column(),
                bindings.len()
            ));
        }

        if let Some(from_date) = &request.filters.from_date {
            validate_date(from_date)?;
            bindings.push(from_date.clone());
            conditions.push(format!(
                "{} >= ${}::timestamptz",
                request.export_type.time_column(),
                bindings.len()
            ));
        }

        if let Some(to_date) = &request.filters.to_date {
            validate_date(to_date)?;
            bindings.push(to_date.clone());
            conditions.push(format!(
                "{} <= ${}::timestamptz",
                request.export_type.time_column(),
                bindings.len()
            ));
        }

        if let Some(status) = &request.filters.status {
            bindings.push(status.clone());
            conditions.push(format!(
                "{}::text = ${}",
                request.export_type.status_column(),
                bindings.len()
            ));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let selected: Vec<String> = columns
            .iter()
            .map(|column| format!("\"{column}\"::text"))
            .collect();

        let sql = format!(
            "SELECT {} FROM {}{} LIMIT {limit}",
            selected.join(", "),
            request.export_type.table(),
            where_clause
        );

        let mut query = sqlx::query(&sql);

        for binding in &bindings {
            query = query.bind(binding);
        }

        let mut stream = query.fetch(&self.pool);
        let mut rows: Vec<Vec<String>> = Vec::new();

        while let Some(row) = stream
            .try_next()
            .await
            .map_err(|error| format!("export query failed: {error}"))?
        {
            let mut values = Vec::with_capacity(columns.len());

            for index in 0..columns.len() {
                let value: Option<String> = row
                    .try_get(index)
                    .map_err(|error| format!("cannot read column {index}: {error}"))?;

                values.push(value.unwrap_or_default());
            }

            rows.push(values);
        }

        let row_count = rows.len() as u64;
        let encoded =
            Exporter::render(request.format, &columns, &rows, request.export_type.table())?;

        Ok((encoded, row_count, columns))
    }

    /// The column names a table actually has, for diagnostics.
    pub async fn describe(&self, table: &str) -> Result<HashMap<String, String>, String> {
        if ![
            "transactions",
            "blocks",
            "tokens",
            "nfts",
            "token_transfers",
            "nft_transfers",
            "contracts",
            "logs",
        ]
        .contains(&table)
        {
            return Err(format!("unknown table {table}"));
        }

        let rows = sqlx::query(
            "SELECT column_name, data_type FROM information_schema.columns \
             WHERE table_name = $1 ORDER BY ordinal_position",
        )
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| format!("cannot describe {table}: {error}"))?;

        Ok(rows
            .iter()
            .map(|row| {
                let name: String = row.get::<String, _>(0);
                let data_type: String = row.get::<String, _>(1);
                (name, data_type)
            })
            .collect())
    }
}

/// Reject a filter value that is not a 20-byte hex address.
fn validate_address(address: &str) -> Result<(), String> {
    let trimmed = address.trim_start_matches("0x");

    if trimmed.len() != 40 || !trimmed.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(format!("{address} is not a valid address"));
    }

    Ok(())
}

/// Reject a filter value that is not an ISO-8601 timestamp.
fn validate_date(date: &str) -> Result<(), String> {
    chrono::DateTime::parse_from_rfc3339(date)
        .map(|_| ())
        .or_else(|_| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map(|_| ()))
        .map_err(|_| format!("{date} is not an ISO-8601 timestamp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(export_type: ExportType, columns: Vec<String>) -> ExportRequest {
        ExportRequest {
            export_type,
            format: ExportFormat::Csv,
            filters: ExportFilters::default(),
            columns,
            limit: None,
        }
    }

    #[test]
    fn a_column_outside_the_allow_list_is_refused() {
        let request = request(
            ExportType::Transactions,
            vec!["hash".to_string(), "input".to_string()],
        );

        let error = ExportService::resolve_columns(&request).unwrap_err();

        assert!(error.contains("not exportable"));
    }

    #[test]
    fn an_empty_column_list_falls_back_to_the_defaults() {
        let request = request(ExportType::Blocks, Vec::new());

        let columns = ExportService::resolve_columns(&request).unwrap();

        assert_eq!(columns.len(), 8);
        assert_eq!(columns[0], "number");
    }

    #[test]
    fn csv_has_a_header_and_one_line_per_row() {
        let columns = vec!["hash".to_string(), "value".to_string()];
        let rows = vec![
            vec!["0xabc".to_string(), "10".to_string()],
            vec!["0xdef".to_string(), "20".to_string()],
        ];

        let csv = String::from_utf8(Exporter::to_csv(&columns, &rows).unwrap()).unwrap();

        assert_eq!(csv, "hash,value\n0xabc,10\n0xdef,20\n");
    }

    #[test]
    fn csv_quotes_a_value_that_contains_a_comma() {
        let columns = vec!["name".to_string()];
        let rows = vec![vec!["a,b".to_string()]];

        let csv = String::from_utf8(Exporter::to_csv(&columns, &rows).unwrap()).unwrap();

        assert!(csv.contains("\"a,b\""));
    }

    #[test]
    fn json_rows_carry_the_column_names() {
        let columns = vec!["hash".to_string(), "value".to_string()];
        let rows = vec![vec!["0xabc".to_string(), "10".to_string()]];

        let json: serde_json::Value =
            serde_json::from_slice(&Exporter::to_json(&columns, &rows).unwrap()).unwrap();

        assert_eq!(json[0]["hash"], "0xabc");
        assert_eq!(json[0]["value"], "10");
    }

    #[test]
    fn an_excel_workbook_starts_with_the_zip_magic() {
        let columns = vec!["hash".to_string()];
        let rows = vec![vec!["0xabc".to_string()]];

        let workbook = Exporter::to_excel(&columns, &rows, "transactions").unwrap();

        // An XLSX file is a ZIP container, so it opens with "PK".
        assert_eq!(&workbook[..2], b"PK");
        assert!(workbook.len() > 100);
    }

    #[test]
    fn a_parquet_file_starts_with_the_parquet_magic() {
        let columns = vec!["hash".to_string(), "value".to_string()];
        let rows = vec![
            vec!["0xabc".to_string(), "10".to_string()],
            vec!["0xdef".to_string(), "20".to_string()],
        ];

        let parquet = Exporter::to_parquet(&columns, &rows).unwrap();

        assert_eq!(&parquet[..4], b"PAR1");
        assert_eq!(&parquet[parquet.len() - 4..], b"PAR1");
    }

    #[test]
    fn a_parquet_file_with_no_rows_still_has_a_schema() {
        let columns = vec!["hash".to_string()];

        let parquet = Exporter::to_parquet(&columns, &[]).unwrap();

        assert_eq!(&parquet[..4], b"PAR1");
    }

    #[test]
    fn a_malformed_address_filter_is_refused() {
        assert!(validate_address("0x123").is_err());
        assert!(validate_address("0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
        assert!(validate_address("0x0000000000000000000000000000000000000000").is_ok());
    }

    #[test]
    fn a_malformed_date_filter_is_refused() {
        assert!(validate_date("not a date").is_err());
        assert!(validate_date("2024-01-01").is_ok());
        assert!(validate_date("2024-01-01T00:00:00Z").is_ok());
    }

    #[test]
    fn every_format_has_an_extension_and_a_media_type() {
        for format in [
            ExportFormat::Csv,
            ExportFormat::Excel,
            ExportFormat::Json,
            ExportFormat::Parquet,
        ] {
            assert!(!Exporter::extension(format).is_empty());
            assert!(Exporter::content_type(format).contains('/'));
        }
    }

    #[test]
    fn each_export_type_names_a_distinct_table() {
        let types = [
            ExportType::Transactions,
            ExportType::Blocks,
            ExportType::Tokens,
            ExportType::Nfts,
            ExportType::TokenTransfers,
            ExportType::NftTransfers,
            ExportType::Contracts,
            ExportType::Logs,
        ];

        let mut tables: Vec<&str> = types.iter().map(ExportType::table).collect();
        tables.sort_unstable();
        tables.dedup();

        assert_eq!(tables.len(), types.len());
    }

    #[test]
    fn the_default_columns_are_all_exportable() {
        for export_type in [
            ExportType::Transactions,
            ExportType::Blocks,
            ExportType::Tokens,
            ExportType::Nfts,
            ExportType::TokenTransfers,
            ExportType::NftTransfers,
            ExportType::Contracts,
            ExportType::Logs,
        ] {
            for column in export_type.default_columns() {
                assert!(export_type.columns().contains(&column.as_str()));
            }
        }
    }
}
