use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use chrono::{Local, TimeZone};
use clap::{Args, Parser, Subcommand, ValueEnum};
use oore_contract::{
    ApiError, BootstrapTokenRecord, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse,
    ListBuildsResponse, ListRunnersResponse, LocalLoginRequest, LocalLoginResponse,
    OidcConfigureRequest, OidcConfigureResponse, RegisterRunnerResponse, RemoteAuthMode,
    RuntimeMode, SetupCompleteResponse, SetupOidcStartRequest, SetupOidcStartResponse,
    SetupOidcVerifyRequest, SetupOidcVerifyResponse, SetupState, SetupStateFile, SetupStatus,
    UserProfileResponse, parse_repository_pipeline_yaml,
};
use rand::RngCore;
use ring::aead::{self, AES_256_GCM, Aad, BoundKey, NONCE_LEN, Nonce, NonceSequence, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const FAVICON_DATA_URI: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAzMiAzMiI+CiAgPGRlZnM+CiAgICA8Y2lyY2xlIGlkPSJjdXQiIGN4PSIxNiIgY3k9IjE2IiByPSI3IiAvPgogICAgPG1hc2sgaWQ9ImhvbGUiPgogICAgICA8cmVjdCB4PSIwIiB5PSIwIiB3aWR0aD0iMzIiIGhlaWdodD0iMzIiIGZpbGw9IndoaXRlIiAvPgogICAgICA8dXNlIGhyZWY9IiNjdXQiIGZpbGw9ImJsYWNrIiAvPgogICAgPC9tYXNrPgogICAgPGNsaXBQYXRoIGlkPSJsZWZ0Ij4KICAgICAgPHJlY3QgeD0iMCIgeT0iMCIgd2lkdGg9IjE1IiBoZWlnaHQ9IjMyIiAvPgogICAgPC9jbGlwUGF0aD4KICAgIDxjbGlwUGF0aCBpZD0icmlnaHQiPgogICAgICA8cmVjdCB4PSIxNyIgeT0iMCIgd2lkdGg9IjE1IiBoZWlnaHQ9IjMyIiAvPgogICAgPC9jbGlwUGF0aD4KICA8L2RlZnM+CiAgPHJlY3QKICAgIHg9IjIiCiAgICB5PSIyIgogICAgd2lkdGg9IjI4IgogICAgaGVpZ2h0PSIyOCIKICAgIHJ4PSI2IgogICAgZmlsbD0iI2Y0OWYxZSIKICAgIGNsaXAtcGF0aD0idXJsKCNsZWZ0KSIKICAgIG1hc2s9InVybCgjaG9sZSkiCiAgLz4KICA8cmVjdAogICAgeD0iMiIKICAgIHk9IjIiCiAgICB3aWR0aD0iMjgiCiAgICBoZWlnaHQ9IjI4IgogICAgcng9IjYiCiAgICBmaWxsPSIjZjQ5ZjFlIgogICAgY2xpcC1wYXRoPSJ1cmwoI3JpZ2h0KSIKICAgIG1hc2s9InVybCgjaG9sZSkiCiAgLz4KPC9zdmc+Cg==";
const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:8787";
const CONFIG_KEY_DAEMON_URL: &str = "daemon_url";
const CONFIG_KEY_SESSION_TOKEN: &str = "session_token";

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct CliConfigFile {
    daemon_url: Option<String>,
    session_token: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct StatusSummary {
    daemon_url: String,
    setup_status: SetupStatus,
    authenticated: bool,
    queue_depth: Option<i64>,
    active_builds: Option<i64>,
    recent_builds: Option<Vec<oore_contract::Build>>,
    runners: Option<Vec<oore_contract::Runner>>,
}

#[derive(Debug, serde::Serialize)]
struct DoctorCheckResult {
    name: String,
    status: String,
    detail: Option<String>,
    install_hint: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DoctorReport {
    checks: Vec<DoctorCheckResult>,
    missing_count: usize,
    warning_count: usize,
}

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
    Frontend(FrontendArgs),
    Login(LoginArgs),
    Status(StatusArgs),
    Runner(RunnerArgs),
    Config(ConfigArgs),
    Doctor(DoctorArgs),
    Pipeline(PipelineArgs),
    /// Print the installed oore version
    Version,
    /// Update oore and oored to the latest release
    Update(UpdateArgs),
    /// Create, verify, or restore an encrypted-state backup
    Backup(BackupArgs),
}

#[derive(Debug, Args)]
struct FrontendArgs {
    #[command(subcommand)]
    command: FrontendSubcommand,
}

#[derive(Debug, Subcommand)]
enum FrontendSubcommand {
    /// Create a short-lived, single-use frontend pairing code.
    Invite(FrontendInviteArgs),
}

#[derive(Debug, Args)]
struct FrontendInviteArgs {
    /// How long the pairing code remains valid.
    #[arg(long, default_value = "10m")]
    ttl: String,

    /// Path to the setup database file.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// Print machine-readable output.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct BackupArgs {
    #[command(subcommand)]
    command: BackupSubcommand,
}

#[derive(Debug, Subcommand)]
enum BackupSubcommand {
    /// Create a consistent SQLite snapshot and package it with the encryption key.
    Create(BackupCreateArgs),
    /// Verify a backup manifest, checksums, and SQLite integrity.
    Verify(BackupVerifyArgs),
    /// Atomically restore a verified backup while the daemon is stopped.
    Restore(BackupRestoreArgs),
}

#[derive(Debug, Args)]
struct BackupCreateArgs {
    /// Destination .tar.gz file.
    #[arg(long)]
    output: PathBuf,
    /// Path to the SQLite state file.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
}

#[derive(Debug, Args)]
struct BackupVerifyArgs {
    /// Backup .tar.gz file.
    #[arg(long)]
    input: PathBuf,
}

#[derive(Debug, Args)]
struct BackupRestoreArgs {
    /// Backup .tar.gz file.
    #[arg(long)]
    input: PathBuf,
    /// Path to the SQLite state file to restore.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BackupManifest {
    format: String,
    created_at: i64,
    files: HashMap<String, String>,
}

#[derive(Debug, Args)]
struct PipelineArgs {
    #[command(subcommand)]
    command: PipelineSubcommand,
}

#[derive(Debug, Subcommand)]
enum PipelineSubcommand {
    /// Validate repository pipeline YAML using the runner's schema.
    Validate(PipelineValidateArgs),
}

#[derive(Debug, Args)]
struct PipelineValidateArgs {
    /// Repository pipeline file to validate.
    #[arg(default_value = ".oore.yaml")]
    path: PathBuf,
}

fn handle_pipeline_validate(args: PipelineValidateArgs) -> anyhow::Result<()> {
    let raw = fs::read_to_string(&args.path)
        .with_context(|| format!("failed to read {}", args.path.display()))?;
    parse_repository_pipeline_yaml(&raw).map_err(anyhow::Error::msg)?;
    println!("{} is valid", args.path.display());
    Ok(())
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Only check for updates, don't install
    #[arg(long, default_value = "false")]
    check: bool,

    /// Reinstall even if already on the latest version
    #[arg(long, default_value = "false")]
    force: bool,

    /// Release channel to update from: stable, beta, alpha
    ///
    /// If not specified, this defaults to the installed channel (if available) or infers it from
    /// the installed VERSION file; falls back to stable.
    #[arg(long)]
    channel: Option<String>,

    /// GitHub repository in `owner/name` format
    ///
    /// If not specified, this defaults to the installed GITHUB_REPO file (if available) or
    /// `devaryakjha/oore.build`.
    #[arg(long, env = "OORE_GITHUB_REPO")]
    repo: Option<String>,
}

#[derive(Debug, Args)]
struct SetupArgs {
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,

    #[command(subcommand)]
    command: Option<SetupSubcommand>,
}

#[derive(Debug, Subcommand)]
enum SetupSubcommand {
    /// Initialize setup directly on the backend host
    Init(SetupInitArgs),
    /// Generate a bootstrap token for web UI setup
    Token(SetupTokenArgs),
    /// Alias for 'token' (deprecated, use 'token' instead)
    #[command(hide = true)]
    Open(SetupTokenArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SetupInitMode {
    /// Loopback-only local mode; creates the local owner and completes setup.
    Local,
    /// Remote mode where an upstream trusted proxy authenticates users.
    TrustedProxy,
}

#[derive(Debug, Args)]
struct SetupInitArgs {
    /// Setup mode to initialize.
    #[arg(long, value_enum)]
    mode: SetupInitMode,

    /// Initial owner email.
    #[arg(long)]
    owner_email: String,

    /// Trusted proxy identity header. Required only for trusted-proxy mode.
    #[arg(long, default_value = "x-oore-user-email")]
    user_email_header: String,

    /// Trusted proxy peer CIDR. Repeat for multiple frontend/proxy networks.
    #[arg(long = "trusted-proxy-cidr")]
    trusted_proxy_cidrs: Vec<String>,

    /// Shared secret expected from the trusted proxy/oore-web hop.
    #[arg(long, env = "OORE_TRUSTED_PROXY_SHARED_SECRET")]
    shared_secret: Option<String>,

    /// File containing the shared secret expected from the trusted proxy/oore-web hop.
    #[arg(long, env = "OORE_TRUSTED_PROXY_SHARED_SECRET_FILE")]
    shared_secret_file: Option<String>,

    /// Path to the setup database file.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// Re-initialize an incomplete setup. Refuses to change a ready instance.
    #[arg(long, default_value = "false")]
    force: bool,

    /// Print machine-readable output.
    #[arg(long, default_value = "false")]
    json: bool,
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
struct StatusArgs {
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,

    /// Session token for authenticated status details.
    /// If omitted, falls back to OORE_SESSION_TOKEN or stored CLI config.
    #[arg(long, env = "OORE_SESSION_TOKEN")]
    token: Option<String>,

    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
struct LoginArgs {
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,

    /// Import an existing session token and validate it against /v1/users/me.
    #[arg(long)]
    token: Option<String>,

    /// Optional local-login email (local mode). Defaults to owner@local server-side.
    #[arg(long)]
    email: Option<String>,

    #[arg(long, default_value = "false")]
    json: bool,
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

#[derive(Debug, Args)]
struct DoctorArgs {
    #[arg(long, default_value = "false")]
    json: bool,

    /// Check requirements for a build platform. Repeat for multiple platforms.
    #[arg(long, value_enum)]
    platform: Vec<DoctorPlatform>,

    /// Check Android, iOS, and macOS requirements.
    #[arg(long, conflicts_with = "platform")]
    all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DoctorPlatform {
    Android,
    Ios,
    Macos,
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

fn read_env_trimmed(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn resolve_cli_config_path() -> anyhow::Result<PathBuf> {
    if let Some(path) = read_env_trimmed("OORE_CONFIG_FILE") {
        return Ok(PathBuf::from(path));
    }
    Ok(resolve_install_root()?.join("config.json"))
}

fn load_cli_config() -> anyhow::Result<CliConfigFile> {
    let path = resolve_cli_config_path()?;
    if !path.exists() {
        return Ok(CliConfigFile::default());
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read CLI config {}", path.display()))?;
    let cfg = serde_json::from_str::<CliConfigFile>(&raw)
        .with_context(|| format!("failed to parse CLI config {}", path.display()))?;
    Ok(cfg)
}

fn save_cli_config(cfg: &CliConfigFile) -> anyhow::Result<PathBuf> {
    let path = resolve_cli_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create CLI config directory {}", parent.display())
        })?;
    }

    let data = serde_json::to_vec_pretty(cfg).context("failed to serialize CLI config")?;
    let mut file =
        fs::File::create(&path).with_context(|| format!("failed to write {}", path.display()))?;
    file.write_all(&data)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to finalize {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }

    Ok(path)
}

fn resolve_daemon_url(cli_value: Option<&str>) -> anyhow::Result<String> {
    if let Some(v) = cli_value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        return Ok(v);
    }

    if let Some(v) = read_env_trimmed("OORE_DAEMON_URL") {
        return Ok(v);
    }

    let cfg = load_cli_config()?;
    if let Some(v) = cfg.daemon_url.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        return Ok(v);
    }

    Ok(DEFAULT_DAEMON_URL.to_string())
}

fn resolve_session_token(cli_value: Option<&str>) -> anyhow::Result<Option<String>> {
    if let Some(v) = cli_value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        return Ok(Some(v));
    }

    if let Some(v) = read_env_trimmed("OORE_SESSION_TOKEN") {
        return Ok(Some(v));
    }

    let cfg = load_cli_config()?;
    Ok(cfg.session_token.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }))
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

const TRUSTED_PROXY_SHARED_SECRET_HEADER: &str = "x-oore-trusted-proxy-secret";

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

struct SingleNonce(Option<[u8; NONCE_LEN]>);

impl NonceSequence for SingleNonce {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let bytes = self.0.take().ok_or(ring::error::Unspecified)?;
        Ok(Nonce::assume_unique_for_key(bytes))
    }
}

fn resolve_encryption_key_path() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("OORE_ENCRYPTION_KEY_FILE") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Ok(data_dir) = std::env::var("OORED_DATA_DIR")
        && !data_dir.trim().is_empty()
    {
        return Ok(PathBuf::from(data_dir.trim()).join("encryption.key"));
    }
    if let Ok(data_dir) = std::env::var("OORE_DATA_DIR")
        && !data_dir.trim().is_empty()
    {
        return Ok(PathBuf::from(data_dir.trim()).join("encryption.key"));
    }

    let data_dir =
        dirs::data_dir().context("could not determine platform data directory (dirs::data_dir)")?;
    Ok(data_dir.join("oore").join("encryption.key"))
}

fn load_or_generate_encryption_key() -> anyhow::Result<Vec<u8>> {
    let path = resolve_encryption_key_path()?;
    if path.exists() {
        let key = fs::read(&path)
            .with_context(|| format!("failed to read encryption key: {}", path.display()))?;
        if key.len() != 32 {
            anyhow::bail!(
                "encryption key at {} has invalid length: expected 32 bytes, got {}",
                path.display(),
                key.len()
            );
        }
        return Ok(key);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let rng = SystemRandom::new();
    let mut key = vec![0u8; 32];
    rng.fill(&mut key)
        .map_err(|_| anyhow::anyhow!("failed to generate encryption key"))?;
    fs::write(&path, &key)
        .with_context(|| format!("failed to write encryption key: {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }
    Ok(key)
}

fn encrypt_secret(plaintext: &str, key: &[u8]) -> anyhow::Result<String> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate encryption nonce"))?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
    let mut sealing_key = aead::SealingKey::new(unbound_key, SingleNonce(Some(nonce_bytes)));
    let mut in_out = plaintext.as_bytes().to_vec();
    sealing_key
        .seal_in_place_append_tag(Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;

    let mut out = Vec::with_capacity(NONCE_LEN + in_out.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&in_out);
    Ok(BASE64.encode(out))
}

fn normalize_email(raw: &str) -> anyhow::Result<String> {
    let email = raw.trim().to_lowercase();
    if email.is_empty() || email.len() > 256 || !email.contains('@') {
        anyhow::bail!("owner email must be a valid email address");
    }
    Ok(email)
}

fn normalize_header_name(raw: &str) -> anyhow::Result<String> {
    let header = raw.trim().to_ascii_lowercase();
    let valid = !header.is_empty()
        && header.len() <= 128
        && header.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '!' | '#'
                        | '$'
                        | '%'
                        | '&'
                        | '\''
                        | '*'
                        | '+'
                        | '-'
                        | '.'
                        | '^'
                        | '_'
                        | '`'
                        | '|'
                        | '~'
                )
        });
    if !valid {
        anyhow::bail!("trusted proxy user email header is invalid");
    }
    Ok(header)
}

fn trusted_proxy_subject_for_email(email: &str) -> String {
    format!("trusted-proxy::{}", email.trim().to_lowercase())
}

fn local_subject_for_email(email: &str) -> String {
    format!("local::{}", email.trim().to_lowercase())
}

fn resolve_secret_value(
    value: Option<&str>,
    file: Option<&str>,
    label: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }

    let Some(file) = file else {
        return Ok(None);
    };
    let file = file.trim();
    if file.is_empty() {
        return Ok(None);
    }
    let secret =
        fs::read_to_string(file).with_context(|| format!("failed to read {label} file: {file}"))?;
    let secret = secret.trim();
    if secret.is_empty() {
        anyhow::bail!("{label} file is empty: {file}");
    }
    Ok(Some(secret.to_string()))
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

fn normalize_trusted_proxy_cidrs(values: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cidr = trimmed
            .parse::<ipnet::IpNet>()
            .with_context(|| format!("invalid trusted proxy CIDR: {trimmed}"))?;
        let canonical = cidr.to_string();
        if !normalized.iter().any(|existing| existing == &canonical) {
            normalized.push(canonical);
        }
    }
    Ok(normalized)
}

async fn persist_instance_preferences(
    pool: &SqlitePool,
    runtime_mode: RuntimeMode,
    remote_auth_mode: RemoteAuthMode,
    now: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, remote_auth_mode, updated_by, created_at, updated_at)
         VALUES (1, 'file', ?1, ?2, NULL, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            key_storage_mode = excluded.key_storage_mode,
            runtime_mode = excluded.runtime_mode,
            remote_auth_mode = excluded.remote_auth_mode,
            updated_at = excluded.updated_at",
    )
    .bind(runtime_mode.to_string())
    .bind(remote_auth_mode.to_string())
    .bind(now)
    .execute(pool)
    .await
    .context("failed to save instance preferences")?;
    Ok(())
}

