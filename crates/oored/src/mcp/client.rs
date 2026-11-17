//! Sends a tool's API calls through the daemon's own router.
//!
//! A tool never reads the database or checks permissions itself. It sends
//! the same request an API client would, with the caller's own
//! `Authorization` header, to the router the daemon serves. So every tool
//! runs the API handler's code and permission checks, token limits
//! included, and cannot drift from them.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

/// The largest API response a tool reads. Bigger answers are cut off as an
/// error rather than read into memory.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Headers of the MCP request that are not passed on to the API: they
/// describe the MCP request's own body or protocol.
const DROPPED_HEADERS: &[&str] = &[
    "accept",
    "content-length",
    "content-type",
    "content-encoding",
    "transfer-encoding",
    "connection",
    "expect",
    "idempotency-key",
    "last-event-id",
    "mcp-protocol-version",
    "mcp-session-id",
    "mcp-method",
    "mcp-name",
];

/// A failed API call, in the API's own words.
#[derive(Debug, Clone)]
pub struct ApiFailure {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    /// Whether the body was an Oore API error. A bare 404 without one means
    /// the route does not exist on this daemon.
    pub from_api: bool,
}

impl ApiFailure {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "mcp_internal".to_string(),
            message: message.into(),
            hint: None,
            from_api: false,
        }
    }

    /// The route is missing on this daemon, as opposed to the API answering
    /// that the thing asked for does not exist.
    pub fn route_missing(&self) -> bool {
        !self.from_api && matches!(self.status.as_u16(), 404 | 405)
    }
}

/// The caller's view of the API, for one MCP request.
#[derive(Clone)]
pub struct ApiClient {
    router: Router,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<SocketAddr>>,
}

impl ApiClient {
    /// `headers` are the MCP request's headers. The caller's
    /// `Authorization`, `Host` and forwarding headers go on every API call.
    pub fn new(
        router: Router,
        incoming: &HeaderMap,
        connect_info: Option<ConnectInfo<SocketAddr>>,
    ) -> Self {
        let mut headers = HeaderMap::new();
        for (name, value) in incoming {
            if DROPPED_HEADERS.contains(&name.as_str()) {
                continue;
            }
            headers.append(name.clone(), value.clone());
        }
        Self {
            router,
            headers,
            connect_info,
        }
    }

    pub async fn get(&self, path: &str) -> Result<Value, ApiFailure> {
        self.send(Method::GET, path, None, None).await
    }

    pub async fn post(&self, path: &str, body: Option<Value>) -> Result<Value, ApiFailure> {
        self.send(Method::POST, path, body, None).await
    }

    pub async fn post_idempotent(
        &self,
        path: &str,
        body: Option<Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value, ApiFailure> {
        self.send(Method::POST, path, body, idempotency_key).await
    }

    pub async fn put(&self, path: &str, body: Value) -> Result<Value, ApiFailure> {
        self.send(Method::PUT, path, Some(body), None).await
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value, ApiFailure> {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(headers) = builder.headers_mut() {
            headers.extend(self.headers.clone());
            headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
            if let Some(key) = idempotency_key {
                let value = HeaderValue::from_str(key).map_err(|_| ApiFailure {
                    status: StatusCode::BAD_REQUEST,
                    code: "invalid_idempotency_key".to_string(),
                    message: "idempotency_key must be printable ASCII".to_string(),
                    hint: None,
                    from_api: false,
                })?;
                headers.insert(HeaderName::from_static("idempotency-key"), value);
            }
        }
        let request_body = match body {
            Some(value) => {
                if let Some(headers) = builder.headers_mut() {
                    headers.insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json"),
                    );
                }
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let mut request = builder
            .body(request_body)
            .map_err(|error| ApiFailure::internal(format!("bad API request: {error}")))?;
        if let Some(info) = self.connect_info {
            request.extensions_mut().insert(info);
        }

        let response = match self.router.clone().oneshot(request).await {
            Ok(response) => response,
            Err(never) => match never {},
        };
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
            .await
            .map_err(|_| ApiFailure::internal("The API answer was too large to read."))?;

        if status.is_success() {
            if bytes.is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_slice(&bytes).map_err(|_| {
                ApiFailure::internal("The API answered with something other than JSON.")
            });
        }

        let parsed: Option<Value> = serde_json::from_slice(&bytes).ok();
        let field = |name: &str| {
            parsed
                .as_ref()
                .and_then(|value| value.get(name))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let code = field("code");
        Err(ApiFailure {
            status,
            from_api: code.is_some(),
            code: code.unwrap_or_else(|| default_code(status).to_string()),
            message: field("error").unwrap_or_else(|| {
                status
                    .canonical_reason()
                    .unwrap_or("Request failed")
                    .to_string()
            }),
            hint: field("hint").or_else(|| field("details")),
        })
    }
}

fn default_code(status: StatusCode) -> &'static str {
    match status.as_u16() {
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        405 => "method_not_allowed",
        409 => "conflict",
        429 => "rate_limited",
        _ => "request_failed",
    }
}

/// How long a tool may wait in one call before it answers. Clients give
/// up on a tool call after about a minute.
pub const MAX_WAIT: Duration = Duration::from_secs(50);
