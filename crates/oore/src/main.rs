use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use chrono::{Local, TimeZone};
use clap::{Args, Parser, Subcommand};
use oore_contract::{
    ApiError, BootstrapTokenRecord, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse,
    BuildStatus, ClaimJobResponse, ClaimedJob, JobStatusResponse, OidcConfigureRequest,
    OidcConfigureResponse, RegisterRunnerResponse, SetupCompleteResponse, SetupOidcStartRequest,
    SetupOidcStartResponse, SetupOidcVerifyRequest, SetupOidcVerifyResponse, SetupState,
    SetupStateFile, SetupStatus, StepResult,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const FAVICON_DATA_URI: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAzMiAzMiI+CiAgPGRlZnM+CiAgICA8Y2lyY2xlIGlkPSJjdXQiIGN4PSIxNiIgY3k9IjE2IiByPSI3IiAvPgogICAgPG1hc2sgaWQ9ImhvbGUiPgogICAgICA8cmVjdCB4PSIwIiB5PSIwIiB3aWR0aD0iMzIiIGhlaWdodD0iMzIiIGZpbGw9IndoaXRlIiAvPgogICAgICA8dXNlIGhyZWY9IiNjdXQiIGZpbGw9ImJsYWNrIiAvPgogICAgPC9tYXNrPgogICAgPGNsaXBQYXRoIGlkPSJsZWZ0Ij4KICAgICAgPHJlY3QgeD0iMCIgeT0iMCIgd2lkdGg9IjE1IiBoZWlnaHQ9IjMyIiAvPgogICAgPC9jbGlwUGF0aD4KICAgIDxjbGlwUGF0aCBpZD0icmlnaHQiPgogICAgICA8cmVjdCB4PSIxNyIgeT0iMCIgd2lkdGg9IjE1IiBoZWlnaHQ9IjMyIiAvPgogICAgPC9jbGlwUGF0aD4KICA8L2RlZnM+CiAgPHJlY3QKICAgIHg9IjIiCiAgICB5PSIyIgogICAgd2lkdGg9IjI4IgogICAgaGVpZ2h0PSIyOCIKICAgIHJ4PSI2IgogICAgZmlsbD0iI2Y0OWYxZSIKICAgIGNsaXAtcGF0aD0idXJsKCNsZWZ0KSIKICAgIG1hc2s9InVybCgjaG9sZSkiCiAgLz4KICA8cmVjdAogICAgeD0iMiIKICAgIHk9IjIiCiAgICB3aWR0aD0iMjgiCiAgICBoZWlnaHQ9IjI4IgogICAgcng9IjYiCiAgICBmaWxsPSIjZjQ5ZjFlIgogICAgY2xpcC1wYXRoPSJ1cmwoI3JpZ2h0KSIKICAgIG1hc2s9InVybCgjaG9sZSkiCiAgLz4KPC9zdmc+Cg==";

#[derive(Debug, Parser)]
#[command(name = "oore")]
#[command(about = "oore operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Setup(SetupArgs),
    Login,
    Status,
    Runner(RunnerArgs),
    Config(ConfigArgs),
    Doctor,
}

#[derive(Debug, Args)]
struct SetupArgs {
    #[arg(long, env = "OORE_DAEMON_URL", default_value = "http://127.0.0.1:8787")]
    daemon_url: String,

    #[command(subcommand)]
    command: Option<SetupSubcommand>,
}

#[derive(Debug, Subcommand)]
enum SetupSubcommand {
    Open(SetupOpenArgs),
}

#[derive(Debug, Args)]
struct SetupOpenArgs {
    #[arg(long, default_value = "15m")]
    ttl: String,

    #[arg(long, default_value = "false")]
    json: bool,

    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
}

#[derive(Debug, Args)]
struct RunnerArgs {
    #[command(subcommand)]
    command: RunnerSubcommand,
}

#[derive(Debug, Subcommand)]
enum RunnerSubcommand {
    /// Register this host as a build runner
    Register(RunnerRegisterArgs),
    /// Start the runner daemon (poll for jobs, execute builds)
    Start(RunnerStartArgs),
}

#[derive(Debug, Args)]
struct RunnerRegisterArgs {
    /// Daemon URL to register with
    #[arg(long, env = "OORE_DAEMON_URL", default_value = "http://127.0.0.1:8787")]
    daemon_url: String,
    /// Runner name (defaults to hostname)
    #[arg(long)]
    name: Option<String>,
    /// Auth session token for registration
    #[arg(long, env = "OORE_SESSION_TOKEN")]
    token: String,
}

#[derive(Debug, Args)]
struct RunnerStartArgs {
    /// Daemon URL (loaded from config if not specified)
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,
    /// Path to runner config file
    #[arg(long, default_value = "~/.oore/runner.json")]
    config: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct RunnerConfig {
    runner_id: String,
    runner_token: String,
    daemon_url: String,
    name: String,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
enum ConfigSubcommand {
    Set(ConfigSetArgs),
    Get(ConfigGetArgs),
}

#[derive(Debug, Args)]
struct ConfigSetArgs {
    key: String,
    value: String,
}

#[derive(Debug, Args)]
struct ConfigGetArgs {
    key: String,
}

fn parse_ttl(raw: &str) -> anyhow::Result<Duration> {
    humantime::parse_duration(raw).with_context(|| format!("invalid ttl value: {raw}"))
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn resolve_db_path(override_path: Option<&str>) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(PathBuf::from(p));
    }

    if let Ok(p) = std::env::var("OORE_SETUP_STATE_FILE") {
        return Ok(PathBuf::from(p));
    }

    let data_dir = dirs::data_dir()
        .context("could not determine platform data directory (dirs::data_dir)")?;
    Ok(data_dir.join("oore").join("oore.db"))
}

// ── SQLite state helpers ────────────────────────────────────────

const CREATE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS setup_state (
    id                          INTEGER PRIMARY KEY CHECK (id = 1) DEFAULT 1,
    schema_version              INTEGER NOT NULL DEFAULT 1,
    instance_id                 TEXT    NOT NULL,
    setup_state                 TEXT    NOT NULL DEFAULT 'bootstrap_pending',
    bootstrap_token_hash        TEXT,
    bootstrap_token_expires_at  INTEGER,
    bootstrap_token_consumed_at INTEGER,
    session_hash                TEXT,
    session_expires_at          INTEGER,
    oidc_issuer_url             TEXT,
    oidc_client_id              TEXT,
    oidc_has_client_secret      INTEGER,
    oidc_authorization_endpoint TEXT,
    oidc_token_endpoint         TEXT,
    oidc_userinfo_endpoint      TEXT,
    oidc_jwks_uri               TEXT,
    oidc_configured_at          INTEGER,
    oidc_encrypted_client_secret TEXT,
    oidc_secret_stored_at        INTEGER,
    owner_email                 TEXT,
    owner_oidc_subject          TEXT,
    owner_created_at            INTEGER,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
)
"#;

async fn connect_db(path: &PathBuf) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to connect to database: {}", path.display()))?;