async fn ensure_setup_init_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    for table in ["instance_preferences", "trusted_proxy_settings", "users"] {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        )
        .bind(table)
        .fetch_optional(pool)
        .await
        .with_context(|| format!("failed to inspect database schema for {table}"))?;
        if exists.is_none() {
            anyhow::bail!(
                "setup database is not migrated yet (missing table: {table}). Start `oored run` once, then rerun `oore setup init`."
            );
        }
    }
    Ok(())
}

async fn persist_trusted_proxy_settings(
    pool: &SqlitePool,
    owner_email: &str,
    user_email_header: &str,
    trusted_proxy_cidrs: &[String],
    encrypted_shared_secret: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    let cidrs_json = serde_json::to_string(trusted_proxy_cidrs)
        .context("failed to serialize trusted proxy CIDRs")?;
    sqlx::query(
        "INSERT INTO trusted_proxy_settings (id, user_email_header, setup_owner_email, trusted_proxy_cidrs_json, encrypted_shared_secret, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, NULL, ?5, ?5)
         ON CONFLICT(id) DO UPDATE SET
            user_email_header = excluded.user_email_header,
            setup_owner_email = excluded.setup_owner_email,
            trusted_proxy_cidrs_json = excluded.trusted_proxy_cidrs_json,
            encrypted_shared_secret = excluded.encrypted_shared_secret,
            updated_at = excluded.updated_at",
    )
    .bind(user_email_header)
    .bind(owner_email)
    .bind(cidrs_json)
    .bind(encrypted_shared_secret)
    .bind(now)
    .execute(pool)
    .await
    .context("failed to save trusted proxy settings")?;
    Ok(())
}

async fn upsert_owner_user(
    pool: &SqlitePool,
    owner_email: &str,
    subject: &str,
    now: i64,
) -> anyhow::Result<()> {
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'owner', 'active', ?5, ?5)
         ON CONFLICT(email) DO UPDATE SET
            oidc_subject = excluded.oidc_subject,
            display_name = excluded.display_name,
            role = 'owner',
            status = 'active',
            updated_at = excluded.updated_at",
    )
    .bind(&user_id)
    .bind(owner_email)
    .bind(subject)
    .bind(owner_email)
    .bind(now)
    .execute(pool)
    .await
    .context("failed to create owner user")?;
    Ok(())
}

