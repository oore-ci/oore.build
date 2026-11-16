//! What to do next for each API error code. Every error response passes
//! through [`enrich_api_errors`], which adds `hint` and `retryable` when the
//! handler set neither, and through [`normalize_extractor_rejection`], which
//! turns axum's plain-text rejections into the error envelope with `fields`.
//!
//! The error code table in the docs is generated from the handlers and this
//! catalog: `make gen-error-codes`.

use axum::Json;
use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use oore_contract::{ApiError, ApiErrorField};

/// Error bodies larger than this are passed through untouched.
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// A code's next step and whether the same request may succeed later.
pub(crate) struct CodeAdvice {
    pub(crate) code: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) retryable: bool,
}

const fn advice(code: &'static str, retryable: bool, hint: &'static str) -> CodeAdvice {
    CodeAdvice {
        code,
        hint,
        retryable,
    }
}

pub(crate) const CATALOG: &[CodeAdvice] = &[
    advice(
        "missing_auth",
        false,
        "Send `Authorization: Bearer <token>` with a session or API token. Create an API token in Settings > API tokens.",
    ),
    advice(
        "invalid_session",
        false,
        "The token is unknown, revoked or expired. Sign in again or create a new API token.",
    ),
    advice(
        "session_expired",
        false,
        "Sign in again to get a new session.",
    ),
    advice("no_session", false, "Sign in first."),
    advice(
        "invalid_token",
        false,
        "The token is unknown, revoked or expired. Create a new one.",
    ),
    advice(
        "missing_token",
        false,
        "Send the token this endpoint asks for.",
    ),
    advice(
        "token_expired",
        false,
        "The link or token expired. Ask for a new one.",
    ),
    advice(
        "token_consumed",
        false,
        "This one-time token was already used. Ask for a new one.",
    ),
    advice(
        "permission_denied",
        false,
        "Your role in this app does not allow this. Ask an app maintainer or an instance admin for access.",
    ),
    advice(
        "insufficient_role",
        false,
        "Your instance role does not allow this. Ask the owner or an admin.",
    ),
    advice(
        "forbidden",
        false,
        "You are not allowed to do this. Ask the owner or an admin.",
    ),
    advice(
        "role_escalation",
        false,
        "You cannot give someone a higher role than your own.",
    ),
    advice(
        "not_found",
        false,
        "Check the ID. Apps you are not a member of also answer not found.",
    ),
    advice(
        "invalid_request",
        false,
        "The parameters or JSON body do not match the operation. See `fields` and the operation in /v1/openapi.json.",
    ),
    advice(
        "invalid_input",
        false,
        "Fix the value named in the message and send the request again.",
    ),
    advice(
        "validation_error",
        false,
        "Fix the values named in the message and send the request again.",
    ),
    advice("payload_too_large", false, "Send a smaller body."),
    advice(
        "rate_limited",
        true,
        "Wait a little, then send the request again.",
    ),
    advice(
        "too_many_attempts",
        true,
        "Wait a few minutes before trying again.",
    ),
    advice(
        "too_many_pending",
        true,
        "Let some pending requests finish, then try again.",
    ),
    advice(
        "too_many_waits",
        true,
        "Too many requests are waiting for builds. Let one finish, or poll without `wait`.",
    ),
    advice(
        "stream_limit",
        true,
        "Close another live stream, then reconnect.",
    ),
    advice(
        "stream_token_limit",
        true,
        "Use or let expire the stream tokens you already have, then ask again.",
    ),
    advice(
        "store_error",
        true,
        "The daemon could not read or write its database. Try again; if it persists, check the daemon log.",
    ),
    advice(
        "db_error",
        true,
        "The daemon could not read or write its database. Try again; if it persists, check the daemon log.",
    ),
    advice(
        "internal_error",
        true,
        "Try again; if it persists, check the daemon log.",
    ),
    advice(
        "session_error",
        true,
        "Try again; if it persists, check the daemon log.",
    ),
    advice(
        "storage_error",
        true,
        "The artifact store failed. Try again, and check the artifact storage settings if it persists.",
    ),
    advice(
        "storage_unavailable",
        true,
        "The artifact store cannot be reached. Check Settings > Artifacts.",
    ),
    advice(
        "storage_not_configured",
        false,
        "Set up artifact storage in Settings > Artifacts.",
    ),
    advice(
        "concurrent_modification",
        true,
        "Someone changed this at the same time. Load it again and retry.",
    ),
    advice(
        "preferences_changed",
        true,
        "The settings changed while you saved. Load them again and retry.",
    ),
    advice(
        "source_changed",
        true,
        "The source changed while you worked. Load it again and retry.",
    ),
    advice(
        "github_api_error",
        true,
        "GitHub did not answer as expected. Try again in a moment.",
    ),
    advice(
        "gitlab_api_error",
        true,
        "GitLab did not answer as expected. Try again in a moment.",
    ),
    advice(
        "gitlab_unreachable",
        true,
        "GitLab cannot be reached from the daemon. Check its network.",
    ),
    advice(
        "gitlab_unavailable",
        true,
        "GitLab is unavailable. Try again later.",
    ),
    advice(
        "source_unavailable",
        true,
        "The code host is unavailable. Try again later.",
    ),
    advice(
        "source_api_error",
        true,
        "The code host did not answer as expected. Try again in a moment.",
    ),
    advice(
        "apple_api_error",
        true,
        "App Store Connect did not answer as expected. Try again in a moment.",
    ),
    advice(
        "sync_failed",
        true,
        "The store could not be reached. Try again in a moment.",
    ),
    advice(
        "webhook_queue_unavailable",
        true,
        "The daemon is busy. Try again in a moment.",
    ),
    advice(
        "store_rejected",
        false,
        "The store refused the request. Read `error` for its reason, fix it in the store or the pipeline, then try again.",
    ),
    advice(
        "build_not_terminal",
        false,
        "Wait until the build has finished, then try again.",
    ),
    advice(
        "build_not_succeeded",
        false,
        "Only a build that succeeded can do this. Run the build again first.",
    ),
    advice(
        "invalid_transition",
        false,
        "The build or release is no longer in a state that allows this. Load it again to see its current state.",
    ),
    advice(
        "invalid_release_state",
        false,
        "The release is no longer in a state that allows this. Load it again to see its current state.",
    ),
    advice(
        "not_awaiting_approval",
        false,
        "The build is not waiting for approval any more.",
    ),
    advice(
        "release_frozen",
        false,
        "Releases are frozen for this app. Wait for the freeze to end or ask a maintainer to lift it.",
    ),
    advice(
        "releases_in_progress",
        false,
        "Wait for the releases in progress to finish first.",
    ),
    advice(
        "retry_in_progress",
        false,
        "A retry of this step is already running. Wait for it to finish.",
    ),
    advice(
        "step_not_retryable",
        false,
        "This step cannot be retried on its own. Run the whole build again.",
    ),
    advice(
        "active_builds",
        false,
        "Wait for the running builds to finish, or cancel them first.",
    ),
    advice(
        "publish_secret_missing",
        false,
        "Add the store credential the pipeline's publish step names, then try again.",
    ),
    advice(
        "signing_identity_missing",
        false,
        "Add the signing identity the pipeline uses, then try again.",
    ),
    advice(
        "setup_incomplete",
        false,
        "Finish setting up the instance first.",
    ),
    advice(
        "already_configured",
        false,
        "Setup is already complete. Sign in instead.",
    ),
    advice(
        "mcp_owner_required",
        false,
        "Only the owner can turn the MCP server on or off.",
    ),
    advice(
        "external_access_owner_required",
        false,
        "Only the owner can change External Access sign-in settings.",
    ),
    advice(
        "loopback_required",
        false,
        "Run this on the daemon's own machine.",
    ),
    advice(
        "local_login_loopback_required",
        false,
        "Passwordless sign-in only works on the daemon's own machine. Use the configured sign-in elsewhere.",
    ),
    advice(
        "user_disabled",
        false,
        "This account is turned off. Ask the owner or an admin to turn it on.",
    ),
    advice("name_taken", false, "Choose another name."),
    advice(
        "already_exists",
        false,
        "It already exists. Use the existing one or choose another name.",
    ),
    advice(
        "artifact_expired",
        false,
        "Retention removed this file. Run the build again to get a new one.",
    ),
];

