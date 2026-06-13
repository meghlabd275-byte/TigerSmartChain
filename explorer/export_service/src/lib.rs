//! Bulk Export Service - CSV, Excel, Historical Data API

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Excel,
    Json,
    Parquet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

pub struct ExportService;

impl ExportService {
    /// Create export job
    pub fn create_job(request: ExportRequest) -> ExportJob {
        ExportJob {
            id: uuid::Uuid::new_v4().to_string(),
            request,
            status: JobStatus::Pending,
            file_url: None,
            created_at: chrono::Utc::now().timestamp(),
            completed_at: None,
        }
    }

    /// Export to CSV
    pub fn export_csv<T: Serialize>(data: &[T], headers: &[&str]) -> Result<String, String> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        
        // Write headers
        wtr.write_record(headers).map_err(|e| e.to_string())?;
        
        // Write data
        for record in data {
            if let Ok(fields) = serde_json::to_value(record) {
                if let Some(arr) = fields.as_array() {
                    let row: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                    let _ = wtr.write_record(&row);
                }
            }
        }
        
        let data = wtr.into_inner().map_err(|e| e.to_string())?;
        String::from_utf8(data).map_err(|e| e.to_string())
    }

    /// Export to Excel
    pub fn export_excel<T: Serialize>(data: &[T], sheet_name: &str) -> Result<Vec<u8>, String> {
        use calamine::{Workbook, Xlsx, DataType};
        
        let mut workbook = Xlsx::new();
        let sheet = workbook.add_worksheet(sheet_name);
        
        // Write headers
        if let Some(first) = data.first() {
            if let Ok(fields) = serde_json::to_value(first) {
                if let Some(arr) = fields.as_array() {
                    for (i, field) in arr.iter().enumerate() {
                        sheet.write_value(0, i as u32, DataType::String(field.to_string()));
                    }
                }
            }
        }
        
        // Write data
        for (row_idx, record) in data.iter().enumerate() {
            if let Ok(fields) = serde_json::to_value(record) {
                if let Some(arr) = fields.as_array() {
                    for (col_idx, field) in arr.iter().enumerate() {
                        let value = match field {
                            serde_json::Value::Number(n) => DataType::Float(n.as_f64().unwrap_or(0.0)),
                            serde_json::Value::Bool(b) => DataType::Bool(*b),
                            _ => DataType::String(field.to_string()),
                        };
                        sheet.write_value((row_idx + 1) as u32, col_idx as u32, value);
                    }
                }
            }
        }
        
        let mut buffer = vec![];
        workbook.save(&mut std::io::Cursor::new(&mut buffer)).map_err(|e| e.to_string())?;
        
        Ok(buffer)
    }

    /// Generate historical data URL
    pub fn generate_historical_url(job_id: &str) -> String {
        format!("https://api.tigerscan.io/v1/export/download/{}", job_id)
    }

    /// Stream data for large exports
    pub fn stream_csv(data: impl Iterator<Item = impl Serialize>) -> String {
        let mut wtr = csv::Writer::from_writer(vec![]);
        
        for record in data {
            if let Ok(fields) = serde_json::to_value(&record) {
                if let Some(arr) = fields.as_array() {
                    let row: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                    let _ = wtr.write_record(&row);
                }
            }
        }
        
        String::from_utf8(wtr.into_inner().unwrap_or_default()).unwrap_or_default()
    }
}

// Data types for export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionExport {
    pub hash: String,
    pub block_number: u64,
    pub timestamp: i64,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas_price: String,
    pub gas_used: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockExport {
    pub number: u64,
    pub hash: String,
    pub timestamp: i64,
    pub transactions: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub miner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExport {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
    pub holders: u64,
    pub transfers: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftExport {
    pub token_address: String,
    pub token_id: String,
    pub owner: String,
    pub name: Option<String>,
    pub image_url: Option<String>,
    pub attributes: Option<String>,
}
