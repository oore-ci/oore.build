use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use chrono::{Local, TimeZone};
use clap::{Args, Parser, Subcommand};
use oore_contract::{
    ApiError, BootstrapTokenRecord, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse,
    OidcConfigureRequest, OidcConfigureResponse, RegisterRunnerResponse, SetupCompleteResponse,
    SetupOidcStartRequest, SetupOidcStartResponse, SetupOidcVerifyRequest, SetupOidcVerifyResponse,
    SetupState, SetupStateFile, SetupStatus,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    /// Print the installed oore version
    Version,
    /// Update oore and oored to the latest release
    Update(UpdateArgs),
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Only check for updates, don't install
    #[arg(long, default_value = "false")]
    check: bool,

    /// Reinstall even if already on the latest version
    #[arg(long, default_value = "false")]
    force: bool,
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
    /// Generate a bootstrap token for web UI setup
    Token(SetupTokenArgs),
    /// Alias for 'token' (deprecated, use 'token' instead)
    #[command(hide = true)]
    Open(SetupTokenArgs),
}

#[derive(Debug, Args)]
struct SetupTokenArgs {
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

fn resolve_data_dir() -> anyhow::Result<PathBuf> {
    for key in ["OORED_DATA_DIR", "OORE_DATA_DIR"] {
        if let Ok(raw) = std::env::var(key) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Ok(PathBuf::from(trimmed));
            }
        }
    }

    let data_dir =
        dirs::data_dir().context("could not determine platform data directory (dirs::data_dir)")?;
    Ok(data_dir.join("oore"))
}

fn resolve_db_path(override_path: Option<&str>) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(PathBuf::from(p));
    }

    if let Ok(p) = std::env::var("OORE_SETUP_STATE_FILE") {
        return Ok(PathBuf::from(p));
    }

    Ok(resolve_data_dir()?.join("oore.db"))
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
                    let expires_at: i64 = row.try_get("bootstrap_token_expires_at").unwrap_or(0);
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
                    has_client_secret: row.try_get::<i32, _>("oidc_has_client_secret").unwrap_or(0)
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
    .bind(
        state
            .oidc_config
            .as_ref()
            .map(|c| c.has_client_secret as i32),
    )
    .bind(
        state
            .oidc_config
            .as_ref()
            .map(|c| &c.authorization_endpoint),
    )
    .bind(state.oidc_config.as_ref().map(|c| &c.token_endpoint))
    .bind(
        state
            .oidc_config
            .as_ref()
            .and_then(|c| c.userinfo_endpoint.as_ref()),
    )
    .bind(state.oidc_config.as_ref().map(|c| &c.jwks_uri))
    .bind(state.oidc_config.as_ref().map(|c| c.configured_at))
    .bind(
        state
            .oidc_secret
            .as_ref()
            .map(|s| &s.encrypted_client_secret),
    )
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

async fn handle_setup_token(args: SetupTokenArgs) -> anyhow::Result<()> {
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
        println!(
            "Expires: {} ({} from now)",
            format_epoch_local(expires_at),
            ttl_display
        );
        println!("State:   {}", state_display);
        println!("DB:      {}", db_display);
        println!();
        println!("To complete setup, either:");
        println!("  1. Open https://ci.oore.build/setup and paste this token");
        println!("  2. Run: oore setup");
        println!();
        println!("Note: The hosted UI requires your backend to be reachable over HTTPS.");
        println!("      For local-only setups, use the CLI (option 2) or expose your");
        println!("      backend via a tunnel (e.g. cloudflared).");
    }

    Ok(())
}

// ── OIDC loopback helpers ───────────────────────────────────────