async fn handle_setup_init(args: SetupInitArgs) -> anyhow::Result<()> {
    let db_path = resolve_db_path(args.state_file.as_deref())?;
    let pool = connect_db(&db_path).await?;
    let mut state = load_or_create_state(&pool).await?;

    if state.setup_state == SetupState::Ready {
        anyhow::bail!("setup is already complete; refusing to change a ready instance");
    }
    if !args.force
        && !matches!(
            state.setup_state,
            SetupState::Uninitialized | SetupState::BootstrapPending
        )
    {
        anyhow::bail!(
            "setup is already in {} state; pass --force to re-initialize before owner creation",
            state_label(state.setup_state)
        );
    }

    let owner_email = normalize_email(&args.owner_email)?;
    let now = now_epoch_secs();
    ensure_setup_init_schema(&pool).await?;

    let (runtime_mode, remote_auth_mode, owner_subject) = match args.mode {
        SetupInitMode::Local => (
            RuntimeMode::Local,
            RemoteAuthMode::Oidc,
            local_subject_for_email(&owner_email),
        ),
        SetupInitMode::TrustedProxy => (
            RuntimeMode::Remote,
            RemoteAuthMode::TrustedProxy,
            trusted_proxy_subject_for_email(&owner_email),
        ),
    };

    persist_instance_preferences(&pool, runtime_mode, remote_auth_mode, now).await?;

    let mut trusted_proxy_cidrs = Vec::new();
    let mut user_email_header = None;
    let mut has_shared_secret = false;
    if args.mode == SetupInitMode::TrustedProxy {
        let header = normalize_header_name(&args.user_email_header)?;
        let cidrs = normalize_trusted_proxy_cidrs(args.trusted_proxy_cidrs)?;
        let shared_secret = resolve_secret_value(
            args.shared_secret.as_deref(),
            args.shared_secret_file.as_deref(),
            "trusted proxy shared secret",
        )?
        .context(
            "trusted-proxy setup init requires a shared secret; pass --shared-secret-file or set OORE_TRUSTED_PROXY_SHARED_SECRET_FILE",
        )?;
        let key = load_or_generate_encryption_key()?;
        let encrypted_shared_secret = Some(encrypt_secret(&shared_secret, &key)?);
        has_shared_secret = encrypted_shared_secret.is_some();
        persist_trusted_proxy_settings(
            &pool,
            &owner_email,
            &header,
            &cidrs,
            encrypted_shared_secret.as_deref(),
            now,
        )
        .await?;
        trusted_proxy_cidrs = cidrs;
        user_email_header = Some(header);
    }

    state.owner = Some(oore_contract::OwnerRecord {
        email: owner_email.clone(),
        oidc_subject: Some(owner_subject.clone()),
        created_at: now,
    });
    upsert_owner_user(&pool, &owner_email, &owner_subject, now).await?;
    state.setup_state = SetupState::Ready;
    state.setup_session = None;
    state.bootstrap_token = None;
    state.updated_at = now;
    save_state(&pool, &state).await?;

    if args.json {
        let output = serde_json::json!({
            "ok": true,
            "state": "ready",
            "mode": match args.mode {
                SetupInitMode::Local => "local",
                SetupInitMode::TrustedProxy => "trusted_proxy",
            },
            "owner_email": owner_email,
            "database": db_path.display().to_string(),
            "instance_id": state.instance_id,
            "trusted_proxy": {
                "user_email_header": user_email_header,
                "trusted_proxy_cidrs": trusted_proxy_cidrs,
                "has_shared_secret": has_shared_secret,
                "shared_secret_header": TRUSTED_PROXY_SHARED_SECRET_HEADER,
            },
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("Setup initialized.");
    println!();
    println!("State:    ready");
    println!("Instance: {}", state.instance_id);
    println!("Owner:    {}", owner_email);
    println!("DB:       {}", db_path.display());
    match args.mode {
        SetupInitMode::Local => {
            println!("Mode:     local");
            println!();
            println!("Local login is available only from loopback.");
        }
        SetupInitMode::TrustedProxy => {
            println!("Mode:     remote trusted-proxy");
            println!(
                "Header:   {}",
                user_email_header.as_deref().unwrap_or("x-oore-user-email")
            );
            if trusted_proxy_cidrs.is_empty() {
                println!("Peers:    loopback only");
            } else {
                println!("Peers:    {}", trusted_proxy_cidrs.join(", "));
            }
            println!(
                "Secret:   {}",
                if has_shared_secret {
                    "configured"
                } else {
                    "not configured"
                }
            );
            if has_shared_secret {
                println!("Proxy must forward: {}", TRUSTED_PROXY_SHARED_SECRET_HEADER);
            }
        }
    }

    Ok(())
}

async fn handle_frontend_invite(args: FrontendInviteArgs) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;
    if ttl.is_zero() || ttl > Duration::from_secs(60 * 60) {
        anyhow::bail!("frontend pairing ttl must be between 1 second and 1 hour");
    }

    let db_path = resolve_db_path(args.state_file.as_deref())?;
    let pool = connect_db(&db_path).await?;
    let configured: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM trusted_proxy_settings \
         WHERE id = 1 AND encrypted_shared_secret IS NOT NULL LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .context("trusted-proxy setup is not migrated; update and start oored before pairing")?;
    if configured.is_none() {
        anyhow::bail!("frontend pairing requires a configured Trusted Proxy backend");
    }

    let mut token_bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut token_bytes);
    let code = format!("fp_{}", URL_SAFE_NO_PAD.encode(token_bytes));
    let token_hash = hex::encode(Sha256::digest(code.as_bytes()));
    let now = now_epoch_secs();
    let expires_at = now + ttl.as_secs() as i64;
    let invite_id = uuid::Uuid::new_v4().to_string();

    let mut transaction = pool.begin().await?;
    sqlx::query("UPDATE frontend_pairing_invites SET consumed_at = ?1 WHERE consumed_at IS NULL")
        .bind(now)
        .execute(&mut *transaction)
        .await
        .context("failed to revoke previous frontend pairing invites")?;
    sqlx::query(
        "INSERT INTO frontend_pairing_invites (id, token_hash, expires_at, consumed_at, created_at) \
         VALUES (?1, ?2, ?3, NULL, ?4)",
    )
    .bind(&invite_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .context("failed to create frontend pairing invite; update and start oored first")?;
    let audit_details = serde_json::json!({
        "source": "local_cli",
        "expires_at": expires_at,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, resource_type, resource_id, details, created_at) \
         VALUES (NULL, 'frontend_pairing_invite_created', 'frontend_pairing_invite', ?1, ?2, ?3)",
    )
    .bind(&invite_id)
    .bind(audit_details)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .context("failed to audit frontend pairing invite")?;
    transaction.commit().await?;

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "code": code,
                "expires_at": expires_at,
                "single_use": true,
            })
        );
    } else {
        println!(
            "Frontend pairing code (single-use, expires in {}):",
            args.ttl
        );
        println!("{code}");
    }
    Ok(())
}

