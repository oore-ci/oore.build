//! The MCP server: `POST /mcp`, so agents in Claude Code, Cursor, VS Code
//! and other MCP clients can use Oore as a set of tools.
//!
//! - Protocol: MCP revision 2026-07-28 (stateless Streamable HTTP, one POST
//!   per request), through the official Rust SDK `rmcp`. Clients on older
//!   revisions are served statelessly too.
//! - Sign-in: `Authorization: Bearer <API token>` (a sign-in session token
//!   works as well, for `oore mcp` on a signed-in machine).
//! - Off until an owner turns on `mcp_enabled`. While it is off the route
//!   answers 404, as if it did not exist.
//! - Every tool sends ordinary API requests through the daemon's own router
//!   with the caller's credentials (see [`client`]), so permissions, token
//!   limits and audit entries are exactly the API's.

mod client;
mod extras;
mod logs;
mod shape;
mod tools;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, GetPromptRequestParams,
    GetPromptResponse, Implementation, ListPromptsResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ServerCapabilities,
    ServerConfig,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tower::ServiceExt;

use crate::AppState;
use crate::extractors::authenticate_bearer;
use crate::session::SessionInfo;
use crate::util::{api_err, extract_bearer};

use client::ApiClient;

/// The path of the MCP endpoint.
pub const PATH: &str = "/mcp";

/// Largest MCP request body. Tool arguments are small; a pipeline file to
/// validate is the biggest.
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

const INSTRUCTIONS: &str = "Oore builds, tests and ships mobile apps. Apps are Oore projects; each has \
pipelines that build it. To look into a failed build call diagnose_build; to follow a build call \
wait_for_build repeatedly until done is true. Results are trimmed: log tools say when lines were \
left out and how to read more.";

/// Adds `POST /mcp` to the finished router. Call it last, so tool calls go
/// through every layer the API has.
pub fn mount(router: Router, state: Arc<AppState>) -> Router {
    let api = router.clone();
    let handler = OoreMcp { api };
    let mut config = StreamableHttpServerConfig::default();
    // Sessions are gone in 2026-07-28; older clients are served statelessly
    // as well, so nothing is kept between requests.
    config.legacy_session_mode = false;
    config.json_response = true;
    config.sse_keep_alive = None;
    // Host and Origin are checked in `entry`, against the request itself.
    config.allowed_hosts = Vec::new();
    config.max_request_body_bytes = MAX_REQUEST_BYTES;
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    let mcp = Router::new().route(PATH, any(entry)).with_state(McpState {
        app: state,
        service,
    });
    router.merge(mcp)
}

#[derive(Clone)]
struct McpState {
    app: Arc<AppState>,
    service: StreamableHttpService<OoreMcp, NeverSessionManager>,
}

/// Who is calling, put on the request for the tool handlers.
#[derive(Clone)]
struct Caller {
    session: SessionInfo,
}

/// Whether an owner turned MCP on. A database without the setting yet
/// (before the preference's migration) counts as off.
pub async fn mcp_enabled(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT mcp_enabled FROM instance_preferences WHERE id = 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some_and(|value| value != 0)
}

/// Browsers send `Origin`; agents do not. A browser page may only call the
/// endpoint from the daemon's own origin, which stops DNS rebinding against
/// a daemon on loopback.
fn origin_allowed(headers: &axum::http::HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let Ok(parsed) = url::Url::parse(origin) else {
        return false;
    };
    let Some(origin_host) = parsed.host_str() else {
        return false;
    };
    let origin_authority = match parsed.port() {
        Some(port) => format!("{origin_host}:{port}"),
        None => origin_host.to_string(),
    };
    let default_port = match parsed.scheme() {
        "https" => ":443",
        "http" => ":80",
        _ => return false,
    };
    let host = host.to_ascii_lowercase();
    let origin_authority = origin_authority.to_ascii_lowercase();
    host == origin_authority
        || host.strip_suffix(default_port) == Some(origin_authority.as_str())
        || origin_authority.strip_suffix(default_port) == Some(host.as_str())
}

fn unauthorized(rejection: (StatusCode, axum::Json<oore_contract::ApiError>)) -> Response {
    let mut response = rejection.into_response();
    if response.status() == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"oore\""),
        );
    }
    response
}