/// Open a URL in the default browser (macOS-only in V1).
fn open_browser(url: &str) -> bool {
    std::process::Command::new("open").arg(url).spawn().is_ok()
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
    let path = first_line.split_whitespace().nth(1).unwrap_or("/");

    // Parse query params using url crate
    let fake_base = format!("http://localhost{}", path);
    let parsed = url::Url::parse(&fake_base).context("failed to parse callback URL")?;
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
            <meta name=\"theme-color\" content=\"#dc7702\"><title>Authentication failed</title></head>\
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
        <meta name=\"theme-color\" content=\"#dc7702\"><title>Authentication successful</title></head>\
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

/// Acquire a session token by generating a bootstrap token and verifying it with the daemon.
/// Used both for initial setup (step 1) and when resuming from a later state with an expired session.
async fn acquire_session(client: &reqwest::Client, daemon_url: &str) -> anyhow::Result<String> {
    let db_path = resolve_db_path(None)?;
    let pool = connect_db(&db_path).await?;
    let mut local_state = load_or_create_state(&pool).await?;

    let plaintext_token =
        generate_bootstrap_token(&mut local_state, &pool, Duration::from_secs(15 * 60)).await?;

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
        Ok(body.session_token)
    } else {
        let sc = verify_resp.status();
        let msg = extract_error_message(verify_resp).await;
        if sc == reqwest::StatusCode::CONFLICT {
            anyhow::bail!("Setup is already complete. Instance is in ready state.");
        } else if sc == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!(
                "Bootstrap token is invalid. Please regenerate with: oore setup token"
            );
        } else {
            anyhow::bail!(
                "Bootstrap verification failed (HTTP {}): {}",
                sc.as_u16(),
                msg
            );
        }
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
        Ok(resp) if resp.status().is_success() => resp
            .json()
            .await
            .context("failed to parse setup-status response")?,
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

    // Acquire session token inline via helper function

    // ── Step 1: Bootstrap token verification ────────────────────

    if current_state == SetupState::BootstrapPending || current_state == SetupState::Uninitialized {
        println!("[Step 1/4] Bootstrap token verification");
        println!();

        let db_path = resolve_db_path(None)?;
        println!("  Database: {}", db_path.display());

        println!("  Generating bootstrap token (TTL: 15m)...");
        println!("  Verifying token with daemon...");

        session_token = Some(acquire_session(&client, &daemon_url).await?);
        current_state = SetupState::BootstrapPending;

        println!();
        println!("  \u{2713} Bootstrap verified. Session token acquired.");
        println!();
    }

    // ── Step 2: OIDC Configuration ──────────────────────────────

    if current_state == SetupState::BootstrapPending {
        println!("[Step 2/4] OIDC provider configuration");
        println!();

        // Acquire session token if we don't have one (resuming setup)
        if session_token.is_none() {
            println!("  Acquiring session token...");
            session_token = Some(acquire_session(&client, &daemon_url).await?);
            println!();
        }
        let token = session_token.as_ref().unwrap();

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
                println!(
                    "  \u{2713} OIDC provider configured. Issuer: {}",
                    body.discovered_issuer
                );
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

        // Acquire session token if we don't have one (resuming setup)
        if session_token.is_none() {
            println!("  Acquiring session token...");
            session_token = Some(acquire_session(&client, &daemon_url).await?);
            println!();
        }
        let token = session_token.as_ref().unwrap();

        // Bind to a random free port
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("failed to bind loopback listener for OIDC callback")?;
        let local_port = listener
            .local_addr()
            .context("failed to get listener address")?
            .port();
        let redirect_uri = format!("http://localhost:{}/auth/callback", local_port);

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
                anyhow::bail!("OIDC start failed (HTTP {}): {}", status_code.as_u16(), msg);
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

        // Acquire session token if we don't have one (resuming setup)
        if session_token.is_none() {
            println!("  Acquiring session token...");
            session_token = Some(acquire_session(&client, &daemon_url).await?);
            println!();
        }
        let token = session_token.as_ref().unwrap();

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
            println!(
                "  \u{2713} Setup complete! Instance ID: {}",
                body.instance_id
            );
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
                anyhow::bail!(
                    "Setup completion failed (HTTP {}): {}",
                    status_code.as_u16(),
                    msg
                );
            }
        }
    }

    Ok(())
}

// ── Runner execution engine ─────────────────────────────────────

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
        let err: ApiError = resp
            .json()
            .await
            .context("failed to parse error response")?;
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

    let shared_cfg = oore_runner::RunnerConfig {
        runner_id: config.runner_id,
        runner_token: config.runner_token,
        daemon_url: config.daemon_url,
        name: config.name,
    };

    oore_runner::run_runner_forever(shared_cfg, Some(daemon_url)).await
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
        (
            "rustc",
            &["--version"],
            "curl https://sh.rustup.rs -sSf | sh",
        ),
        (
            "cargo",
            &["--version"],
            "curl https://sh.rustup.rs -sSf | sh",
        ),
        (
            "bun",
            &["--version"],
            "curl -fsSL https://bun.sh/install | bash",
        ),
        (
            "fvm",
            &["--version"],
            "brew tap leoafarias/fvm && brew install fvm",
        ),
        (
            "flutter",
            &["--version"],
            "fvm install <version> && fvm use <version>",
        ),
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