pub(crate) fn advice_for(code: &str) -> Option<&'static CodeAdvice> {
    CATALOG.iter().find(|entry| entry.code == code)
}

/// When no code says otherwise: rate limits and server-side failures may
/// pass on a later try; a client error needs a different request.
fn retryable_by_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

/// Adds `hint` and `retryable` to every JSON error envelope that lacks them.
pub async fn enrich_api_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !(status.is_client_error() || status.is_server_error()) || !is_json {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_ERROR_BODY_BYTES).await else {
        // Too large to be an error envelope; the body is gone, so say so.
        return (
            status,
            Json(ApiError::new(
                "internal_error",
                "The error response was too large",
            )),
        )
            .into_response();
    };
    let Ok(mut error) = serde_json::from_slice::<ApiError>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    let advice = advice_for(&error.code);
    if error.hint.is_none() {
        error.hint = advice.map(|advice| advice.hint.to_string());
    }
    if error.retryable.is_none() {
        error.retryable =
            Some(advice.map_or_else(|| retryable_by_status(status), |advice| advice.retryable));
    }
    let Ok(body) = serde_json::to_vec(&error) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Response::from_parts(parts, Body::from(body))
}

/// The rejected input named in one of axum's rejection messages, such as
/// `Failed to deserialize the JSON body into the target type: steps[0].run:
/// invalid type: ...` or `... missing field `name` at line 1 column 2`.
fn rejected_field(message: &str) -> Option<ApiErrorField> {
    let (_, detail) = message.split_once(": ")?;
    let detail = detail.trim();
    if let Some(rest) = detail.split_once("missing field `").map(|(_, rest)| rest)
        && let Some((name, _)) = rest.split_once('`')
    {
        let prefix = detail
            .split_once(": ")
            .map(|(path, _)| path)
            .filter(|path| !path.contains(' ') && *path != ".");
        let path = prefix.map_or_else(|| name.to_string(), |prefix| format!("{prefix}.{name}"));
        return Some(ApiErrorField {
            path,
            message: format!("missing field `{name}`"),
        });
    }
    let (path, reason) = detail.split_once(": ")?;
    (!path.is_empty() && !path.contains(' ')).then(|| ApiErrorField {
        path: path.to_string(),
        message: reason
            .split(" at line ")
            .next()
            .unwrap_or(reason)
            .to_string(),
    })
}