    // Create table if not exists
    sqlx::query(CREATE_TABLE_SQL)
        .execute(&pool)
        .await
        .context("failed to create setup_state table")?;

    Ok(pool)
}

fn setup_state_to_str(state: SetupState) -> &'static str {
    match state {
        SetupState::Uninitialized => "uninitialized",
        SetupState::BootstrapPending => "bootstrap_pending",
        SetupState::IdpConfigured => "idp_configured",
        SetupState::OwnerCreated => "owner_created",
        SetupState::Ready => "ready",
        _ => "unknown",
    }
}

fn str_to_setup_state(s: &str) -> anyhow::Result<SetupState> {
    match s {
        "uninitialized" => Ok(SetupState::Uninitialized),
        "bootstrap_pending" => Ok(SetupState::BootstrapPending),
        "idp_configured" => Ok(SetupState::IdpConfigured),
        "owner_created" => Ok(SetupState::OwnerCreated),
        "ready" => Ok(SetupState::Ready),
        other => anyhow::bail!("unknown setup state: {other}"),
    }
}

async fn load_state(pool: &SqlitePool) -> anyhow::Result<Option<SetupStateFile>> {
    let row = sqlx::query("SELECT * FROM setup_state WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("failed to query setup_state")?;

    match row {
        None => Ok(None),
        Some(row) => {
            let schema_version: i64 = row.try_get("schema_version")?;
            let instance_id: String = row.try_get("instance_id")?;
            let state_str: String = row.try_get("setup_state")?;
            let setup_state = str_to_setup_state(&state_str)?;

            let bootstrap_token = {
                let hash: Option<String> = row.try_get("bootstrap_token_hash")?;
                hash.map(|hash| {
                    let expires_at: i64 =
                        row.try_get("bootstrap_token_expires_at").unwrap_or(0);
                    let consumed_at: Option<i64> =
                        row.try_get("bootstrap_token_consumed_at").unwrap_or(None);
                    BootstrapTokenRecord {
                        hash,
                        expires_at,
                        consumed_at,
                    }
                })
            };

            let setup_session = {
                let hash: Option<String> = row.try_get("session_hash")?;
                hash.map(|hash| {
                    let expires_at: i64 = row.try_get("session_expires_at").unwrap_or(0);
                    oore_contract::SetupSessionRecord { hash, expires_at }
                })
            };

            let oidc_config = {
                let issuer_url: Option<String> = row.try_get("oidc_issuer_url")?;
                issuer_url.map(|issuer_url| oore_contract::OidcConfigRecord {
                    issuer_url,
                    client_id: row.try_get("oidc_client_id").unwrap_or_default(),
                    has_client_secret: row
                        .try_get::<i32, _>("oidc_has_client_secret")
                        .unwrap_or(0)
                        != 0,
                    authorization_endpoint: row
                        .try_get("oidc_authorization_endpoint")
                        .unwrap_or_default(),
                    token_endpoint: row.try_get("oidc_token_endpoint").unwrap_or_default(),
                    userinfo_endpoint: row.try_get("oidc_userinfo_endpoint").unwrap_or(None),
                    jwks_uri: row.try_get("oidc_jwks_uri").unwrap_or_default(),
                    configured_at: row.try_get("oidc_configured_at").unwrap_or(0),
                })
            };

            let oidc_secret = {
                let encrypted: Option<String> = row.try_get("oidc_encrypted_client_secret")?;
                encrypted.map(|encrypted_client_secret| oore_contract::OidcSecretRecord {
                    encrypted_client_secret,
                    stored_at: row.try_get("oidc_secret_stored_at").unwrap_or(0),
                })
            };

            let owner = {
                let email: Option<String> = row.try_get("owner_email")?;
                email.map(|email| oore_contract::OwnerRecord {
                    email,
                    oidc_subject: row.try_get("owner_oidc_subject").unwrap_or(None),
                    created_at: row.try_get("owner_created_at").unwrap_or(0),
                })
            };

            let created_at: i64 = row.try_get("created_at")?;
            let updated_at: i64 = row.try_get("updated_at")?;

            Ok(Some(SetupStateFile {
                schema_version: u32::try_from(schema_version)
                    .context("schema_version out of u32 range")?,
                instance_id,
                setup_state,
                bootstrap_token,
                setup_session,
                oidc_config,
                oidc_secret,
                owner,
                created_at,
                updated_at,
            }))
        }
    }
}

async fn save_state(pool: &SqlitePool, state: &SetupStateFile) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO setup_state (
            id, schema_version, instance_id, setup_state,
            bootstrap_token_hash, bootstrap_token_expires_at, bootstrap_token_consumed_at,
            session_hash, session_expires_at,
            oidc_issuer_url, oidc_client_id, oidc_has_client_secret,
            oidc_authorization_endpoint, oidc_token_endpoint,
            oidc_userinfo_endpoint, oidc_jwks_uri, oidc_configured_at,
            oidc_encrypted_client_secret, oidc_secret_stored_at,
            owner_email, owner_oidc_subject, owner_created_at,
            created_at, updated_at
        ) VALUES (
            1, ?1, ?2, ?3,
            ?4, ?5, ?6,
            ?7, ?8,
            ?9, ?10, ?11,
            ?12, ?13,
            ?14, ?15, ?16,
            ?17, ?18,
            ?19, ?20, ?21,
            ?22, ?23
        )
        "#,
    )
    .bind(state.schema_version as i64)
    .bind(&state.instance_id)
    .bind(setup_state_to_str(state.setup_state))
    .bind(state.bootstrap_token.as_ref().map(|t| &t.hash))
    .bind(state.bootstrap_token.as_ref().map(|t| t.expires_at))
    .bind(state.bootstrap_token.as_ref().and_then(|t| t.consumed_at))
    .bind(state.setup_session.as_ref().map(|s| &s.hash))
    .bind(state.setup_session.as_ref().map(|s| s.expires_at))
    .bind(state.oidc_config.as_ref().map(|c| &c.issuer_url))
    .bind(state.oidc_config.as_ref().map(|c| &c.client_id))
    .bind(state.oidc_config.as_ref().map(|c| c.has_client_secret as i32))
    .bind(state.oidc_config.as_ref().map(|c| &c.authorization_endpoint))
    .bind(state.oidc_config.as_ref().map(|c| &c.token_endpoint))
    .bind(state.oidc_config.as_ref().and_then(|c| c.userinfo_endpoint.as_ref()))
    .bind(state.oidc_config.as_ref().map(|c| &c.jwks_uri))
    .bind(state.oidc_config.as_ref().map(|c| c.configured_at))
    .bind(state.oidc_secret.as_ref().map(|s| &s.encrypted_client_secret))
    .bind(state.oidc_secret.as_ref().map(|s| s.stored_at))
    .bind(state.owner.as_ref().map(|o| &o.email))
    .bind(state.owner.as_ref().and_then(|o| o.oidc_subject.as_ref()))
    .bind(state.owner.as_ref().map(|o| o.created_at))
    .bind(state.created_at)
    .bind(state.updated_at)
    .execute(pool)
    .await
    .context("failed to save setup state")?;

    Ok(())
}

