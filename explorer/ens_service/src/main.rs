//! TigerScan ENS Resolution Service
//!
//! Serves ENS lookups over JSON-RPC. Every response comes from the registry,
//! resolver or reverse lookup; a failure from any of them is reported as a
//! JSON-RPC error rather than a placeholder record.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::{error, info, warn};

use tigerscan_ens::{Database, ENSCache, ENSQuery, ENSRegistry, Resolver, ReverseLookup, Server};

const DEFAULT_PORT: u16 = 9040;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
    #[serde(default)]
    id: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: serde_json::Value,
}

impl JsonRpcResponse {
    fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    fn failure(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
            id,
        }
    }
}

/// Dispatch one request against the ENS server.
async fn dispatch(server: &Server, request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id;

    // A request that does not declare JSON-RPC 2.0 is not answered as one.
    if !request.jsonrpc.is_empty() && request.jsonrpc != "2.0" {
        return JsonRpcResponse::failure(id, -32600, "jsonrpc must be \"2.0\"");
    }

    match request.method.as_str() {
        "ens_resolve" => {
            let name = match request.params.get("name").and_then(|v| v.as_str()) {
                Some(name) if !name.is_empty() => name,
                _ => {
                    return JsonRpcResponse::failure(id, -32602, "name is required");
                }
            };

            let query = ENSQuery {
                name: name.to_string(),
                resolver_type: None,
                include_text: request.params.get("include_text").and_then(|v| v.as_bool()),
                include_coins: request.params.get("include_coins").and_then(|v| v.as_bool()),
                cache_only: request.params.get("cache_only").and_then(|v| v.as_bool()),
            };

            match server.resolve(query).await {
                Ok(record) => JsonRpcResponse::success(
                    id,
                    serde_json::to_value(record).unwrap_or(serde_json::Value::Null),
                ),
                Err(error) => JsonRpcResponse::failure(id, -32000, error.to_string()),
            }
        }

        "ens_reverse" => {
            let address = match request.params.get("address").and_then(|v| v.as_str()) {
                Some(address) if !address.is_empty() => address,
                _ => {
                    return JsonRpcResponse::failure(id, -32602, "address is required");
                }
            };

            match server.reverse_lookup(address).await {
                Ok(name) => JsonRpcResponse::success(id, serde_json::json!({ "name": name })),
                Err(error) => JsonRpcResponse::failure(id, -32000, error.to_string()),
            }
        }

        "ens_domain" => {
            let name = match request.params.get("name").and_then(|v| v.as_str()) {
                Some(name) if !name.is_empty() => name,
                _ => {
                    return JsonRpcResponse::failure(id, -32602, "name is required");
                }
            };

            match server.get_domain(name).await {
                Ok(domain) => JsonRpcResponse::success(
                    id,
                    serde_json::to_value(domain).unwrap_or(serde_json::Value::Null),
                ),
                Err(error) => JsonRpcResponse::failure(id, -32000, error.to_string()),
            }
        }

        "ens_available" => {
            let name = match request.params.get("name").and_then(|v| v.as_str()) {
                Some(name) if !name.is_empty() => name,
                _ => {
                    return JsonRpcResponse::failure(id, -32602, "name is required");
                }
            };

            match server.is_available(name).await {
                Ok(available) => {
                    JsonRpcResponse::success(id, serde_json::json!({ "available": available }))
                }
                Err(error) => JsonRpcResponse::failure(id, -32000, error.to_string()),
            }
        }

        "ens_health" => JsonRpcResponse::success(id, serde_json::json!({ "status": "ok" })),

        other => JsonRpcResponse::failure(id, -32601, format!("unknown method: {other}")),
    }
}

fn json_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("the content type header is a valid literal")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let rpc_url = std::env::var("TGR_RPC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());
    let database_url = std::env::var("DATABASE_URL").unwrap_or_default();
    let port: u16 = std::env::var("ENS_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(&database_url)
        .await
        .map_err(|error| format!("cannot reach PostgreSQL at DATABASE_URL: {error}"))?;

    let database = Arc::new(Database::new(pool));
    database.init().await?;

    let cache = Arc::new(ENSCache::new(10_000, 300));

    let registry_address = std::env::var("ENS_REGISTRY_ADDRESS")
        .unwrap_or_else(|_| "0x00000000000C2E074eC69A0dFb2997BA6C7d2e1e".to_string());

    let registry = Arc::new(ENSRegistry::new(
        &rpc_url,
        &registry_address,
        database.clone(),
        cache.clone(),
    )?);

    if !registry.is_reachable().await {
        return Err(format!("no node is answering at {rpc_url}").into());
    }

    let resolver = Arc::new(Resolver::new(&rpc_url, database.clone())?);
    let reverse = Arc::new(ReverseLookup::new(&rpc_url, database, registry.clone())?);

    let server = Arc::new(Server::new(registry, resolver, reverse, cache));

    let listener = tiny_http::Server::http(SocketAddr::from(([0, 0, 0, 0], port)))
        .map_err(|error| format!("cannot bind port {port}: {error}"))?;

    info!("TigerScan ENS service listening on port {port}");

    for request in listener.incoming_requests() {
        let server = server.clone();
        let mut body = String::new();
        let mut request = request;

        if let Err(error) = request.as_reader().read_to_string(&mut body) {
            warn!("unreadable request body: {error}");
            let _ = request.respond(Response::empty(StatusCode::from(400)));
            continue;
        }

        let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(&body);

        let response = match parsed {
            Ok(parsed) => dispatch(&server, parsed).await,
            Err(error) => JsonRpcResponse::failure(
                serde_json::Value::Null,
                -32700,
                format!("invalid JSON: {error}"),
            ),
        };

        let payload = serde_json::to_string(&response).unwrap_or_else(|error| {
            error!(%error, "response could not be serialised");
            "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32603,\"message\":\"internal error\"},\"id\":null}"
                .to_string()
        });

        let mut response = Response::from_string(payload).with_status_code(StatusCode::from(200));
        response.add_header(json_header());
        response.add_header(
            Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..])
                .expect("the header name is a valid literal"),
        );

        if let Err(error) = request.respond(response) {
            warn!("could not write response: {error}");
        }
    }

    Ok(())
}