/// Convert axum's plain-text extractor rejections into the API error
/// envelope, naming the rejected input in `fields` when axum says which.
pub async fn normalize_extractor_rejection(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_plain_text = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/plain"));

    if !is_plain_text
        || !matches!(
            status,
            StatusCode::BAD_REQUEST
                | StatusCode::UNSUPPORTED_MEDIA_TYPE
                | StatusCode::UNPROCESSABLE_ENTITY
                | StatusCode::PAYLOAD_TOO_LARGE
        )
    {
        return response;
    }

    if status == StatusCode::PAYLOAD_TOO_LARGE {
        return crate::util::api_err(status, "payload_too_large", "Request body is too large")
            .into_response();
    }

    let message = to_bytes(response.into_body(), MAX_ERROR_BODY_BYTES)
        .await
        .ok()
        .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
        .unwrap_or_default();
    let mut error = ApiError::new("invalid_request", "Request parameters or body are invalid");
    if let Some(field) = rejected_field(&message) {
        error = error.with_fields(vec![field]);
    }
    (StatusCode::BAD_REQUEST, Json(error)).into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn rejection_messages_name_the_field() {
        let missing = rejected_field(
            "Failed to deserialize the JSON body into the target type: missing field `name` at line 1 column 2",
        )
        .unwrap();
        assert_eq!(missing.path, "name");
        let nested = rejected_field(
            "Failed to deserialize the JSON body into the target type: steps[0].run: invalid type: integer `1`, expected a string at line 1 column 20",
        )
        .unwrap();
        assert_eq!(nested.path, "steps[0].run");
        assert_eq!(
            nested.message,
            "invalid type: integer `1`, expected a string"
        );
        let query = rejected_field(
            "Failed to deserialize query string: limit: invalid digit found in string",
        )
        .unwrap();
        assert_eq!(query.path, "limit");
        assert!(rejected_field("Expected request with `Content-Type: application/json`").is_none());
    }

    #[test]
    fn catalog_codes_are_unique() {
        let mut seen = BTreeSet::new();
        for entry in CATALOG {
            assert!(seen.insert(entry.code), "{} is listed twice", entry.code);
        }
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// Every `api_err(StatusCode::X, "code", ...)` in the daemon, with the
    /// statuses each code is sent with.
    fn codes_in_handlers() -> BTreeMap<String, BTreeSet<u16>> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        let mut codes: BTreeMap<String, BTreeSet<u16>> = BTreeMap::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let mut rest = text.as_str();
            while let Some(start) = rest.find("api_err(") {
                rest = &rest[start + "api_err(".len()..];
                let args = rest.trim_start();
                let args = args.trim_prefix("axum::http::");
                let Some(args) = args.strip_prefix("StatusCode::") else {
                    continue;
                };
                let name_end = args
                    .find(|c: char| !(c.is_ascii_uppercase() || c == '_'))
                    .unwrap_or(args.len());
                let status = status_by_name(&args[..name_end]);
                let after = args[name_end..].trim_start();
                let Some(after) = after.strip_prefix(',') else {
                    continue;
                };
                let Some(after) = after.trim_start().strip_prefix('"') else {
                    continue;
                };
                let Some(end) = after.find('"') else { continue };
                let code = &after[..end];
                if let Some(status) = status
                    && !code.is_empty()
                    && code
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    codes.entry(code.to_string()).or_default().insert(status);
                }
            }
        }
        // Answered by the extractor and body-limit layers rather than a handler.
        codes
            .entry("invalid_request".into())
            .or_default()
            .insert(400);
        codes
            .entry("payload_too_large".into())
            .or_default()
            .insert(413);
        codes
    }

    fn status_by_name(name: &str) -> Option<u16> {
        Some(match name {
            "BAD_REQUEST" => 400,
            "UNAUTHORIZED" => 401,
            "FORBIDDEN" => 403,
            "NOT_FOUND" => 404,
            "CONFLICT" => 409,
            "GONE" => 410,
            "PRECONDITION_FAILED" => 412,
            "PAYLOAD_TOO_LARGE" => 413,
            "UNSUPPORTED_MEDIA_TYPE" => 415,
            "UNPROCESSABLE_ENTITY" => 422,
            "TOO_MANY_REQUESTS" => 429,
            "INTERNAL_SERVER_ERROR" => 500,
            "NOT_IMPLEMENTED" => 501,
            "BAD_GATEWAY" => 502,
            "SERVICE_UNAVAILABLE" => 503,
            "GATEWAY_TIMEOUT" => 504,
            _ => return None,
        })
    }

    const START: &str = "{/* error-codes:start — generated by `make gen-error-codes` */}";
    const END: &str = "{/* error-codes:end */}";

    /// The code table, padded the way `oxfmt` formats Markdown tables.
    fn code_table() -> String {
        let mut rows: Vec<[String; 4]> = vec![[
            "Code".into(),
            "Status".into(),
            "Retry".into(),
            "What to do".into(),
        ]];
        for (code, statuses) in codes_in_handlers() {
            let status_list = statuses
                .iter()
                .map(|status| format!("`{status}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let advice = advice_for(&code);
            let retry = advice.map_or_else(
                || {
                    statuses
                        .iter()
                        .any(|status| retryable_by_status(StatusCode::from_u16(*status).unwrap()))
                },
                |advice| advice.retryable,
            );
            let hint = advice.map_or("", |advice| advice.hint).replace('|', "\\|");
            rows.push([
                format!("`{code}`"),
                status_list,
                if retry { "yes" } else { "no" }.to_string(),
                hint,
            ]);
        }
        let widths: Vec<usize> = (0..4)
            .map(|column| {
                rows.iter()
                    .map(|row| row[column].chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(3)
            })
            .collect();
        let line = |cells: Vec<String>| {
            let padded: Vec<String> = cells
                .iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell}{}", " ".repeat(width - cell.chars().count())))
                .collect();
            format!("| {} |\n", padded.join(" | "))
        };
        let mut table = line(rows[0].to_vec());
        table.push_str(&line(widths.iter().map(|w| "-".repeat(*w)).collect()));
        for row in &rows[1..] {
            table.push_str(&line(row.to_vec()));
        }
        table
    }

    #[test]
    fn every_catalog_code_is_sent_by_a_handler() {
        let codes = codes_in_handlers();
        for entry in CATALOG {
            assert!(
                codes.contains_key(entry.code),
                "{} has advice but no handler sends it",
                entry.code
            );
        }
    }

    /// Keeps the docs' code table equal to the handlers. Run
    /// `make gen-error-codes` to rewrite it.
    #[test]
    fn docs_error_code_table_is_current() {
        let page = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/docs/content/docs/reference/(product-values)/errors.mdx");
        let text = std::fs::read_to_string(&page).unwrap();
        let start = text.find(START).expect("errors.mdx has the start marker");
        let end = text.find(END).expect("errors.mdx has the end marker");
        let generated = format!("{START}\n\n{}\n{END}", code_table());
        let current = &text[start..end + END.len()];
        if current == generated {
            return;
        }
        if std::env::var_os("OORE_UPDATE_ERROR_CODES").is_some() {
            let updated = format!("{}{generated}{}", &text[..start], &text[end + END.len()..]);
            std::fs::write(&page, updated).unwrap();
            return;
        }
        panic!("the error code table in errors.mdx is stale; run make gen-error-codes");
    }
}