async fn load_or_create_state(pool: &SqlitePool) -> anyhow::Result<SetupStateFile> {
    if let Some(state) = load_state(pool).await? {
        return Ok(state);
    }

    let now = now_epoch_secs();
    let state = SetupStateFile {
        schema_version: SetupStateFile::CURRENT_SCHEMA_VERSION,
        instance_id: uuid::Uuid::new_v4().to_string(),
        setup_state: SetupState::BootstrapPending,
        bootstrap_token: None,
        setup_session: None,
        oidc_config: None,
        oidc_secret: None,
        owner: None,
        created_at: now,
        updated_at: now,
    };

    save_state(pool, &state).await?;
    Ok(state)
}

// ── End SQLite helpers ──────────────────────────────────────────

fn format_epoch_local(epoch: i64) -> String {
    Local::now()
        .timezone()
        .timestamp_opt(epoch, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| epoch.to_string())
}

fn format_ttl_human(ttl: &Duration) -> String {
    let total_secs = ttl.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{}s", seconds));
    }
    parts.join("")
}

/// Generate a bootstrap token and save it to the database.
/// Returns the plaintext token.
async fn generate_bootstrap_token(
    state: &mut SetupStateFile,
    pool: &SqlitePool,
    ttl: Duration,
) -> anyhow::Result<String> {
    let mut token_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut token_bytes);
    let plaintext_token = hex::encode(token_bytes);

    let token_hash = hex::encode(Sha256::digest(plaintext_token.as_bytes()));
    let expires_at = now_epoch_secs() + ttl.as_secs() as i64;

    state.bootstrap_token = Some(BootstrapTokenRecord {
        hash: token_hash,
        expires_at,
        consumed_at: None,
    });
    state.updated_at = now_epoch_secs();

    save_state(pool, state).await?;

    Ok(plaintext_token)
}

async fn handle_setup_open(args: SetupOpenArgs) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;

    // 1. Resolve database path
    let db_path = resolve_db_path(args.state_file.as_deref())?;

    // 2. Connect and load or create state
    let pool = connect_db(&db_path).await?;
    let mut state = load_or_create_state(&pool).await?;

    // 3. Validate state — if Ready, error out
    if state.setup_state == SetupState::Ready {
        eprintln!("Setup is already complete. Instance is in 'ready' state.");
        std::process::exit(1);
    }

    // 4. Generate bootstrap token
    let plaintext_token = generate_bootstrap_token(&mut state, &pool, ttl).await?;

    // 5. Calculate expiry (for display — token record already has it)
    let expires_at = state.bootstrap_token.as_ref().unwrap().expires_at;

    // 6. Output
    let state_display = match state.setup_state {
        SetupState::Uninitialized => "uninitialized",
        SetupState::BootstrapPending => "bootstrap_pending",
        SetupState::IdpConfigured => "idp_configured",
        SetupState::OwnerCreated => "owner_created",
        SetupState::Ready => unreachable!("ready state is rejected above"),
        _ => "unknown",
    };
    let db_display = db_path.display();

    if args.json {
        let output = serde_json::json!({
            "token": plaintext_token,
            "expires_at": expires_at,
            "state": state_display,
            "database": db_display.to_string(),
            "instance_id": state.instance_id,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let ttl_display = format_ttl_human(&ttl);
        println!("Bootstrap token generated.");
        println!();
        println!("Token:   {}", plaintext_token);
        println!("Expires: {} ({} from now)", format_epoch_local(expires_at), ttl_display);
        println!("State:   {}", state_display);
        println!("DB:      {}", db_display);
        println!();
        println!("To complete setup, either:");
        println!("  1. Open http://localhost:3000/setup in your browser and paste this token");
        println!("  2. Run: oore setup");
    }

    Ok(())
}

// ── OIDC loopback helpers ───────────────────────────────────────

/// Open a URL in the default browser (macOS-only in V1).
fn open_browser(url: &str) -> bool {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .is_ok()
}

/// Accept a single HTTP request on the listener, extract OIDC callback params.
/// Returns `(code, state)` on success.
async fn wait_for_oidc_callback(listener: TcpListener) -> anyhow::Result<(String, String)> {
    let (mut stream, _addr) = listener
        .accept()
        .await
        .context("failed to accept OIDC callback connection")?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .context("failed to read OIDC callback request")?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the GET request line: "GET /path?query HTTP/1.1"
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/");

    // Parse query params using url crate
    let fake_base = format!("http://localhost{}", path);
    let parsed = url::Url::parse(&fake_base)
        .context("failed to parse callback URL")?;
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

    // Check for OIDC error
    if let Some(error) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(|d| format!(": {}", d))
            .unwrap_or_default();
        let error_page = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
            <!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
            <link rel=\"icon\" href=\"{favicon}\"><link rel=\"apple-touch-icon\" href=\"{favicon}\">\
            <meta name=\"theme-color\" content=\"#f49f1e\"><title>Authentication failed</title></head>\
            <body><h2>Authentication failed</h2><p>{error}{desc}</p><p>You can close this tab.</p></body></html>",
            favicon = FAVICON_DATA_URI,
            error = error,
            desc = desc,
        );
        stream.write_all(error_page.as_bytes()).await.ok();
        stream.shutdown().await.ok();
        anyhow::bail!("OIDC authentication error: {}{}", error, desc);
    }

    let code = params
        .get("code")
        .cloned()
        .context("OIDC callback missing 'code' parameter")?;
    let state = params
        .get("state")
        .cloned()
        .context("OIDC callback missing 'state' parameter")?;

    // Send success page
    let success_page = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
        <!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
        <link rel=\"icon\" href=\"{favicon}\"><link rel=\"apple-touch-icon\" href=\"{favicon}\">\
        <meta name=\"theme-color\" content=\"#f49f1e\"><title>Authentication successful</title></head>\
        <body><h2>Authentication successful</h2><p>You can close this tab and return to the terminal.</p></body></html>",
        favicon = FAVICON_DATA_URI,
    );
    stream.write_all(success_page.as_bytes()).await.ok();
    stream.shutdown().await.ok();

    Ok((code, state))
}

// ── Interactive setup flow ──────────────────────────────────────

fn state_label(state: SetupState) -> &'static str {
    match state {
        SetupState::Uninitialized => "uninitialized",
        SetupState::BootstrapPending => "bootstrap_pending",
        SetupState::IdpConfigured => "idp_configured",
        SetupState::OwnerCreated => "owner_created",
        SetupState::Ready => "ready",
        _ => "unknown",
    }
}