// ── Self-update helpers ──────────────────────────────────────────

const LATEST_JSON_URL: &str = "https://dl.oore.build/releases/latest.json";

#[derive(Debug, serde::Deserialize)]
struct ReleaseManifest {
    tag: String,
    version: String,
    #[allow(dead_code)]
    released_at: String,
    assets: ReleaseAssets,
}

#[derive(Debug, serde::Deserialize)]
struct ReleaseAssets {
    darwin_arm64: String,
    darwin_x86_64: String,
    checksums: String,
}

/// Map `std::env::consts::ARCH` to the archive naming convention.
fn release_arch() -> anyhow::Result<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Ok("arm64"),
        "x86_64" => Ok("x86_64"),
        other => anyhow::bail!("unsupported architecture: {other}"),
    }
}

/// Resolve the oore install root (`OORE_INSTALL_ROOT` or `~/.oore`).
fn resolve_install_root() -> anyhow::Result<PathBuf> {
    if let Ok(val) = std::env::var("OORE_INSTALL_ROOT") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".oore"))
}

/// Extract SHA-256 checksum for `filename` from checksums.txt content
/// (format: `<hash>  <filename>` per line, like sha256sum output).
fn parse_checksum(text: &str, filename: &str) -> anyhow::Result<String> {
    for line in text.lines() {
        // Handle both "hash  filename" and "hash filename"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == filename {
            return Ok(parts[0].to_lowercase());
        }
    }
    anyhow::bail!("checksum not found for {filename} in checksums.txt")
}