async fn entry(State(state): State<McpState>, request: Request) -> Response {
    if !mcp_enabled(&state.app.db).await {
        return api_err(StatusCode::NOT_FOUND, "not_found", "Not found").into_response();
    }
    if !origin_allowed(request.headers()) {
        return api_err(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "MCP requests from a browser must come from this Oore instance's own address.",
        )
        .into_response();
    }

    let (mut parts, body) = request.into_parts();
    // Resolve the credential here rather than through `AuthUser`: `/mcp`
    // sits outside the API's request-scope layer, and its tool calls go
    // back through that layer, where a token's limits apply.
    let resolved = match extract_bearer(&parts.headers) {
        Some(token) => authenticate_bearer(&state.app, token).await,
        None => Err(api_err(
            StatusCode::UNAUTHORIZED,
            "missing_auth",
            "Authorization header required",
        )),
    };
    let session = match resolved {
        Ok(Some(session)) => session,
        Ok(None) => {
            return unauthorized(api_err(
                StatusCode::UNAUTHORIZED,
                "invalid_session",
                "Invalid or expired token",
            ));
        }
        Err(rejection) => return unauthorized(rejection),
    };
    parts.extensions.insert(Caller { session });
    let request = Request::from_parts(parts, body);
    match state.service.clone().oneshot(request).await {
        Ok(response) => response.map(Body::new),
        Err(never) => match never {},
    }
}

#[derive(Clone)]
struct OoreMcp {
    /// The daemon's router, without `/mcp`.
    api: Router,
}

impl OoreMcp {
    fn parts(context: &RequestContext<RoleServer>) -> Option<&axum::http::request::Parts> {
        context.extensions.get::<axum::http::request::Parts>()
    }

    fn caller(context: &RequestContext<RoleServer>) -> Option<&Caller> {
        Self::parts(context)?.extensions.get::<Caller>()
    }

    fn client(&self, context: &RequestContext<RoleServer>) -> Option<ApiClient> {
        let parts = Self::parts(context)?;
        let connect_info = parts.extensions.get::<ConnectInfo<SocketAddr>>().copied();
        Some(ApiClient::new(
            self.api.clone(),
            &parts.headers,
            connect_info,
        ))
    }
}

/// Whether the caller sees a tool. The API checks every call again; this
/// only keeps tools a caller can never use out of the list.
fn visible(spec: &tools::ToolSpec, session: &SessionInfo) -> bool {
    match spec.kind {
        tools::Kind::Read => true,
        tools::Kind::Build => session.role != "qa_viewer",
    }
}

impl ServerHandler for OoreMcp {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .build();
        ServerConfig::new(capabilities)
            .with_server_info(
                Implementation::new("oore", env!("CARGO_PKG_VERSION"))
                    .with_title("Oore")
                    .with_website_url("https://docs.oore.build/build/automate/mcp"),
            )
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let Some(caller) = Self::caller(&context) else {
            return Err(rmcp::ErrorData::internal_error("missing caller", None));
        };
        let listed = tools::TOOLS
            .iter()
            .filter(|spec| visible(spec, &caller.session))
            .map(tools::describe)
            .collect();
        Ok(ListToolsResult::with_all_items(listed))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let (Some(caller), Some(api)) = (Self::caller(&context), self.client(&context)) else {
            return Err(rmcp::ErrorData::internal_error("missing caller", None));
        };
        let Some(spec) = tools::find(&request.name).filter(|spec| visible(spec, &caller.session))
        else {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Unknown tool: {}", request.name),
                None,
            ));
        };
        let arguments = request.arguments.unwrap_or_default();
        let result = match tools::call(spec, &api, &arguments, &context.ct).await {
            Ok(value) => {
                let mut result =
                    CallToolResult::success(vec![ContentBlock::text(value.to_string())]);
                result.structured_content = Some(value);
                result
            }
            Err(error) => error.into_result(),
        };
        Ok(result.into())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult::with_all_items(extras::resources()))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        extras::read(&request.uri).map(Into::into).ok_or_else(|| {
            rmcp::ErrorData::resource_not_found(format!("No resource {}", request.uri), None)
        })
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, rmcp::ErrorData> {
        Ok(ListPromptsResult::with_all_items(extras::prompts()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, rmcp::ErrorData> {
        extras::get(&request.name, request.arguments.as_ref())
            .map(Into::into)
            .ok_or_else(|| {
                rmcp::ErrorData::invalid_params(format!("No prompt {}", request.name), None)
            })
    }
}

#[cfg(test)]
mod tests;