async fn handle_setup_token(args: SetupTokenArgs, daemon_url: &str) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;

    // 1. Resolve database path
    let db_path = resolve_db_path(args.state_file.as_deref())?;

    // 2. Connect and load or create state
    let pool = connect_db(&db_path).await?;
    let mut state = load_or_create_state(&pool).await?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("failed to build HTTP client")?;
    let daemon_status = fetch_setup_status(&client, daemon_url).await;
    if let Ok(status) = &daemon_status {
        if status.instance_id != state.instance_id {
            eprintln!(
                "Daemon instance mismatch.\n\
                 Daemon:   {} (instance {})\n\
                 State DB: {} (instance {})\n\
                 Set OORE_SETUP_STATE_FILE (or --state-file) to the daemon setup DB and retry.",
                daemon_url,
                status.instance_id,
                db_path.display(),
                state.instance_id
            );
            std::process::exit(1);
        }
    } else if let Err(err) = daemon_status {
        eprintln!(
            "Warning: unable to verify daemon instance at {}: {}",
            daemon_url, err
        );
    }

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
                let mut msg = format!("{} [{}]", api_err.error, api_err.code);
                if let Some(details) = api_err.details {
                    let details = details.trim();
                    if !details.is_empty() {
                        msg = format!("{msg}: {details}");
                    }
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

async fn fetch_setup_status(
    client: &reqwest::Client,
    daemon_url: &str,
) -> anyhow::Result<SetupStatus> {
    let status_url = format!(
        "{}/v1/public/setup-status",
        daemon_url.trim_end_matches('/')
    );
    let response = client
        .get(&status_url)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse setup-status response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "daemon status check failed at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

async fn fetch_user_profile(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
) -> anyhow::Result<UserProfileResponse> {
    let url = format!("{}/v1/users/me", daemon_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse /v1/users/me response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "token validation failed at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

async fn fetch_build_list(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
    status_filter: Option<&str>,
    limit: Option<i64>,
) -> anyhow::Result<ListBuildsResponse> {
    let url = format!("{}/v1/builds", daemon_url.trim_end_matches('/'));
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(status) = status_filter {
        query.push(("status", status.to_string()));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }

    let response = client
        .get(&url)
        .bearer_auth(token)
        .query(&query)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse /v1/builds response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "failed to fetch builds at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

async fn fetch_runner_list(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
) -> anyhow::Result<ListRunnersResponse> {
    let url = format!("{}/v1/runners", daemon_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if response.status().is_success() {
        return response
            .json()
            .await
            .context("failed to parse /v1/runners response");
    }

    let status = response.status();
    let msg = extract_error_message(response).await;
    anyhow::bail!(
        "failed to fetch runners at {daemon_url} (HTTP {}): {}",
        status.as_u16(),
        msg
    )
}

/// Acquire a session token by generating a bootstrap token and verifying it with the daemon.
/// Used both for initial setup (step 1) and when resuming from a later state with an expired session.
async fn acquire_session(client: &reqwest::Client, daemon_url: &str) -> anyhow::Result<String> {
    let db_path = resolve_db_path(None)?;
    let pool = connect_db(&db_path).await?;
    let mut local_state = load_or_create_state(&pool).await?;
    let remote_status = fetch_setup_status(client, daemon_url).await?;
    if remote_status.instance_id != local_state.instance_id {
        anyhow::bail!(
            "daemon instance mismatch: daemon {} is instance {}, but local setup state {} is instance {}. \
Use OORE_SETUP_STATE_FILE or --state-file to point at the daemon setup DB and retry",
            daemon_url,
            remote_status.instance_id,
            db_path.display(),
            local_state.instance_id
        );
    }

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
            anyhow::bail!("Bootstrap token is invalid. Please regenerate with: oore setup token");
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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to build HTTP client")?;

    println!("oore setup — interactive instance configuration");
    println!();

    let mode_choice = dialoguer::Select::new()
        .with_prompt("What kind of setup do you want?")
        .default(0)
        .item("Local Only - loopback-only owner login, no external auth")
        .item("Remote Trusted Proxy - an upstream proxy provides user identity")
        .item("Remote OIDC - users sign in with an identity provider")
        .item("Generate a web setup token")
        .interact()
        .context("failed to read setup mode")?;

    match mode_choice {
        0 => {
            let owner_email: String = dialoguer::Input::new()
                .with_prompt("Owner email")
                .default("owner@local".to_string())
                .interact_text()
                .context("failed to read owner email")?;
            return handle_setup_init(SetupInitArgs {
                mode: SetupInitMode::Local,
                owner_email,
                user_email_header: "x-oore-user-email".to_string(),
                trusted_proxy_cidrs: Vec::new(),
                shared_secret: None,
                shared_secret_file: None,
                state_file: None,
                force: false,
                json: false,
            })
            .await;
        }
        1 => {
            let owner_email: String = dialoguer::Input::new()
                .with_prompt("Initial owner email")
                .interact_text()
                .context("failed to read owner email")?;
            let user_email_header: String = dialoguer::Input::new()
                .with_prompt("Trusted proxy user email header")
                .default("x-oore-user-email".to_string())
                .interact_text()
                .context("failed to read trusted proxy header")?;
            let cidrs_raw: String = dialoguer::Input::new()
                .with_prompt("Trusted proxy CIDRs (comma-separated, leave blank for loopback only)")
                .allow_empty(true)
                .interact_text()
                .context("failed to read trusted proxy CIDRs")?;
            let shared_secret: String = dialoguer::Password::new()
                .with_prompt("Shared secret injected by proxy/oore-web (recommended)")
                .allow_empty_password(true)
                .interact()
                .context("failed to read trusted proxy shared secret")?;
            let trusted_proxy_cidrs = cidrs_raw
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect();
            return handle_setup_init(SetupInitArgs {
                mode: SetupInitMode::TrustedProxy,
                owner_email,
                user_email_header,
                trusted_proxy_cidrs,
                shared_secret: if shared_secret.trim().is_empty() {
                    None
                } else {
                    Some(shared_secret)
                },
                shared_secret_file: None,
                state_file: None,
                force: false,
                json: false,
            })
            .await;
        }
        3 => {
            return handle_setup_token(
                SetupTokenArgs {
                    ttl: "15m".to_string(),
                    json: false,
                    state_file: None,
                },
                daemon_url,
            )
            .await;
        }
        _ => {}
    }

    // ── Step 0: Check daemon connectivity and get current state ──

    let status: SetupStatus = match fetch_setup_status(&client, daemon_url).await {
        Ok(status) => status,
        Err(e) => {
            eprintln!(
                "Cannot reach oored at {}. Is the daemon running? Start it with: oored run",
                daemon_url
            );
            return Err(e);
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

        session_token = Some(acquire_session(&client, daemon_url).await?);
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
            session_token = Some(acquire_session(&client, daemon_url).await?);
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
            session_token = Some(acquire_session(&client, daemon_url).await?);
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
            session_token = Some(acquire_session(&client, daemon_url).await?);
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
            println!("Your Oore instance is ready. Run 'oore status' to verify.");
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

async fn handle_login(args: LoginArgs) -> anyhow::Result<()> {
    let daemon_url = resolve_daemon_url(args.daemon_url.as_deref())?;
    let client = reqwest::Client::new();

    if let Some(token) = args.token.as_deref() {
        let profile = fetch_user_profile(&client, &daemon_url, token).await.map_err(|e| {
            anyhow::anyhow!(
                "{e}\nToken was rejected. Re-authenticate in web UI or run `oore login` without --token in local mode."
            )
        })?;
        let mut cfg = load_cli_config()?;
        cfg.daemon_url = Some(daemon_url.clone());
        cfg.session_token = Some(token.to_string());
        let config_path = save_cli_config(&cfg)?;

        if args.json {
            let output = serde_json::json!({
                "ok": true,
                "auth_mode": "token_import",
                "daemon_url": daemon_url,
                "config_path": config_path,
                "user": profile.user,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        println!("Token validated and stored.");
        println!("Daemon: {}", daemon_url);
        println!("User:   {}", profile.user.email);
        println!("Config: {}", config_path.display());
        return Ok(());
    }

    let login_url = format!("{}/v1/auth/local/login", daemon_url.trim_end_matches('/'));
    let response = client
        .post(&login_url)
        .json(&LocalLoginRequest { email: args.email })
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {daemon_url}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let msg = extract_error_message(response).await;
        anyhow::bail!(
            "local login failed at {daemon_url} (HTTP {}): {}",
            status.as_u16(),
            msg
        );
    }

    let login_body: LocalLoginResponse = response
        .json()
        .await
        .context("failed to parse local login response")?;
    let mut cfg = load_cli_config()?;
    cfg.daemon_url = Some(daemon_url.clone());
    cfg.session_token = Some(login_body.session_token.clone());
    let config_path = save_cli_config(&cfg)?;

    if args.json {
        let output = serde_json::json!({
            "ok": true,
            "auth_mode": "local_login",
            "daemon_url": daemon_url,
            "config_path": config_path,
            "expires_at": login_body.expires_at,
            "user": login_body.user,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("Login succeeded (local mode).");
    println!("Daemon:  {}", daemon_url);
    println!("User:    {}", login_body.user.email);
    println!("Expires: {}", format_epoch_local(login_body.expires_at));
    println!("Config:  {}", config_path.display());
    Ok(())
}

async fn handle_status(args: StatusArgs) -> anyhow::Result<()> {
    let daemon_url = resolve_daemon_url(args.daemon_url.as_deref())?;
    let client = reqwest::Client::new();
    let mut summary = StatusSummary {
        daemon_url: daemon_url.clone(),
        setup_status: fetch_setup_status(&client, &daemon_url).await?,
        authenticated: false,
        queue_depth: None,
        active_builds: None,
        recent_builds: None,
        runners: None,
    };

    let session_token = resolve_session_token(args.token.as_deref())?;
    if let Some(token) = session_token.as_deref() {
        match fetch_user_profile(&client, &daemon_url, token).await {
            Ok(_) => {
                summary.authenticated = true;
                let queued =
                    fetch_build_list(&client, &daemon_url, token, Some("queued"), Some(1)).await?;
                let running =
                    fetch_build_list(&client, &daemon_url, token, Some("running"), Some(1)).await?;
                let recent = fetch_build_list(&client, &daemon_url, token, None, Some(5)).await?;
                let runners = fetch_runner_list(&client, &daemon_url, token).await?;
                summary.queue_depth = Some(queued.total);
                summary.active_builds = Some(queued.total + running.total);
                summary.recent_builds = Some(recent.builds);
                summary.runners = Some(runners.runners);
            }
            Err(err) => {
                if args.token.is_some() {
                    return Err(err);
                }
                eprintln!("Warning: stored token is invalid or expired ({err})");
            }
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!("oore status");
    println!();
    println!("Daemon:   {}", summary.daemon_url);
    println!("Instance: {}", summary.setup_status.instance_id);
    println!("State:    {}", summary.setup_status.state);
    println!("Mode:     {}", summary.setup_status.runtime_mode);
    println!(
        "Setup:    {}",
        if summary.setup_status.setup_mode {
            "in_progress"
        } else {
            "complete"
        }
    );

    if summary.authenticated {
        println!();
        println!("Authenticated: yes");
        if let Some(queue_depth) = summary.queue_depth {
            println!("Queue depth:   {}", queue_depth);
        }
        if let Some(active_builds) = summary.active_builds {
            println!("Active builds: {}", active_builds);
        }
        if let Some(runners) = &summary.runners {
            let online = runners
                .iter()
                .filter(|runner| runner.status == "online" || runner.status == "busy")
                .count();
            println!(
                "Runners:       {} total ({} online/busy)",
                runners.len(),
                online
            );
        }
        if let Some(recent) = &summary.recent_builds {
            if recent.is_empty() {
                println!("Recent builds: none");
            } else {
                println!("Recent builds:");
                for build in recent {
                    println!(
                        "  - #{} {} ({})",
                        build.build_number, build.status, build.trigger_type
                    );
                }
            }
        }
    } else {
        println!();
        println!("Authenticated: no");
        println!(
            "Tip: run `oore login` (or `oore login --token <session_token>`) for queue/build/runner details."
        );
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
            if let Some(stripped) = p.strip_prefix("~/") {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(stripped)
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

fn config_key_supported(key: &str) -> bool {
    matches!(key, CONFIG_KEY_DAEMON_URL | CONFIG_KEY_SESSION_TOKEN)
}

fn handle_config_set(args: ConfigSetArgs) -> anyhow::Result<()> {
    let mut cfg = load_cli_config()?;
    match args.key.as_str() {
        CONFIG_KEY_DAEMON_URL => {
            let value = args.value.trim();
            if value.is_empty() {
                anyhow::bail!("{CONFIG_KEY_DAEMON_URL} cannot be empty");
            }
            cfg.daemon_url = Some(value.to_string());
        }
        CONFIG_KEY_SESSION_TOKEN => {
            let value = args.value.trim();
            if value.is_empty() {
                anyhow::bail!("{CONFIG_KEY_SESSION_TOKEN} cannot be empty");
            }
            cfg.session_token = Some(value.to_string());
        }
        _ => anyhow::bail!("unsupported config key"),
    }

    let path = save_cli_config(&cfg)?;
    println!("Saved {} to {}", args.key, path.display());
    Ok(())
}

fn handle_config_get(args: ConfigGetArgs) -> anyhow::Result<()> {
    let cfg = load_cli_config()?;
    match args.key.as_str() {
        CONFIG_KEY_DAEMON_URL => match cfg.daemon_url {
            Some(value) if !value.trim().is_empty() => {
                println!("{}", value);
                Ok(())
            }
            _ => anyhow::bail!("{CONFIG_KEY_DAEMON_URL} is not set"),
        },
        CONFIG_KEY_SESSION_TOKEN => match cfg.session_token {
            Some(value) if !value.trim().is_empty() => {
                println!("{}", value);
                Ok(())
            }
            _ => anyhow::bail!("{CONFIG_KEY_SESSION_TOKEN} is not set"),
        },
        _ => anyhow::bail!("unsupported config key"),
    }
}

fn command_output(cmd: &str, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new(cmd).args(args).output().ok()
}

fn command_version(cmd: &str, args: &[&str]) -> Option<String> {
    command_output(cmd, args)
        .and_then(|out| {
            if out.status.success() {
                let output = if out.stdout.is_empty() {
                    out.stderr
                } else {
                    out.stdout
                };
                String::from_utf8(output).ok()
            } else {
                None
            }
        })
        .and_then(|s| s.lines().next().map(|line| line.trim().to_string()))
}

fn doctor_check(
    name: &str,
    status: &str,
    detail: Option<String>,
    install_hint: Option<&str>,
) -> DoctorCheckResult {
    DoctorCheckResult {
        name: name.to_string(),
        status: status.to_string(),
        detail,
        install_hint: install_hint.map(str::to_string),
    }
}

fn doctor_has_platform(args: &DoctorArgs, platform: DoctorPlatform) -> bool {
    args.all || args.platform.contains(&platform)
}

fn android_sdk_root() -> Option<PathBuf> {
    ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .find_map(|key| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|path| path.is_dir())
        })
}

fn add_xcode_checks(checks: &mut Vec<DoctorCheckResult>) {
    if std::env::consts::OS != "macos" {
        checks.push(doctor_check(
            "xcode",
            "missing",
            Some("iOS and macOS builds require a macOS runner".to_string()),
            Some("run this job on macOS with Xcode installed"),
        ));
        return;
    }

    let developer_dir =
        command_version("xcode-select", &["-p"]).filter(|path| Path::new(path).is_dir());
    let xcode_version = command_version("xcodebuild", &["-version"]);
    if let (Some(developer_dir), Some(xcode_version)) = (developer_dir, xcode_version) {
        checks.push(doctor_check(
            "xcode",
            "ok",
            Some(format!("{} ({})", xcode_version, developer_dir)),
            None,
        ));
    } else {
        checks.push(doctor_check(
            "xcode",
            "missing",
            None,
            Some("install Xcode, then run: sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer"),
        ));
    }
}

fn add_signing_warnings(checks: &mut Vec<DoctorCheckResult>) {
    let codesign = command_output("security", &["find-identity", "-v", "-p", "codesigning"]);
    match codesign {
        Some(output) if output.status.success() => {
            let output = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if output.contains("0 valid identities found") {
                checks.push(doctor_check(
                    "codesign_identity",
                    "warning",
                    Some("0 valid identities found".to_string()),
                    Some("import an Apple development or distribution certificate before signing"),
                ));
            } else {
                checks.push(doctor_check(
                    "codesign_identity",
                    "ok",
                    Some("signing identity available".to_string()),
                    None,
                ));
            }
        }
        _ => checks.push(doctor_check(
            "codesign_identity",
            "warning",
            None,
            Some("import an Apple development or distribution certificate before signing"),
        )),
    }

    if let Some(version) = command_version("xcrun", &["notarytool", "--version"]) {
        checks.push(doctor_check("notarytool", "ok", Some(version), None));
    } else {
        checks.push(doctor_check(
            "notarytool",
            "warning",
            None,
            Some("install a current Xcode version before notarizing macOS apps"),
        ));
    }
}

fn run_doctor_checks(args: DoctorArgs) -> anyhow::Result<()> {
    let mut checks: Vec<DoctorCheckResult> = Vec::new();

    let base_checks: [(&str, &[&str], &str); 3] = [
        ("git", &["--version"], "brew install git"),
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
    ];

    for (name, command_args, install_hint) in base_checks {
        if let Some(version) = command_version(name, command_args) {
            checks.push(doctor_check(name, "ok", Some(version), None));
        } else {
            checks.push(doctor_check(name, "missing", None, Some(install_hint)));
        }
    }

    if doctor_has_platform(&args, DoctorPlatform::Android) {
        if let Some(version) = command_version("java", &["-version"]) {
            checks.push(doctor_check("java", "ok", Some(version), None));
        } else {
            checks.push(doctor_check(
                "java",
                "missing",
                None,
                Some("install a supported JDK and set JAVA_HOME"),
            ));
        }

        if let Some(sdk_root) = android_sdk_root() {
            let adb = sdk_root.join("platform-tools/adb");
            if adb.is_file() {
                checks.push(doctor_check(
                    "android_sdk",
                    "ok",
                    Some(sdk_root.display().to_string()),
                    None,
                ));
            } else {
                checks.push(doctor_check(
                    "android_sdk",
                    "missing",
                    Some(sdk_root.display().to_string()),
                    Some("install Android SDK Platform-Tools in this SDK"),
                ));
            }
        } else {
            checks.push(doctor_check(
                "android_sdk",
                "missing",
                None,
                Some("install Android Studio, then set ANDROID_HOME to its SDK directory"),
            ));
        }
    } else {
        checks.push(doctor_check(
            "android",
            "skipped",
            Some("run `oore doctor --platform android` for Android checks".to_string()),
            None,
        ));
    }

    let apple_selected = doctor_has_platform(&args, DoctorPlatform::Ios)
        || doctor_has_platform(&args, DoctorPlatform::Macos);
    if apple_selected {
        add_xcode_checks(&mut checks);
        add_signing_warnings(&mut checks);
    } else {
        checks.push(doctor_check(
            "apple_platforms",
            "skipped",
            Some(
                "run `oore doctor --platform ios` or `--platform macos` for Xcode checks"
                    .to_string(),
            ),
            None,
        ));
    }

    let missing_count = checks
        .iter()
        .filter(|check| check.status == "missing")
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| check.status == "warning")
        .count();
    let report = DoctorReport {
        checks,
        missing_count,
        warning_count,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("oore doctor -- environment checks");
        for check in &report.checks {
            let detail = check
                .detail
                .as_deref()
                .or(check.install_hint.as_deref())
                .unwrap_or("");
            println!("  [{}] {:<18} {}", check.status, check.name, detail);
        }
        if report.missing_count == 0 {
            println!("All selected required checks passed.");
        } else {
            println!("{} required issue(s) found.", report.missing_count);
        }
        if report.warning_count > 0 {
            println!("{} optional warning(s) found.", report.warning_count);
        }
    }

    if report.missing_count == 0 {
        Ok(())
    } else {
        let missing_names: Vec<String> = report
            .checks
            .iter()
            .filter(|check| check.status == "missing")
            .map(|check| check.name.clone())
            .collect();
        anyhow::bail!("missing required tools: {}", missing_names.join(", "))
    }
}

// ── Self-update helpers ──────────────────────────────────────────

const DEFAULT_GITHUB_REPO: &str = "devaryakjha/oore.build";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseChannel {
    Stable,
    Beta,
    Alpha,
}

impl ReleaseChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }

    fn parse(raw: &str) -> anyhow::Result<Self> {
        match raw.trim().to_lowercase().as_str() {
            "stable" | "prod" | "production" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "alpha" => Ok(Self::Alpha),
            other => anyhow::bail!("invalid channel '{other}', expected: stable|beta|alpha"),
        }
    }

    fn tag_marker(self) -> Option<&'static str> {
        match self {
            Self::Stable => None,
            Self::Alpha => Some("-alpha."),
            Self::Beta => Some("-beta."),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, serde::Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

fn read_trimmed_file(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn parse_semver_loose(raw: &str) -> anyhow::Result<semver::Version> {
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_prefix('v').unwrap_or(trimmed);
    semver::Version::parse(trimmed).with_context(|| format!("invalid semver version: {raw}"))
}

fn infer_channel_from_version(v: &semver::Version) -> ReleaseChannel {
    let pre = v.pre.as_str();
    if pre.starts_with("alpha") {
        ReleaseChannel::Alpha
    } else if pre.starts_with("beta") {
        ReleaseChannel::Beta
    } else {
        ReleaseChannel::Stable
    }
}

fn read_installed_channel(install_root: &Path) -> Option<ReleaseChannel> {
    let raw = read_trimmed_file(&install_root.join("CHANNEL"))?;
    ReleaseChannel::parse(&raw).ok()
}

fn read_installed_repo(install_root: &Path) -> Option<String> {
    read_trimmed_file(&install_root.join("GITHUB_REPO"))
}

fn github_token() -> Option<String> {
    for key in ["OORE_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(val) = std::env::var(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn github_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("oore/{}/update", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build HTTP client")
}

async fn fetch_latest_release(
    client: &reqwest::Client,
    repo: &str,
    channel: ReleaseChannel,
) -> anyhow::Result<GitHubRelease> {
    if !repo.contains('/') {
        anyhow::bail!("invalid GitHub repo '{repo}', expected: owner/name");
    }

    let token = github_token();
    let base = format!("https://api.github.com/repos/{repo}");

    let mut req = match channel {
        ReleaseChannel::Stable => client.get(format!("{base}/releases/latest")),
        ReleaseChannel::Alpha | ReleaseChannel::Beta => {
            client.get(format!("{base}/releases?per_page=100"))
        }
    };

    req = req
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(token) = token {
        req = req.bearer_auth(token);
    }

    if channel == ReleaseChannel::Stable {
        let rel: GitHubRelease = req
            .send()
            .await
            .context("failed to fetch latest stable release")?
            .error_for_status()
            .context("latest stable release request failed")?
            .json()
            .await
            .context("failed to parse latest stable release JSON")?;

        if rel.draft {
            anyhow::bail!("latest stable release is a draft (tag: {})", rel.tag_name);
        }
        if rel.prerelease {
            anyhow::bail!(
                "latest stable release is marked prerelease (tag: {}), refusing to use it for stable channel",
                rel.tag_name
            );
        }

        return Ok(rel);
    }

    let list: Vec<GitHubRelease> = req
        .send()
        .await
        .context("failed to fetch release list")?
        .error_for_status()
        .context("release list request failed")?
        .json()
        .await
        .context("failed to parse release list JSON")?;

    let marker = channel.tag_marker().context("missing tag marker")?;
    let rel = list
        .into_iter()
        .find(|r| !r.draft && r.prerelease && r.tag_name.contains(marker))
        .with_context(|| {
            format!(
                "no {channel} release found in {repo}",
                channel = channel.as_str()
            )
        })?;
    Ok(rel)
}

fn find_asset_url(release: &GitHubRelease, name: &str) -> anyhow::Result<String> {
    release
        .assets
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.browser_download_url.clone())
        .with_context(|| format!("release {} is missing asset {name}", release.tag_name))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if !src.is_dir() {
        anyhow::bail!("expected directory: {}", src.display());
    }
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create directory {}", dst.display()))?;

    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry.context("failed to read directory entry")?;
        let ty = entry.file_type().context("failed to read file type")?;
        let from = entry.path();
        let to = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} -> {}", from.display(), to.display())
            })?;
        }
    }

    Ok(())
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

    // Prefer deriving install root from the current executable path so `oore update` works even
    // when users install to a non-default root and don't export `OORE_INSTALL_ROOT`.
    if let Ok(exe) = std::env::current_exe()
        && let Some(bin_dir) = exe.parent()
        && bin_dir.file_name() == Some(std::ffi::OsStr::new("bin"))
        && let Some(root) = bin_dir.parent()
    {
        return Ok(root.to_path_buf());
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

fn endpoint_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

/// Check whether the daemon is reachable at its configured address.
async fn check_daemon_running(client: &reqwest::Client, daemon_url: &str) -> bool {
    endpoint_is_healthy(client, &endpoint_url(daemon_url, "/healthz")).await
}

fn lsof_name_matches_socket(name: &str, listen: SocketAddr) -> bool {
    let name = name.strip_prefix("TCP ").unwrap_or(name);
    let name = name.strip_suffix(" (LISTEN)").unwrap_or(name);
    if name == listen.to_string() {
        return true;
    }
    listen.ip().is_unspecified() && name == format!("*:{}", listen.port())
}

fn oored_listener_pids(lsof_fields: &str, listen: SocketAddr) -> Vec<i32> {
    let mut pid = None;
    let mut command_is_oored = false;
    let mut matches = Vec::new();

    for field in lsof_fields.lines() {
        match field.as_bytes().first() {
            Some(b'p') => {
                pid = field[1..].parse().ok();
                command_is_oored = false;
            }
            Some(b'c') => command_is_oored = field.get(1..) == Some("oored"),
            Some(b'n') if command_is_oored && lsof_name_matches_socket(&field[1..], listen) => {
                if let Some(pid) = pid
                    && !matches.contains(&pid)
                {
                    matches.push(pid);
                }
            }
            _ => {}
        }
    }

    matches
}

fn process_is_oored(pid: i32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let command = String::from_utf8(output.stdout).ok()?;
            Path::new(command.trim())
                .file_name()
                .map(|name| name == "oored")
        })
        .unwrap_or(false)
}

fn terminate_oored(pid: i32) -> bool {
    if !process_is_oored(pid) {
        return false;
    }
    unsafe { libc::kill(pid, 0) == 0 && libc::kill(pid, libc::SIGTERM) == 0 }
}

/// Stop the daemon using PID file first, then lsof fallback (mirrors uninstall.sh).
fn stop_daemon(install_root: &Path, listen: &str) -> anyhow::Result<()> {
    let pid_file = install_root.join("oored.pid");
    let mut stopped = false;

    // Try PID file first
    if pid_file.exists() {
        if let Ok(contents) = fs::read_to_string(&pid_file)
            && let Ok(pid) = contents.trim().parse::<i32>()
            && terminate_oored(pid)
        {
            stopped = true;
        }
        let _ = fs::remove_file(&pid_file);
    }

    let listen = listen
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid daemon listen address: {listen}"))?;

    // Ask by port, then filter the structured output by exact address and command.
    if let Ok(output) = std::process::Command::new("lsof")
        .args([
            "-nP",
            "-a",
            &format!("-iTCP:{}", listen.port()),
            "-sTCP:LISTEN",
            "-Fpcn",
        ])
        .output()
        && output.status.success()
    {
        for pid in oored_listener_pids(&String::from_utf8_lossy(&output.stdout), listen) {
            if terminate_oored(pid) {
                stopped = true;
            }
        }
    }
    if stopped {
        std::thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}

/// Restart the daemon and verify it becomes healthy.
async fn restart_daemon(
    install_root: &Path,
    listen: &str,
    daemon_url: &str,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    let bin_dir = install_root.join("bin");
    let oored_bin = bin_dir.join("oored");
    let log_path = install_root.join("logs").join("oored.log");
    let pid_file = install_root.join("oored.pid");

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory: {}", parent.display()))?;
    }

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log file: {}", log_path.display()))?;

    let child = std::process::Command::new(&oored_bin)
        .args(["run", "--listen", listen])
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
        if check_daemon_running(client, daemon_url).await {
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

const BACKUP_DATABASE_FILE: &str = "oore.db";
const BACKUP_KEY_FILE: &str = "encryption.key";
const BACKUP_MANIFEST_FILE: &str = "manifest.json";

fn resolve_key_path() -> anyhow::Result<PathBuf> {
    Ok(resolve_data_dir()?.join(BACKUP_KEY_FILE))
}

fn set_private_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "failed to set restrictive permissions on {}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn snapshot_sqlite_path(source: &Path, destination: &Path) -> anyhow::Result<()> {
    use std::str::FromStr;

    let source_url = format!("sqlite://{}?mode=rw", source.display());
    let options = SqliteConnectOptions::from_str(&source_url)
        .with_context(|| format!("failed to open SQLite database {}", source.display()))?;
    let destination = destination.display().to_string().replace('\'', "''");
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("failed to connect to SQLite database")?;
        sqlx::query(&format!("VACUUM INTO '{destination}'"))
            .execute(&pool)
            .await
            .context("failed to create a consistent SQLite snapshot")?;
        pool.close().await;
        Ok::<(), anyhow::Error>(())
    })
}

fn sqlite_integrity_check(path: &Path) -> anyhow::Result<()> {
    use std::str::FromStr;

    let source_url = format!("sqlite://{}?mode=ro", path.display());
    let options = SqliteConnectOptions::from_str(&source_url)
        .with_context(|| format!("failed to open SQLite snapshot {}", path.display()))?;
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("failed to connect to SQLite snapshot")?;
        let result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .context("failed to run SQLite integrity check")?;
        pool.close().await;
        if result != "ok" {
            anyhow::bail!("SQLite integrity check failed: {result}");
        }
        Ok::<(), anyhow::Error>(())
    })
}

fn unpack_backup(input: &Path, destination: &Path) -> anyhow::Result<BackupManifest> {
    let file = fs::File::open(input)
        .with_context(|| format!("failed to open backup {}", input.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut found = HashMap::new();

    for entry in archive.entries().context("failed to read backup archive")? {
        let mut entry = entry.context("failed to read backup entry")?;
        let path = entry.path().context("invalid backup entry path")?;
        let name = path
            .to_str()
            .context("backup entry path is not UTF-8")?
            .to_string();
        if !matches!(
            name.as_str(),
            BACKUP_DATABASE_FILE | BACKUP_KEY_FILE | BACKUP_MANIFEST_FILE
        ) || path.components().count() != 1
        {
            anyhow::bail!("backup contains unsupported path: {name}");
        }
        if found.insert(name.clone(), ()).is_some() {
            anyhow::bail!("backup contains duplicate path: {name}");
        }
        entry
            .unpack(destination.join(&name))
            .with_context(|| format!("failed to unpack backup entry {name}"))?;
    }

    for name in [BACKUP_DATABASE_FILE, BACKUP_KEY_FILE, BACKUP_MANIFEST_FILE] {
        if !found.contains_key(name) {
            anyhow::bail!("backup is missing {name}");
        }
    }

    let manifest = serde_json::from_slice::<BackupManifest>(
        &fs::read(destination.join(BACKUP_MANIFEST_FILE))
            .context("failed to read backup manifest")?,
    )
    .context("failed to parse backup manifest")?;
    if manifest.format != "oore-backup-v1" {
        anyhow::bail!("unsupported backup format: {}", manifest.format);
    }
    for name in [BACKUP_DATABASE_FILE, BACKUP_KEY_FILE] {
        let expected = manifest
            .files
            .get(name)
            .with_context(|| format!("backup manifest is missing checksum for {name}"))?;
        let actual = hex::encode(Sha256::digest(
            fs::read(destination.join(name))
                .with_context(|| format!("failed to read backup {name}"))?,
        ));
        if expected != &actual {
            anyhow::bail!("backup checksum mismatch for {name}");
        }
    }

    let key = fs::read(destination.join(BACKUP_KEY_FILE)).context("failed to read backup key")?;
    if key.len() != 32 {
        anyhow::bail!("backup encryption key has invalid length");
    }
    sqlite_integrity_check(&destination.join(BACKUP_DATABASE_FILE))?;
    Ok(manifest)
}

fn backup_create(args: BackupCreateArgs) -> anyhow::Result<()> {
    let database = resolve_db_path(args.state_file.as_deref())?;
    let key = resolve_key_path()?;
    if !database.is_file() {
        anyhow::bail!("database does not exist: {}", database.display());
    }
    if !key.is_file() {
        anyhow::bail!("encryption key does not exist: {}", key.display());
    }
    if args.output.exists() {
        anyhow::bail!(
            "backup destination already exists: {}",
            args.output.display()
        );
    }
    let parent = args
        .output
        .parent()
        .context("backup output must have a parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create backup directory {}", parent.display()))?;

    let stage =
        tempfile::tempdir_in(parent).context("failed to create backup staging directory")?;
    let snapshot = stage.path().join(BACKUP_DATABASE_FILE);
    snapshot_sqlite_path(&database, &snapshot)?;
    let staged_key = stage.path().join(BACKUP_KEY_FILE);
    fs::copy(&key, &staged_key).context("failed to copy encryption key into backup")?;
    set_private_permissions(&staged_key)?;

    let mut files = HashMap::new();
    for name in [BACKUP_DATABASE_FILE, BACKUP_KEY_FILE] {
        files.insert(
            name.to_string(),
            hex::encode(Sha256::digest(fs::read(stage.path().join(name))?)),
        );
    }
    let manifest = BackupManifest {
        format: "oore-backup-v1".to_string(),
        created_at: now_epoch_secs(),
        files,
    };
    fs::write(
        stage.path().join(BACKUP_MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest).context("failed to serialize backup manifest")?,
    )
    .context("failed to write backup manifest")?;

    let temp_output = parent.join(format!(
        ".{}.tmp",
        args.output
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ));
    let file = fs::File::create(&temp_output)
        .with_context(|| format!("failed to create backup {}", temp_output.display()))?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for name in [BACKUP_DATABASE_FILE, BACKUP_KEY_FILE, BACKUP_MANIFEST_FILE] {
        archive
            .append_path_with_name(stage.path().join(name), name)
            .with_context(|| format!("failed to add {name} to backup"))?;
    }
    archive
        .finish()
        .context("failed to finish backup archive")?;
    let encoder = archive
        .into_inner()
        .context("failed to finalize backup archive")?;
    encoder
        .finish()
        .context("failed to finalize backup compression")?;
    set_private_permissions(&temp_output)?;
    fs::rename(&temp_output, &args.output)
        .with_context(|| format!("failed to publish backup {}", args.output.display()))?;
    println!("Created backup: {}", args.output.display());
    Ok(())
}

fn backup_verify(args: BackupVerifyArgs) -> anyhow::Result<()> {
    let stage = tempfile::tempdir().context("failed to create backup verification directory")?;
    let manifest = unpack_backup(&args.input, stage.path())?;
    println!(
        "Backup verified: {} (created {})",
        args.input.display(),
        manifest.created_at
    );
    Ok(())
}

fn database_is_open(path: &Path) -> bool {
    std::process::Command::new("lsof")
        .args(["-t", "--", &path.display().to_string()])
        .output()
        .map(|output| output.status.success() && !output.stdout.is_empty())
        .unwrap_or(false)
}

fn backup_restore(args: BackupRestoreArgs) -> anyhow::Result<()> {
    let client = github_client()?;
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    let daemon_url = resolve_daemon_url(None)?;
    if runtime.block_on(check_daemon_running(&client, &daemon_url)) {
        anyhow::bail!(
            "refusing restore while oored is running; stop the managed service or daemon first"
        );
    }

    let database = resolve_db_path(args.state_file.as_deref())?;
    if database_is_open(&database) {
        anyhow::bail!(
            "refusing restore while the state database is open; stop the managed or unmanaged oored process first"
        );
    }
    let stage = tempfile::tempdir().context("failed to create restore staging directory")?;
    unpack_backup(&args.input, stage.path())?;
    let key = resolve_key_path()?;
    for path in [&database, &key] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create restore directory {}", parent.display())
            })?;
        }
    }

    let nonce = now_epoch_secs();
    let database_old = database.with_extension(format!("restore-{nonce}.old"));
    let key_old = key.with_extension(format!("restore-{nonce}.old"));
    let database_new = database.with_extension(format!("restore-{nonce}.new"));
    let key_new = key.with_extension(format!("restore-{nonce}.new"));
    fs::copy(stage.path().join(BACKUP_DATABASE_FILE), &database_new)?;
    fs::copy(stage.path().join(BACKUP_KEY_FILE), &key_new)?;
    set_private_permissions(&key_new)?;

    let rollback = || -> anyhow::Result<()> {
        if database_old.exists() {
            fs::rename(&database_old, &database)?;
        }
        if key_old.exists() {
            fs::rename(&key_old, &key)?;
        }
        Ok(())
    };
    if database.exists() {
        fs::rename(&database, &database_old)?;
    }
    if key.exists() {
        fs::rename(&key, &key_old)?;
    }
    let result = (|| -> anyhow::Result<()> {
        fs::rename(&database_new, &database)?;
        fs::rename(&key_new, &key)?;
        set_private_permissions(&key)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&database);
        let _ = fs::remove_file(&key);
        rollback().context("restore failed and rollback failed")?;
        return Err(error);
    }
    let _ = fs::remove_file(database_old);
    let _ = fs::remove_file(key_old);
    println!("Restored backup: {}", args.input.display());
    Ok(())
}

const DAEMON_SERVICE_LABEL: &str = "build.oore.oored";
const WEB_SERVICE_LABEL: &str = "build.oore.oore-web";

fn launch_agent_plist(label: &str) -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("could not determine home directory")?
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist")))
}

fn managed_service_plist(label: &str) -> anyhow::Result<(PathBuf, bool)> {
    let system_plist = PathBuf::from("/Library/LaunchDaemons").join(format!("{label}.plist"));
    if system_plist.is_file() {
        return Ok((system_plist, true));
    }
    Ok((launch_agent_plist(label)?, false))
}

fn url_from_socket_address(address: SocketAddr) -> String {
    let ip = match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, address.port()))
}

fn url_from_listen_address(listen: &str) -> anyhow::Result<String> {
    let listen = listen.trim();
    if listen.is_empty() {
        anyhow::bail!("service listen address is empty");
    }
    if let Ok(address) = listen.parse::<SocketAddr>() {
        return Ok(url_from_socket_address(address));
    }

    let url = url::Url::parse(listen)
        .or_else(|_| url::Url::parse(&format!("http://{listen}")))
        .with_context(|| format!("invalid service listen address: {listen}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("unsupported service URL scheme: {}", url.scheme());
    }
    let host = url
        .host_str()
        .context("service listen URL is missing a host")?;
    let port = url.port().unwrap_or(80);
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(url_from_socket_address(SocketAddr::new(ip, port)));
    }
    Ok(format!("http://{host}:{port}"))
}

fn daemon_url_from_listen_address(listen: &str) -> anyhow::Result<String> {
    let address = listen
        .trim()
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid daemon listen address: {listen}"))?;
    Ok(url_from_socket_address(address))
}

fn daemon_listen_address_from_url(daemon_url: &str) -> anyhow::Result<String> {
    let url =
        url::Url::parse(daemon_url).with_context(|| format!("invalid daemon URL: {daemon_url}"))?;
    if url.scheme() != "http" {
        anyhow::bail!("unmanaged daemon URL must use http: {daemon_url}");
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!(
            "unmanaged daemon URL must not include a path, query, or fragment: {daemon_url}"
        );
    }
    let host = url
        .host_str()
        .context("unmanaged daemon URL is missing a host")?;
    let ip = if host.eq_ignore_ascii_case("localhost") {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        host.parse::<IpAddr>()
            .with_context(|| format!("unmanaged daemon URL must use an IP address: {daemon_url}"))?
    };
    let port = url
        .port_or_known_default()
        .context("unmanaged daemon URL is missing a port")?;
    Ok(SocketAddr::new(ip, port).to_string())
}

fn plist_program_arguments(plist: &Path) -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("plutil")
        .args(["-extract", "ProgramArguments", "json", "-o", "-"])
        .arg(plist)
        .output()
        .with_context(|| format!("failed to read launchd plist {}", plist.display()))?;
    if !output.status.success() {
        anyhow::bail!("failed to read ProgramArguments from {}", plist.display());
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("invalid ProgramArguments in {}", plist.display()))
}

fn listen_address_from_program_arguments(program_args: &[String]) -> anyhow::Result<String> {
    for (index, arg) in program_args.iter().enumerate() {
        if arg == "--listen" {
            return program_args
                .get(index + 1)
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .context("--listen has no value");
        }
        if let Some(value) = arg.strip_prefix("--listen=")
            && !value.trim().is_empty()
        {
            return Ok(value.to_string());
        }
    }
    anyhow::bail!("service ProgramArguments do not include --listen")
}

fn managed_service_listen_address(label: &str) -> anyhow::Result<String> {
    let (plist, _) = managed_service_plist(label)?;
    if !plist.is_file() {
        anyhow::bail!("managed service plist is missing: {}", plist.display());
    }
    let program_args = plist_program_arguments(&plist)?;
    listen_address_from_program_arguments(&program_args)
        .with_context(|| format!("failed to find listen address in {}", plist.display()))
}

fn launchd_service_loaded(label: &str) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let Ok((_, system)) = managed_service_plist(label) else {
        return false;
    };
    let service = if system {
        format!("system/{label}")
    } else {
        let Ok(uid) = std::process::Command::new("id").arg("-u").output() else {
            return false;
        };
        let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        format!("gui/{uid}/{label}")
    };
    std::process::Command::new("launchctl")
        .args(["print", &service])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn restart_launchd_service(label: &str) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed service restart is supported on macOS only");
    }
    let (plist, system) = managed_service_plist(label)?;
    if !plist.is_file() {
        anyhow::bail!("managed service plist is missing: {}", plist.display());
    }
    let (service, domain) = if system {
        (format!("system/{label}"), "system".to_string())
    } else {
        let uid = std::process::Command::new("id")
            .arg("-u")
            .output()
            .context("failed to determine current user id")?;
        let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        (format!("gui/{uid}/{label}"), format!("gui/{uid}"))
    };
    let command = if system { "sudo" } else { "launchctl" };
    let prefix: &[&str] = if system { &["launchctl"] } else { &[] };
    let _ = std::process::Command::new(command)
        .args(prefix)
        .args(["bootout", &service])
        .status();
    let status = std::process::Command::new(command)
        .args(prefix)
        .args(["bootstrap", &domain, &plist.display().to_string()])
        .status()
        .context("failed to bootstrap launchd service")?;
    if !status.success() {
        anyhow::bail!("failed to bootstrap managed service {label}");
    }
    let _ = std::process::Command::new(command)
        .args(prefix)
        .args(["kickstart", "-k", &service])
        .status();
    Ok(())
}