/// Try to parse a daemon error response body into an ApiError.
/// Falls back to the raw body text if parsing fails.
async fn extract_error_message(resp: reqwest::Response) -> String {
    let status = resp.status();
    match resp.text().await {
        Ok(body) => {
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                let mut msg = api_err.error;
                if let Some(details) = api_err.details {
                    msg = format!("{}: {}", msg, details);
                }
                msg
            } else if body.is_empty() {
                format!("HTTP {status}")
            } else {
                body
            }
        }
        Err(_) => format!("HTTP {status}"),
    }
}

async fn handle_setup_interactive(daemon_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    println!("oore setup — interactive instance configuration");
    println!();

    // ── Step 0: Check daemon connectivity and get current state ──

    let status_url = format!("{}/v1/public/setup-status", daemon_url);
    let status_resp = client.get(&status_url).send().await;

    let status: SetupStatus = match status_resp {
        Ok(resp) if resp.status().is_success() => {
            resp.json().await.context("failed to parse setup-status response")?
        }
        Ok(resp) => {
            let msg = extract_error_message(resp).await;
            anyhow::bail!("Cannot reach oored at {daemon_url}: {msg}");
        }
        Err(e) => {
            eprintln!(
                "Cannot reach oored at {}. Is the daemon running? Start it with: oored run",
                daemon_url
            );
            return Err(e.into());
        }
    };

    println!("Connected to oored at {}", daemon_url);
    println!("Instance:  {}", status.instance_id);
    println!("State:     {}", state_label(status.state));
    println!();

    // If already ready, nothing to do
    if status.state == SetupState::Ready {
        println!("Setup is already complete. Instance is in ready state.");
        return Ok(());
    }

    let mut current_state = status.state;
    let mut session_token: Option<String> = None;

    // ── Step 1: Bootstrap token verification ────────────────────

    if current_state == SetupState::BootstrapPending || current_state == SetupState::Uninitialized {
        println!("[Step 1/4] Bootstrap token verification");
        println!();

        // Load the local state from the database
        let db_path = resolve_db_path(None)?;
        println!("  Database: {}", db_path.display());
        let pool = connect_db(&db_path).await?;
        let mut local_state = load_or_create_state(&pool).await?;

        let plaintext_token = {
            println!("  Generating bootstrap token (TTL: 15m)...");
            generate_bootstrap_token(&mut local_state, &pool, Duration::from_secs(15 * 60)).await?
        };

        // Verify the token against the daemon
        println!("  Verifying token with daemon...");
        let verify_url = format!("{}/v1/setup/bootstrap-token/verify", daemon_url);
        let verify_resp = client
            .post(&verify_url)
            .json(&BootstrapTokenVerifyRequest {
                token: plaintext_token,
            })
            .send()
            .await
            .context("failed to reach daemon for token verification")?;

        if verify_resp.status().is_success() {
            let body: BootstrapTokenVerifyResponse = verify_resp
                .json()
                .await
                .context("failed to parse bootstrap verify response")?;
            session_token = Some(body.session_token);
            current_state = SetupState::BootstrapPending; // daemon is now ready for OIDC
            println!();
            println!("  \u{2713} Bootstrap verified. Session token acquired.");
        } else {
            let status_code = verify_resp.status();
            let msg = extract_error_message(verify_resp).await;
            if status_code == reqwest::StatusCode::CONFLICT {
                anyhow::bail!("Setup is already complete. Instance is in ready state.");
            } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                anyhow::bail!("Bootstrap token is invalid. Please regenerate with: oore setup open");
            } else {
                anyhow::bail!("Bootstrap verification failed (HTTP {}): {}", status_code.as_u16(), msg);
            }
        }
        println!();
    }

    // ── Step 2: OIDC Configuration ──────────────────────────────

    if current_state == SetupState::BootstrapPending {
        println!("[Step 2/4] OIDC provider configuration");
        println!();

        // Ensure we have a session token (if resuming, we might not)
        let token = session_token.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Session expired or invalid. Please restart setup.")
        })?;

        loop {
            let issuer_url: String = dialoguer::Input::new()
                .with_prompt("  OIDC Issuer URL")
                .interact_text()
                .context("failed to read issuer URL")?;

            let client_id: String = dialoguer::Input::new()
                .with_prompt("  Client ID")
                .interact_text()
                .context("failed to read client ID")?;

            let client_secret: String = dialoguer::Password::new()
                .with_prompt("  Client Secret (optional, press Enter to skip)")
                .allow_empty_password(true)
                .interact()
                .context("failed to read client secret")?;

            let client_secret_opt = if client_secret.is_empty() {
                None
            } else {
                Some(client_secret)
            };

            println!();
            println!("  Configuring OIDC provider...");

            let oidc_url = format!("{}/v1/setup/oidc/configure", daemon_url);
            let oidc_resp = client
                .post(&oidc_url)
                .bearer_auth(token)
                .json(&OidcConfigureRequest {
                    issuer_url,
                    client_id,
                    client_secret: client_secret_opt,
                })
                .send()
                .await
                .context("failed to reach daemon for OIDC configuration")?;

            if oidc_resp.status().is_success() {
                let body: OidcConfigureResponse = oidc_resp
                    .json()
                    .await
                    .context("failed to parse OIDC configure response")?;
                current_state = SetupState::IdpConfigured;
                println!("  \u{2713} OIDC provider configured. Issuer: {}", body.discovered_issuer);
                println!();
                break;
            } else {
                let status_code = oidc_resp.status();
                let msg = extract_error_message(oidc_resp).await;
                if status_code == reqwest::StatusCode::CONFLICT {
                    // Check if already configured — might have been done in a previous run
                    println!("  OIDC is already configured, advancing to next step.");
                    current_state = SetupState::IdpConfigured;
                    println!();
                    break;
                } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                    anyhow::bail!("Session expired or invalid. Please restart setup.");
                } else {
                    eprintln!("  Error: {}", msg);
                    let retry = dialoguer::Confirm::new()
                        .with_prompt("  Retry OIDC configuration?")
                        .default(true)
                        .interact()
                        .unwrap_or(false);
                    if !retry {
                        anyhow::bail!("OIDC configuration aborted by user.");
                    }
                    println!();
                }
            }
        }
    } else if current_state == SetupState::IdpConfigured {
        println!("[Step 2/4] OIDC provider configuration");
        println!("  Already configured, skipping.");
        println!();
    }

    // ── Step 3: Owner OIDC authentication ─────────────────────

    if current_state == SetupState::IdpConfigured {
        println!("[Step 3/4] Owner account setup");
        println!();

        let token = session_token.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Session expired or invalid. Please restart setup.")
        })?;

        // Bind to a random free port
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("failed to bind loopback listener for OIDC callback")?;
        let local_port = listener
            .local_addr()
            .context("failed to get listener address")?
            .port();
        let redirect_uri = format!("http://localhost:{}", local_port);

        println!("  You'll authenticate via your OIDC provider to prove your identity.");
        println!();
        println!("  Before continuing, ensure this redirect URI is whitelisted");
        println!("  in your OIDC provider's allowed callback URLs:");
        println!();
        println!("    {}", redirect_uri);
        println!();

        let confirm = dialoguer::Confirm::new()
            .with_prompt("  Continue with OIDC authentication?")
            .default(false)
            .interact()
            .unwrap_or(false);

        if !confirm {
            println!();
            println!("Setup paused. You can resume later with: oore setup");
            return Ok(());
        }

        println!();
        println!("  Starting OIDC flow...");

        // Call start-oidc
        let start_url = format!("{}/v1/setup/owner/start-oidc", daemon_url);
        let start_resp = client
            .post(&start_url)
            .bearer_auth(token)
            .json(&SetupOidcStartRequest {
                redirect_uri: redirect_uri.clone(),
            })
            .send()
            .await
            .context("failed to reach daemon for OIDC start")?;

        if start_resp.status().is_success() {
            let start_body: SetupOidcStartResponse = start_resp
                .json()
                .await
                .context("failed to parse OIDC start response")?;

            // Open browser
            if !open_browser(&start_body.authorization_url) {
                println!();
                println!("  Could not open browser automatically.");
                println!("  Please open the following URL manually:");
                println!();
                println!("    {}", start_body.authorization_url);
                println!();
            }

            println!("  Waiting for authentication callback...");

            // Wait for the OIDC provider to redirect back
            let (code, callback_state) = wait_for_oidc_callback(listener).await?;

            println!("  Verifying identity...");

            // Call verify-oidc
            let verify_url = format!("{}/v1/setup/owner/verify-oidc", daemon_url);
            let verify_resp = client
                .post(&verify_url)
                .bearer_auth(token)
                .json(&SetupOidcVerifyRequest {
                    code,
                    state: callback_state,
                })
                .send()
                .await
                .context("failed to reach daemon for OIDC verification")?;

            if verify_resp.status().is_success() {
                let body: SetupOidcVerifyResponse = verify_resp
                    .json()
                    .await
                    .context("failed to parse OIDC verify response")?;
                current_state = SetupState::OwnerCreated;
                println!(
                    "  \u{2713} Owner verified: {} (sub: {})",
                    body.owner_email, body.oidc_subject
                );
                println!();
            } else {
                let status_code = verify_resp.status();
                let msg = extract_error_message(verify_resp).await;
                if status_code == reqwest::StatusCode::CONFLICT {
                    println!("  Owner is already set, advancing to next step.");
                    current_state = SetupState::OwnerCreated;
                    println!();
                } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                    anyhow::bail!("Session expired or invalid. Please restart setup.");
                } else {
                    anyhow::bail!(
                        "OIDC verification failed (HTTP {}): {}",
                        status_code.as_u16(),
                        msg
                    );
                }
            }
        } else {
            let status_code = start_resp.status();
            let msg = extract_error_message(start_resp).await;
            if status_code == reqwest::StatusCode::CONFLICT {
                println!("  Owner is already set, advancing to next step.");
                current_state = SetupState::OwnerCreated;
                println!();
            } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                anyhow::bail!("Session expired or invalid. Please restart setup.");
            } else {
                anyhow::bail!(
                    "OIDC start failed (HTTP {}): {}",
                    status_code.as_u16(),
                    msg
                );
            }
        }
    } else if current_state == SetupState::OwnerCreated {
        println!("[Step 3/4] Owner account setup");
        println!("  Already configured, skipping.");
        println!();
    }

    // ── Step 4: Complete setup ──────────────────────────────────

    if current_state == SetupState::OwnerCreated {
        println!("[Step 4/4] Finalize setup");
        println!();

        let token = session_token.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Session expired or invalid. Please restart setup.")
        })?;

        let confirm = dialoguer::Confirm::new()
            .with_prompt("  Complete setup? This will lock all setup endpoints.")
            .default(false)
            .interact()
            .unwrap_or(false);

        if !confirm {
            println!();
            println!("Setup not finalized. You can resume later with: oore setup");
            return Ok(());
        }

        println!();
        println!("  Completing setup...");

        let complete_url = format!("{}/v1/setup/complete", daemon_url);
        let complete_resp = client
            .post(&complete_url)
            .bearer_auth(token)
            .send()
            .await
            .context("failed to reach daemon for setup completion")?;

        if complete_resp.status().is_success() {
            let body: SetupCompleteResponse = complete_resp
                .json()
                .await
                .context("failed to parse setup complete response")?;
            println!("  \u{2713} Setup complete! Instance ID: {}", body.instance_id);
            println!();
            println!("Your oore.build instance is ready. Run 'oore status' to verify.");
        } else {
            let status_code = complete_resp.status();
            let msg = extract_error_message(complete_resp).await;
            if status_code == reqwest::StatusCode::CONFLICT {
                println!("  Setup is already complete. Instance is in ready state.");
            } else if status_code == reqwest::StatusCode::UNAUTHORIZED {
                anyhow::bail!("Session expired or invalid. Please restart setup.");
            } else {
                anyhow::bail!("Setup completion failed (HTTP {}): {}", status_code.as_u16(), msg);
            }
        }
    }

    Ok(())
}

