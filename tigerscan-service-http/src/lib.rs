//! Shared serving loop for the TigerScan explorer services.
//!
//! Every explorer service answers JSON-RPC over plain HTTP. Before this crate
//! existed each service carried its own copy of the accept loop, and the copies
//! had drifted: some read the request body through `Request::as_vec`, which does
//! not exist, and some called `Response::add_header` on the value returned by
//! `Request::respond`, which returns `Result<(), IoError>`. Both are compile
//! errors, so those services never built.
//!
//! This crate holds one correct implementation:
//!
//! * the body is read to the end through `Request::as_reader`,
//! * only `POST` is accepted,
//! * the response is built first and its headers attached with `with_header`,
//! * every response carries the security headers the browser-facing services
//!   need,
//! * a request that cannot be read is answered with a JSON error and a 4xx
//!   status rather than dropped.

#![forbid(unsafe_code)]

use std::io::{Cursor, Read};
use std::net::SocketAddr;

use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use tracing::{error, warn};

/// Largest request body accepted, in bytes. Anything larger is refused before it
/// is buffered, so a client cannot exhaust memory with one request.
pub const MAX_BODY_BYTES: usize = 1 << 20;

/// The response type the services already return from their handlers.
pub type JsonResponse = Response<Cursor<Vec<u8>>>;

/// The security headers every service response carries.
fn security_headers() -> Vec<Header> {
    let literals: [(&[u8], &[u8]); 6] = [
        (b"Content-Type", b"application/json"),
        (b"X-Content-Type-Options", b"nosniff"),
        (b"X-Frame-Options", b"DENY"),
        (b"X-XSS-Protection", b"1; mode=block"),
        (
            b"Strict-Transport-Security",
            b"max-age=31536000; includeSubDomains",
        ),
        (
            b"Content-Security-Policy",
            b"default-src 'none'; frame-ancestors 'none'",
        ),
    ];

    literals
        .iter()
        .map(|(name, value)| {
            Header::from_bytes(*name, *value).expect("every header above is a valid literal")
        })
        .collect()
}

/// Build a 200 response from any serialisable value.
pub fn json_response<T: serde::Serialize>(value: &T) -> JsonResponse {
    let payload = serde_json::to_string(value)
        .unwrap_or_else(|_| "{\"error\":\"response is not serialisable\"}".to_string());

    Response::from_string(payload).with_status_code(StatusCode::from(200))
}

/// Attach the security headers to a response.
pub fn with_security_headers<R: Read>(response: Response<R>) -> Response<R> {
    security_headers()
        .into_iter()
        .fold(response, |response, header| response.with_header(header))
}

/// A JSON error response with the given status code.
fn error_response(code: i32, message: &str, status: u16) -> JsonResponse {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "error": { "code": code, "message": message },
        "id": serde_json::Value::Null,
    })
    .to_string();

    with_security_headers(Response::from_string(payload).with_status_code(StatusCode::from(status)))
}

/// Read a request body, refusing anything over `MAX_BODY_BYTES`.
fn read_body(request: &mut Request) -> Result<Vec<u8>, String> {
    let declared = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Content-Length"))
        .and_then(|header| header.value.as_str().parse::<usize>().ok());

    if let Some(length) = declared {
        if length > MAX_BODY_BYTES {
            return Err(format!(
                "request body of {length} bytes exceeds the {MAX_BODY_BYTES} byte limit"
            ));
        }
    }

    let mut body = Vec::with_capacity(declared.unwrap_or(0).min(MAX_BODY_BYTES));

    request
        .as_reader()
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| format!("cannot read request body: {error}"))?;

    if body.len() > MAX_BODY_BYTES {
        return Err(format!(
            "request body exceeds the {MAX_BODY_BYTES} byte limit"
        ));
    }

    Ok(body)
}

/// Answer one request with a handler that does not need the client address.
pub fn serve_one<H>(mut request: Request, handler: &H) -> Result<(), String>
where
    H: Fn(&[u8]) -> JsonResponse,
{
    if request.method() != &Method::Post {
        return request
            .respond(error_response(-32600, "only POST is accepted", 405))
            .map_err(|error| format!("cannot write response: {error}"));
    }

    let body = match read_body(&mut request) {
        Ok(body) => body,
        Err(message) => {
            warn!(%message, "rejecting request");

            return request
                .respond(error_response(-32600, &message, 400))
                .map_err(|error| format!("cannot write response: {error}"));
        }
    };

    request
        .respond(with_security_headers(handler(&body)))
        .map_err(|error| format!("cannot write response: {error}"))
}

/// Answer one request with a handler that receives the client address.
pub fn serve_one_with_ip<H>(mut request: Request, handler: &H) -> Result<(), String>
where
    H: Fn(&[u8], &str) -> JsonResponse,
{
    if request.method() != &Method::Post {
        return request
            .respond(error_response(-32600, "only POST is accepted", 405))
            .map_err(|error| format!("cannot write response: {error}"));
    }

    let ip = request
        .remote_addr()
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let body = match read_body(&mut request) {
        Ok(body) => body,
        Err(message) => {
            warn!(%message, "rejecting request");

            return request
                .respond(error_response(-32600, &message, 400))
                .map_err(|error| format!("cannot write response: {error}"));
        }
    };

    request
        .respond(with_security_headers(handler(&body, &ip)))
        .map_err(|error| format!("cannot write response: {error}"))
}

/// Serve requests until the process stops.
///
/// The handler runs on the calling thread. These services are backed by
/// synchronous state, so spawning a task per request only to block on it adds
/// latency without adding throughput.
pub fn serve<H>(addr: SocketAddr, handler: H) -> Result<(), String>
where
    H: Fn(&[u8]) -> JsonResponse + Send + Sync,
{
    let server = Server::http(addr).map_err(|error| format!("cannot bind {addr}: {error}"))?;

    for request in server.incoming_requests() {
        if let Err(error) = serve_one(request, &handler) {
            error!(%error, "request failed");
        }
    }

    Ok(())
}

/// Serve requests, passing the client address to the handler.
pub fn serve_with_ip<H>(addr: SocketAddr, handler: H) -> Result<(), String>
where
    H: Fn(&[u8], &str) -> JsonResponse + Send + Sync,
{
    let server = Server::http(addr).map_err(|error| format!("cannot bind {addr}: {error}"))?;

    for request in server.incoming_requests() {
        if let Err(error) = serve_one_with_ip(request, &handler) {
            error!(%error, "request failed");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_security_headers_are_complete() {
        let names: Vec<String> = security_headers()
            .iter()
            .map(|header| header.field.as_str().to_string())
            .collect();

        for expected in [
            "Content-Type",
            "X-Content-Type-Options",
            "X-Frame-Options",
            "X-XSS-Protection",
            "Strict-Transport-Security",
            "Content-Security-Policy",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn every_header_parses() {
        // `Header::from_bytes` rejects control characters; this pins the literals.
        assert_eq!(security_headers().len(), 6);
    }

    #[test]
    fn an_error_response_carries_the_requested_status() {
        let response = error_response(-32600, "only POST is accepted", 405);
        assert_eq!(response.status_code().0, 405);
    }

    #[test]
    fn the_body_limit_is_one_mebibyte() {
        assert_eq!(MAX_BODY_BYTES, 1_048_576);
    }
}
