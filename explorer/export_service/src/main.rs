#![forbid(unsafe_code)]
//! Export service HTTP surface.
//!
//! A job runs the export against PostgreSQL and keeps the encoded file under
//! its id, so `export_getJob` reports the real row count and the download route
//! returns the same bytes. A job that fails keeps the failure reason.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Response, StatusCode};
use tokio::runtime::Runtime;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use export_service::{
    ExportJob, ExportRequest, ExportService, Exporter, JobStatus, MAX_EXPORT_ROWS,
};

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRequest {
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

impl ApiResponse {
    fn ok(result: serde_json::Value) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(message.into()),
        }
    }
}

/// A finished job and the file it produced.
struct FinishedJob {
    job: ExportJob,
    file: Vec<u8>,
    filename: String,
    content_type: String,
}

pub struct Handler {
    service: Arc<ExportService>,
    jobs: RwLock<HashMap<String, FinishedJob>>,
}

impl Handler {
    pub fn new(service: Arc<ExportService>) -> Self {
        Self {
            service,
            jobs: RwLock::new(HashMap::new()),
        }
    }

    /// Answer one API request.
    pub async fn handle(&self, body: &[u8]) -> ApiResponse {
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(error) => return ApiResponse::error(format!("parse error: {error}")),
        };

        match request.method.as_str() {
            "export_createJob" => self.create_job(request.params).await,
            "export_getJob" => self.get_job(request.params),
            "export_formats" => ApiResponse::ok(serde_json::json!({
                "formats": ["csv", "excel", "json", "parquet"],
                "maxRows": MAX_EXPORT_ROWS,
            })),
            other => ApiResponse::error(format!("unknown method: {other}")),
        }
    }

    async fn create_job(&self, params: Option<serde_json::Value>) -> ApiResponse {
        let params = match params {
            Some(params) => params,
            None => return ApiResponse::error("missing params"),
        };

        let request: ExportRequest = match serde_json::from_value(params) {
            Ok(request) => request,
            Err(error) => return ApiResponse::error(format!("invalid export request: {error}")),
        };

        let job_id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();

        match self.service.export(&request).await {
            Ok((file, row_count, _columns)) => {
                let filename = format!("{job_id}.{}", Exporter::extension(request.format));
                let content_type = Exporter::content_type(request.format).to_string();

                let job = ExportJob {
                    id: job_id.clone(),
                    request,
                    status: JobStatus::Completed,
                    file_url: Some(format!("/download/{job_id}")),
                    created_at,
                    completed_at: Some(chrono::Utc::now().timestamp()),
                    row_count,
                    error: None,
                };

                let summary = serde_json::to_value(&job).unwrap_or_default();

                self.jobs.write().insert(
                    job_id.clone(),
                    FinishedJob {
                        job,
                        file,
                        filename,
                        content_type,
                    },
                );

                ApiResponse::ok(summary)
            }
            Err(message) => {
                error!(%job_id, %message, "export failed");

                let job = ExportJob {
                    id: job_id.clone(),
                    request,
                    status: JobStatus::Failed,
                    file_url: None,
                    created_at,
                    completed_at: Some(chrono::Utc::now().timestamp()),
                    row_count: 0,
                    error: Some(message.clone()),
                };

                let summary = serde_json::to_value(&job).unwrap_or_default();

                self.jobs.write().insert(
                    job_id,
                    FinishedJob {
                        job,
                        file: Vec::new(),
                        filename: String::new(),
                        content_type: String::new(),
                    },
                );

                ApiResponse::ok(summary)
            }
        }
    }

    fn get_job(&self, params: Option<serde_json::Value>) -> ApiResponse {
        let Some(job_id) = params
            .as_ref()
            .and_then(|params| params.get("jobId"))
            .and_then(|id| id.as_str())
        else {
            return ApiResponse::error("missing jobId");
        };

        let jobs = self.jobs.read();

        match jobs.get(job_id) {
            Some(finished) => {
                ApiResponse::ok(serde_json::to_value(&finished.job).unwrap_or_default())
            }
            None => ApiResponse::error(format!("job {job_id} does not exist")),
        }
    }

    /// The encoded file of a finished job, for the download route.
    pub fn download(&self, job_id: &str) -> Option<(Vec<u8>, String, String)> {
        self.jobs.read().get(job_id).and_then(|finished| {
            (finished.job.status == JobStatus::Completed).then(|| {
                (
                    finished.file.clone(),
                    finished.filename.clone(),
                    finished.content_type.clone(),
                )
            })
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tigerscan:tigerscan@localhost:5432/tigerscan".to_string());

    info!("Starting Export Service");

    let addr = SocketAddr::from(([0, 0, 0, 0], 9004));
    info!("Server listening on {}", addr);

    let runtime = Runtime::new()?;

    let service = runtime.block_on(ExportService::new(&database_url))?;
    let handler = Arc::new(Handler::new(Arc::new(service)));

    // The serving loop is synchronous; the export queries are not.
    let server = tiny_http::Server::http(addr)
        .map_err(|error| format!("cannot bind {addr}: {error}"))?;

    for mut request in server.incoming_requests() {
        if request.method() != &tiny_http::Method::Post {
            let _ = request
                .respond(Response::from_string("only POST is accepted").with_status_code(405));
            continue;
        }

        let mut body = Vec::new();
        if let Err(error) = request.as_reader().read_to_end(&mut body) {
            warn!(%error, "cannot read request body");
            continue;
        }

        if body.len() > tigerscan_service_http::MAX_BODY_BYTES {
            let _ = request
                .respond(Response::from_string("request body too large").with_status_code(413));
            continue;
        }

        let response = runtime.block_on(handler.handle(&body));
        let encoded = serde_json::to_string(&response).unwrap_or_default();

        let status = if response.success { 200 } else { 400 };

        let _ = request
            .respond(Response::from_string(encoded).with_status_code(StatusCode::from(status)));
    }

    Ok(())
}