// ── Runner execution engine ─────────────────────────────────────

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn detect_capabilities() -> serde_json::Value {
    let os_version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let xcode_version = std::process::Command::new("xcodebuild")
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_default();

    let arch = std::env::consts::ARCH.to_string();

    serde_json::json!({
        "os": "macos",
        "os_version": os_version,
        "arch": arch,
        "xcode_version": xcode_version,
    })
}

fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

async fn handle_runner_register(args: RunnerRegisterArgs) -> anyhow::Result<()> {
    let name = args.name.unwrap_or_else(get_hostname);

    let capabilities = detect_capabilities().await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/runners/register", args.daemon_url))
        .bearer_auth(&args.token)
        .json(&serde_json::json!({
            "name": name,
            "capabilities": capabilities,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let err: ApiError = resp.json().await.context("failed to parse error response")?;
        anyhow::bail!("Registration failed: {} - {}", err.code, err.error);
    }

    let result: RegisterRunnerResponse = resp.json().await?;

    // Save config
    let config = RunnerConfig {
        runner_id: result.runner.id.clone(),
        runner_token: result.token,
        daemon_url: args.daemon_url,
        name: result.runner.name.clone(),
    };

    let config_dir = dirs::home_dir().context("no home dir")?.join(".oore");
    fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("runner.json");

    // Write with restrictive permissions (0600) since the file contains the runner token
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&config_path)?;
        std::io::Write::write_all(&mut file, serde_json::to_string_pretty(&config)?.as_bytes())?;
    }

    println!("Runner registered successfully!");
    println!("  ID: {}", result.runner.id);
    println!("  Name: {}", result.runner.name);
    println!("  Config saved to: {}", config_path.display());
    println!("\nStart the runner with: oore runner start");

    Ok(())
}