/// Check if the daemon is reachable on 127.0.0.1:8787.
async fn check_daemon_running(client: &reqwest::Client) -> bool {
    client
        .get("http://127.0.0.1:8787/healthz")
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Stop the daemon using PID file first, then lsof fallback (mirrors uninstall.sh).
fn stop_daemon(install_root: &Path) -> anyhow::Result<()> {
    let pid_file = install_root.join("oored.pid");

    // Try PID file first
    if pid_file.exists() {
        if let Ok(contents) = fs::read_to_string(&pid_file) {
            let pid_str = contents.trim();
            if !pid_str.is_empty() {
                if let Ok(pid) = pid_str.parse::<i32>() {
                    // Check if process exists (kill -0)
                    unsafe {
                        if libc::kill(pid, 0) == 0 {
                            libc::kill(pid, libc::SIGTERM);
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
            }
        }
        let _ = fs::remove_file(&pid_file);
    }

    // lsof fallback: kill anything still listening on port 8787
    if let Ok(output) = std::process::Command::new("lsof")
        .args(["-nP", "-iTCP:8787", "-sTCP:LISTEN", "-t"])
        .output()
    {
        if output.status.success() {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid_str in pids.split_whitespace() {
                if let Ok(pid) = pid_str.parse::<i32>() {
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                }
            }
            if !pids.trim().is_empty() {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }

    Ok(())
}

/// Restart the daemon and verify it becomes healthy.
async fn restart_daemon(install_root: &Path, client: &reqwest::Client) -> anyhow::Result<()> {
    let bin_dir = install_root.join("bin");
    let oored_bin = bin_dir.join("oored");
    let log_path = install_root.join("oored.log");
    let pid_file = install_root.join("oored.pid");

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log file: {}", log_path.display()))?;

    let child = std::process::Command::new(&oored_bin)
        .args(["run", "--listen", "127.0.0.1:8787"])
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .stdin(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start oored: {}", oored_bin.display()))?;

    fs::write(&pid_file, child.id().to_string())
        .with_context(|| format!("failed to write PID file: {}", pid_file.display()))?;

    // Poll healthz up to 15 seconds
    for _ in 0..15 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if check_daemon_running(client).await {
            return Ok(());
        }
    }

    anyhow::bail!(
        "Daemon failed to become healthy after restart. Check logs: {}",
        log_path.display()
    )
}

/// Set a file as executable (chmod 755).
fn set_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

async fn handle_update(args: UpdateArgs) -> anyhow::Result<()> {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .context("failed to parse current version")?;

    let client = reqwest::Client::new();

    // 1. Fetch release manifest
    let manifest: ReleaseManifest = client
        .get(LATEST_JSON_URL)
        .send()
        .await
        .context("failed to fetch release manifest")?
        .error_for_status()
        .context("release manifest request failed")?
        .json()
        .await
        .context("failed to parse release manifest")?;

    let latest = semver::Version::parse(&manifest.version)
        .with_context(|| format!("invalid version in manifest: {}", manifest.version))?;

    // 2. Compare versions
    println!("Current version: {current}");
    println!("Latest version:  {latest} ({})", manifest.tag);

    if current >= latest && !args.force {
        println!("Already up to date.");
        return Ok(());
    }

    if current < latest {
        println!("Update available: {current} -> {latest}");
    } else {
        println!("Reinstalling version {latest} (--force).");
    }

    // 3. Check-only mode
    if args.check {
        return Ok(());
    }

    // 4. Select archive URL by architecture
    let arch = release_arch()?;
    let archive_url = match arch {
        "arm64" => &manifest.assets.darwin_arm64,
        "x86_64" => &manifest.assets.darwin_x86_64,
        _ => unreachable!(),
    };

    // Derive the filename from the URL
    let archive_filename = archive_url
        .rsplit('/')
        .next()
        .context("could not determine archive filename from URL")?;

    println!("Downloading {archive_filename}...");

    // 5. Download archive + checksums in parallel
    let (archive_resp, checksums_resp) = tokio::try_join!(
        async {
            client
                .get(archive_url)
                .send()
                .await
                .context("failed to download archive")?
                .error_for_status()
                .context("archive download failed")
        },
        async {
            client
                .get(&manifest.assets.checksums)
                .send()
                .await
                .context("failed to download checksums")?
                .error_for_status()
                .context("checksums download failed")
        }
    )?;

    let archive_bytes = archive_resp
        .bytes()
        .await
        .context("failed to read archive bytes")?;
    let checksums_text = checksums_resp
        .text()
        .await
        .context("failed to read checksums text")?;

    // 6. Verify SHA-256 checksum
    let expected_hash = parse_checksum(&checksums_text, archive_filename)?;
    let actual_hash = hex::encode(Sha256::digest(&archive_bytes));

    if actual_hash != expected_hash {
        anyhow::bail!(
            "Checksum mismatch!\n  Expected: {expected_hash}\n  Actual:   {actual_hash}"
        );
    }
    println!("Checksum verified (SHA-256).");

    // 7. Extract tar.gz into tempdir
    let tmpdir = tempfile::tempdir().context("failed to create temporary directory")?;
    let decoder = flate2::read::GzDecoder::new(&archive_bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(tmpdir.path())
        .context("failed to extract archive")?;

    // 8. Verify expected files exist
    let extracted_bin = tmpdir.path().join("bin");
    let extracted_oore = extracted_bin.join("oore");
    let extracted_oored = extracted_bin.join("oored");
    let extracted_version = tmpdir.path().join("VERSION");

    if !extracted_oore.exists() {
        anyhow::bail!("archive missing bin/oore");
    }
    if !extracted_oored.exists() {
        anyhow::bail!("archive missing bin/oored");
    }
    if !extracted_version.exists() {
        anyhow::bail!("archive missing VERSION");
    }

    // 9. Stop daemon if running
    let install_root = resolve_install_root()?;
    let daemon_was_running = check_daemon_running(&client).await;

    if daemon_was_running {
        println!("oored daemon is running. Stopping before update...");
        stop_daemon(&install_root)?;
    }

    // 10. Copy binaries into install root
    let bin_dir = install_root.join("bin");
    fs::create_dir_all(&bin_dir).context("failed to create bin directory")?;

    fs::copy(&extracted_oore, bin_dir.join("oore"))
        .context("failed to copy oore binary")?;
    fs::copy(&extracted_oored, bin_dir.join("oored"))
        .context("failed to copy oored binary")?;
    fs::copy(&extracted_version, install_root.join("VERSION"))
        .context("failed to copy VERSION file")?;

    set_executable(&bin_dir.join("oore"))?;
    set_executable(&bin_dir.join("oored"))?;

    println!("Updated to version {latest}.");

    // 11. Restart daemon if it was running
    if daemon_was_running {
        println!("Restarting oored daemon...");
        restart_daemon(&install_root, &client).await?;
        println!("Daemon restarted successfully.");
    }

    // 12. Note about current process
    if current != latest {
        println!(
            "\nNote: This process is still running version {current}. \
             The new version ({latest}) will be used on next invocation."
        );
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Setup(setup) => match setup.command {
            Some(SetupSubcommand::Token(args) | SetupSubcommand::Open(args)) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_token(args))?;
            }
            None => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
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
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_register(args))?;
            }
            RunnerSubcommand::Start(args) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
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
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Update(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_update(args))?;
        }
    }

    Ok(())
}