async fn endpoint_is_healthy(client: &reqwest::Client, url: &str) -> bool {
    client
        .get(url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

async fn wait_for_endpoint(client: &reqwest::Client, url: &str, name: &str) -> anyhow::Result<()> {
    for _ in 0..15 {
        if endpoint_is_healthy(client, url).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("{name} failed its readiness check: {url}")
}

fn copy_release_snapshot(install_root: &Path, snapshot: &Path) -> anyhow::Result<()> {
    for relative in [
        "bin/oore",
        "bin/oored",
        "bin/oore-web",
        "VERSION",
        "CHANNEL",
        "GITHUB_REPO",
        "LICENSE",
    ] {
        let source = install_root.join(relative);
        if source.is_file() {
            let destination = snapshot.join(relative);
            fs::create_dir_all(destination.parent().expect("snapshot file parent"))?;
            fs::copy(&source, &destination)?;
        }
    }
    let web = install_root.join("web-dist");
    if web.is_dir() {
        copy_dir_recursive(&web, &snapshot.join("web-dist"))?;
    }
    Ok(())
}

fn atomic_replace_file(source: &Path, destination: &Path, executable: bool) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("release destination has no parent")?;
    fs::create_dir_all(parent)?;
    let next = parent.join(format!(
        ".{}.update-{}",
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        std::process::id()
    ));
    fs::copy(source, &next)
        .with_context(|| format!("failed to stage {}", destination.display()))?;
    if executable {
        set_executable(&next)?;
    }
    fs::rename(&next, destination)
        .with_context(|| format!("failed to atomically replace {}", destination.display()))?;
    Ok(())
}

fn atomic_replace_directory(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("release destination has no parent")?;
    let stem = destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let next = parent.join(format!(".{stem}.update-{}", std::process::id()));
    let old = parent.join(format!(".{stem}.previous-{}", std::process::id()));
    if next.exists() {
        fs::remove_dir_all(&next)?;
    }
    copy_dir_recursive(source, &next)?;
    if destination.exists() {
        fs::rename(destination, &old)?;
    }
    if let Err(error) = fs::rename(&next, destination) {
        if old.exists() {
            let _ = fs::rename(&old, destination);
        }
        return Err(error)
            .with_context(|| format!("failed to atomically replace {}", destination.display()));
    }
    if old.exists() {
        fs::remove_dir_all(old)?;
    }
    Ok(())
}

fn restore_release_snapshot(install_root: &Path, snapshot: &Path) -> anyhow::Result<()> {
    for relative in [
        "bin/oore",
        "bin/oored",
        "bin/oore-web",
        "VERSION",
        "CHANNEL",
        "GITHUB_REPO",
        "LICENSE",
    ] {
        let source = snapshot.join(relative);
        let destination = install_root.join(relative);
        if source.is_file() {
            atomic_replace_file(&source, &destination, relative.starts_with("bin/"))?;
        } else if destination.exists() {
            fs::remove_file(destination)?;
        }
    }
    let source = snapshot.join("web-dist");
    let destination = install_root.join("web-dist");
    if source.is_dir() {
        atomic_replace_directory(&source, &destination)?;
    } else if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    Ok(())
}

fn install_staged_release(
    stage: &Path,
    install_root: &Path,
    channel: ReleaseChannel,
    repo: &str,
) -> anyhow::Result<()> {
    for (relative, executable) in [
        ("bin/oore", true),
        ("bin/oored", true),
        ("bin/oore-web", true),
        ("VERSION", false),
    ] {
        let source = stage.join(relative);
        if source.is_file() {
            atomic_replace_file(&source, &install_root.join(relative), executable)?;
        }
    }
    let web = stage.join("web-dist");
    if web.is_dir() {
        atomic_replace_directory(&web, &install_root.join("web-dist"))?;
    }
    let license = stage.join("LICENSE");
    if license.is_file() {
        atomic_replace_file(&license, &install_root.join("LICENSE"), false)?;
    }
    fs::write(install_root.join("CHANNEL"), channel.as_str())?;
    fs::write(install_root.join("GITHUB_REPO"), repo)?;
    Ok(())
}

async fn run_blocking_update_step<F, T>(operation: F) -> anyhow::Result<T>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .context("blocking update step failed")?
}

async fn handle_update(args: UpdateArgs) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;

    let current_str = read_trimmed_file(&install_root.join("VERSION"))
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let current = parse_semver_loose(&current_str).context("failed to parse current version")?;

    let repo = args
        .repo
        .clone()
        .or_else(|| read_installed_repo(&install_root))
        .unwrap_or_else(|| DEFAULT_GITHUB_REPO.to_string());

    let channel = if let Some(raw) = &args.channel {
        ReleaseChannel::parse(raw)?
    } else if let Some(ch) = read_installed_channel(&install_root) {
        ch
    } else {
        infer_channel_from_version(&current)
    };

    let client = github_client()?;

    // 1. Fetch latest release metadata for the selected channel
    let rel = fetch_latest_release(&client, &repo, channel).await?;
    let latest_str = rel.tag_name.trim().trim_start_matches('v').to_string();
    let latest = parse_semver_loose(&latest_str)
        .with_context(|| format!("invalid version in release tag: {}", rel.tag_name))?;

    // 2. Compare versions
    println!("Channel:         {}", channel.as_str());
    println!("GitHub repo:     {repo}");
    println!("Current version: {current}");
    println!("Latest version:  {latest} ({})", rel.tag_name);

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

    // 4. Select archive + checksum assets for this architecture
    let arch = release_arch()?;
    let archive_filename = format!("oore_{latest_str}_darwin_{arch}.tar.gz");
    let checksums_filename = format!("oore_{latest_str}_checksums.txt");
    let archive_url = find_asset_url(&rel, &archive_filename)?;
    let checksums_url = find_asset_url(&rel, &checksums_filename)?;

    println!("Downloading {archive_filename}...");

    // 5. Download archive + checksums in parallel
    let (archive_resp, checksums_resp) = tokio::try_join!(
        async {
            client
                .get(&archive_url)
                .send()
                .await
                .context("failed to download archive")?
                .error_for_status()
                .context("archive download failed")
        },
        async {
            client
                .get(&checksums_url)
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
    let expected_hash = parse_checksum(&checksums_text, &archive_filename)?;
    let actual_hash = hex::encode(Sha256::digest(&archive_bytes));

    if actual_hash != expected_hash {
        anyhow::bail!("Checksum mismatch!\n  Expected: {expected_hash}\n  Actual:   {actual_hash}");
    }
    println!("Checksum verified (SHA-256).");

    // 7. Extract outside the install then stage the verified release inside it.
    let tmpdir = tempfile::tempdir().context("failed to create temporary directory")?;
    let decoder = flate2::read::GzDecoder::new(&archive_bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(tmpdir.path())
        .context("failed to extract archive")?;

    // 8. Verify expected files exist.
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

    // Keep both a restorable release snapshot and a consistent data backup before
    // touching the installed files. The staging directory is inside the install
    // root so the final renames stay on one filesystem.
    fs::create_dir_all(&install_root)
        .with_context(|| format!("failed to create install root {}", install_root.display()))?;
    let update_stage = tempfile::Builder::new()
        .prefix(".update-")
        .tempdir_in(&install_root)
        .context("failed to create update staging directory")?;
    let staged_release = update_stage.path().join("release");
    copy_dir_recursive(tmpdir.path(), &staged_release)?;
    let previous_release = update_stage.path().join("previous");
    copy_release_snapshot(&install_root, &previous_release)?;

    let backup_dir = install_root.join("backups");
    let backup_path = backup_dir.join(format!(
        "pre-update-{}-{}.tar.gz",
        current,
        now_epoch_secs()
    ));
    let backup_output = backup_path.clone();
    run_blocking_update_step(move || {
        backup_create(BackupCreateArgs {
            output: backup_output,
            state_file: None,
        })
    })
    .await?;
    println!("Created pre-update backup: {}", backup_path.display());

    // 9. Preserve running service state. The managed service plist is the source
    // of truth for its address; unmanaged daemons use the persisted CLI URL.
    let daemon_is_managed = launchd_service_loaded(DAEMON_SERVICE_LABEL);
    let managed_daemon_listen = daemon_is_managed
        .then(|| managed_service_listen_address(DAEMON_SERVICE_LABEL))
        .transpose()?;
    let daemon_url = match managed_daemon_listen.as_deref() {
        Some(listen) => daemon_url_from_listen_address(listen)?,
        None => resolve_daemon_url(None)?,
    };
    let daemon_was_running = check_daemon_running(&client, &daemon_url).await;
    let unmanaged_daemon_listen = if daemon_was_running && !daemon_is_managed {
        Some(daemon_listen_address_from_url(&daemon_url)?)
    } else {
        None
    };
    let daemon_should_be_running = daemon_is_managed || daemon_was_running;
    let daemon_ready_url = endpoint_url(&daemon_url, "/readyz");

    let web_is_managed = launchd_service_loaded(WEB_SERVICE_LABEL);
    let web_ready_url = web_is_managed
        .then(|| -> anyhow::Result<String> {
            let listen = managed_service_listen_address(WEB_SERVICE_LABEL)?;
            Ok(endpoint_url(
                &url_from_listen_address(&listen)?,
                "/__oore_web_healthz",
            ))
        })
        .transpose()?;
    let web_was_running = match web_ready_url.as_deref() {
        Some(url) => endpoint_is_healthy(&client, url).await,
        None => false,
    };

    if let Some(listen) = &unmanaged_daemon_listen {
        println!("oored daemon is running. Stopping before update...");
        stop_daemon(&install_root, listen)?;
    }

    // 10. Atomically install the staged release and only retain it if every
    // readiness check succeeds. On any failure, restore the prior release.
    let update_result = async {
        install_staged_release(&staged_release, &install_root, channel, &repo)?;
        if daemon_is_managed {
            restart_launchd_service(DAEMON_SERVICE_LABEL)?;
        } else if let Some(listen) = &unmanaged_daemon_listen {
            restart_daemon(&install_root, listen, &daemon_url, &client).await?;
        }
        if daemon_should_be_running {
            wait_for_endpoint(&client, &daemon_ready_url, "oored").await?;
        }
        if web_is_managed {
            restart_launchd_service(WEB_SERVICE_LABEL)?;
        } else if web_was_running {
            anyhow::bail!("refusing to replace an unmanaged local web process; install it as the launchd service first");
        }
        if let Some(url) = &web_ready_url {
            wait_for_endpoint(&client, url, "oore-web").await?;
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if let Err(error) = update_result {
        eprintln!("Update failed; restoring the previous release...");
        let rollback = async {
            restore_release_snapshot(&install_root, &previous_release)?;
            if daemon_is_managed {
                restart_launchd_service(DAEMON_SERVICE_LABEL)?;
            } else if let Some(listen) = &unmanaged_daemon_listen {
                restart_daemon(&install_root, listen, &daemon_url, &client).await?;
            }
            if daemon_should_be_running {
                wait_for_endpoint(&client, &daemon_ready_url, "rollback oored").await?;
            }
            if web_is_managed {
                restart_launchd_service(WEB_SERVICE_LABEL)?;
            }
            if let Some(url) = &web_ready_url {
                wait_for_endpoint(&client, url, "rollback oore-web").await?;
            }
            Ok::<(), anyhow::Error>(())
        }
        .await;
        if let Err(rollback_error) = rollback {
            return Err(error.context(format!("rollback also failed: {rollback_error}")));
        }
        return Err(error);
    }
    println!("Updated to version {latest}.");

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
            Some(SetupSubcommand::Init(args)) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_init(args))?;
            }
            Some(SetupSubcommand::Token(args) | SetupSubcommand::Open(args)) => {
                let daemon_url = resolve_daemon_url(setup.daemon_url.as_deref())?;
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_token(args, &daemon_url))?;
            }
            None => {
                let daemon_url = resolve_daemon_url(setup.daemon_url.as_deref())?;
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_setup_interactive(&daemon_url))?;
            }
        },
        Commands::Frontend(frontend) => match frontend.command {
            FrontendSubcommand::Invite(args) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_frontend_invite(args))?;
            }
        },
        Commands::Login(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_login(args))?;
        }
        Commands::Status(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_status(args))?;
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
                if !config_key_supported(&args.key) {
                    eprintln!(
                        "Unsupported config key '{}'. Supported keys: {}, {}",
                        args.key, CONFIG_KEY_DAEMON_URL, CONFIG_KEY_SESSION_TOKEN
                    );
                    std::process::exit(2);
                }
                handle_config_set(args)?;
            }
            ConfigSubcommand::Get(args) => {
                if !config_key_supported(&args.key) {
                    eprintln!(
                        "Unsupported config key '{}'. Supported keys: {}, {}",
                        args.key, CONFIG_KEY_DAEMON_URL, CONFIG_KEY_SESSION_TOKEN
                    );
                    std::process::exit(2);
                }
                handle_config_get(args)?;
            }
        },
        Commands::Doctor(args) => {
            run_doctor_checks(args)?;
        }
        Commands::Pipeline(pipeline) => match pipeline.command {
            PipelineSubcommand::Validate(args) => handle_pipeline_validate(args)?,
        },
        Commands::Version => {
            let install_root = resolve_install_root()?;
            if let Some(v) = read_trimmed_file(&install_root.join("VERSION")) {
                println!("{v}");
            } else {
                println!("{}", env!("CARGO_PKG_VERSION"));
            }
        }
        Commands::Update(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_update(args))?;
        }
        Commands::Backup(args) => match args.command {
            BackupSubcommand::Create(args) => backup_create(args)?,
            BackupSubcommand::Verify(args) => backup_verify(args)?,
            BackupSubcommand::Restore(args) => backup_restore(args)?,
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_update_step_can_run_runtime_backed_backup_work() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let value = runtime
            .block_on(run_blocking_update_step(|| {
                let nested = tokio::runtime::Runtime::new()?;
                nested.block_on(async { Ok::<_, anyhow::Error>(42) })
            }))
            .unwrap();

        assert_eq!(value, 42);
    }

    #[test]
    fn update_rollback_restores_previous_release() {
        let temp = tempfile::tempdir().unwrap();
        let install = temp.path().join("install");
        let stage = temp.path().join("stage");
        let snapshot = temp.path().join("snapshot");

        for root in [&install, &stage] {
            fs::create_dir_all(root.join("bin")).unwrap();
            fs::create_dir_all(root.join("web-dist")).unwrap();
        }
        fs::write(install.join("bin/oore"), "old-binary").unwrap();
        fs::write(install.join("VERSION"), "1.0.0").unwrap();
        fs::write(install.join("web-dist/index.html"), "old-web").unwrap();
        fs::write(stage.join("bin/oore"), "new-binary").unwrap();
        fs::write(stage.join("VERSION"), "2.0.0").unwrap();
        fs::write(stage.join("web-dist/index.html"), "new-web").unwrap();

        copy_release_snapshot(&install, &snapshot).unwrap();
        install_staged_release(&stage, &install, ReleaseChannel::Stable, "oorebuild/oore").unwrap();
        assert_eq!(
            fs::read_to_string(install.join("VERSION")).unwrap(),
            "2.0.0"
        );

        restore_release_snapshot(&install, &snapshot).unwrap();
        assert_eq!(
            fs::read_to_string(install.join("bin/oore")).unwrap(),
            "old-binary"
        );
        assert_eq!(
            fs::read_to_string(install.join("VERSION")).unwrap(),
            "1.0.0"
        );
        assert_eq!(
            fs::read_to_string(install.join("web-dist/index.html")).unwrap(),
            "old-web"
        );
    }

    #[test]
    fn managed_daemon_uses_the_plist_listen_address() {
        let program_args = vec![
            "/Users/me/.oore/bin/oored".to_string(),
            "run".to_string(),
            "--listen".to_string(),
            "10.23.0.8:9876".to_string(),
        ];

        let listen = listen_address_from_program_arguments(&program_args).unwrap();
        assert_eq!(listen, "10.23.0.8:9876");
        assert_eq!(
            daemon_url_from_listen_address(&listen).unwrap(),
            "http://10.23.0.8:9876"
        );
    }

    #[test]
    fn readiness_url_uses_a_reachable_wildcard_bind_address() {
        assert_eq!(
            url_from_listen_address("0.0.0.0:9876").unwrap(),
            "http://127.0.0.1:9876"
        );
        assert_eq!(
            url_from_listen_address("http://web.internal:4174/ignored").unwrap(),
            "http://web.internal:4174"
        );
        assert_eq!(
            daemon_listen_address_from_url("http://10.23.0.8:9876").unwrap(),
            "10.23.0.8:9876"
        );
    }

    #[test]
    fn lsof_fallback_only_selects_oored_on_the_exact_socket() {
        let fields = "\
p101\n\
coored\n\
n10.23.0.9:9876\n\
p102\n\
cother\n\
n10.23.0.8:9876\n\
p103\n\
coored\n\
n10.23.0.8:9876\n";

        assert_eq!(
            oored_listener_pids(fields, "10.23.0.8:9876".parse().unwrap()),
            vec![103]
        );
        assert_eq!(
            oored_listener_pids("p104\ncoored\nn*:9876\n", "0.0.0.0:9876".parse().unwrap()),
            vec![104]
        );
    }
}