async fn handle_runner_start(args: RunnerStartArgs) -> anyhow::Result<()> {
    let config_path = args
        .config
        .map(|p| {
            if p.starts_with("~/") {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(&p[2..])
            } else {
                PathBuf::from(p)
            }
        })
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".oore/runner.json")
        });

    let config: RunnerConfig = serde_json::from_str(
        &fs::read_to_string(&config_path)
            .context("Runner not registered. Run 'oore runner register' first.")?,
    )?;

    let daemon_url = args.daemon_url.unwrap_or(config.daemon_url.clone());
    let client = reqwest::Client::new();

    println!("Starting runner '{}' ({})", config.name, config.runner_id);
    println!("Connecting to: {}", daemon_url);

    // Detect capabilities once at startup for heartbeat reporting
    let capabilities = detect_capabilities().await;

    // Set up graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("\nShutting down runner...");
        let _ = shutdown_tx_clone.send(true);
    });

    // Spawn heartbeat task (sends real capabilities detected at startup)
    let hb_client = client.clone();
    let hb_url = daemon_url.clone();
    let hb_token = config.runner_token.clone();
    let hb_runner_id = config.runner_id.clone();
    let hb_capabilities = capabilities.clone();
    let mut hb_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = hb_shutdown.changed() => break,
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    let _ = hb_client
                        .post(format!("{}/v1/runners/{}/heartbeat", hb_url, hb_runner_id))
                        .bearer_auth(&hb_token)
                        .json(&serde_json::json!({ "status": "online", "capabilities": hb_capabilities }))
                        .send()
                        .await;
                }
            }
        }
    });

    // Main claim loop
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                println!("Runner shutdown complete.");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                match claim_and_execute(&client, &daemon_url, &config).await {
                    Ok(true) => { /* executed a job, immediately try another */ }
                    Ok(false) => { /* no jobs available, sleep and retry */ }
                    Err(e) => {
                        eprintln!("Error during claim/execute: {}", e);
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn claim_and_execute(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> anyhow::Result<bool> {
    let resp = client
        .post(format!(
            "{}/v1/runners/{}/claim",
            daemon_url, config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Claim request failed: {}", resp.status());
    }

    let claim: ClaimJobResponse = resp.json().await?;
    let job = match claim.job {
        Some(j) => j,
        None => return Ok(false),
    };

    println!(
        "Claimed build {} (#{}) for project {}",
        job.build_id, job.build_number, job.project_id
    );

    // Report status: running
    report_status(
        client, daemon_url, config, &job.build_id, "running", None, None, &[],
    )
    .await?;

    // Execute build (with cancellation checking between and during steps)
    let (steps, result) = execute_build(&job, client, daemon_url, config).await;

    // Report final status — steps are always available regardless of success/failure
    match result {
        Ok(()) => {
            report_status(
                client,
                daemon_url,
                config,
                &job.build_id,
                "succeeded",
                Some(0),
                None,
                &steps,
            )
            .await?;
            println!("Build {} succeeded", job.build_id);
        }
        Err(e) => {
            // If the build was externally terminated (canceled/timed_out),
            // the daemon already set the terminal state — do NOT report "failed".
            if e.downcast_ref::<BuildTerminated>().is_some() {
                println!("Build {} was externally terminated, skipping status report", job.build_id);
            } else {
                report_status(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    "failed",
                    Some(1),
                    Some(&e.to_string()),
                    &steps,
                )
                .await?;
                eprintln!("Build {} failed: {}", job.build_id, e);
            }
        }
    }

    Ok(true)
}

struct WorkspaceCleanup {
    path: PathBuf,
}

impl Drop for WorkspaceCleanup {
    fn drop(&mut self) {
        if self.path.exists() {
            if let Err(e) = fs::remove_dir_all(&self.path) {
                eprintln!(
                    "Warning: failed to clean up workspace {}: {}",
                    self.path.display(),
                    e
                );
            }
        }
    }
}

/// Sentinel error: build was externally terminated (canceled or timed out by the daemon).
/// Distinguished from real build failures so the runner does not attempt to report "failed".
#[derive(Debug)]
struct BuildTerminated {
    status: String,
}

impl std::fmt::Display for BuildTerminated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "build was externally terminated (status: {})", self.status)
    }
}

impl std::error::Error for BuildTerminated {}

/// Poll the daemon to check if a build is still active (not canceled or timed out).
/// Returns `Err(BuildTerminated)` if the build reached a terminal state externally.
async fn check_build_active(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let status_resp: JobStatusResponse = r.json().await?;
            let status: BuildStatus = status_resp
                .status
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;

            if status.is_terminal() {
                return Err(BuildTerminated {
                    status: status_resp.status,
                }
                .into());
            }
            Ok(())
        }
        Ok(_) | Err(_) => {
            // If we can't reach the daemon, don't abort — let the build continue.
            // The lease timeout mechanism will handle truly orphaned builds.
            Ok(())
        }
    }
}

/// Continuously poll for cancellation/timeout. Resolves only when the build is terminated.
/// Used inside `select!` to race against a running child process.
async fn poll_cancellation(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if check_build_active(client, daemon_url, config, build_id)
            .await
            .is_err()
        {
            return;
        }
    }
}

async fn execute_build(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> (Vec<StepResult>, anyhow::Result<()>) {
    // Create ephemeral workspace
    let workspace = PathBuf::from(format!("/tmp/oore-builds/{}", job.build_id));
    if let Err(e) = fs::create_dir_all(&workspace) {
        return (vec![], Err(e.into()));
    }

    // Ensure cleanup on exit (success or failure)
    let _cleanup = WorkspaceCleanup {
        path: workspace.clone(),
    };

    let snapshot = &job.config_snapshot;
    let mut steps = Vec::new();
    let mut log_seq: i64 = 0;

    // Check for cancellation before starting
    if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id).await {
        return (steps, Err(e));
    }

    // Step: checkout — require repo_url and at least commit_sha or branch
    let repo_url = snapshot
        .get("repo_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if repo_url.is_empty() {
        return (steps, Err(anyhow::anyhow!("Build config snapshot has no repo_url — cannot checkout source")));
    }

    if job.commit_sha.is_none() && job.branch.is_none() {
        return (steps, Err(anyhow::anyhow!("Build has neither commit_sha nor branch — cannot checkout source")));
    }

    let start = now_unix();

    // Checkout strategy:
    // 1. commit_sha present → git init + fetch exact commit + checkout FETCH_HEAD (reproducible)
    // 2. branch only → git clone --depth 1 --branch <branch> (branch HEAD)
    let child = if let Some(sha) = &job.commit_sha {
        // Fetch exact commit for reproducibility (webhook/pinned builds).
        // Uses env vars to avoid shell injection.
        match tokio::process::Command::new("sh")
            .arg("-c")
            .arg("git init && git fetch --depth 1 \"$OORE_REPO\" \"$OORE_SHA\" && git checkout FETCH_HEAD")
            .env("OORE_REPO", repo_url)
            .env("OORE_SHA", sha)
            .current_dir(&workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return (steps, Err(e.into())),
        }
    } else if let Some(branch) = &job.branch {
        // Clone branch HEAD (manual triggers without commit pinning)
        match tokio::process::Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg("--branch")
            .arg(branch)
            .arg(repo_url)
            .arg(".")
            .current_dir(&workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return (steps, Err(e.into())),
        }
    } else {
        unreachable!() // guarded above
    };

    let clone_status = run_and_stream(
        child,
        client,
        daemon_url,
        config,
        &job.build_id,
        &mut log_seq,
        poll_cancellation(client, daemon_url, config, &job.build_id),
    )
    .await;

    let finished = now_unix();
    match clone_status {
        None => {
            steps.push(StepResult {
                name: "checkout".to_string(),
                status: "failed".to_string(),
                exit_code: None,
                started_at: start,
                finished_at: finished,
                duration_ms: (finished - start) * 1000,
            });
            return (steps, Err(BuildTerminated { status: "canceled".to_string() }.into()));
        }
        Some(status) => {
            let exit_code = status.code();
            let success = exit_code == Some(0);
            steps.push(StepResult {
                name: "checkout".to_string(),
                status: if success { "succeeded" } else { "failed" }.to_string(),
                exit_code,
                started_at: start,
                finished_at: finished,
                duration_ms: (finished - start) * 1000,
            });
            if !success {
                return (steps, Err(anyhow::anyhow!("Git checkout failed")));
            }
        }
    }

    // Step: read .oore.yml and execute steps
    let config_path = snapshot
        .get("config_path")
        .and_then(|v| v.as_str())
        .unwrap_or(".oore.yml");

    let oore_config_path = workspace.join(config_path);
    if oore_config_path.exists() {
        let content = match fs::read_to_string(&oore_config_path) {
            Ok(c) => c,
            Err(e) => return (steps, Err(e.into())),
        };
        for (i, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Check for cancellation/timeout between steps
            if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id).await {
                return (steps, Err(e));
            }

            let step_name = format!("step-{}", i + 1);
            let start = now_unix();

            let child = match tokio::process::Command::new("sh")
                .arg("-c")
                .arg(line)
                .current_dir(&workspace)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(c) => c,
                Err(e) => return (steps, Err(e.into())),
            };

            let step_status = run_and_stream(
                child,
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                poll_cancellation(client, daemon_url, config, &job.build_id),
            )
            .await;

            let finished = now_unix();
            match step_status {
                None => {
                    steps.push(StepResult {
                        name: step_name,
                        status: "failed".to_string(),
                        exit_code: None,
                        started_at: start,
                        finished_at: finished,
                        duration_ms: (finished - start) * 1000,
                    });
                    return (steps, Err(BuildTerminated { status: "canceled".to_string() }.into()));
                }
                Some(status) => {
                    let exit_code = status.code().unwrap_or(-1);
                    steps.push(StepResult {
                        name: step_name,
                        status: if exit_code == 0 { "succeeded" } else { "failed" }.to_string(),
                        exit_code: Some(exit_code),
                        started_at: start,
                        finished_at: finished,
                        duration_ms: (finished - start) * 1000,
                    });
                    if exit_code != 0 {
                        return (steps, Err(anyhow::anyhow!("Step failed with exit code {}", exit_code)));
                    }
                }
            }
        }
    }

    // Scan workspace for artifacts and register them with the daemon.
    // Errors here are logged but don't fail the build — artifact upload is best-effort.
    scan_and_upload_artifacts(&workspace, client, daemon_url, config, &job.build_id, snapshot).await;

    (steps, Ok(()))
}

// ── Artifact scanning and upload ────────────────────────────────

/// Known artifact file extensions and their type identifiers.
fn artifact_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "apk" => Some("apk"),
        "ipa" => Some("ipa"),
        _ => None,
    }
}

/// Walk a directory recursively, collecting all file paths.
fn walk_dir_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    fn walk(dir: &std::path::Path, result: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip .git and hidden directories
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                walk(&path, result);
            } else if path.is_file() {
                result.push(path);
            }
        }
    }
    walk(dir, &mut result);
    result
}

/// Compute SHA-256 checksum of a file using buffered I/O.
fn compute_file_sha256(path: &std::path::Path) -> anyhow::Result<String> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Scan the workspace for artifact files and upload them to the daemon.
///
/// Looks for files matching known extensions (`.apk`, `.ipa`) and any additional
/// `*.ext` patterns specified in `config_snapshot.artifact_patterns`.
async fn scan_and_upload_artifacts(
    workspace: &std::path::Path,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    snapshot: &serde_json::Value,
) {
    let all_files = walk_dir_files(workspace);

    // Collect custom extension patterns from config snapshot (e.g., ["*.zip", "*.dSYM.zip"])
    let custom_extensions: Vec<String> = snapshot
        .get("artifact_patterns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter_map(|pat| pat.strip_prefix("*."))
                .map(|ext| ext.to_lowercase())
                .collect()
        })
        .unwrap_or_default();

    let mut artifacts: Vec<(PathBuf, String)> = Vec::new(); // (path, artifact_type)

    for path in &all_files {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            // Check known extensions first
            if let Some(art_type) = artifact_type_for_extension(ext) {
                artifacts.push((path.clone(), art_type.to_string()));
            } else if custom_extensions.contains(&ext.to_lowercase()) {
                artifacts.push((path.clone(), "generic".to_string()));
            }
        }
    }

    if artifacts.is_empty() {
        return;
    }

    println!("Found {} artifact(s) to upload", artifacts.len());

    for (path, artifact_type) in &artifacts {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let file_size = fs::metadata(path).map(|m| m.len() as i64).ok();

        let checksum = match compute_file_sha256(path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Warning: failed to compute checksum for {}: {}", name, e);
                None
            }
        };

        // Register artifact with the daemon
        let body = serde_json::json!({
            "name": name,
            "artifact_type": artifact_type,
            "file_size": file_size,
            "checksum": checksum,
            "metadata": {},
        });

        let resp = match client
            .post(format!(
                "{}/v1/runners/{}/jobs/{}/artifacts",
                daemon_url, config.runner_id, build_id
            ))
            .bearer_auth(&config.runner_token)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Warning: failed to register artifact {}: {}", name, e);
                continue;
            }
        };

        if !resp.status().is_success() {
            eprintln!(
                "Warning: artifact registration failed for {} (HTTP {})",
                name,
                resp.status()
            );
            continue;
        }

        // Parse response to get upload URL
        let create_resp: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: failed to parse artifact response for {}: {}", name, e);
                continue;
            }
        };

        let upload_url = create_resp
            .get("upload_url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if upload_url.is_empty() {
            println!("  Registered artifact {} (no S3 upload URL — storage not configured)", name);
            continue;
        }

        // Upload file to S3 via presigned PUT URL
        match tokio::fs::read(path).await {
            Ok(bytes) => {
                match client.put(upload_url).body(bytes).send().await {
                    Ok(r) if r.status().is_success() => {
                        println!("  Uploaded artifact {}", name);
                    }
                    Ok(r) => {
                        eprintln!(
                            "Warning: S3 upload failed for {} (HTTP {})",
                            name,
                            r.status()
                        );
                    }
                    Err(e) => {
                        eprintln!("Warning: S3 upload failed for {}: {}", name, e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to read artifact file {}: {}", name, e);
            }
        }
    }
}

async fn report_status(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    status: &str,
    exit_code: Option<i32>,
    error_message: Option<&str>,
    steps: &[StepResult],
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "status": status,
        "exit_code": exit_code,
        "error_message": error_message,
        "steps": steps,
    });

    let resp = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/status",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Status update failed: {}", resp.status());
    }

    Ok(())
}

/// Run a child process, capture its stdout/stderr, and upload log chunks in real-time.
/// Returns the exit status (or None if canceled).
async fn run_and_stream(
    mut child: tokio::process::Child,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    seq: &mut i64,
    cancel_fut: impl std::future::Future<Output = ()>,
) -> Option<std::process::ExitStatus> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Spawn tasks to read stdout/stderr and upload chunks
    let client_out = client.clone();
    let daemon_out = daemon_url.to_string();
    let config_out_id = config.runner_id.clone();
    let config_out_token = config.runner_token.clone();
    let build_out = build_id.to_string();

    // We use a shared sequence counter via a channel.
    // stdout and stderr tasks send lines back, and the main task uploads them.
    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<(String, String)>(256);

    if let Some(stdout) = stdout {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send((line, "stdout".to_string())).await.is_err() {
                    break;
                }
            }
        });
    }

    if let Some(stderr) = stderr {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send((line, "stderr".to_string())).await.is_err() {
                    break;
                }
            }
        });
    }

    // Drop our copy so channel closes when both readers finish
    drop(line_tx);

    // Spawn a task that batches and uploads log lines
    let upload_client = client_out;
    let upload_daemon = daemon_out;
    let upload_config_id = config_out_id;
    let upload_config_token = config_out_token;
    let upload_build = build_out;
    let seq_start = *seq;
    let upload_handle = tokio::spawn(async move {
        let mut local_seq = seq_start;
        let mut batch = Vec::new();
        let mut interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                line = line_rx.recv() => {
                    match line {
                        Some((content, stream)) => {
                            batch.push(serde_json::json!({
                                "sequence": local_seq,
                                "content": content,
                                "stream": stream,
                            }));
                            local_seq += 1;

                            // Flush if batch is large enough
                            if batch.len() >= 50 {
                                let body = serde_json::json!({ "chunks": batch });
                                let _ = upload_client
                                    .post(format!(
                                        "{}/v1/runners/{}/jobs/{}/logs",
                                        upload_daemon, upload_config_id, upload_build
                                    ))
                                    .bearer_auth(&upload_config_token)
                                    .json(&body)
                                    .send()
                                    .await;
                                batch = Vec::new();
                            }
                        }
                        None => {
                            // Channel closed — flush remaining
                            if !batch.is_empty() {
                                let body = serde_json::json!({ "chunks": batch });
                                let _ = upload_client
                                    .post(format!(
                                        "{}/v1/runners/{}/jobs/{}/logs",
                                        upload_daemon, upload_config_id, upload_build
                                    ))
                                    .bearer_auth(&upload_config_token)
                                    .json(&body)
                                    .send()
                                    .await;
                            }
                            return local_seq;
                        }
                    }
                }
                _ = interval.tick() => {
                    // Periodic flush for responsiveness
                    if !batch.is_empty() {
                        let body = serde_json::json!({ "chunks": batch });
                        let _ = upload_client
                            .post(format!(
                                "{}/v1/runners/{}/jobs/{}/logs",
                                upload_daemon, upload_config_id, upload_build
                            ))
                            .bearer_auth(&upload_config_token)
                            .json(&body)
                            .send()
                            .await;
                        batch = Vec::new();
                    }
                }
            }
        }
    });

    // Wait for child to complete or cancellation
    let status = tokio::select! {
        result = child.wait() => {
            match result {
                Ok(s) => Some(s),
                Err(_) => None,
            }
        },
        _ = cancel_fut => {
            child.kill().await.ok();
            None
        }
    };

    // Wait for upload task to finish and update sequence counter
    if let Ok(final_seq) = upload_handle.await {
        *seq = final_seq;
    }

    status
}

fn command_version(cmd: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout).ok()
            } else {
                None
            }
        })
        .and_then(|s| s.lines().next().map(|line| line.trim().to_string()))
}

fn run_doctor_checks() -> anyhow::Result<()> {
    let checks: [(&str, &[&str], &str); 7] = [
        ("git", &["--version"], "brew install git"),
        ("rustc", &["--version"], "curl https://sh.rustup.rs -sSf | sh"),
        ("cargo", &["--version"], "curl https://sh.rustup.rs -sSf | sh"),
        ("bun", &["--version"], "curl -fsSL https://bun.sh/install | bash"),
        ("fvm", &["--version"], "brew tap leoafarias/fvm && brew install fvm"),
        ("flutter", &["--version"], "fvm install <version> && fvm use <version>"),
        ("xcodebuild", &["-version"], "xcode-select --install"),
    ];

    println!("oore doctor -- environment checks");
    let mut missing = Vec::new();

    for (cmd, args, install_hint) in checks {
        match command_version(cmd, args) {
            Some(version) => {
                println!("  [ok] {:<10} {}", cmd, version);
            }
            None => {
                println!("  [missing] {:<10} install: {}", cmd, install_hint);
                missing.push(cmd);
            }
        }
    }

    if missing.is_empty() {
        println!("All required tools are installed.");
        Ok(())
    } else {
        anyhow::bail!("missing required tools: {}", missing.join(", "))
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Setup(setup) => match setup.command {
            Some(SetupSubcommand::Open(args)) => {
                let runtime = tokio::runtime::Runtime::new()
                    .context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_open(args))?;
            }
            None => {
                let runtime = tokio::runtime::Runtime::new()
                    .context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_interactive(&setup.daemon_url))?;
            }
        },
        Commands::Login => {
            println!("login flow placeholder");
        }
        Commands::Status => {
            println!("status command placeholder");
        }
        Commands::Runner(runner) => match runner.command {
            RunnerSubcommand::Register(args) => {
                let runtime = tokio::runtime::Runtime::new()
                    .context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_register(args))?;
            }
            RunnerSubcommand::Start(args) => {
                let runtime = tokio::runtime::Runtime::new()
                    .context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_start(args))?;
            }
        },
        Commands::Config(config) => match config.command {
            ConfigSubcommand::Set(args) => {
                println!("config set placeholder: {}={}", args.key, args.value);
            }
            ConfigSubcommand::Get(args) => {
                println!("config get placeholder: {}", args.key);
            }
        },
        Commands::Doctor => {
            run_doctor_checks()?;
        }
    }

    Ok(())
}
