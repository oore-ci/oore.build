use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use chrono::{Local, TimeZone};
use clap::{Args, Parser, Subcommand, ValueEnum};
use oore_contract::{
    ApiError, BootstrapTokenRecord, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse,
    DeferredRuntimeUpdateRequest, LOCAL_RECOVERY_MAX_TTL_SECS, LOCAL_RECOVERY_MIN_TTL_SECS,
    LOCAL_RECOVERY_SOCKET_DIR, LOCAL_RECOVERY_SOCKET_FILE, ListBuildsResponse, ListRunnersResponse,
    LocalLoginRequest, LocalLoginResponse, LocalRecoveryMintRequest, LocalRecoveryMintResponse,
    OidcConfigureRequest, OidcConfigureResponse, RegisterRunnerResponse, RemoteAuthMode,
    RuntimeMode, SetupCompleteResponse, SetupOidcStartRequest, SetupOidcStartResponse,
    SetupOidcVerifyRequest, SetupOidcVerifyResponse, SetupState, SetupStateFile, SetupStatus,
    UserProfileResponse, parse_repository_pipeline_yaml,
};
use oore_runner::RunnerConfig;
use rand::RngCore;
use ring::aead::{self, AES_256_GCM, Aad, BoundKey, NONCE_LEN, Nonce, NonceSequence, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixStream};

mod update_supervisor;

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
    /// Mint a single-use browser recovery link over the local management socket.
    Recovery(RecoveryArgs),
    Status(StatusArgs),
    Runner(RunnerArgs),
    Config(ConfigArgs),
    Doctor(DoctorArgs),
    Pipeline(PipelineArgs),
    /// Print the installed oore version
    Version,
    /// Update oore and oored to the latest release
    Update(UpdateArgs),
    #[command(hide = true)]
    UpdateSupervisor(UpdateSupervisorArgs),
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
    /// `oore-ci/oore.build`.
    #[arg(long, env = "OORE_GITHUB_REPO")]
    repo: Option<String>,

    /// Install an already verified and extracted release using the normal update transaction.
    #[arg(long, hide = true, value_name = "DIRECTORY", conflicts_with = "check")]
    staged_release: Option<PathBuf>,

    /// Migrate a legacy embedded runner into the managed local runner service.
    #[arg(long, hide = true, requires = "staged_release")]
    ensure_managed_runner: bool,

    #[arg(long, hide = true)]
    deferred_parent_pid: Option<u32>,

    #[arg(long, hide = true)]
    deferred_state_file: Option<PathBuf>,

    #[arg(long, hide = true)]
    deferred_key_file: Option<PathBuf>,

    #[arg(long, hide = true)]
    deferred_daemon_url: Option<String>,

    #[arg(long, hide = true)]
    deferred_status_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct UpdateSupervisorArgs {
    #[arg(long)]
    request_file: PathBuf,
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
struct RecoveryArgs {
    /// Account to recover. Required when more than one active account exists.
    #[arg(long)]
    email: Option<String>,

    /// Web UI base URL placed before the non-leaking recovery fragment.
    #[arg(long, env = "OORE_WEB_URL", default_value = "http://127.0.0.1:4173")]
    web_url: String,

    /// How long the single-use capability remains valid (maximum 5m).
    #[arg(long, default_value = "5m")]
    ttl: String,

    /// Path to the setup database file used to locate the management socket.
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,

    /// Print machine-readable output.
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
    /// Install and start the runner as a boot-time macOS service
    InstallService(RunnerServiceArgs),
    /// Stop and remove the managed macOS runner service
    UninstallService,
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

#[derive(Debug, Args)]
struct RunnerServiceArgs {
    /// Path to runner config file
    #[arg(long)]
    config: Option<String>,
    /// Install or repair the runner managed by this local Oore backend
    #[arg(long, default_value = "false")]
    managed_local: bool,
    /// Daemon URL stored when this local installation is enrolled
    #[arg(long, env = "OORE_DAEMON_URL")]
    daemon_url: Option<String>,
    /// Path to the local Oore SQLite database
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
    /// Managed runner name (defaults to hostname)
    #[arg(long)]
    name: Option<String>,
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

fn recovery_socket_path(override_path: Option<&str>) -> anyhow::Result<PathBuf> {
    let database_path = resolve_db_path(override_path)?;
    let parent = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(parent
        .join(LOCAL_RECOVERY_SOCKET_DIR)
        .join(LOCAL_RECOVERY_SOCKET_FILE))
}

fn validate_recovery_socket(path: &Path) -> anyhow::Result<()> {
    // SAFETY: geteuid has no preconditions and does not dereference memory.
    let expected_uid = unsafe { libc::geteuid() };
    let parent = path
        .parent()
        .context("management socket path has no parent directory")?;
    let parent_metadata = fs::symlink_metadata(parent).with_context(|| {
        format!(
            "failed to inspect management socket directory {}",
            parent.display()
        )
    })?;
    if parent_metadata.file_type().is_symlink()
        || !parent_metadata.is_dir()
        || parent_metadata.uid() != expected_uid
        || parent_metadata.mode() & 0o7777 != 0o700
    {
        anyhow::bail!(
            "management socket directory {} must be owned by the current user with mode 0700",
            parent.display()
        );
    }

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect management socket {}", path.display()))?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_socket()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o7777 != 0o600
    {
        anyhow::bail!(
            "management socket {} must be owned by the current user with mode 0600",
            path.display()
        );
    }
    Ok(())
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
        .json(&LocalLoginRequest {
            email: args.email,
            recovery_capability: None,
        })
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

async fn handle_recovery(args: RecoveryArgs) -> anyhow::Result<()> {
    let ttl = parse_ttl(&args.ttl)?;
    let ttl_seconds = ttl.as_secs();
    if !(LOCAL_RECOVERY_MIN_TTL_SECS..=LOCAL_RECOVERY_MAX_TTL_SECS).contains(&ttl_seconds) {
        anyhow::bail!(
            "recovery TTL must be between {}s and {}s",
            LOCAL_RECOVERY_MIN_TTL_SECS,
            LOCAL_RECOVERY_MAX_TTL_SECS
        );
    }

    let socket_path = recovery_socket_path(args.state_file.as_deref())?;
    validate_recovery_socket(&socket_path)?;
    let mut stream = UnixStream::connect(&socket_path).await.with_context(|| {
        format!(
            "failed to connect to local recovery socket {}",
            socket_path.display()
        )
    })?;
    let request = LocalRecoveryMintRequest {
        email: args.email,
        ttl_seconds,
    };
    let mut encoded = serde_json::to_vec(&request).context("failed to encode recovery request")?;
    encoded.push(b'\n');
    stream
        .write_all(&encoded)
        .await
        .context("failed to write recovery request")?;
    stream
        .shutdown()
        .await
        .context("failed to finish recovery request")?;

    let mut response_bytes = Vec::new();
    stream
        .take(4097)
        .read_to_end(&mut response_bytes)
        .await
        .context("failed to read recovery response")?;
    if response_bytes.len() > 4096 {
        anyhow::bail!("local recovery response exceeded 4096 bytes");
    }
    let response: LocalRecoveryMintResponse =
        serde_json::from_slice(&response_bytes).context("invalid local recovery response")?;
    let (capability, expires_at, user_email) = match response {
        LocalRecoveryMintResponse::Success {
            capability,
            expires_at,
            user_email,
        } => (capability, expires_at, user_email),
        LocalRecoveryMintResponse::Error { error } => {
            anyhow::bail!("local recovery failed ({}): {}", error.code, error.error)
        }
    };

    let mut recovery_url =
        url::Url::parse(&args.web_url).context("web URL must be an absolute URL")?;
    if !matches!(recovery_url.scheme(), "http" | "https")
        || !recovery_url.username().is_empty()
        || recovery_url.password().is_some()
        || recovery_url.host_str().is_none()
    {
        anyhow::bail!("web URL must be an HTTP(S) URL without embedded credentials");
    }
    recovery_url.set_path("/login");
    recovery_url.set_query(None);
    recovery_url.set_fragment(Some(&format!("recovery={capability}")));

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "recovery_url": recovery_url.as_str(),
                "expires_at": expires_at,
                "user_email": user_email,
            }))?
        );
        return Ok(());
    }

    println!("Recovery link created for {user_email}.");
    println!("Expires: {}", format_epoch_local(expires_at));
    println!("Open this single-use URL:");
    println!("{}", recovery_url.as_str());
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

async fn handle_runner_register(args: RunnerRegisterArgs) -> anyhow::Result<()> {
    oore_runner::require_safe_daemon_url(&args.daemon_url)?;
    let name = args.name.unwrap_or_else(oore_runner::get_hostname);

    let capabilities = oore_runner::detect_capabilities().await;

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

    let config = RunnerConfig {
        runner_id: result.runner.id.clone(),
        runner_token: result.token,
        daemon_url: args.daemon_url,
        name: result.runner.name.clone(),
    };

    let config_path = runner_config_path(None)?;
    write_runner_config(&config_path, &config)?;

    println!("Runner registered successfully!");
    println!("  ID: {}", result.runner.id);
    println!("  Name: {}", result.runner.name);
    println!("  Config saved to: {}", config_path.display());
    println!("\nStart the runner with: oore runner start");

    Ok(())
}

async fn handle_runner_start(args: RunnerStartArgs) -> anyhow::Result<()> {
    let config_path = runner_config_path(args.config)?;

    let config: RunnerConfig = serde_json::from_str(
        &fs::read_to_string(&config_path)
            .context("Runner not registered. Run 'oore runner register' first.")?,
    )?;

    let daemon_url = args.daemon_url.unwrap_or(config.daemon_url.clone());

    oore_runner::run_runner_forever(config, Some(daemon_url)).await
}

fn runner_config_path(value: Option<String>) -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    let path = match value {
        Some(path) if path == "~" => home,
        Some(path) if path.starts_with("~/") => home.join(&path[2..]),
        Some(path) => PathBuf::from(path),
        None => home.join(".oore/runner.json"),
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()
            .context("failed to resolve the current directory")?
            .join(path))
    }
}

fn read_runner_config(path: &Path) -> anyhow::Result<Option<RunnerConfig>> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read runner config {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse runner config {}", path.display()))
        .map(Some)
}

fn write_runner_config(path: &Path, config: &RunnerConfig) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create runner config directory {}",
            parent.display()
        )
    })?;
    let mut staged = tempfile::Builder::new()
        .prefix(".runner-config-")
        .tempfile_in(parent)
        .with_context(|| format!("failed to stage runner config in {}", parent.display()))?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer_pretty(staged.as_file_mut(), config)
        .context("failed to serialize runner config")?;
    staged.as_file_mut().write_all(b"\n")?;
    staged.as_file_mut().sync_all()?;
    staged
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace runner config {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn generate_runner_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn runner_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

async fn connect_existing_runner_db(path: &Path) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to open local Oore database {}", path.display()))
}

async fn resolve_local_runner_config(
    pool: &SqlitePool,
    existing: Option<&RunnerConfig>,
    daemon_url: &str,
    name: &str,
    capabilities: &serde_json::Value,
    adopt_installed_registration: bool,
) -> anyhow::Result<(RunnerConfig, bool)> {
    let capabilities = serde_json::to_string(capabilities)?;
    let existing_row = if let Some(config) = existing {
        sqlx::query("SELECT id, token_hash, registered_by FROM runners WHERE id = ?1")
            .bind(&config.runner_id)
            .fetch_optional(pool)
            .await
            .context("failed to validate existing runner registration")?
    } else {
        None
    };

    if let (Some(config), Some(row)) = (existing, existing_row.as_ref()) {
        let registered_by: Option<String> = row.try_get("registered_by")?;
        if registered_by.is_some() && !adopt_installed_registration {
            anyhow::bail!(
                "the existing runner config belongs to a manually registered runner; use a separate config for the managed local runner"
            );
        }
        let expected_hash: String = row.try_get("token_hash")?;
        if expected_hash == runner_token_hash(&config.runner_token) {
            sqlx::query(
                "UPDATE runners SET name = ?1, capabilities = ?2, updated_at = ?3 \
                 WHERE id = ?4",
            )
            .bind(name)
            .bind(&capabilities)
            .bind(now_epoch_secs())
            .bind(&config.runner_id)
            .execute(pool)
            .await
            .context("failed to refresh managed runner metadata")?;
            return Ok((
                RunnerConfig {
                    runner_id: config.runner_id.clone(),
                    runner_token: config.runner_token.clone(),
                    daemon_url: daemon_url.to_string(),
                    name: name.to_string(),
                },
                false,
            ));
        }
        if adopt_installed_registration {
            anyhow::bail!("the installed runner config does not match its backend registration");
        }
    }

    let token = generate_runner_token();
    let token_hash = runner_token_hash(&token);
    let now = now_epoch_secs();
    let managed_id = match existing_row.as_ref() {
        Some(row) if row.try_get::<Option<String>, _>("registered_by")?.is_none() => {
            Some(row.try_get::<String, _>("id")?)
        }
        _ => {
            let named = sqlx::query_scalar::<_, String>(
                "SELECT id FROM runners WHERE name = ?1 AND registered_by IS NULL LIMIT 1",
            )
            .bind(name)
            .fetch_optional(pool)
            .await
            .context("failed to find the managed local runner")?;
            if named.is_some() {
                named
            } else {
                let existing = sqlx::query_scalar::<_, String>(
                    "SELECT id FROM runners WHERE registered_by IS NULL ORDER BY created_at LIMIT 2",
                )
                .fetch_all(pool)
                .await
                .context("failed to find a legacy managed runner")?;
                (existing.len() == 1).then(|| existing[0].clone())
            }
        }
    };

    let runner_id = if let Some(id) = managed_id {
        sqlx::query(
            "UPDATE runners \
             SET name = ?1, token_hash = ?2, status = 'offline', capabilities = ?3, \
                 last_heartbeat_at = NULL, updated_at = ?4 \
             WHERE id = ?5 AND registered_by IS NULL",
        )
        .bind(name)
        .bind(&token_hash)
        .bind(&capabilities)
        .bind(now)
        .bind(&id)
        .execute(pool)
        .await
        .context("failed to refresh the managed local runner")?;
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO runners \
             (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'offline', ?4, NULL, ?5, ?5)",
        )
        .bind(&id)
        .bind(name)
        .bind(&token_hash)
        .bind(&capabilities)
        .bind(now)
        .execute(pool)
        .await
        .context("failed to enroll the managed local runner")?;
        id
    };

    Ok((
        RunnerConfig {
            runner_id,
            runner_token: token,
            daemon_url: daemon_url.to_string(),
            name: name.to_string(),
        },
        true,
    ))
}

async fn publish_local_runner_config(
    pool: &SqlitePool,
    path: &Path,
    previous: Option<&RunnerConfig>,
    config: &RunnerConfig,
    enrolled: bool,
) -> anyhow::Result<()> {
    if let Err(error) = write_runner_config(path, config) {
        if enrolled
            && let Some(previous) = previous
            && previous.runner_id == config.runner_id
        {
            sqlx::query("UPDATE runners SET token_hash = ?1 WHERE id = ?2")
                .bind(runner_token_hash(&previous.runner_token))
                .bind(&previous.runner_id)
                .execute(pool)
                .await
                .context("failed to restore the previous runner token after config write failed")?;
        }
        return Err(error);
    }
    Ok(())
}

async fn ensure_runner_service_config(
    args: &RunnerServiceArgs,
    config_path: PathBuf,
    adopt_installed_registration: bool,
) -> anyhow::Result<(PathBuf, RunnerConfig, bool)> {
    let existing = read_runner_config(&config_path)?;

    if !args.managed_local {
        let config = existing.context(
            "runner is not registered; run `oore runner register` before installing the service",
        )?;
        oore_runner::require_safe_daemon_url(&config.daemon_url)?;
        write_runner_config(&config_path, &config)?;
        return Ok((config_path, config, false));
    }

    let database = resolve_db_path(args.state_file.as_deref())?;
    if !database.is_file() {
        anyhow::bail!(
            "local Oore database not found at {}; start or set up the backend before installing its managed runner",
            database.display()
        );
    }

    let daemon_url = match args.daemon_url.as_deref() {
        Some(url) => url.trim().to_string(),
        None => match existing.as_ref() {
            Some(config) => config.daemon_url.clone(),
            None => resolve_daemon_url(None)?,
        },
    };
    oore_runner::require_safe_daemon_url(&daemon_url)?;

    let name = args
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(oore_runner::get_hostname);
    if name.is_empty() || name.len() > 255 {
        anyhow::bail!("runner name must be between 1 and 255 characters");
    }

    let pool = connect_existing_runner_db(&database).await?;
    let capabilities = oore_runner::detect_capabilities().await;
    let (config, enrolled) = resolve_local_runner_config(
        &pool,
        existing.as_ref(),
        &daemon_url,
        &name,
        &capabilities,
        adopt_installed_registration,
    )
    .await?;
    publish_local_runner_config(&pool, &config_path, existing.as_ref(), &config, enrolled).await?;
    pool.close().await;
    Ok((config_path, config, enrolled))
}

fn managed_local_runner_config_path() -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("could not determine home directory")?
        .join(".oore/managed-runner.json"))
}

fn seed_managed_local_runner_config(source: &Path, destination: &Path) -> anyhow::Result<()> {
    if paths_refer_to_same_file(source, destination) || destination.is_file() {
        return Ok(());
    }
    let config = read_runner_config(source)?.with_context(|| {
        format!(
            "installed managed runner config is missing: {}",
            source.display()
        )
    })?;
    write_runner_config(destination, &config)
}

fn remove_migrated_managed_runner_config(source: &Path, destination: &Path) -> anyhow::Result<()> {
    if paths_refer_to_same_file(source, destination) {
        return Ok(());
    }
    fs::remove_file(source).with_context(|| {
        format!(
            "failed to remove migrated managed runner config {}",
            source.display()
        )
    })
}

async fn runner_config_is_locally_managed(
    config_path: &Path,
    state_file: Option<&str>,
) -> anyhow::Result<bool> {
    let Some(config) = read_runner_config(config_path)? else {
        return Ok(false);
    };
    let database = resolve_db_path(state_file)?;
    if !database.is_file() {
        return Ok(false);
    }
    let pool = connect_existing_runner_db(&database).await?;
    let token_hash =
        sqlx::query_scalar::<_, String>("SELECT token_hash FROM runners WHERE id = ?1")
            .bind(&config.runner_id)
            .fetch_optional(&pool)
            .await
            .context("failed to validate the installed runner service registration")?;
    pool.close().await;

    let Some(token_hash) = token_hash else {
        return Ok(false);
    };
    Ok(token_hash == runner_token_hash(&config.runner_token))
}

async fn migrate_managed_runner_service_config(
    source: &Path,
    destination: &Path,
    state_file: Option<&str>,
) -> anyhow::Result<()> {
    if !runner_config_is_locally_managed(source, state_file).await? {
        anyhow::bail!(
            "the installed runner config does not match its backend registration; uninstall it before installing this backend's managed local runner"
        );
    }
    seed_managed_local_runner_config(source, destination)
}

const RUNNER_CLAIM_BARRIER_LEASE_SECS: i64 = 300;
const RUNNER_DRAIN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
#[cfg(not(test))]
const RUNNER_CLAIM_BARRIER_RENEW_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(test)]
const RUNNER_CLAIM_BARRIER_RENEW_INTERVAL: Duration = Duration::from_millis(25);

struct RunnerClaimBarrier {
    database: PathBuf,
    token: String,
    renewal: tokio::task::JoinHandle<()>,
    renewal_failure: tokio::sync::watch::Receiver<Option<String>>,
}

async fn renew_runner_claim_barrier(database: &Path, token: &str) -> anyhow::Result<()> {
    let pool = connect_existing_runner_db(database).await?;
    let now = now_epoch_secs();
    let renewed = sqlx::query(
        "UPDATE runner_service_transition_lease SET expires_at = ?1 \
         WHERE id = 1 AND token = ?2 AND expires_at >= ?3",
    )
    .bind(now + RUNNER_CLAIM_BARRIER_LEASE_SECS)
    .bind(token)
    .bind(now)
    .execute(&pool)
    .await
    .context("failed to renew the runner claim barrier")?;
    pool.close().await;
    anyhow::ensure!(
        renewed.rows_affected() == 1,
        "runner claim barrier ownership was lost"
    );
    Ok(())
}

fn spawn_runner_claim_barrier_renewal(
    database: PathBuf,
    token: String,
    renewal_failure: tokio::sync::watch::Sender<Option<String>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(RUNNER_CLAIM_BARRIER_RENEW_INTERVAL).await;
            if let Err(error) = renew_runner_claim_barrier(&database, &token).await {
                let message = format!("runner claim barrier renewal failed: {error:#}");
                eprintln!("Warning: {message}");
                let _ = renewal_failure.send(Some(message));
                break;
            }
        }
    })
}

async fn acquire_runner_claim_barrier(database: &Path) -> anyhow::Result<RunnerClaimBarrier> {
    let pool = connect_existing_runner_db(database).await?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS runner_service_transition_lease (\
           id INTEGER PRIMARY KEY CHECK (id = 1), \
           token TEXT NOT NULL, \
           expires_at INTEGER NOT NULL\
         )",
    )
    .execute(&mut *tx)
    .await
    .context("failed to prepare the runner service transition lease")?;
    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS block_runner_claim_during_service_transition \
         BEFORE UPDATE OF status ON builds \
         WHEN OLD.status = 'queued' AND NEW.status = 'scheduled' \
          AND EXISTS (\
            SELECT 1 FROM runner_service_transition_lease \
            WHERE id = 1 AND expires_at >= CAST(strftime('%s', 'now') AS INTEGER)\
          ) \
         BEGIN \
           SELECT RAISE(ABORT, 'runner service transition in progress'); \
         END",
    )
    .execute(&mut *tx)
    .await
    .context("failed to install the runner claim barrier")?;
    let now = now_epoch_secs();
    let active_lease: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM runner_service_transition_lease WHERE id = 1 AND expires_at >= ?1)",
    )
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    if active_lease {
        anyhow::bail!("another runner service transition is already in progress");
    }
    let token = generate_runner_token();
    sqlx::query(
        "INSERT INTO runner_service_transition_lease (id, token, expires_at) VALUES (1, ?1, ?2) \
         ON CONFLICT(id) DO UPDATE SET token = excluded.token, expires_at = excluded.expires_at",
    )
    .bind(&token)
    .bind(now + RUNNER_CLAIM_BARRIER_LEASE_SECS)
    .execute(&mut *tx)
    .await
    .context("failed to acquire the runner claim barrier")?;
    let build_events_exist: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'build_events')",
    )
    .fetch_one(&mut *tx)
    .await?;
    if build_events_exist {
        sqlx::query(
            "INSERT INTO build_events \
             (id, build_id, from_status, to_status, actor, reason, created_at) \
             SELECT lower(hex(randomblob(16))), id, 'scheduled', 'queued', 'system', \
                    'Requeued before a maintenance drain because no runner had accepted the build', ?1 \
             FROM builds WHERE status = 'scheduled' AND runner_id IS NULL",
        )
        .bind(now)
        .execute(&mut *tx)
        .await
        .context("failed to record maintenance requeue events")?;
    }
    sqlx::query(
        "UPDATE builds SET status = 'queued', updated_at = ?1 \
         WHERE status = 'scheduled' AND runner_id IS NULL",
    )
    .bind(now)
    .execute(&mut *tx)
    .await
    .context("failed to requeue unassigned work before maintenance")?;
    tx.commit().await?;
    pool.close().await;
    let database = database.to_path_buf();
    let (renewal_failure_tx, renewal_failure) = tokio::sync::watch::channel(None);
    let renewal =
        spawn_runner_claim_barrier_renewal(database.clone(), token.clone(), renewal_failure_tx);
    Ok(RunnerClaimBarrier {
        database,
        token,
        renewal,
        renewal_failure,
    })
}

impl RunnerClaimBarrier {
    fn ensure_renewal_task_healthy(&self) -> anyhow::Result<()> {
        if let Some(error) = self.renewal_failure.borrow().clone() {
            anyhow::bail!(error);
        }
        anyhow::ensure!(
            !self.renewal.is_finished(),
            "runner claim barrier renewal stopped unexpectedly"
        );
        Ok(())
    }

    async fn ensure_healthy(&self) -> anyhow::Result<()> {
        self.ensure_renewal_task_healthy()?;
        renew_runner_claim_barrier(&self.database, &self.token).await?;
        self.ensure_renewal_task_healthy()
    }

    async fn wait_for_runner(&self, runner_id: &str) -> anyhow::Result<()> {
        self.wait_for_work(Some(runner_id)).await
    }

    async fn wait_for_all_work(&self) -> anyhow::Result<()> {
        self.wait_for_work(None).await
    }

    async fn wait_for_work(&self, runner_id: Option<&str>) -> anyhow::Result<()> {
        let pool = connect_existing_runner_db(&self.database).await?;
        let mut announced = false;
        let started = Instant::now();
        let mut next_progress = Duration::ZERO;
        loop {
            self.ensure_renewal_task_healthy()?;
            let mut tx = pool.begin().await?;
            let now = now_epoch_secs();
            let renewed = sqlx::query(
                "UPDATE runner_service_transition_lease SET expires_at = ?1 \
                 WHERE id = 1 AND token = ?2 AND expires_at >= ?3",
            )
            .bind(now + RUNNER_CLAIM_BARRIER_LEASE_SECS)
            .bind(&self.token)
            .bind(now)
            .execute(&mut *tx)
            .await
            .context("failed to renew the runner claim barrier")?;
            anyhow::ensure!(
                renewed.rows_affected() == 1,
                "runner claim barrier ownership was lost"
            );
            let active: bool = match runner_id {
                Some(runner_id) => {
                    sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM builds \
                     WHERE (runner_id = ?1 AND status IN ('assigned', 'running')) \
                        OR (runner_id IS NULL AND status = 'assigned'))",
                    )
                    .bind(runner_id)
                    .fetch_one(&mut *tx)
                    .await
                }
                None => {
                    sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM builds \
                     WHERE status IN ('assigned', 'running'))",
                    )
                    .fetch_one(&mut *tx)
                    .await
                }
            }
            .context("failed to check runner drain state")?;
            tx.commit().await?;
            if !active {
                break;
            }
            if started.elapsed() >= RUNNER_DRAIN_TIMEOUT {
                pool.close().await;
                anyhow::bail!(
                    "timed out after {} minutes waiting for assigned or running builds to finish; the update was not started",
                    RUNNER_DRAIN_TIMEOUT.as_secs() / 60
                );
            }
            if !announced {
                println!("Pausing new claims while running work finishes...");
                announced = true;
            }
            if started.elapsed() >= next_progress {
                println!(
                    "Still waiting for assigned or running work ({} minutes elapsed)...",
                    started.elapsed().as_secs() / 60
                );
                next_progress = started.elapsed() + Duration::from_secs(30);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        pool.close().await;
        Ok(())
    }

    async fn release(&self) -> anyhow::Result<()> {
        self.ensure_healthy().await?;
        let pool = connect_existing_runner_db(&self.database).await?;
        let released =
            sqlx::query("DELETE FROM runner_service_transition_lease WHERE id = 1 AND token = ?1")
                .bind(&self.token)
                .execute(&pool)
                .await
                .context("failed to release the runner claim barrier")?;
        pool.close().await;
        anyhow::ensure!(
            released.rows_affected() == 1,
            "runner claim barrier ownership was lost before release"
        );
        self.renewal.abort();
        Ok(())
    }
}

impl Drop for RunnerClaimBarrier {
    fn drop(&mut self) {
        self.renewal.abort();
    }
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

fn command_output(cmd: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new(cmd).args(args).output().ok()
}

fn command_version(cmd: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<String> {
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

fn managed_runner_service_doctor_result(
    system_plist_present: bool,
    system_service_loaded: bool,
    system_service_running: bool,
    legacy_plist_present: bool,
    legacy_service_loaded: bool,
    readiness_error: Option<&str>,
) -> DoctorCheckResult {
    let repair_hint = Some(
        "run `oore runner install-service --managed-local` on the backend Mac, or `oore runner install-service` for a registered external runner",
    );

    if system_plist_present && system_service_loaded {
        if !system_service_running {
            return doctor_check(
                "runner_service",
                "warning",
                Some(
                    "boot-time system service is loaded but not running; it may be crash-looping (check ~/.oore/logs/oore-runner.log)"
                        .to_string(),
                ),
                repair_hint,
            );
        }
        if let Some(error) = readiness_error {
            return doctor_check(
                "runner_service",
                "warning",
                Some(format!(
                    "boot-time system service is running but has not authenticated with the backend recently: {error}"
                )),
                repair_hint,
            );
        }
        if legacy_plist_present || legacy_service_loaded {
            return doctor_check(
                "runner_service",
                "warning",
                Some(
                    "boot-time service is loaded, but a legacy login-session service remains"
                        .to_string(),
                ),
                repair_hint,
            );
        }
        return doctor_check(
            "runner_service",
            "ok",
            Some("boot-time system service is installed, running, and authenticated".to_string()),
            None,
        );
    }

    if system_plist_present {
        return doctor_check(
            "runner_service",
            "warning",
            Some("boot-time system service is installed but not loaded".to_string()),
            repair_hint,
        );
    }
    if system_service_loaded {
        let detail = if system_service_running {
            "system service is running without an installed plist and will not survive reboot"
        } else {
            "system service is loaded but not running and has no installed plist"
        };
        return doctor_check(
            "runner_service",
            "warning",
            Some(detail.to_string()),
            repair_hint,
        );
    }
    if legacy_plist_present || legacy_service_loaded {
        return doctor_check(
            "runner_service",
            "warning",
            Some(
                "legacy login-session runner detected; it cannot start before GUI login"
                    .to_string(),
            ),
            repair_hint,
        );
    }

    doctor_check(
        "runner_service",
        "warning",
        Some("managed runner service is not installed".to_string()),
        repair_hint,
    )
}

fn add_managed_runner_service_check(checks: &mut Vec<DoctorCheckResult>) {
    if std::env::consts::OS != "macos" {
        checks.push(doctor_check(
            "runner_service",
            "skipped",
            Some("managed runner services are supported on macOS".to_string()),
            None,
        ));
        return;
    }

    let system_plist = system_runner_plist();
    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    let system_service_output =
        command_output("/bin/launchctl", &["print", system_service.as_str()]);
    let system_service_loaded = system_service_output
        .as_ref()
        .is_some_and(|output| output.status.success());
    let system_service_running = system_service_output
        .as_ref()
        .is_some_and(|output| output.status.success() && launchd_job_is_running(&output.stdout));
    let legacy_plist = launch_agent_plist(RUNNER_SERVICE_LABEL).ok();
    let legacy_service_loaded = current_user_launchd_domain()
        .ok()
        .and_then(|(_, service)| command_output("/bin/launchctl", &["print", service.as_str()]))
        .is_some_and(|output| output.status.success());
    let readiness_error = if system_plist.is_file() && system_service_running {
        match managed_runner_service_spec(&system_plist) {
            Ok(Some(spec)) => launchd_job_pid(
                &system_service_output
                    .as_ref()
                    .expect("running launchd service has output")
                    .stdout,
            )
            .context("managed runner has no process id")
            .and_then(|pid| verify_managed_runner_service(&spec, pid, None))
            .err()
            .map(|error| error.to_string()),
            Ok(None) => Some(
                "service needs reinstalling to enable authenticated readiness checks".to_string(),
            ),
            Err(error) => Some(error.to_string()),
        }
    } else {
        None
    };

    checks.push(managed_runner_service_doctor_result(
        system_plist.is_file(),
        system_service_loaded,
        system_service_running,
        legacy_plist.as_deref().is_some_and(Path::is_file),
        legacy_service_loaded,
        readiness_error.as_deref(),
    ));
}

fn android_sdk_root() -> Option<PathBuf> {
    let mut roots = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
        .collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Library/Android/sdk"));
    }
    roots
        .into_iter()
        .find(|path| path.join("platform-tools/adb").is_file())
}

fn valid_android_signing_java_home(path: &Path) -> bool {
    path.join("bin/java").is_file() && path.join("bin/jarsigner").is_file()
}

fn android_signing_java_home() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("JAVA_HOME").map(PathBuf::from)
        && valid_android_signing_java_home(&path)
    {
        return Some(path);
    }

    #[cfg(target_os = "macos")]
    {
        let mut application_roots = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            application_roots.push(PathBuf::from(home).join("Applications"));
        }
        application_roots.push(PathBuf::from("/Applications"));
        for root in application_roots {
            for app in ["Android Studio.app", "Android Studio Preview.app"] {
                let path = root.join(app).join("Contents/jbr/Contents/Home");
                if valid_android_signing_java_home(&path) {
                    return Some(path);
                }
            }
        }

        if let Ok(output) = std::process::Command::new("/usr/libexec/java_home").output()
            && output.status.success()
        {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            if valid_android_signing_java_home(&path) {
                return Some(path);
            }
        }
    }

    None
}

fn add_android_checks(checks: &mut Vec<DoctorCheckResult>) {
    let java = android_signing_java_home()
        .map(|home| home.join("bin/java"))
        .unwrap_or_else(|| PathBuf::from("java"));
    if let Some(version) = command_version(&java, &["-version"]) {
        let detail = if java.is_absolute() {
            format!("{version} ({})", java.display())
        } else {
            version
        };
        checks.push(doctor_check("java", "ok", Some(detail), None));
    } else {
        checks.push(doctor_check(
            "java",
            "missing",
            None,
            Some("install Android Studio or a supported JDK and set JAVA_HOME"),
        ));
    }

    if let Some(sdk_root) = android_sdk_root() {
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
            None,
            Some("install Android Studio or set ANDROID_HOME to an SDK with Platform-Tools"),
        ));
    }
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
    let missing_tools = ["/usr/bin/security", "/usr/bin/codesign"]
        .into_iter()
        .filter(|tool| !Path::new(tool).is_file())
        .collect::<Vec<_>>();
    if missing_tools.is_empty() {
        checks.push(doctor_check(
            "apple_signing_tools",
            "ok",
            Some(
                "job-scoped signing tools are available; build credentials are validated from each temporary keychain"
                    .to_string(),
            ),
            None,
        ));
    } else {
        checks.push(doctor_check(
            "apple_signing_tools",
            "missing",
            Some(format!("missing {}", missing_tools.join(", "))),
            Some("install Xcode and its command-line tools before signing Apple builds"),
        ));
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

    add_managed_runner_service_check(&mut checks);

    if doctor_has_platform(&args, DoctorPlatform::Android) {
        add_android_checks(&mut checks);
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
            if check.detail.is_some()
                && let Some(install_hint) = check.install_hint.as_deref()
            {
                println!("      Fix: {install_hint}");
            }
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

const DEFAULT_GITHUB_REPO: &str = "oore-ci/oore.build";
const LEGACY_GITHUB_REPO: &str = "devaryakjha/oore.build";
const DEFAULT_RELEASE_INDEX_BASE_URL: &str = "https://releases.oore.build";

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
}

#[derive(Debug, serde::Deserialize)]
struct ReleaseManifest {
    schema_version: u32,
    channel: String,
    version: String,
    tag: String,
    download_base_url: String,
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
    read_trimmed_file(&install_root.join("GITHUB_REPO")).map(|repo| normalize_github_repo(&repo))
}

fn normalize_github_repo(repo: &str) -> String {
    let repo = repo.trim();
    if repo == LEGACY_GITHUB_REPO {
        DEFAULT_GITHUB_REPO.to_string()
    } else {
        repo.to_string()
    }
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("oore/{}/update", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build HTTP client")
}

async fn fetch_latest_release(
    client: &reqwest::Client,
    repo: &str,
    channel: ReleaseChannel,
) -> anyhow::Result<ReleaseManifest> {
    let base = std::env::var("OORE_RELEASE_INDEX_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_RELEASE_INDEX_BASE_URL.to_string());
    let url = format!(
        "{}/latest/{}.json",
        base.trim_end_matches('/'),
        channel.as_str()
    );
    let release: ReleaseManifest = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| {
            format!(
                "failed to fetch {channel} release index",
                channel = channel.as_str()
            )
        })?
        .error_for_status()
        .with_context(|| format!("release index request failed: {url}"))?
        .json()
        .await
        .context("failed to parse release index JSON")?;
    if release.schema_version != 1 || release.channel != channel.as_str() {
        anyhow::bail!(
            "invalid {} release index response from {url}",
            channel.as_str()
        );
    }
    if release.tag.trim_start_matches('v') != release.version {
        anyhow::bail!(
            "release index tag and version do not match: {}",
            release.tag
        );
    }
    let expected_download_base = format!(
        "https://github.com/{repo}/releases/download/{}",
        release.tag
    );
    if release.download_base_url.trim_end_matches('/') != expected_download_base {
        anyhow::bail!("release index asset source does not match GitHub repo {repo}");
    }
    Ok(release)
}

fn find_asset_url(release: &ReleaseManifest, name: &str) -> String {
    format!("{}/{name}", release.download_base_url.trim_end_matches('/'))
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

fn create_private_file(path: &Path) -> anyhow::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create private file {}", path.display()))
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
        if !entry.header().entry_type().is_file() {
            anyhow::bail!("backup entry {name} must be a regular file");
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

fn create_backup_archive(database: &Path, key: &Path, output: &Path) -> anyhow::Result<()> {
    if !database.is_file() {
        anyhow::bail!("database does not exist: {}", database.display());
    }
    if !key.is_file() {
        anyhow::bail!("encryption key does not exist: {}", key.display());
    }
    if output.exists() {
        anyhow::bail!("backup destination already exists: {}", output.display());
    }
    let parent = output
        .parent()
        .context("backup output must have a parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create backup directory {}", parent.display()))?;

    let stage =
        tempfile::tempdir_in(parent).context("failed to create backup staging directory")?;
    let snapshot = stage.path().join(BACKUP_DATABASE_FILE);
    snapshot_sqlite_path(database, &snapshot)?;
    let staged_key = stage.path().join(BACKUP_KEY_FILE);
    fs::copy(key, &staged_key).context("failed to copy encryption key into backup")?;
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
        output.file_name().unwrap_or_default().to_string_lossy()
    ));
    let file = create_private_file(&temp_output)
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
    fs::rename(&temp_output, output)
        .with_context(|| format!("failed to publish backup {}", output.display()))?;
    Ok(())
}

fn backup_create(args: BackupCreateArgs) -> anyhow::Result<()> {
    let database = resolve_db_path(args.state_file.as_deref())?;
    let key = resolve_key_path()?;
    create_backup_archive(&database, &key, &args.output)?;
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

fn sqlite_sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

fn restore_verified_backup(input: &Path, database: &Path, key: &Path) -> anyhow::Result<()> {
    if database_is_open(database) {
        anyhow::bail!(
            "refusing restore while the state database is open; stop the managed or unmanaged oored process first"
        );
    }
    let stage = tempfile::tempdir().context("failed to create restore staging directory")?;
    unpack_backup(input, stage.path())?;
    for path in [database, key] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create restore directory {}", parent.display())
            })?;
        }
    }

    let nonce = format!("{}-{}", now_epoch_secs(), std::process::id());
    let database_old = database.with_extension(format!("restore-{nonce}.old"));
    let key_old = key.with_extension(format!("restore-{nonce}.old"));
    let database_new = database.with_extension(format!("restore-{nonce}.new"));
    let key_new = key.with_extension(format!("restore-{nonce}.new"));
    let wal = sqlite_sidecar_path(database, "-wal");
    let shm = sqlite_sidecar_path(database, "-shm");
    let wal_old = wal.with_extension(format!("restore-{nonce}.old"));
    let shm_old = shm.with_extension(format!("restore-{nonce}.old"));
    fs::copy(stage.path().join(BACKUP_DATABASE_FILE), &database_new)?;
    fs::copy(stage.path().join(BACKUP_KEY_FILE), &key_new)?;
    set_private_permissions(&key_new)?;

    let rollback = || -> anyhow::Result<()> {
        for (old, current) in [
            (&database_old, database),
            (&key_old, key),
            (&wal_old, &wal),
            (&shm_old, &shm),
        ] {
            if old.exists() {
                fs::rename(old, current)?;
            }
        }
        Ok(())
    };
    for (current, old) in [
        (database, &database_old),
        (key, &key_old),
        (&wal, &wal_old),
        (&shm, &shm_old),
    ] {
        if current.exists() {
            fs::rename(current, old)?;
        }
    }
    let result = (|| -> anyhow::Result<()> {
        fs::rename(&database_new, database)?;
        fs::rename(&key_new, key)?;
        set_private_permissions(key)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(database);
        let _ = fs::remove_file(key);
        rollback().context("restore failed and rollback failed")?;
        return Err(error);
    }
    for old in [database_old, key_old, wal_old, shm_old] {
        let _ = fs::remove_file(old);
    }
    Ok(())
}

fn backup_restore(args: BackupRestoreArgs) -> anyhow::Result<()> {
    let client = http_client()?;
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    let daemon_url = resolve_daemon_url(None)?;
    if runtime.block_on(check_daemon_running(&client, &daemon_url)) {
        anyhow::bail!(
            "refusing restore while oored is running; stop the managed service or daemon first"
        );
    }

    let database = resolve_db_path(args.state_file.as_deref())?;
    let key = resolve_key_path()?;
    restore_verified_backup(&args.input, &database, &key)?;
    println!("Restored backup: {}", args.input.display());
    Ok(())
}

const DAEMON_SERVICE_LABEL: &str = "build.oore.oored";
const WEB_SERVICE_LABEL: &str = "build.oore.oore-web";
const RUNNER_SERVICE_LABEL: &str = "build.oore.oore-runner";
const RUNNER_STABLE_RUNNING_POLLS: usize = 4;
const RUNNER_AUTHENTICATED_START_POLLS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedRunnerServiceSpec {
    executable: PathBuf,
    config: PathBuf,
    daemon_url: Option<String>,
    acknowledgement: PathBuf,
    service_pid_is_parent: bool,
}

fn managed_runner_update_service_from_program_arguments(
    install_root: &Path,
    program_arguments: &[String],
) -> Option<&'static str> {
    let runner_arguments = runner_command_from_program_arguments(program_arguments);
    let configured = fs::canonicalize(Path::new(runner_arguments.first()?)).ok()?;
    let installed = fs::canonicalize(install_root.join("bin/oore")).ok()?;
    (configured == installed
        && runner_arguments.get(1).map(String::as_str) == Some("runner")
        && runner_arguments.get(2).map(String::as_str) == Some("start"))
    .then_some(RUNNER_SERVICE_LABEL)
}

fn runner_command_from_program_arguments(program_arguments: &[String]) -> &[String] {
    if runner_service_uses_user_bootstrap_wrapper(program_arguments) {
        &program_arguments[8..]
    } else {
        program_arguments
    }
}

fn runner_service_uses_user_bootstrap_wrapper(program_arguments: &[String]) -> bool {
    program_arguments.first().map(String::as_str) == Some("/bin/launchctl")
        && program_arguments.get(1).map(String::as_str) == Some("asuser")
        && program_arguments.get(3).map(String::as_str) == Some("/usr/bin/sudo")
        && program_arguments.get(4).map(String::as_str) == Some("-E")
        && program_arguments.get(5).map(String::as_str) == Some("-H")
        && program_arguments.get(6).map(String::as_str) == Some("-u")
        && program_arguments.len() > 8
}

fn value_from_program_arguments(program_arguments: &[String], option: &str) -> Option<String> {
    program_arguments
        .windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
        .or_else(|| {
            let prefix = format!("{option}=");
            program_arguments.iter().find_map(|argument| {
                argument
                    .strip_prefix(&prefix)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
        })
}

fn runner_config_from_program_arguments(program_arguments: &[String]) -> Option<PathBuf> {
    value_from_program_arguments(
        runner_command_from_program_arguments(program_arguments),
        "--config",
    )
    .map(PathBuf::from)
}

fn managed_runner_service_spec_from_document(
    document: &serde_json::Value,
) -> anyhow::Result<Option<ManagedRunnerServiceSpec>> {
    let arguments = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .context("managed runner service has no ProgramArguments")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("managed runner ProgramArguments must contain only strings")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let runner_arguments = runner_command_from_program_arguments(&arguments);
    anyhow::ensure!(
        runner_arguments.get(1).map(String::as_str) == Some("runner")
            && runner_arguments.get(2).map(String::as_str) == Some("start"),
        "managed runner service does not launch `oore runner start`"
    );
    let executable = runner_arguments
        .first()
        .map(PathBuf::from)
        .context("managed runner service has no executable")?;
    let config = runner_config_from_program_arguments(runner_arguments)
        .context("managed runner service does not specify --config")?;
    let acknowledgement = document
        .get("EnvironmentVariables")
        .and_then(serde_json::Value::as_object)
        .and_then(|environment| environment.get(oore_runner::RUNNER_SERVICE_ACK_PATH_ENV));
    let Some(acknowledgement) = acknowledgement else {
        return Ok(None);
    };
    let acknowledgement = acknowledgement
        .as_str()
        .map(PathBuf::from)
        .context("managed runner acknowledgement path must be a string")?;
    anyhow::ensure!(
        acknowledgement.is_absolute(),
        "managed runner acknowledgement path must be absolute"
    );
    Ok(Some(ManagedRunnerServiceSpec {
        executable,
        config,
        daemon_url: value_from_program_arguments(runner_arguments, "--daemon-url"),
        acknowledgement,
        service_pid_is_parent: runner_arguments.len() != arguments.len(),
    }))
}

fn managed_runner_service_spec(plist: &Path) -> anyhow::Result<Option<ManagedRunnerServiceSpec>> {
    managed_runner_service_spec_from_document(&plist_json(plist)?)
}

fn ensure_runner_acknowledgement_path(install_root: &Path) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let directory = install_root.join("run");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
                "runner runtime path is not a directory: {}",
                directory.display()
            );
            anyhow::ensure!(
                metadata.uid() == unsafe { libc::geteuid() },
                "runner runtime directory is not owned by the runner account: {}",
                directory.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&directory).with_context(|| {
                format!(
                    "failed to create runner runtime directory {}",
                    directory.display()
                )
            })?;
        }
        Err(error) => return Err(error.into()),
    }
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).with_context(|| {
        format!(
            "failed to secure runner runtime directory {}",
            directory.display()
        )
    })?;
    Ok(directory.join(oore_runner::RUNNER_SERVICE_ACK_FILE))
}

fn clear_runner_service_acknowledgement(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to clear runner service acknowledgement {}",
                path.display()
            )
        }),
    }
}

fn verify_managed_runner_service(
    spec: &ManagedRunnerServiceSpec,
    pid: u32,
    not_before: Option<i64>,
) -> anyhow::Result<()> {
    let mut config = read_runner_config(&spec.config)?.with_context(|| {
        format!(
            "managed runner config is missing: {}",
            spec.config.display()
        )
    })?;
    if let Some(daemon_url) = &spec.daemon_url {
        config.daemon_url.clone_from(daemon_url);
    }
    let expected_pid = if spec.service_pid_is_parent {
        let ack: oore_runner::RunnerServiceAck =
            serde_json::from_slice(&fs::read(&spec.acknowledgement)?)
                .context("runner service acknowledgement is invalid")?;
        anyhow::ensure!(
            process_is_descendant_of(ack.pid, pid)?,
            "runner acknowledgement process is not owned by the active service"
        );
        ack.pid
    } else {
        pid
    };
    oore_runner::verify_runner_service_ack(
        &spec.acknowledgement,
        &config,
        &spec.executable,
        expected_pid,
        not_before,
        Duration::from_secs(oore_runner::RUNNER_SERVICE_ACK_MAX_AGE_SECS),
    )?;
    Ok(())
}

fn process_is_descendant_of(mut process: u32, ancestor: u32) -> anyhow::Result<bool> {
    for _ in 0..16 {
        if process == ancestor {
            return Ok(true);
        }
        let output = std::process::Command::new("/bin/ps")
            .args(["-o", "ppid=", "-p", &process.to_string()])
            .output()
            .context("failed to inspect runner service process ownership")?;
        if !output.status.success() {
            return Ok(false);
        }
        process = String::from_utf8(output.stdout)?
            .trim()
            .parse()
            .unwrap_or(0);
        if process <= 1 {
            return Ok(false);
        }
    }
    Ok(false)
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn managed_runner_update_service(install_root: &Path) -> anyhow::Result<Option<&'static str>> {
    let (plist, _) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    if !plist.is_file() {
        return Ok(None);
    }
    let program_arguments = plist_program_arguments(&plist)?;
    Ok(managed_runner_update_service_from_program_arguments(
        install_root,
        &program_arguments,
    ))
}

fn managed_runner_service_requires_repair(plist: &Path) -> anyhow::Result<bool> {
    Ok(runner_service_uses_user_bootstrap_wrapper(
        &plist_program_arguments(plist)?,
    ))
}

fn managed_runner_process_pid() -> anyhow::Result<Option<u32>> {
    let (_, system) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    let service = if system {
        format!("system/{RUNNER_SERVICE_LABEL}")
    } else {
        current_user_launchd_domain()?.1
    };
    let output = std::process::Command::new("/bin/launchctl")
        .args(["print", &service])
        .output()
        .context("failed to inspect the managed runner before update")?;
    Ok(output
        .status
        .success()
        .then(|| launchd_job_pid(&output.stdout))
        .flatten())
}

fn launchd_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[allow(clippy::too_many_arguments)]
fn render_runner_launch_daemon(
    executable: &Path,
    config: &Path,
    acknowledgement: &Path,
    home: &Path,
    working_dir: &Path,
    log_path: &Path,
    path: &str,
    user: &str,
) -> String {
    let values = [
        executable.display().to_string(),
        "runner".to_string(),
        "start".to_string(),
        "--config".to_string(),
        config.display().to_string(),
    ];
    let arguments = values
        .iter()
        .map(|value| format!("        <string>{}</string>", launchd_xml_escape(value)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{RUNNER_SERVICE_LABEL}</string>
    <key>UserName</key>
    <string>{}</string>
    <key>SessionCreate</key>
    <true/>
    <key>ProgramArguments</key>
    <array>
{arguments}
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>{}</string>
        <key>PATH</key>
        <string>{}</string>
        <key>{}</key>
        <string>{}</string>
    </dict>
    <key>WorkingDirectory</key>
    <string>{}</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
</dict>
</plist>
"#,
        launchd_xml_escape(user),
        launchd_xml_escape(&home.display().to_string()),
        launchd_xml_escape(path),
        oore_runner::RUNNER_SERVICE_ACK_PATH_ENV,
        launchd_xml_escape(&acknowledgement.display().to_string()),
        launchd_xml_escape(&working_dir.display().to_string()),
        launchd_xml_escape(&log_path.display().to_string()),
        launchd_xml_escape(&log_path.display().to_string()),
    )
}

fn current_user_launchd_domain() -> anyhow::Result<(String, String)> {
    let uid = current_user_id()?;
    Ok((
        format!("gui/{uid}"),
        format!("gui/{uid}/{RUNNER_SERVICE_LABEL}"),
    ))
}

fn current_user_id() -> anyhow::Result<String> {
    let output = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .context("failed to determine current user id")?;
    if !output.status.success() {
        anyhow::bail!("failed to determine current user id");
    }
    let uid = String::from_utf8(output.stdout)
        .context("current user id was not valid UTF-8")?
        .trim()
        .to_string();
    Ok(uid)
}

fn current_user_name() -> anyhow::Result<String> {
    if unsafe { libc::geteuid() } == 0 {
        anyhow::bail!(
            "run `oore runner install-service` as the runner account, without sudo; Oore will request administrator access only for launchd setup"
        );
    }
    let output = std::process::Command::new("/usr/bin/id")
        .arg("-un")
        .output()
        .context("failed to determine current user name")?;
    if !output.status.success() {
        anyhow::bail!("failed to determine current user name");
    }
    let user = String::from_utf8(output.stdout)
        .context("current user name was not valid UTF-8")?
        .trim()
        .to_string();
    if user.is_empty() || user == "root" {
        anyhow::bail!("the managed runner must use a non-root account");
    }
    Ok(user)
}

fn sudo_tool_output(tool: &str, args: &[&OsStr]) -> anyhow::Result<std::process::Output> {
    std::process::Command::new("/usr/bin/sudo")
        .arg("-n")
        .arg(tool)
        .args(args)
        .output()
        .with_context(|| format!("failed to run privileged {tool}"))
}

fn sudo_tool_checked(tool: &str, args: &[&OsStr], action: &str) -> anyhow::Result<()> {
    let output = sudo_tool_output(tool, args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    anyhow::bail!("{action} failed: {detail}")
}

fn remove_staged_system_runner_plist(path: &Path) {
    let _ = sudo_tool_output("/bin/rm", &[OsStr::new("-f"), path.as_os_str()]);
}

fn system_runner_plist() -> PathBuf {
    PathBuf::from("/Library/LaunchDaemons").join(format!("{RUNNER_SERVICE_LABEL}.plist"))
}

fn write_system_runner_plist(contents: &str) -> anyhow::Result<PathBuf> {
    let target = system_runner_plist();
    let staged = PathBuf::from("/Library/LaunchDaemons").join(format!(
        ".{RUNNER_SERVICE_LABEL}.{}.tmp",
        std::process::id()
    ));

    let result = (|| -> anyhow::Result<()> {
        let mut child = std::process::Command::new("/usr/bin/sudo")
            .args(["-n", "/usr/bin/tee"])
            .arg(&staged)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .context("failed to stage the system runner plist")?;
        let write_result = child
            .stdin
            .take()
            .context("failed to open privileged plist input")?
            .write_all(contents.as_bytes());
        let status = child.wait()?;
        write_result?;
        if !status.success() {
            anyhow::bail!("failed to stage the system runner plist");
        }

        sudo_tool_checked(
            "/bin/chmod",
            &[OsStr::new("0644"), staged.as_os_str()],
            "securing the system runner plist",
        )?;
        sudo_tool_checked(
            "/usr/sbin/chown",
            &[OsStr::new("root:wheel"), staged.as_os_str()],
            "setting the system runner plist owner",
        )?;
        sudo_tool_checked(
            "/usr/bin/plutil",
            &[OsStr::new("-lint"), staged.as_os_str()],
            "validating the system runner plist",
        )?;
        sudo_tool_checked(
            "/bin/mv",
            &[OsStr::new("-f"), staged.as_os_str(), target.as_os_str()],
            "installing the system runner plist",
        )
    })();

    if result.is_err() {
        remove_staged_system_runner_plist(&staged);
    }
    result?;
    Ok(target)
}

fn user_launchd_service_loaded(service: &str) -> bool {
    std::process::Command::new("/bin/launchctl")
        .args(["print", service])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn stop_user_launchd_service(service: &str) -> anyhow::Result<()> {
    if !user_launchd_service_loaded(service) {
        return Ok(());
    }
    let _ = std::process::Command::new("/bin/launchctl")
        .args(["bootout", service])
        .output();
    if !wait_for_launchd_service_unloaded(|| Ok(user_launchd_service_loaded(service)))? {
        anyhow::bail!("failed to stop legacy runner service {service}");
    }
    Ok(())
}

fn wait_for_launchd_service_unloaded(
    mut is_loaded: impl FnMut() -> anyhow::Result<bool>,
) -> anyhow::Result<bool> {
    for _ in 0..20 {
        if !is_loaded()? {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(false)
}

fn system_launchd_service_loaded(service: &str) -> anyhow::Result<bool> {
    Ok(sudo_tool_output(
        "/bin/launchctl",
        &[OsStr::new("print"), OsStr::new(service)],
    )?
    .status
    .success())
}

fn system_launchd_service_output(service: &str) -> anyhow::Result<std::process::Output> {
    std::process::Command::new("/bin/launchctl")
        .args(["print", service])
        .output()
        .with_context(|| format!("failed to inspect system runner service {service}"))
}

fn launchd_job_pid(output: &[u8]) -> Option<u32> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .find_map(|line| {
            line.strip_prefix("pid = ")
                .and_then(|pid| pid.parse::<u32>().ok())
                .filter(|pid| *pid > 0)
        })
}

fn launchd_job_is_running(output: &[u8]) -> bool {
    let output = String::from_utf8_lossy(output);
    output
        .lines()
        .map(str::trim)
        .any(|line| line == "state = running")
        && launchd_job_pid(output.as_bytes()).is_some()
}

fn launchd_runner_handoff_completed(output: &[u8], previous_pid: Option<u32>) -> bool {
    launchd_job_is_running(output)
        && previous_pid.is_none_or(|pid| launchd_job_pid(output) != Some(pid))
}

fn launchd_job_is_stably_running(consecutive_running: &mut usize, running: bool) -> bool {
    if running {
        *consecutive_running += 1;
    } else {
        *consecutive_running = 0;
    }
    *consecutive_running >= RUNNER_STABLE_RUNNING_POLLS
}

fn stop_system_launchd_service(service: &str) -> anyhow::Result<()> {
    if !system_launchd_service_loaded(service)? {
        return Ok(());
    }
    let _ = sudo_tool_output(
        "/bin/launchctl",
        &[OsStr::new("bootout"), OsStr::new(service)],
    );
    if !wait_for_launchd_service_unloaded(|| system_launchd_service_loaded(service))? {
        anyhow::bail!("failed to stop system runner service {service}");
    }
    Ok(())
}

fn start_system_runner_service(
    plist: &Path,
    service: &str,
    require_authenticated_acknowledgement: bool,
) -> anyhow::Result<()> {
    let spec = managed_runner_service_spec(plist)?;
    anyhow::ensure!(
        !require_authenticated_acknowledgement || spec.is_some(),
        "managed runner service is missing its authenticated readiness configuration"
    );
    if let Some(spec) = &spec {
        clear_runner_service_acknowledgement(&spec.acknowledgement)?;
    }
    let started_at = now_epoch_secs();
    let bootstrap = || {
        sudo_tool_checked(
            "/bin/launchctl",
            &[
                OsStr::new("bootstrap"),
                OsStr::new("system"),
                plist.as_os_str(),
            ],
            "starting the system runner service",
        )
    };
    if bootstrap().is_err() {
        std::thread::sleep(Duration::from_millis(250));
        bootstrap()?;
    }
    sudo_tool_checked(
        "/bin/launchctl",
        &[
            OsStr::new("kickstart"),
            OsStr::new("-k"),
            OsStr::new(service),
        ],
        "starting the managed runner",
    )?;
    let mut consecutive_running = 0;
    let mut readiness_error = None;
    for _ in 0..RUNNER_AUTHENTICATED_START_POLLS {
        let output = sudo_tool_output(
            "/bin/launchctl",
            &[OsStr::new("print"), OsStr::new(service)],
        )?;
        let running = output.status.success() && launchd_job_is_running(&output.stdout);
        if launchd_job_is_stably_running(&mut consecutive_running, running) {
            let Some(spec) = &spec else {
                return Ok(());
            };
            let pid =
                launchd_job_pid(&output.stdout).context("managed runner has no process id")?;
            match verify_managed_runner_service(spec, pid, Some(started_at)) {
                Ok(()) => return Ok(()),
                Err(error) => readiness_error = Some(error.to_string()),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Some(error) = readiness_error {
        anyhow::bail!(
            "managed runner started but did not authenticate with the backend: {error} (check ~/.oore/logs/oore-runner.log)"
        );
    }
    anyhow::bail!("managed runner did not remain running after launchd started it")
}

fn ensure_system_runner_service_active(
    plist: &Path,
    service: &str,
    previous_pid: Option<u32>,
) -> anyhow::Result<()> {
    let spec = managed_runner_service_spec(plist)?;
    if let Some(spec) = &spec {
        clear_runner_service_acknowledgement(&spec.acknowledgement)?;
    }
    let started_at = now_epoch_secs();
    let mut output = system_launchd_service_output(service)?;
    if let Some(previous_pid) = previous_pid {
        let mut waiting = false;
        while output.status.success() && launchd_job_pid(&output.stdout) == Some(previous_pid) {
            if !waiting {
                println!(
                    "The runner is finishing its active build; waiting for the updated runner to take over..."
                );
                waiting = true;
            }
            std::thread::sleep(Duration::from_millis(500));
            output = system_launchd_service_output(service)?;
        }
    }

    if !output.status.success() {
        let bootstrap = || {
            sudo_tool_checked(
                "/bin/launchctl",
                &[
                    OsStr::new("bootstrap"),
                    OsStr::new("system"),
                    plist.as_os_str(),
                ],
                "loading the system runner service",
            )
        };
        if bootstrap().is_err() {
            std::thread::sleep(Duration::from_millis(250));
            bootstrap()?;
        }
    }

    if !output.status.success() || !launchd_job_is_running(&output.stdout) {
        sudo_tool_checked(
            "/bin/launchctl",
            &[OsStr::new("kickstart"), OsStr::new(service)],
            "starting the managed runner",
        )?;
    }
    let mut consecutive_running = 0;
    let mut readiness_error = None;
    for _ in 0..RUNNER_AUTHENTICATED_START_POLLS {
        output = system_launchd_service_output(service)?;
        let running = output.status.success()
            && launchd_runner_handoff_completed(&output.stdout, previous_pid);
        if launchd_job_is_stably_running(&mut consecutive_running, running) {
            let Some(spec) = &spec else {
                return Ok(());
            };
            let pid =
                launchd_job_pid(&output.stdout).context("managed runner has no process id")?;
            match verify_managed_runner_service(spec, pid, Some(started_at)) {
                Ok(()) => return Ok(()),
                Err(error) => readiness_error = Some(error.to_string()),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Some(error) = readiness_error {
        anyhow::bail!(
            "managed runner became active but did not authenticate with the backend: {error} (check ~/.oore/logs/oore-runner.log)"
        );
    }
    anyhow::bail!("managed runner did not become active after launchd started it")
}

fn restore_legacy_runner_service(plist: &Path, domain: &str, service: &str) -> anyhow::Result<()> {
    if !plist.is_file() {
        anyhow::bail!("legacy runner plist is missing: {}", plist.display());
    }
    let status = std::process::Command::new("/bin/launchctl")
        .args(["bootstrap", domain, &plist.display().to_string()])
        .status()
        .context("failed to restore the legacy runner service")?;
    if !status.success() {
        anyhow::bail!("failed to restore the legacy runner service");
    }
    let status = std::process::Command::new("/bin/launchctl")
        .args(["kickstart", "-k", service])
        .status()
        .context("failed to restart the legacy runner service")?;
    if !status.success() || !user_launchd_service_loaded(service) {
        anyhow::bail!("failed to verify the restored legacy runner service");
    }
    Ok(())
}

fn rollback_runner_service_install(
    install_root: &Path,
    config_path: &Path,
    previous_config: Option<&RunnerConfig>,
    migrated_config: Option<(&Path, &RunnerConfig)>,
    previous_system: Option<(&str, bool)>,
    previous_legacy: Option<(&Path, &str, &str, &str, bool)>,
) -> anyhow::Result<()> {
    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    stop_system_launchd_service(&system_service)?;
    let plist = system_runner_plist();
    if let Some((contents, _)) = previous_system {
        write_system_runner_plist(contents)?;
    } else {
        sudo_tool_checked(
            "/bin/rm",
            &[OsStr::new("-f"), plist.as_os_str()],
            "removing the failed system runner plist",
        )?;
    }
    if let Some(config) = previous_config {
        write_runner_config(config_path, config)?;
    }
    if let Some((path, config)) = migrated_config {
        write_runner_config(path, config)?;
    }
    restore_runner_release_marker(install_root)?;
    if previous_system.is_some_and(|(_, was_loaded)| was_loaded) {
        start_system_runner_service(&plist, &system_service, false)?;
    }
    if let Some((plist, contents, domain, service, was_loaded)) = previous_legacy {
        fs::write(plist, contents).with_context(|| {
            format!("failed to restore legacy runner plist {}", plist.display())
        })?;
        if was_loaded {
            restore_legacy_runner_service(plist, domain, service)?;
        }
    }
    Ok(())
}

async fn handle_runner_install_service(args: RunnerServiceArgs) -> anyhow::Result<()> {
    handle_runner_install_service_inner(args, false, None).await
}

async fn handle_runner_install_service_inner(
    args: RunnerServiceArgs,
    transition_barrier_held: bool,
    update_ack: Option<(&Path, &str, &str)>,
) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed runner service installation is supported on macOS only");
    }

    let user = current_user_name()?;
    if args.managed_local && args.config.is_some() {
        anyhow::bail!("--managed-local cannot be combined with --config");
    }
    let previous_system_plist = system_runner_plist();
    let legacy_plist = launch_agent_plist(RUNNER_SERVICE_LABEL)?;
    let (domain, service) = current_user_launchd_domain()?;
    let legacy_was_loaded = user_launchd_service_loaded(&service);
    let mut installed_service_configs = Vec::new();
    for existing_plist in [&previous_system_plist, &legacy_plist] {
        if !existing_plist.is_file() {
            continue;
        }
        let program_arguments = plist_program_arguments(existing_plist)?;
        let existing_config = runner_config_from_program_arguments(&program_arguments)
            .with_context(|| {
                format!(
                    "existing runner service has no config path: {}",
                    existing_plist.display()
                )
            })?;
        installed_service_configs.push(existing_config);
    }
    if let Some(first) = installed_service_configs.first()
        && installed_service_configs
            .iter()
            .any(|path| !paths_refer_to_same_file(first, path))
    {
        anyhow::bail!(
            "system and login-session runner services use different configs; remove the stale service before reinstalling"
        );
    }

    let existing_managed_config = args
        .managed_local
        .then(|| installed_service_configs.first().cloned())
        .flatten();
    let requested_config = if args.managed_local {
        managed_local_runner_config_path()?
    } else {
        let requested_config = runner_config_path(args.config.clone())?;
        for existing_config in &installed_service_configs {
            if !paths_refer_to_same_file(&requested_config, existing_config) {
                anyhow::bail!(
                    "existing runner service uses {}; uninstall it before switching to {}",
                    existing_config.display(),
                    requested_config.display()
                );
            }
        }
        requested_config
    };
    let previous_config = read_runner_config(&requested_config)?;
    if let Some(existing_config) = existing_managed_config.as_deref() {
        migrate_managed_runner_service_config(
            existing_config,
            &requested_config,
            args.state_file.as_deref(),
        )
        .await?;
    }
    let home = dirs::home_dir().context("could not determine home directory")?;
    let logs = home.join(".oore/logs");
    let log_path = logs.join("oore-runner.log");
    let working_dir = fs::canonicalize(resolve_install_root()?)
        .context("failed to resolve the Oore install root")?;
    let executable = fs::canonicalize(working_dir.join("bin/oore"))
        .context("failed to resolve the installed oore executable")?;
    let path = std::env::var("PATH").unwrap_or_else(|_| {
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string()
    });
    fs::create_dir_all(&logs)?;
    let acknowledgement = ensure_runner_acknowledgement_path(&working_dir)?;

    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    let previous_system_contents = previous_system_plist
        .is_file()
        .then(|| {
            fs::read_to_string(&previous_system_plist).with_context(|| {
                format!(
                    "failed to preserve existing runner service {}",
                    previous_system_plist.display()
                )
            })
        })
        .transpose()?;
    let previous_legacy_contents = legacy_plist
        .is_file()
        .then(|| {
            fs::read_to_string(&legacy_plist).with_context(|| {
                format!(
                    "failed to preserve legacy runner service {}",
                    legacy_plist.display()
                )
            })
        })
        .transpose()?;

    authorize_system_service_restart()?;
    let system_was_loaded = system_launchd_service_loaded(&system_service)?;
    if system_was_loaded && previous_system_contents.is_none() {
        anyhow::bail!(
            "the existing system runner is loaded but its plist is missing; refusing to replace a service that cannot be restored"
        );
    }

    let claim_barrier = if legacy_was_loaded && !transition_barrier_held {
        let existing_config = existing_managed_config
            .as_deref()
            .context("legacy runner service has no managed config to drain before migration")?;
        let legacy_runner = read_runner_config(existing_config)?.with_context(|| {
            format!(
                "installed managed runner config is missing: {}",
                existing_config.display()
            )
        })?;
        let database = resolve_db_path(args.state_file.as_deref())?;
        let barrier = acquire_runner_claim_barrier(&database).await?;
        if let Err(error) = barrier.wait_for_runner(&legacy_runner.runner_id).await {
            barrier
                .release()
                .await
                .context("runner drain failed and releasing the claim barrier also failed")?;
            return Err(error);
        }
        Some(barrier)
    } else {
        None
    };
    if let Some(barrier) = claim_barrier.as_ref() {
        barrier.ensure_healthy().await?;
    }

    let install_result = async {
        let (config, runner, enrolled) =
            ensure_runner_service_config(
                &args,
                requested_config.clone(),
                existing_managed_config.is_some(),
            )
            .await?;
        let restore_config = || {
            if enrolled {
                Ok(())
            } else if let Some(config) = previous_config.as_ref() {
                write_runner_config(&requested_config, config)
            } else {
                Ok(())
            }
        };
        let config = match fs::canonicalize(&config)
            .with_context(|| format!("failed to resolve runner config {}", config.display()))
        {
            Ok(config) => config,
            Err(error) => {
                restore_config()?;
                return Err(error);
            }
        };
        let rendered = render_runner_launch_daemon(
            &executable,
            &config,
            &acknowledgement,
            &home,
            &working_dir,
            &log_path,
            &path,
            &user,
        );
        let plist = match write_system_runner_plist(&rendered) {
            Ok(plist) => plist,
            Err(error) => {
                restore_config()?;
                return Err(error);
            }
        };

        let rollback_transition = || {
            rollback_runner_service_install(
                &working_dir,
                &requested_config,
                if enrolled {
                    None
                } else {
                    previous_config.as_ref()
                },
                existing_managed_config
                    .as_deref()
                    .filter(|path| !paths_refer_to_same_file(path, &requested_config))
                    .map(|path| (path, &runner)),
                previous_system_contents
                    .as_deref()
                    .map(|contents| (contents, system_was_loaded)),
                previous_legacy_contents.as_deref().map(|contents| {
                    (
                        legacy_plist.as_path(),
                        contents,
                        domain.as_str(),
                        service.as_str(),
                        legacy_was_loaded,
                    )
                }),
            )
        };

        let transition = publish_runner_release_marker(&working_dir)
            .and_then(|()| stop_user_launchd_service(&service))
            .and_then(|()| stop_system_launchd_service(&system_service))
            .and_then(|()| start_system_runner_service(&plist, &system_service, true))
            .and_then(|()| {
                if legacy_plist.exists() {
                    fs::remove_file(&legacy_plist).with_context(|| {
                        format!(
                            "failed to remove legacy runner service {}",
                            legacy_plist.display()
                        )
                    })?;
                }
                Ok(())
            })
            .and_then(|()| {
                if let Some(existing_config) = existing_managed_config.as_deref() {
                    remove_migrated_managed_runner_config(existing_config, &requested_config)?;
                }
                Ok(())
            });
        if let Err(error) = transition {
            if let Err(rollback_error) = rollback_transition() {
                anyhow::bail!(
                    "{error}; restoring the previous runner service also failed: {rollback_error}"
                );
            }
            return Err(error);
        }
        if let Some((database, installed_version, restored_version)) = update_ack {
            let acknowledgement = async {
                let ack =
                    prepare_runner_update_ack(database, installed_version, restored_version).await?;
                ack.wait_for(ReleaseActivation::Installed).await
            }
            .await;
            if let Err(error) = acknowledgement {
                if let Err(rollback_error) = rollback_transition() {
                    anyhow::bail!(
                        "{error}; restoring the previous runner service also failed: {rollback_error}"
                    );
                }
                return Err(error);
            }
        }
        Ok((runner, enrolled, plist))
    }
    .await;
    let barrier_release = match claim_barrier {
        Some(barrier) => barrier.release().await,
        None => Ok(()),
    };
    let (runner, enrolled, plist) = match (install_result, barrier_release) {
        (Ok(result), Ok(())) => result,
        (Err(error), Ok(())) => return Err(error),
        (Ok(_), Err(error)) => return Err(error),
        (Err(error), Err(release_error)) => {
            return Err(error.context(format!(
                "releasing the runner claim barrier also failed: {release_error}"
            )));
        }
    };
    println!("Installed and started boot-time runner service: {RUNNER_SERVICE_LABEL}");
    println!("Runner: {} ({})", runner.name, runner.runner_id);
    if enrolled {
        println!("This local Oore installation was enrolled automatically.");
    }
    println!("Plist: {}", plist.display());
    println!("Logs:  {}", log_path.display());
    Ok(())
}

fn handle_runner_uninstall_service() -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed runner service removal is supported on macOS only");
    }
    let _ = current_user_name()?;
    let system_plist = system_runner_plist();
    let system_acknowledgement = system_plist
        .is_file()
        .then(|| managed_runner_service_spec(&system_plist))
        .transpose()?
        .flatten()
        .map(|spec| spec.acknowledgement);
    let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
    let system_service_present =
        system_plist.exists() || user_launchd_service_loaded(&system_service);
    if system_service_present {
        authorize_system_service_restart()?;
        stop_system_launchd_service(&system_service)?;
        if let Some(path) = &system_acknowledgement {
            clear_runner_service_acknowledgement(path)?;
        }
        sudo_tool_checked(
            "/bin/rm",
            &[OsStr::new("-f"), system_plist.as_os_str()],
            "removing the system runner plist",
        )?;
    }

    let plist = launch_agent_plist(RUNNER_SERVICE_LABEL)?;
    let legacy_acknowledgement = plist
        .is_file()
        .then(|| managed_runner_service_spec(&plist))
        .transpose()?
        .flatten()
        .map(|spec| spec.acknowledgement);
    let (_, service) = current_user_launchd_domain()?;
    stop_user_launchd_service(&service)?;
    if let Some(path) = &legacy_acknowledgement {
        clear_runner_service_acknowledgement(path)?;
    }
    if plist.exists() {
        fs::remove_file(&plist)?;
    }
    println!("Removed runner service: {RUNNER_SERVICE_LABEL}");
    println!("Runner registration and logs were left untouched.");
    Ok(())
}

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
    Ok(format!("http://127.0.0.1:{}", address.port()))
}

fn plutil_output(plist: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(args)
        .arg(plist)
        .output()
        .with_context(|| format!("failed to read launchd plist {}", plist.display()))?;
    if output.status.success() {
        return Ok(output);
    }
    std::process::Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/bin/plutil"])
        .args(args)
        .arg(plist)
        .output()
        .with_context(|| format!("failed to read protected launchd plist {}", plist.display()))
}

fn plist_program_arguments(plist: &Path) -> anyhow::Result<Vec<String>> {
    let output = plutil_output(plist, &["-extract", "ProgramArguments", "json", "-o", "-"])?;
    if !output.status.success() {
        anyhow::bail!("failed to read ProgramArguments from {}", plist.display());
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("invalid ProgramArguments in {}", plist.display()))
}

fn plist_json(plist: &Path) -> anyhow::Result<serde_json::Value> {
    let output = plutil_output(plist, &["-convert", "json", "-o", "-"])?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to convert launchd plist {} to JSON",
            plist.display()
        );
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("invalid launchd plist {}", plist.display()))
}

fn validate_managed_service_executable(
    document: &serde_json::Value,
    expected: &Path,
    label: &str,
) -> anyhow::Result<()> {
    let configured = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .and_then(|arguments| arguments.first())
        .and_then(serde_json::Value::as_str)
        .context("managed service plist has no executable argument")?;
    let configured = fs::canonicalize(configured)
        .with_context(|| format!("failed to resolve {label} executable {configured}"))?;
    let expected = fs::canonicalize(expected)
        .with_context(|| format!("failed to resolve expected {label} executable"))?;
    anyhow::ensure!(
        configured == expected,
        "managed service {label} runs {}, not the selected install at {}",
        configured.display(),
        expected.display()
    );
    Ok(())
}

fn daemon_data_paths_from_plist(
    document: &serde_json::Value,
    fallback_data_dir: &Path,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let program_arguments = document
        .get("ProgramArguments")
        .and_then(serde_json::Value::as_array)
        .context("managed daemon plist has no ProgramArguments")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("managed daemon ProgramArguments must be strings")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let environment = document
        .get("EnvironmentVariables")
        .and_then(serde_json::Value::as_object);
    let env_value = |key: &str| {
        environment
            .and_then(|values| values.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
    };
    let working_directory = document
        .get("WorkingDirectory")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from);
    let absolute = |raw: &str| {
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            path
        } else {
            working_directory
                .as_deref()
                .unwrap_or_else(|| Path::new("."))
                .join(path)
        }
    };
    let data_dir = env_value("OORED_DATA_DIR")
        .or_else(|| env_value("OORE_DATA_DIR"))
        .map(&absolute)
        .or_else(|| {
            env_value("HOME").map(|home| {
                absolute(home)
                    .join("Library")
                    .join("Application Support")
                    .join("oore")
            })
        })
        .unwrap_or_else(|| fallback_data_dir.to_path_buf());
    let database = value_from_program_arguments(&program_arguments, "--state-file")
        .as_deref()
        .or_else(|| env_value("OORE_SETUP_STATE_FILE"))
        .map(&absolute)
        .unwrap_or_else(|| data_dir.join(BACKUP_DATABASE_FILE));
    let key = env_value("OORE_ENCRYPTION_KEY_FILE")
        .map(&absolute)
        .unwrap_or_else(|| data_dir.join(BACKUP_KEY_FILE));
    Ok((database, key))
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
        let Ok(uid) = std::process::Command::new("/usr/bin/id").arg("-u").output() else {
            return false;
        };
        let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        format!("gui/{uid}/{label}")
    };
    std::process::Command::new("/bin/launchctl")
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
        let uid = std::process::Command::new("/usr/bin/id")
            .arg("-u")
            .output()
            .context("failed to determine current user id")?;
        let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        (format!("gui/{uid}/{label}"), format!("gui/{uid}"))
    };
    let command = if system {
        "/usr/bin/sudo"
    } else {
        "/bin/launchctl"
    };
    let prefix: &[&str] = if system {
        &["-n", "/bin/launchctl"]
    } else {
        &[]
    };
    if launchd_service_loaded(label) {
        let mut bootout = std::process::Command::new(command);
        bootout.args(prefix).args(["bootout", &service]);
        let status =
            run_command_with_timeout(&mut bootout, "launchd bootout", Duration::from_secs(15))?;
        if !status.success() && launchd_service_loaded(label) {
            anyhow::bail!("failed to stop managed service {label} before restart");
        }
    }

    let mut bootstrap = std::process::Command::new(command);
    bootstrap
        .args(prefix)
        .args(["bootstrap", &domain, &plist.display().to_string()]);
    let status =
        run_command_with_timeout(&mut bootstrap, "launchd bootstrap", Duration::from_secs(15))?;
    if !status.success() {
        anyhow::bail!("failed to bootstrap managed service {label}");
    }
    let mut kickstart = std::process::Command::new(command);
    kickstart.args(prefix).args(["kickstart", "-k", &service]);
    let _ = run_command_with_timeout(&mut kickstart, "launchd kickstart", Duration::from_secs(15))?;
    Ok(())
}

fn stop_launchd_service(label: &str) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        anyhow::bail!("managed service stop is supported on macOS only");
    }
    let (_, system) = managed_service_plist(label)?;
    let service = if system {
        format!("system/{label}")
    } else {
        let uid = std::process::Command::new("/usr/bin/id")
            .arg("-u")
            .output()
            .context("failed to determine current user id")?;
        let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        format!("gui/{uid}/{label}")
    };
    let mut command = if system {
        let mut command = std::process::Command::new("/usr/bin/sudo");
        command.args(["-n", "/bin/launchctl"]);
        command
    } else {
        std::process::Command::new("/bin/launchctl")
    };
    let status = run_command_with_timeout(
        command.args(["bootout", &service]),
        "launchd bootout",
        Duration::from_secs(15),
    )?;
    if !status.success() && launchd_service_loaded(label) {
        anyhow::bail!("failed to stop managed service {label}");
    }
    Ok(())
}

fn run_command_with_timeout(
    command: &mut std::process::Command,
    description: &str,
    timeout: Duration,
) -> anyhow::Result<std::process::ExitStatus> {
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {description}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to wait for {description}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "{description} timed out after {} seconds",
                timeout.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn authorize_system_service_restart() -> anyhow::Result<()> {
    println!("\nAdministrator access is required to manage Oore's macOS system services.");
    println!("Your password is requested by sudo and is not stored by Oore.");
    let status = std::process::Command::new("/usr/bin/sudo")
        .arg("-v")
        .status()
        .context("failed to request administrator access")?;
    if !status.success() {
        anyhow::bail!("administrator access was not granted; installed files were not changed");
    }
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

async fn wait_for_daemon_release(
    client: &reqwest::Client,
    ready_url: &str,
    expected_package_version: &str,
    name: &str,
) -> anyhow::Result<()> {
    let mut health_url = url::Url::parse(ready_url)
        .with_context(|| format!("invalid {name} readiness URL: {ready_url}"))?;
    health_url.set_path("/healthz");
    health_url.set_query(None);
    health_url.set_fragment(None);
    for _ in 0..30 {
        let ready = endpoint_is_healthy(client, ready_url).await;
        let version_matches = if ready {
            match client
                .get(health_url.clone())
                .timeout(Duration::from_secs(3))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    response
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|value| {
                            value
                                .get("package_version")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        })
                        .as_deref()
                        == Some(expected_package_version)
                }
                _ => false,
            }
        } else {
            false
        };
        if version_matches {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!(
        "{name} did not become ready as package version {expected_package_version}: {ready_url}"
    )
}

async fn daemon_package_version(
    client: &reqwest::Client,
    daemon_url: &str,
) -> anyhow::Result<String> {
    let response = client
        .get(endpoint_url(daemon_url, "/healthz"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .context("failed to read the running daemon version")?
        .error_for_status()
        .context("running daemon health check failed")?;
    let payload: serde_json::Value = response
        .json()
        .await
        .context("running daemon health response was invalid")?;
    payload
        .get("package_version")
        .and_then(serde_json::Value::as_str)
        .filter(|version| !version.is_empty())
        .map(str::to_string)
        .context("running daemon health response omitted package_version")
}

fn copy_release_snapshot(install_root: &Path, snapshot: &Path) -> anyhow::Result<()> {
    for relative in [
        "bin/oore",
        "bin/oored",
        "bin/oore-web",
        "VERSION",
        "WEB_VERSION",
        "CHANNEL",
        "WEB_CHANNEL",
        "GITHUB_REPO",
        "WEB_GITHUB_REPO",
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

fn runner_release_marker_path(install_root: &Path) -> PathBuf {
    install_root.join(oore_runner::RUNNER_RELEASE_MARKER_FILE)
}

#[cfg(test)]
fn read_runner_release_marker(install_root: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let path = runner_release_marker_path(install_root);
    match fs::read(&path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("failed to read runner release marker {}", path.display())),
    }
}

fn write_runner_release_marker(install_root: &Path, contents: &[u8]) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(install_root)?;
    let path = runner_release_marker_path(install_root);
    let mut staged = tempfile::Builder::new()
        .prefix(".runner-release-")
        .tempfile_in(install_root)
        .context("failed to stage the runner release marker")?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))?;
    staged.as_file_mut().write_all(contents)?;
    staged.as_file_mut().sync_all()?;
    staged
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to publish runner release marker {}", path.display()))?;
    Ok(())
}

fn publish_runner_release_marker(install_root: &Path) -> anyhow::Result<()> {
    let marker = oore_runner::runner_release_marker(&install_root.join("bin/oore"))?;
    write_runner_release_marker(install_root, format!("{marker}\n").as_bytes())
}

fn ensure_runner_release_marker(install_root: &Path) -> anyhow::Result<()> {
    publish_runner_release_marker(install_root)
}

fn restore_runner_release_marker(install_root: &Path) -> anyhow::Result<()> {
    publish_runner_release_marker(install_root)
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
        "WEB_VERSION",
        "CHANNEL",
        "WEB_CHANNEL",
        "GITHUB_REPO",
        "WEB_GITHUB_REPO",
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
        let version = stage.join("VERSION");
        if version.is_file() {
            atomic_replace_file(&version, &install_root.join("WEB_VERSION"), false)?;
        }
        atomic_replace_directory(&web, &install_root.join("web-dist"))?;
        fs::write(install_root.join("WEB_CHANNEL"), channel.as_str())?;
        fs::write(install_root.join("WEB_GITHUB_REPO"), repo)?;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReleaseActivation {
    Installed,
    Restored,
}

struct UpdateServicePlan {
    defer_daemon_restart: bool,
    daemon_is_managed: bool,
    unmanaged_daemon_listen: Option<String>,
    daemon_should_be_running: bool,
    daemon_ready_url: String,
    installed_version: String,
    restored_version: String,
    runner_service: Option<&'static str>,
    runner_previous_pid: Option<u32>,
    runner_ack: Option<RunnerUpdateAck>,
    runner_installed_version: String,
    runner_restored_version: String,
    install_managed_runner: bool,
    managed_runner_service_existed: bool,
    managed_runner_config_existed: bool,
    managed_runner_database: Option<PathBuf>,
    web_is_managed: bool,
    web_was_running: bool,
    web_ready_url: Option<String>,
}

struct UpdateRollbackState {
    backup: PathBuf,
    database: PathBuf,
    key: PathBuf,
}

struct ManagedDaemonStoppedIntentGuard {
    restore_stopped: bool,
}

impl ManagedDaemonStoppedIntentGuard {
    fn disarm(&mut self) {
        self.restore_stopped = false;
    }
}

impl Drop for ManagedDaemonStoppedIntentGuard {
    fn drop(&mut self) {
        if self.restore_stopped {
            let _ = stop_launchd_service(DAEMON_SERVICE_LABEL);
        }
    }
}

struct ManagedDaemonServiceMigration {
    source_plist: PathBuf,
    source_was_system: bool,
    source_was_loaded: bool,
    source_service: String,
    transformed_plist: PathBuf,
    source_snapshot: PathBuf,
    target_plist: PathBuf,
}

impl ManagedDaemonServiceMigration {
    fn apply(&self) -> anyhow::Result<()> {
        sudo_tool_checked(
            "/usr/bin/install",
            &[
                OsStr::new("-o"),
                OsStr::new("root"),
                OsStr::new("-g"),
                OsStr::new("wheel"),
                OsStr::new("-m"),
                OsStr::new("0600"),
                self.transformed_plist.as_os_str(),
                self.target_plist.as_os_str(),
            ],
            "installing the migrated oored service",
        )?;
        if !self.source_was_system && self.source_was_loaded {
            let status = std::process::Command::new("/bin/launchctl")
                .args(["bootout", &self.source_service])
                .status()
                .context("failed to stop the legacy oored login service")?;
            if !status.success() {
                anyhow::bail!("failed to stop the legacy oored login service");
            }
        }
        Ok(())
    }

    fn rollback(&self) -> anyhow::Result<()> {
        let system_service = format!("system/{DAEMON_SERVICE_LABEL}");
        stop_system_launchd_service(&system_service)?;
        if self.source_was_system {
            sudo_tool_checked(
                "/usr/bin/install",
                &[
                    OsStr::new("-o"),
                    OsStr::new("root"),
                    OsStr::new("-g"),
                    OsStr::new("wheel"),
                    OsStr::new("-m"),
                    OsStr::new("0600"),
                    self.source_snapshot.as_os_str(),
                    self.target_plist.as_os_str(),
                ],
                "restoring the previous oored service",
            )?;
        } else {
            sudo_tool_checked(
                "/bin/rm",
                &[OsStr::new("-f"), self.target_plist.as_os_str()],
                "removing the migrated oored system service",
            )?;
            if let Some(parent) = self.source_plist.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&self.source_snapshot, &self.source_plist).with_context(|| {
                format!(
                    "failed to restore legacy oored service {}",
                    self.source_plist.display()
                )
            })?;
        }
        Ok(())
    }

    fn commit(&self) -> anyhow::Result<()> {
        if !self.source_was_system && self.source_plist.is_file() {
            fs::rename(&self.source_plist, &self.source_snapshot).with_context(|| {
                format!(
                    "failed to retire legacy oored service {}",
                    self.source_plist.display()
                )
            })?;
        }
        Ok(())
    }
}

struct ManagedRunnerServiceSnapshot {
    system_plist: PathBuf,
    system_contents: Option<Vec<u8>>,
    system_was_loaded: bool,
    legacy_plist: PathBuf,
    legacy_contents: Option<Vec<u8>>,
    legacy_domain: String,
    legacy_service: String,
    legacy_was_loaded: bool,
    configs: Vec<(PathBuf, Option<Vec<u8>>)>,
}

trait ManagedRunnerServiceRollback {
    fn stop_and_restore_files(&self) -> anyhow::Result<()>;
    fn start(&self) -> anyhow::Result<()>;
}

impl ManagedRunnerServiceSnapshot {
    fn capture() -> anyhow::Result<Self> {
        let system_plist = system_runner_plist();
        let legacy_plist = launch_agent_plist(RUNNER_SERVICE_LABEL)?;
        let (legacy_domain, legacy_service) = current_user_launchd_domain()?;
        let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
        let mut config_paths: Vec<PathBuf> = Vec::new();
        for plist in [&system_plist, &legacy_plist] {
            if plist.is_file() {
                let arguments = plist_program_arguments(plist)?;
                if let Some(path) = runner_config_from_program_arguments(&arguments)
                    && !config_paths
                        .iter()
                        .any(|existing| paths_refer_to_same_file(existing, &path))
                {
                    config_paths.push(path);
                }
            }
        }
        let managed_config = managed_local_runner_config_path()?;
        if !config_paths
            .iter()
            .any(|existing| paths_refer_to_same_file(existing, &managed_config))
        {
            config_paths.push(managed_config);
        }
        let configs = config_paths
            .into_iter()
            .map(|path| {
                let contents = match fs::read(&path) {
                    Ok(contents) => Some(contents),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.into()),
                };
                Ok((path, contents))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            system_contents: system_plist
                .is_file()
                .then(|| read_service_plist_bytes(&system_plist, true))
                .transpose()?,
            system_was_loaded: system_launchd_service_loaded(&system_service)?,
            legacy_contents: legacy_plist
                .is_file()
                .then(|| fs::read(&legacy_plist))
                .transpose()?,
            legacy_was_loaded: user_launchd_service_loaded(&legacy_service),
            system_plist,
            legacy_plist,
            legacy_domain,
            legacy_service,
            configs,
        })
    }
}

impl ManagedRunnerServiceRollback for ManagedRunnerServiceSnapshot {
    fn stop_and_restore_files(&self) -> anyhow::Result<()> {
        let active_acknowledgement = self
            .system_plist
            .is_file()
            .then(|| managed_runner_service_spec(&self.system_plist))
            .transpose()?
            .flatten()
            .map(|spec| spec.acknowledgement);
        let system_service = format!("system/{RUNNER_SERVICE_LABEL}");
        stop_system_launchd_service(&system_service)?;
        if let Some(path) = &active_acknowledgement {
            clear_runner_service_acknowledgement(path)?;
        }
        stop_user_launchd_service(&self.legacy_service)?;
        match self.system_contents.as_deref() {
            Some(contents) => {
                let temporary = tempfile::NamedTempFile::new()
                    .context("failed to stage the previous runner service")?;
                fs::write(temporary.path(), contents)?;
                sudo_tool_checked(
                    "/usr/bin/install",
                    &[
                        OsStr::new("-o"),
                        OsStr::new("root"),
                        OsStr::new("-g"),
                        OsStr::new("wheel"),
                        OsStr::new("-m"),
                        OsStr::new("0644"),
                        temporary.path().as_os_str(),
                        self.system_plist.as_os_str(),
                    ],
                    "restoring the previous runner service",
                )?;
            }
            None => sudo_tool_checked(
                "/bin/rm",
                &[OsStr::new("-f"), self.system_plist.as_os_str()],
                "removing the new runner service",
            )?,
        }
        match self.legacy_contents.as_deref() {
            Some(contents) => {
                if let Some(parent) = self.legacy_plist.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&self.legacy_plist, contents)?;
            }
            None => {
                if self.legacy_plist.is_file() {
                    fs::remove_file(&self.legacy_plist)?;
                }
            }
        }
        for (path, contents) in &self.configs {
            match contents {
                Some(contents) => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(path, contents)?;
                    set_private_permissions(path)?;
                }
                None => {
                    if path.is_file() {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn start(&self) -> anyhow::Result<()> {
        if self.system_was_loaded {
            start_system_runner_service(
                &self.system_plist,
                &format!("system/{RUNNER_SERVICE_LABEL}"),
                false,
            )?;
        }
        if self.legacy_was_loaded {
            restore_legacy_runner_service(
                &self.legacy_plist,
                &self.legacy_domain,
                &self.legacy_service,
            )?;
        }
        Ok(())
    }
}

fn read_service_plist_bytes(path: &Path, system: bool) -> anyhow::Result<Vec<u8>> {
    if !system {
        return fs::read(path)
            .with_context(|| format!("failed to preserve service plist {}", path.display()));
    }
    let output = sudo_tool_output("/bin/cat", &[path.as_os_str()])?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to preserve protected service plist {}",
            path.display()
        );
    }
    Ok(output.stdout)
}

fn prepare_managed_daemon_service_migration(
    install_root: &Path,
    source_plist: &Path,
    source_was_system: bool,
    source_was_loaded: bool,
    document: &serde_json::Value,
    transaction_dir: &Path,
) -> anyhow::Result<ManagedDaemonServiceMigration> {
    use std::os::unix::fs::PermissionsExt;

    let mut transformed = document.clone();
    let object = transformed
        .as_object_mut()
        .context("managed daemon plist must contain a dictionary")?;
    let arguments = object
        .get_mut("ProgramArguments")
        .and_then(serde_json::Value::as_array_mut)
        .context("managed daemon plist has no ProgramArguments")?;
    let executable = arguments
        .first_mut()
        .context("managed daemon plist has no executable argument")?;
    *executable = serde_json::Value::String(
        fs::canonicalize(install_root.join("bin/oored"))?
            .display()
            .to_string(),
    );
    if let Some(environment) = object
        .get_mut("EnvironmentVariables")
        .and_then(serde_json::Value::as_object_mut)
    {
        environment.remove("OORED_RUNNER_MODE");
    }
    object.insert(
        "UserName".to_string(),
        serde_json::Value::String(current_user_name()?),
    );

    let source_snapshot = transaction_dir.join("oored-service.previous.plist");
    let mut snapshot = create_private_file(&source_snapshot)?;
    snapshot.write_all(&read_service_plist_bytes(source_plist, source_was_system)?)?;
    snapshot.sync_all()?;

    let transformed_plist = transaction_dir.join("oored-service.next.plist");
    let mut staged = create_private_file(&transformed_plist)?;
    serde_json::to_writer_pretty(&mut staged, &transformed)?;
    staged.write_all(b"\n")?;
    staged.sync_all()?;
    let status = std::process::Command::new("/usr/bin/plutil")
        .args(["-convert", "xml1"])
        .arg(&transformed_plist)
        .status()
        .context("failed to encode the migrated oored service plist")?;
    if !status.success() {
        anyhow::bail!("failed to encode the migrated oored service plist");
    }
    fs::set_permissions(&transformed_plist, fs::Permissions::from_mode(0o600))?;

    let source_service = if source_was_system {
        format!("system/{DAEMON_SERVICE_LABEL}")
    } else {
        let output = std::process::Command::new("/usr/bin/id")
            .arg("-u")
            .output()
            .context("failed to determine current user id")?;
        anyhow::ensure!(
            output.status.success(),
            "failed to determine current user id"
        );
        let uid = String::from_utf8(output.stdout)?.trim().to_string();
        format!("gui/{uid}/{DAEMON_SERVICE_LABEL}")
    };
    Ok(ManagedDaemonServiceMigration {
        source_plist: source_plist.to_path_buf(),
        source_was_system,
        source_was_loaded,
        source_service,
        transformed_plist,
        source_snapshot,
        target_plist: PathBuf::from("/Library/LaunchDaemons")
            .join(format!("{DAEMON_SERVICE_LABEL}.plist")),
    })
}

struct RunnerUpdateAck {
    database: PathBuf,
    runner_id: String,
    baseline_heartbeat: Option<i64>,
    installed_version: String,
    restored_version: String,
}

impl RunnerUpdateAck {
    async fn wait_for(&self, activation: ReleaseActivation) -> anyhow::Result<()> {
        let expected_version = match activation {
            ReleaseActivation::Installed => &self.installed_version,
            ReleaseActivation::Restored => &self.restored_version,
        };
        let pool = connect_existing_runner_db(&self.database).await?;
        for _ in 0..60 {
            let row = sqlx::query(
                "SELECT status, last_heartbeat_at, capabilities FROM runners WHERE id = ?1",
            )
            .bind(&self.runner_id)
            .fetch_optional(&pool)
            .await
            .context("failed to read the runner handoff acknowledgement")?;
            if let Some(row) = row {
                let status: String = row.try_get("status")?;
                let heartbeat: Option<i64> = row.try_get("last_heartbeat_at")?;
                let capabilities: String = row.try_get("capabilities")?;
                let version = serde_json::from_str::<serde_json::Value>(&capabilities)
                    .ok()
                    .and_then(|value| value.get("version")?.as_str().map(str::to_string));
                let fresh = heartbeat.is_some_and(|heartbeat| {
                    self.baseline_heartbeat
                        .is_none_or(|baseline| heartbeat > baseline)
                });
                if status == "online"
                    && fresh
                    && version.as_deref() == Some(expected_version.as_str())
                {
                    pool.close().await;
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        pool.close().await;
        anyhow::bail!(
            "runner {} did not acknowledge version {} after handoff",
            self.runner_id,
            expected_version
        )
    }
}

async fn prepare_runner_update_ack(
    database: &Path,
    installed_version: &str,
    restored_version: &str,
) -> anyhow::Result<RunnerUpdateAck> {
    let (plist, _) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    let arguments = plist_program_arguments(&plist)?;
    let config_path = runner_config_from_program_arguments(&arguments)
        .context("managed runner service does not specify --config")?;
    let config = read_runner_config(&config_path)?.with_context(|| {
        format!(
            "managed runner config is missing: {}",
            config_path.display()
        )
    })?;
    let pool = connect_existing_runner_db(database).await?;
    let baseline_heartbeat =
        sqlx::query_scalar::<_, Option<i64>>("SELECT last_heartbeat_at FROM runners WHERE id = ?1")
            .bind(&config.runner_id)
            .fetch_optional(&pool)
            .await
            .context("failed to capture the runner heartbeat before update")?
            .flatten();
    pool.close().await;
    Ok(RunnerUpdateAck {
        database: database.to_path_buf(),
        runner_id: config.runner_id,
        baseline_heartbeat,
        installed_version: installed_version.to_string(),
        restored_version: restored_version.to_string(),
    })
}

trait UpdateServiceControl {
    fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()>;

    async fn activate_runner_service(
        &mut self,
        label: &str,
        _previous_pid: Option<u32>,
        _ack: Option<&RunnerUpdateAck>,
        _activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        self.restart_managed_service(label)
    }

    fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()>;

    async fn restart_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()>;

    async fn stop_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()>;

    async fn install_managed_runner(
        &mut self,
        _database: &Path,
        _installed_version: &str,
        _restored_version: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("managed runner installation is unavailable")
    }

    fn remove_managed_runner(&mut self, _remove_config: bool) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait_for_endpoint(&mut self, url: &str, name: &str) -> anyhow::Result<()>;

    async fn wait_for_daemon_release(
        &mut self,
        ready_url: &str,
        expected_package_version: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        let _ = expected_package_version;
        self.wait_for_endpoint(ready_url, name).await
    }
}

struct LiveUpdateServiceControl<'a> {
    install_root: &'a Path,
    daemon_url: &'a str,
    client: &'a reqwest::Client,
}

impl UpdateServiceControl for LiveUpdateServiceControl<'_> {
    fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        restart_launchd_service(label)
    }

    async fn activate_runner_service(
        &mut self,
        label: &str,
        previous_pid: Option<u32>,
        ack: Option<&RunnerUpdateAck>,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        if label == RUNNER_SERVICE_LABEL {
            let (plist, system) = managed_service_plist(label)?;
            if system {
                let service = format!("system/{RUNNER_SERVICE_LABEL}");
                ensure_system_runner_service_active(&plist, &service, previous_pid)?;
                if let Some(ack) = ack {
                    ack.wait_for(activation).await?;
                }
                return Ok(());
            }
            let program_arguments = plist_program_arguments(&plist)?;
            let config = runner_config_from_program_arguments(&program_arguments)
                .context("legacy runner service does not specify --config")?;
            let daemon_plist = managed_service_plist(DAEMON_SERVICE_LABEL)?.0;
            let state_file = daemon_plist
                .is_file()
                .then(|| plist_program_arguments(&daemon_plist))
                .transpose()?
                .and_then(|arguments| value_from_program_arguments(&arguments, "--state-file"));
            let mut command = std::process::Command::new(self.install_root.join("bin/oore"));
            command
                .args(["runner", "install-service", "--config"])
                .arg(config)
                .args(["--daemon-url", self.daemon_url])
                .env("OORE_INSTALL_ROOT", self.install_root);
            if let Some(state_file) = state_file {
                command.args(["--state-file", &state_file]);
            }
            let status = command
                .status()
                .context("failed to migrate the legacy runner service")?;
            if !status.success() {
                anyhow::bail!("failed to migrate the legacy runner to a boot-time service");
            }
            if let Some(ack) = ack {
                ack.wait_for(activation).await?;
            }
            return Ok(());
        }
        restart_launchd_service(label)
    }

    fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        stop_launchd_service(label)
    }

    async fn restart_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()> {
        restart_daemon(self.install_root, listen, self.daemon_url, self.client).await
    }

    async fn stop_unmanaged_daemon(&mut self, listen: &str) -> anyhow::Result<()> {
        stop_daemon(self.install_root, listen)
    }

    async fn install_managed_runner(
        &mut self,
        database: &Path,
        installed_version: &str,
        restored_version: &str,
    ) -> anyhow::Result<()> {
        handle_runner_install_service_inner(
            RunnerServiceArgs {
                config: None,
                managed_local: true,
                daemon_url: Some(self.daemon_url.to_string()),
                state_file: Some(database.display().to_string()),
                name: None,
            },
            true,
            Some((database, installed_version, restored_version)),
        )
        .await
    }

    fn remove_managed_runner(&mut self, remove_config: bool) -> anyhow::Result<()> {
        handle_runner_uninstall_service()?;
        if remove_config {
            let config = managed_local_runner_config_path()?;
            if config.is_file() {
                fs::remove_file(&config).with_context(|| {
                    format!("failed to remove new runner config {}", config.display())
                })?;
            }
        }
        Ok(())
    }

    async fn wait_for_endpoint(&mut self, url: &str, name: &str) -> anyhow::Result<()> {
        wait_for_endpoint(self.client, url, name).await
    }

    async fn wait_for_daemon_release(
        &mut self,
        ready_url: &str,
        expected_package_version: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        wait_for_daemon_release(self.client, ready_url, expected_package_version, name).await
    }
}

struct DeferredUpdateServiceControl<'a> {
    install_root: &'a Path,
    transaction_dir: &'a Path,
    client: &'a reqwest::Client,
}

impl UpdateServiceControl for DeferredUpdateServiceControl<'_> {
    fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            label == DAEMON_SERVICE_LABEL,
            "deferred backend update cannot restart unrelated service {label}"
        );
        // The boot-time daemon is KeepAlive. Publishing bin/oored is the only
        // restart action needed, and avoids an interactive sudo dependency.
        Ok(())
    }

    async fn activate_runner_service(
        &mut self,
        label: &str,
        previous_pid: Option<u32>,
        ack: Option<&RunnerUpdateAck>,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            label == RUNNER_SERVICE_LABEL,
            "deferred backend update cannot restart unrelated service {label}"
        );
        let (plist, system) = managed_service_plist(label)?;
        anyhow::ensure!(
            system,
            "deferred updates require the boot-time runner service"
        );
        let spec = managed_runner_service_spec(&plist)?;
        if let Some(spec) = &spec {
            clear_runner_service_acknowledgement(&spec.acknowledgement)?;
        }
        let started_at = now_epoch_secs();
        let mut stable = 0;
        let mut readiness_error = None;
        for _ in 0..RUNNER_AUTHENTICATED_START_POLLS {
            let pid = update_supervisor::launchd_pid(&format!("system/{RUNNER_SERVICE_LABEL}"))?;
            let handed_off = pid.is_some() && previous_pid.is_none_or(|old| pid != Some(old));
            let authenticated = match (spec.as_ref(), pid) {
                (Some(spec), Some(pid)) if handed_off => {
                    match verify_managed_runner_service(spec, pid, Some(started_at)) {
                        Ok(()) => true,
                        Err(error) => {
                            readiness_error = Some(error.to_string());
                            false
                        }
                    }
                }
                (None, _) => handed_off,
                _ => false,
            };
            if authenticated {
                stable += 1;
                if stable >= RUNNER_STABLE_RUNNING_POLLS {
                    break;
                }
            } else {
                stable = 0;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if stable < RUNNER_STABLE_RUNNING_POLLS {
            if let Some(error) = readiness_error {
                anyhow::bail!(
                    "managed runner handed off but did not authenticate from the launchd process: {error}"
                );
            }
            anyhow::bail!("managed runner did not hand off to a new process");
        }
        if let Some(ack) = ack {
            ack.wait_for(activation).await?;
        }
        Ok(())
    }

    fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            label == DAEMON_SERVICE_LABEL,
            "deferred backend update cannot stop unrelated service {label}"
        );
        update_supervisor::quiesce_candidate_daemon(self.install_root, self.transaction_dir)?;
        Ok(())
    }

    async fn restart_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
        anyhow::bail!("deferred backend updates require the managed daemon service")
    }

    async fn stop_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
        anyhow::bail!("deferred backend updates require the managed daemon service")
    }

    async fn wait_for_endpoint(&mut self, url: &str, name: &str) -> anyhow::Result<()> {
        wait_for_endpoint(self.client, url, name).await
    }

    async fn wait_for_daemon_release(
        &mut self,
        ready_url: &str,
        expected_package_version: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        wait_for_daemon_release(self.client, ready_url, expected_package_version, name).await
    }
}

impl UpdateServicePlan {
    async fn activate_dependencies<C: UpdateServiceControl>(
        &self,
        control: &mut C,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        if self.daemon_is_managed && !self.defer_daemon_restart {
            println!("Restarting the backend and verifying its release...");
            control.restart_managed_service(DAEMON_SERVICE_LABEL)?;
        } else if let Some(listen) = &self.unmanaged_daemon_listen {
            control.restart_unmanaged_daemon(listen).await?;
        }
        if (self.daemon_is_managed || self.daemon_should_be_running) && !self.defer_daemon_restart {
            let name = if activation == ReleaseActivation::Restored {
                "rollback oored"
            } else {
                "oored"
            };
            let expected_version = if activation == ReleaseActivation::Restored {
                &self.restored_version
            } else {
                &self.installed_version
            };
            control
                .wait_for_daemon_release(&self.daemon_ready_url, expected_version, name)
                .await?;
        }
        if self.web_is_managed {
            control.restart_managed_service(WEB_SERVICE_LABEL)?;
        } else if self.web_was_running && activation == ReleaseActivation::Installed {
            anyhow::bail!(
                "refusing to replace an unmanaged local web process; install it as the launchd service first"
            );
        }
        if let Some(url) = &self.web_ready_url {
            let name = if activation == ReleaseActivation::Restored {
                "rollback oore-web"
            } else {
                "oore-web"
            };
            control.wait_for_endpoint(url, name).await?;
        }
        Ok(())
    }

    fn restore_daemon_intent<C: UpdateServiceControl>(
        &self,
        control: &mut C,
    ) -> anyhow::Result<()> {
        if self.daemon_is_managed
            && !self.daemon_should_be_running
            && !self.defer_daemon_restart
            && !self.install_managed_runner
            && self.runner_service.is_none()
        {
            control.stop_managed_service(DAEMON_SERVICE_LABEL)?;
        }
        Ok(())
    }

    async fn activate_runner<C: UpdateServiceControl>(
        &self,
        control: &mut C,
        activation: ReleaseActivation,
    ) -> anyhow::Result<()> {
        if self.install_managed_runner && activation == ReleaseActivation::Installed {
            println!("Migrating the runner to its boot-time service...");
            let database = self
                .managed_runner_database
                .as_deref()
                .context("managed runner migration is missing the database path")?;
            control
                .install_managed_runner(
                    database,
                    &self.runner_installed_version,
                    &self.runner_restored_version,
                )
                .await?;
            return Ok(());
        }
        if let Some(label) = self.runner_service {
            let previous_pid = (activation == ReleaseActivation::Installed)
                .then_some(self.runner_previous_pid)
                .flatten();
            control
                .activate_runner_service(label, previous_pid, self.runner_ack.as_ref(), activation)
                .await?;
        }
        Ok(())
    }

    fn remove_new_managed_runner<C: UpdateServiceControl>(
        &self,
        control: &mut C,
    ) -> anyhow::Result<()> {
        if self.install_managed_runner && !self.managed_runner_service_existed {
            control.remove_managed_runner(!self.managed_runner_config_existed)?;
        }
        Ok(())
    }

    async fn stop_daemon_for_database_restore<C: UpdateServiceControl>(
        &self,
        control: &mut C,
    ) -> anyhow::Result<()> {
        if self.daemon_is_managed && !self.defer_daemon_restart {
            control.stop_managed_service(DAEMON_SERVICE_LABEL)?;
        } else if let Some(listen) = &self.unmanaged_daemon_listen {
            control.stop_unmanaged_daemon(listen).await?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn install_release_with_rollback<C: UpdateServiceControl>(
    staged_release: &Path,
    install_root: &Path,
    previous_release: &Path,
    channel: ReleaseChannel,
    repo: &str,
    daemon_migration: Option<&ManagedDaemonServiceMigration>,
    runner_migration: Option<&dyn ManagedRunnerServiceRollback>,
    claim_barrier: Option<&RunnerClaimBarrier>,
    services: &UpdateServicePlan,
    rollback_state: Option<&UpdateRollbackState>,
    control: &mut C,
) -> anyhow::Result<()> {
    if let Some(barrier) = claim_barrier {
        barrier.ensure_healthy().await?;
    }
    ensure_runner_release_marker(install_root)?;
    let update_result: anyhow::Result<()> = async {
        install_staged_release(staged_release, install_root, channel, repo)?;
        if let Some(migration) = daemon_migration {
            migration.apply()?;
        }
        services
            .activate_dependencies(control, ReleaseActivation::Installed)
            .await?;
        publish_runner_release_marker(install_root)?;
        services
            .activate_runner(control, ReleaseActivation::Installed)
            .await?;
        services.restore_daemon_intent(control)?;
        if let Some(barrier) = claim_barrier {
            barrier.release().await?;
        }
        if let Some(migration) = daemon_migration {
            migration.commit()?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = update_result {
        eprintln!("Update failed; restoring the previous release...");
        let rollback: anyhow::Result<()> = async {
            if let Some(snapshot) = runner_migration {
                snapshot.stop_and_restore_files()?;
            } else {
                services.remove_new_managed_runner(control)?;
            }
            if (services.daemon_is_managed || services.daemon_should_be_running)
                && !services.defer_daemon_restart
            {
                services.stop_daemon_for_database_restore(control).await?;
                if let Some(state) = rollback_state {
                    let backup = state.backup.clone();
                    let database = state.database.clone();
                    let key = state.key.clone();
                    run_blocking_update_step(move || {
                        restore_verified_backup(&backup, &database, &key)
                    })
                    .await?;
                }
            }
            if let Some(migration) = daemon_migration {
                migration.rollback()?;
            }
            restore_release_snapshot(install_root, previous_release)?;
            restore_runner_release_marker(install_root)?;
            services
                .activate_dependencies(control, ReleaseActivation::Restored)
                .await?;
            if let Some(snapshot) = runner_migration {
                snapshot.start()?;
                if let Some(ack) = services.runner_ack.as_ref() {
                    ack.wait_for(ReleaseActivation::Restored).await?;
                }
            } else {
                services
                    .activate_runner(control, ReleaseActivation::Restored)
                    .await?;
            }
            services.restore_daemon_intent(control)
        }
        .await;
        if let Err(rollback_error) = rollback {
            return Err(error.context(format!("rollback also failed: {rollback_error}")));
        }
        return Err(error);
    }
    Ok(())
}

struct PreparedUpdateRelease {
    path: PathBuf,
    version: semver::Version,
    package_version: String,
    label: String,
    _temporary: Option<tempfile::TempDir>,
}

fn compiled_daemon_package_version(executable: &Path) -> anyhow::Result<String> {
    let package_output = std::process::Command::new(executable)
        .arg("package-version")
        .output()
        .with_context(|| {
            format!(
                "failed to inspect candidate daemon {}",
                executable.display()
            )
        })?;
    let output = if package_output.status.success() {
        package_output
    } else {
        let empty_install_root =
            tempfile::tempdir().context("failed to prepare legacy daemon package version probe")?;
        let legacy_output = std::process::Command::new(executable)
            .arg("version")
            .env("OORE_INSTALL_ROOT", empty_install_root.path())
            .output()
            .with_context(|| format!("failed to inspect legacy daemon {}", executable.display()))?;
        anyhow::ensure!(
            legacy_output.status.success(),
            "daemon did not report its compiled package version"
        );
        legacy_output
    };
    let version = String::from_utf8(output.stdout)?.trim().to_string();
    anyhow::ensure!(
        !version.is_empty(),
        "candidate daemon package version is empty"
    );
    Ok(version)
}

fn validate_prepared_release(path: &Path) -> anyhow::Result<()> {
    for relative in ["bin/oore", "bin/oored", "VERSION"] {
        anyhow::ensure!(
            path.join(relative).is_file(),
            "prepared release is missing {relative}"
        );
    }
    Ok(())
}

async fn prepare_update_release(
    client: &reqwest::Client,
    repo: &str,
    channel: ReleaseChannel,
    staged_release: Option<&Path>,
) -> anyhow::Result<PreparedUpdateRelease> {
    if let Some(path) = staged_release {
        let path = fs::canonicalize(path)
            .with_context(|| format!("failed to resolve staged release {}", path.display()))?;
        validate_prepared_release(&path)?;
        let raw =
            read_trimmed_file(&path.join("VERSION")).context("staged release VERSION is empty")?;
        let version = parse_semver_loose(&raw).context("invalid staged release VERSION")?;
        let package_version = compiled_daemon_package_version(&path.join("bin/oored"))?;
        return Ok(PreparedUpdateRelease {
            path,
            package_version,
            label: format!("staged {version}"),
            version,
            _temporary: None,
        });
    }

    let release = fetch_latest_release(client, repo, channel).await?;
    let version = parse_semver_loose(&release.version)
        .with_context(|| format!("invalid version in release tag: {}", release.tag))?;
    let arch = release_arch()?;
    let archive_filename = format!("oore_{}_darwin_{arch}.tar.gz", release.version);
    let checksums_filename = format!("oore_{}_checksums.txt", release.version);
    let archive_url = find_asset_url(&release, &archive_filename);
    let checksums_url = find_asset_url(&release, &checksums_filename);
    println!("Downloading {archive_filename}...");
    let (archive_response, checksums_response) = tokio::try_join!(
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
    let archive_bytes = archive_response
        .bytes()
        .await
        .context("failed to read archive bytes")?;
    let checksums = checksums_response
        .text()
        .await
        .context("failed to read checksums text")?;
    let expected_hash = parse_checksum(&checksums, &archive_filename)?;
    let actual_hash = hex::encode(Sha256::digest(&archive_bytes));
    anyhow::ensure!(
        actual_hash == expected_hash,
        "Checksum mismatch!\n  Expected: {expected_hash}\n  Actual:   {actual_hash}"
    );
    println!("Checksum verified (SHA-256).");

    let temporary = tempfile::tempdir().context("failed to create temporary directory")?;
    let decoder = flate2::read::GzDecoder::new(&archive_bytes[..]);
    tar::Archive::new(decoder)
        .unpack(temporary.path())
        .context("failed to extract archive")?;
    validate_prepared_release(temporary.path())?;
    let package_version = compiled_daemon_package_version(&temporary.path().join("bin/oored"))?;
    Ok(PreparedUpdateRelease {
        path: temporary.path().to_path_buf(),
        version,
        package_version,
        label: release.tag,
        _temporary: Some(temporary),
    })
}

fn require_deferred_path(value: Option<&PathBuf>, name: &str) -> anyhow::Result<PathBuf> {
    let path = value
        .cloned()
        .with_context(|| format!("deferred update is missing --{name}"))?;
    anyhow::ensure!(path.is_absolute(), "--{name} must be an absolute path");
    Ok(path)
}

fn require_loopback_daemon_url(raw: Option<&str>) -> anyhow::Result<String> {
    let raw = raw.context("deferred update is missing --deferred-daemon-url")?;
    let url = url::Url::parse(raw).context("invalid deferred daemon URL")?;
    anyhow::ensure!(url.scheme() == "http", "deferred daemon URL must use http");
    let host = url.host_str().context("deferred daemon URL has no host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    anyhow::ensure!(loopback, "deferred daemon URL must use a loopback host");
    anyhow::ensure!(
        url.path() == "/" && url.query().is_none() && url.fragment().is_none(),
        "deferred daemon URL must not contain a path, query, or fragment"
    );
    Ok(raw.trim_end_matches('/').to_string())
}

async fn run_deferred_update(args: &UpdateArgs, status_path: &Path) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let expected_status = install_root.join(".runtime-update-status.json");
    anyhow::ensure!(
        status_path == expected_status,
        "deferred status file must be {}",
        expected_status.display()
    );
    anyhow::ensure!(
        !args.ensure_managed_runner,
        "deferred updates cannot perform first-time service migration"
    );
    let parent_pid = args
        .deferred_parent_pid
        .context("deferred update is missing --deferred-parent-pid")?;
    anyhow::ensure!(parent_pid > 1, "invalid deferred parent pid");
    let database = require_deferred_path(args.deferred_state_file.as_ref(), "deferred-state-file")?;
    let key = require_deferred_path(args.deferred_key_file.as_ref(), "deferred-key-file")?;
    anyhow::ensure!(database.is_file(), "deferred state database does not exist");
    anyhow::ensure!(key.is_file(), "deferred encryption key does not exist");
    let daemon_url = require_loopback_daemon_url(args.deferred_daemon_url.as_deref())?;
    let daemon_ready_url = endpoint_url(&daemon_url, "/readyz");

    let current_raw = read_trimmed_file(&install_root.join("VERSION"))
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let current = parse_semver_loose(&current_raw).context("failed to parse current version")?;
    let repo = args
        .repo
        .clone()
        .map(|repo| normalize_github_repo(&repo))
        .or_else(|| read_installed_repo(&install_root))
        .unwrap_or_else(|| DEFAULT_GITHUB_REPO.to_string());
    let channel = if let Some(raw) = &args.channel {
        ReleaseChannel::parse(raw)?
    } else if let Some(channel) = read_installed_channel(&install_root) {
        channel
    } else {
        infer_channel_from_version(&current)
    };
    let client = http_client()?;
    let prepared =
        prepare_update_release(&client, &repo, channel, args.staged_release.as_deref()).await?;
    if current >= prepared.version && !args.force {
        return Ok(());
    }
    anyhow::ensure!(!args.check, "deferred update cannot use --check");

    let (runner_plist, runner_is_system) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    anyhow::ensure!(
        runner_is_system && runner_plist.is_file(),
        "web updates require the boot-time managed runner service"
    );
    anyhow::ensure!(
        !managed_runner_service_requires_repair(&runner_plist)?,
        "this runner service needs a one-time repair; run the current installer from Terminal before updating from the web UI"
    );
    anyhow::ensure!(
        managed_runner_update_service(&install_root)?.is_some(),
        "managed runner service does not use the selected Oore install"
    );
    let restored_package_version = daemon_package_version(&client, &daemon_url).await?;
    wait_for_daemon_release(
        &client,
        &daemon_ready_url,
        &restored_package_version,
        "existing oored",
    )
    .await?;
    let runner_previous_pid = managed_runner_process_pid()?;
    let runner_ack = prepare_runner_update_ack(
        &database,
        &prepared.version.to_string(),
        &current.to_string(),
    )
    .await?;

    fs::create_dir_all(&install_root)?;
    let update_stage = tempfile::Builder::new()
        .prefix(".supervised-update-")
        .tempdir_in(&install_root)
        .context("failed to create supervised update transaction")?;
    let staged_release = update_stage.path().join("release");
    copy_dir_recursive(&prepared.path, &staged_release)?;
    let previous_release = update_stage.path().join("previous");
    copy_release_snapshot(&install_root, &previous_release)?;

    let barrier = acquire_runner_claim_barrier(&database).await?;
    if let Err(error) = barrier.wait_for_all_work().await {
        barrier.release().await?;
        return Err(error);
    }
    update_supervisor::write_status(
        status_path,
        oore_contract::RuntimeUpdatePhase::Restarting,
        None,
    )?;
    let mut held = match update_supervisor::HeldDaemonExecutable::hold_parent(
        &install_root,
        update_stage.path(),
        parent_pid,
    ) {
        Ok(held) => held,
        Err(error) => {
            barrier.release().await?;
            return Err(error);
        }
    };

    let backup_dir = install_root.join("backups");
    let backup_path = backup_dir.join(format!(
        "pre-update-{}-{}.tar.gz",
        current,
        now_epoch_secs()
    ));
    let backup_database = database.clone();
    let backup_key = key.clone();
    let backup_output = backup_path.clone();
    if let Err(error) = run_blocking_update_step(move || {
        create_backup_archive(&backup_database, &backup_key, &backup_output)
    })
    .await
    {
        held.restore()?;
        wait_for_daemon_release(
            &client,
            &daemon_ready_url,
            &restored_package_version,
            "existing oored",
        )
        .await?;
        barrier.release().await?;
        return Err(error);
    }
    held.release_to_transaction();

    let rollback = UpdateRollbackState {
        backup: backup_path,
        database: database.clone(),
        key,
    };
    let services = UpdateServicePlan {
        defer_daemon_restart: false,
        daemon_is_managed: true,
        unmanaged_daemon_listen: None,
        daemon_should_be_running: true,
        daemon_ready_url,
        installed_version: prepared.package_version,
        restored_version: restored_package_version,
        runner_service: Some(RUNNER_SERVICE_LABEL),
        runner_previous_pid,
        runner_ack: Some(runner_ack),
        runner_installed_version: prepared.version.to_string(),
        runner_restored_version: current.to_string(),
        install_managed_runner: false,
        managed_runner_service_existed: true,
        managed_runner_config_existed: true,
        managed_runner_database: None,
        web_is_managed: false,
        web_was_running: false,
        web_ready_url: None,
    };
    let mut control = DeferredUpdateServiceControl {
        install_root: &install_root,
        transaction_dir: update_stage.path(),
        client: &client,
    };
    let result = install_release_with_rollback(
        &staged_release,
        &install_root,
        &previous_release,
        channel,
        &repo,
        None,
        None,
        Some(&barrier),
        &services,
        Some(&rollback),
        &mut control,
    )
    .await;
    if let Err(error) = result {
        return match barrier.release().await {
            Ok(()) => Err(error),
            Err(release_error) => Err(error.context(format!(
                "releasing the maintenance barrier also failed: {release_error}"
            ))),
        };
    }
    Ok(())
}

async fn handle_deferred_update(args: UpdateArgs) -> anyhow::Result<()> {
    let status_path =
        require_deferred_path(args.deferred_status_file.as_ref(), "deferred-status-file")?;
    update_supervisor::record_owned_result(&status_path, run_deferred_update(&args, &status_path))
        .await
}

async fn handle_update_supervisor(args: UpdateSupervisorArgs) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let install_root = resolve_install_root()?;
    let expected = install_root
        .join("run/runtime-update-queue")
        .join("request.json");
    anyhow::ensure!(
        args.request_file == expected,
        "runtime update request must be {}",
        expected.display()
    );
    let metadata = fs::symlink_metadata(&args.request_file)
        .with_context(|| format!("failed to inspect {}", args.request_file.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "runtime update request is not a file"
    );
    anyhow::ensure!(
        metadata.uid() == unsafe { libc::geteuid() },
        "runtime update request is owned by another user"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "runtime update request permissions are too broad"
    );

    let active = install_root
        .join("run")
        .join(format!("runtime-update-active-{}.json", std::process::id()));
    fs::rename(&args.request_file, &active).context("failed to claim runtime update request")?;
    let result = async {
        let request: DeferredRuntimeUpdateRequest = serde_json::from_slice(
            &fs::read(&active).context("failed to read runtime update request")?,
        )
        .context("invalid runtime update request")?;
        handle_deferred_update(UpdateArgs {
            check: false,
            force: false,
            channel: None,
            repo: None,
            staged_release: None,
            ensure_managed_runner: false,
            deferred_parent_pid: Some(request.parent_pid),
            deferred_state_file: Some(request.database),
            deferred_key_file: Some(request.key),
            deferred_daemon_url: Some(request.daemon_url),
            deferred_status_file: Some(request.status),
        })
        .await
    }
    .await;
    let cleanup = fs::remove_file(&active).context("failed to clear runtime update request");
    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

async fn handle_update(args: UpdateArgs) -> anyhow::Result<()> {
    let install_root = resolve_install_root()?;
    let defer_daemon_restart = std::env::var_os("OORE_UPDATE_DEFER_DAEMON_RESTART").is_some();
    if defer_daemon_restart {
        return handle_deferred_update(args).await;
    }
    let current_str = read_trimmed_file(&install_root.join("VERSION"))
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let current = parse_semver_loose(&current_str).context("failed to parse current version")?;

    let repo = args
        .repo
        .clone()
        .map(|repo| normalize_github_repo(&repo))
        .or_else(|| read_installed_repo(&install_root))
        .unwrap_or_else(|| DEFAULT_GITHUB_REPO.to_string());

    let channel = if let Some(raw) = &args.channel {
        ReleaseChannel::parse(raw)?
    } else if let Some(ch) = read_installed_channel(&install_root) {
        ch
    } else {
        infer_channel_from_version(&current)
    };

    let client = http_client()?;
    let prepared =
        prepare_update_release(&client, &repo, channel, args.staged_release.as_deref()).await?;
    let latest = &prepared.version;
    println!("Channel:         {}", channel.as_str());
    println!("GitHub repo:     {repo}");
    println!("Current version: {current}");
    println!("Latest version:  {latest} ({})", prepared.label);

    if current >= *latest && !args.force {
        println!("Already up to date.");
        return Ok(());
    }

    if current < *latest {
        println!("Update available: {current} -> {latest}");
    } else {
        println!("Reinstalling version {latest} (--force).");
    }
    if args.check {
        return Ok(());
    }

    fs::create_dir_all(&install_root)
        .with_context(|| format!("failed to create install root {}", install_root.display()))?;
    let update_stage = tempfile::Builder::new()
        .prefix(".update-")
        .tempdir_in(&install_root)
        .context("failed to create update staging directory")?;
    let staged_release = update_stage.path().join("release");
    copy_dir_recursive(&prepared.path, &staged_release)?;
    let previous_release = update_stage.path().join("previous");
    copy_release_snapshot(&install_root, &previous_release)?;

    let (daemon_service_plist, daemon_service_is_system) =
        managed_service_plist(DAEMON_SERVICE_LABEL)?;
    let daemon_service_is_installed = daemon_service_plist.is_file();
    let daemon_was_loaded = launchd_service_loaded(DAEMON_SERVICE_LABEL);
    let (runner_plist, runner_is_system) = managed_service_plist(RUNNER_SERVICE_LABEL)?;
    let runner_service_requires_repair = runner_plist.is_file()
        && runner_is_system
        && managed_runner_service_requires_repair(&runner_plist)?;
    if runner_service_requires_repair && !args.ensure_managed_runner {
        anyhow::bail!(
            "this runner service needs a one-time repair; rerun the verified installer for the installed channel before using ordinary updates"
        );
    }
    let (web_plist, web_is_system) = managed_service_plist(WEB_SERVICE_LABEL)?;
    let web_is_managed = launchd_service_loaded(WEB_SERVICE_LABEL);
    if (daemon_service_is_installed && daemon_service_is_system)
        || (runner_plist.is_file() && runner_is_system)
        || (web_is_managed && web_is_system)
        || args.ensure_managed_runner
    {
        authorize_system_service_restart()?;
    }

    let daemon_document = daemon_service_is_installed
        .then(|| plist_json(&daemon_service_plist))
        .transpose()?;
    if let Some(document) = daemon_document.as_ref() {
        validate_managed_service_executable(
            document,
            &install_root.join("bin/oored"),
            DAEMON_SERVICE_LABEL,
        )?;
    }
    let managed_daemon_arguments = daemon_document
        .as_ref()
        .map(|document| {
            document
                .get("ProgramArguments")
                .and_then(serde_json::Value::as_array)
                .context("managed daemon plist has no ProgramArguments")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .context("managed daemon ProgramArguments must be strings")
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .transpose()?;
    let managed_daemon_listen = managed_daemon_arguments
        .as_deref()
        .map(listen_address_from_program_arguments)
        .transpose()?;
    let daemon_url = managed_daemon_listen
        .as_deref()
        .map(daemon_url_from_listen_address)
        .transpose()?
        .unwrap_or(resolve_daemon_url(None)?);
    let unmanaged_daemon_running =
        !daemon_service_is_installed && check_daemon_running(&client, &daemon_url).await;
    if unmanaged_daemon_running {
        anyhow::bail!(
            "refusing to update a running unmanaged oored process because its exact state paths and restart environment cannot be preserved; install the managed service first"
        );
    }
    let (database, key) = if let Some(document) = daemon_document.as_ref() {
        daemon_data_paths_from_plist(document, &resolve_data_dir()?)?
    } else {
        (resolve_db_path(None)?, resolve_key_path()?)
    };

    let legacy_runner_mode = daemon_document
        .as_ref()
        .and_then(|document| document.get("EnvironmentVariables"))
        .and_then(serde_json::Value::as_object)
        .and_then(|environment| environment.get("OORED_RUNNER_MODE"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|mode| matches!(mode, "embedded" | "hybrid"));
    let daemon_migration_required =
        daemon_service_is_installed && (!daemon_service_is_system || legacy_runner_mode);
    if daemon_migration_required && !args.ensure_managed_runner {
        anyhow::bail!(
            "this legacy managed backend needs the transactional service migration; rerun the verified installer to upgrade it safely"
        );
    }
    let daemon_migration = daemon_migration_required
        .then(|| {
            prepare_managed_daemon_service_migration(
                &install_root,
                &daemon_service_plist,
                daemon_service_is_system,
                daemon_was_loaded,
                daemon_document
                    .as_ref()
                    .context("managed daemon migration is missing its plist")?,
                update_stage.path(),
            )
        })
        .transpose()?;

    let managed_runner_service = managed_runner_update_service(&install_root)?;
    let install_managed_runner = args.ensure_managed_runner;
    let managed_runner_service_existed = managed_runner_service.is_some();
    let runner_migration = install_managed_runner
        .then(ManagedRunnerServiceSnapshot::capture)
        .transpose()?;
    let runner_update_service = (!install_managed_runner)
        .then_some(managed_runner_service)
        .flatten();
    if args.ensure_managed_runner && !daemon_service_is_installed {
        anyhow::bail!("--ensure-managed-runner requires an existing managed backend service");
    }
    let managed_runner_config_existed = managed_local_runner_config_path()?.is_file();

    let daemon_ready_url = endpoint_url(&daemon_url, "/readyz");
    let mut stopped_intent_guard = ManagedDaemonStoppedIntentGuard {
        restore_stopped: daemon_service_is_installed && !daemon_was_loaded,
    };
    if daemon_service_is_installed && !daemon_was_loaded {
        println!("The managed backend is stopped; starting it temporarily for a safe drain...");
        restart_launchd_service(DAEMON_SERVICE_LABEL)?;
        wait_for_endpoint(&client, &daemon_ready_url, "existing oored").await?;
    }
    let daemon_was_running =
        daemon_service_is_installed && check_daemon_running(&client, &daemon_url).await;
    let restored_package_version = if daemon_was_running {
        daemon_package_version(&client, &daemon_url).await?
    } else {
        compiled_daemon_package_version(&install_root.join("bin/oored"))?
    };

    if web_is_managed {
        let web_document = plist_json(&web_plist)?;
        validate_managed_service_executable(
            &web_document,
            &install_root.join("bin/oore-web"),
            WEB_SERVICE_LABEL,
        )?;
    }
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

    let runner_previous_pid = runner_update_service
        .map(|_| managed_runner_process_pid())
        .transpose()?
        .flatten();
    let runner_ack = match managed_runner_service {
        Some(_) => Some(
            prepare_runner_update_ack(
                &database,
                &prepared.version.to_string(),
                &current.to_string(),
            )
            .await?,
        ),
        None => None,
    };

    let barrier = acquire_runner_claim_barrier(&database).await?;
    if let Err(error) = barrier.wait_for_all_work().await {
        barrier.release().await?;
        if daemon_service_is_installed && !daemon_was_loaded {
            stop_launchd_service(DAEMON_SERVICE_LABEL)?;
        }
        return Err(error);
    }

    if daemon_service_is_installed && let Err(error) = stop_launchd_service(DAEMON_SERVICE_LABEL) {
        return match barrier.release().await {
            Ok(()) => Err(error),
            Err(release_error) => Err(error.context(format!(
                "stopping oored failed and releasing the maintenance barrier also failed: {release_error}"
            ))),
        };
    }
    let backup_dir = install_root.join("backups");
    let backup_path = backup_dir.join(format!(
        "pre-update-{}-{}.tar.gz",
        current,
        now_epoch_secs()
    ));
    let backup_output = backup_path.clone();
    let backup_database = database.clone();
    let backup_key = key.clone();
    let backup_result = run_blocking_update_step(move || {
        create_backup_archive(&backup_database, &backup_key, &backup_output)
    })
    .await;
    if let Err(error) = backup_result {
        if daemon_service_is_installed {
            restart_launchd_service(DAEMON_SERVICE_LABEL)?;
            wait_for_daemon_release(
                &client,
                &daemon_ready_url,
                &restored_package_version,
                "existing oored",
            )
            .await?;
            if !daemon_was_loaded {
                stop_launchd_service(DAEMON_SERVICE_LABEL)?;
            }
        }
        barrier.release().await?;
        return Err(error);
    }
    println!("Created pre-update backup: {}", backup_path.display());
    let rollback_state = UpdateRollbackState {
        backup: backup_path,
        database: database.clone(),
        key: key.clone(),
    };
    let services = UpdateServicePlan {
        defer_daemon_restart,
        daemon_is_managed: daemon_service_is_installed,
        unmanaged_daemon_listen: None,
        daemon_should_be_running: daemon_was_loaded,
        daemon_ready_url,
        installed_version: prepared.package_version.clone(),
        restored_version: restored_package_version,
        runner_service: runner_update_service,
        runner_previous_pid,
        runner_ack,
        runner_installed_version: prepared.version.to_string(),
        runner_restored_version: current.to_string(),
        install_managed_runner,
        managed_runner_service_existed,
        managed_runner_config_existed,
        managed_runner_database: install_managed_runner.then_some(database),
        web_is_managed,
        web_was_running,
        web_ready_url,
    };
    let mut control = LiveUpdateServiceControl {
        install_root: &install_root,
        daemon_url: &daemon_url,
        client: &client,
    };
    let update_result = install_release_with_rollback(
        &staged_release,
        &install_root,
        &previous_release,
        channel,
        &repo,
        daemon_migration.as_ref(),
        runner_migration
            .as_ref()
            .map(|snapshot| snapshot as &dyn ManagedRunnerServiceRollback),
        Some(&barrier),
        &services,
        Some(&rollback_state),
        &mut control,
    )
    .await;
    if let Err(error) = update_result {
        return match barrier.release().await {
            Ok(()) => Err(error),
            Err(release_error) => Err(error.context(format!(
                "releasing the maintenance barrier also failed: {release_error}"
            ))),
        };
    }
    if !daemon_was_loaded && (install_managed_runner || runner_update_service.is_some()) {
        stopped_intent_guard.disarm();
        println!(
            "The backend was stopped before the upgrade and is now running so its managed runner can stay available."
        );
    }
    println!("Updated to version {latest}.");

    // 12. Note about current process
    if current != *latest {
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
        Commands::Recovery(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_recovery(args))?;
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
            RunnerSubcommand::InstallService(args) => {
                let runtime =
                    tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                runtime.block_on(handle_runner_install_service(args))?;
            }
            RunnerSubcommand::UninstallService => handle_runner_uninstall_service()?,
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
        Commands::UpdateSupervisor(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
            runtime.block_on(handle_update_supervisor(args))?;
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
    fn package_version_probe_supports_legacy_daemons() {
        let temp = tempfile::tempdir().unwrap();
        let daemon = temp.path().join("oored");
        fs::write(
            &daemon,
            "#!/bin/sh\n[ \"$1\" = version ] && [ ! -f \"$OORE_INSTALL_ROOT/VERSION\" ] && printf '0.1.10\\n'\n",
        )
        .unwrap();
        set_executable(&daemon).unwrap();

        assert_eq!(compiled_daemon_package_version(&daemon).unwrap(), "0.1.10");
    }

    #[test]
    fn backup_output_is_private_before_first_write() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("backup.tmp");
        let mut file = create_private_file(&path).unwrap();

        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        file.write_all(b"backup").unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn backup_rejects_link_members() {
        for entry_type in [tar::EntryType::Symlink, tar::EntryType::Link] {
            let temp = tempfile::tempdir().unwrap();
            let input = temp.path().join("backup.tar.gz");
            let file = fs::File::create(&input).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(entry_type);
            header.set_mode(0o600);
            header.set_size(0);
            header.set_path(BACKUP_DATABASE_FILE).unwrap();
            header.set_link_name("/outside-backup").unwrap();
            header.set_cksum();
            archive.append(&header, std::io::empty()).unwrap();
            archive.finish().unwrap();
            archive.into_inner().unwrap().finish().unwrap();

            let destination = temp.path().join("unpack");
            fs::create_dir(&destination).unwrap();
            let error = unpack_backup(&input, &destination).unwrap_err();
            assert!(
                error.to_string().contains("must be a regular file"),
                "unexpected error: {error:#}"
            );
        }
    }

    #[test]
    fn runner_service_is_a_boot_time_daemon_under_the_runner_account() {
        let plist = render_runner_launch_daemon(
            Path::new("/Users/me/.oore/bin/oore"),
            Path::new("/Users/me/.oore/runner.json"),
            Path::new("/Users/me/.oore/run/runner-service-ack.json"),
            Path::new("/Users/me"),
            Path::new("/Users/me/.oore"),
            Path::new("/Users/me/.oore/logs/oore-runner.log"),
            "/usr/bin:/bin",
            "me",
        );

        assert!(plist.contains("<string>build.oore.oore-runner</string>"));
        assert!(plist.contains("<key>UserName</key>\n    <string>me</string>"));
        assert!(plist.contains("<key>SessionCreate</key>\n    <true/>"));
        assert!(!plist.contains("<string>/bin/launchctl</string>"));
        assert!(!plist.contains("<string>/usr/bin/sudo</string>"));
        assert!(!plist.contains("LimitLoadToSessionType"));
        assert!(plist.contains("<string>/Users/me/.oore/bin/oore</string>"));
        assert!(plist.contains("<string>runner</string>\n        <string>start</string>"));
        assert!(plist.contains("<string>/Users/me/.oore/runner.json</string>"));
        assert!(plist.contains("<key>OORE_RUNNER_SERVICE_ACK_PATH</key>"));
        assert!(plist.contains("<string>/Users/me/.oore/run/runner-service-ack.json</string>"));
        assert!(
            plist.contains("<key>WorkingDirectory</key>\n    <string>/Users/me/.oore</string>")
        );
    }

    #[test]
    fn doctor_reports_only_a_complete_boot_service_as_healthy() {
        let healthy = managed_runner_service_doctor_result(true, true, true, false, false, None);
        assert_eq!(healthy.status, "ok");
        assert_eq!(healthy.name, "runner_service");

        for (system_plist, system_loaded, system_running, legacy_plist, legacy_loaded) in [
            (true, false, false, false, false),
            (true, true, false, false, false),
            (false, true, true, false, false),
            (false, false, false, true, false),
            (false, false, false, false, true),
            (false, false, false, false, false),
            (true, true, true, true, false),
        ] {
            let check = managed_runner_service_doctor_result(
                system_plist,
                system_loaded,
                system_running,
                legacy_plist,
                legacy_loaded,
                None,
            );
            assert_eq!(check.status, "warning", "unexpected check: {check:?}");
            assert!(check.install_hint.is_some());
        }
    }

    #[test]
    fn doctor_names_the_login_session_migration() {
        let check = managed_runner_service_doctor_result(false, false, false, true, true, None);
        assert_eq!(check.status, "warning");
        assert!(
            check
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("GUI login"))
        );
        assert!(
            check
                .install_hint
                .as_deref()
                .is_some_and(|hint| hint.contains("oore runner install-service"))
        );
    }

    #[test]
    fn doctor_reports_a_loaded_runner_without_a_process_as_crash_looping() {
        let check = managed_runner_service_doctor_result(true, true, false, false, false, None);

        assert_eq!(check.status, "warning");
        assert!(
            check
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("crash-looping"))
        );
    }

    #[test]
    fn runner_service_escapes_launchd_values() {
        let plist = render_runner_launch_daemon(
            Path::new("/Users/a&b/oore"),
            Path::new("/Users/a&b/runner.json"),
            Path::new("/Users/a&b/runner-service-ack.json"),
            Path::new("/Users/a&b"),
            Path::new("/Users/a&b/.oore"),
            Path::new("/Users/a&b/runner.log"),
            "/usr/bin:/a&b",
            "a&b",
        );

        assert!(plist.contains("/Users/a&amp;b/oore"));
        assert!(plist.contains("/Users/a&amp;b/runner-service-ack.json"));
        assert!(plist.contains("/usr/bin:/a&amp;b"));
        assert!(plist.contains("<string>a&amp;b</string>"));
    }

    #[test]
    fn wrapped_alpha_runner_service_requires_installer_repair() {
        let arguments = vec![
            "/bin/launchctl",
            "asuser",
            "501",
            "/usr/bin/sudo",
            "-E",
            "-H",
            "-u",
            "appbuilder",
            "/Users/appbuilder/.oore/bin/oore",
            "runner",
            "start",
            "--config",
            "/Users/appbuilder/.oore/managed-runner.json",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

        assert!(runner_service_uses_user_bootstrap_wrapper(&arguments));
    }

    #[test]
    fn launchd_runner_health_requires_a_running_process() {
        assert!(launchd_job_is_running(b"state = running\n\tpid = 4242\n"));
        assert!(!launchd_job_is_running(b"state = waiting\n"));
        assert!(!launchd_job_is_running(b"state = running\n"));
    }

    #[test]
    fn runner_handoff_requires_a_new_running_pid() {
        assert!(!launchd_runner_handoff_completed(
            b"state = running\n\tpid = 4242\n",
            Some(4242)
        ));
        assert!(launchd_runner_handoff_completed(
            b"state = running\n\tpid = 4343\n",
            Some(4242)
        ));
    }

    #[test]
    fn managed_daemon_backup_paths_follow_service_arguments_and_environment() {
        let document = serde_json::json!({
            "ProgramArguments": [
                "/opt/oore/bin/oored",
                "run",
                "--state-file",
                "state/custom.db"
            ],
            "EnvironmentVariables": {
                "HOME": "/Users/runner",
                "OORED_DATA_DIR": "data",
                "OORE_SETUP_STATE_FILE": "ignored.db",
                "OORE_ENCRYPTION_KEY_FILE": "secrets/custom.key"
            },
            "WorkingDirectory": "/opt/oore"
        });

        let paths = daemon_data_paths_from_plist(&document, Path::new("/fallback")).unwrap();

        assert_eq!(
            paths,
            (
                PathBuf::from("/opt/oore/state/custom.db"),
                PathBuf::from("/opt/oore/secrets/custom.key")
            )
        );
    }

    #[test]
    fn runner_start_requires_a_consecutive_stable_running_window() {
        let mut consecutive = 0;
        let observations = [true, true, true, false, true, true, true, true]
            .into_iter()
            .map(|running| launchd_job_is_stably_running(&mut consecutive, running))
            .collect::<Vec<_>>();

        assert_eq!(
            observations,
            vec![false, false, false, false, false, false, false, true]
        );
    }

    #[test]
    fn managed_runner_restart_requires_the_installed_oore_executable() {
        let temp = tempfile::tempdir().unwrap();
        let install = temp.path().join("install");
        let managed_oore = install.join("bin/oore");
        let managed_oore_alias = temp.path().join("managed-oore-alias");
        let unrelated_oore = temp.path().join("other/oore");
        fs::create_dir_all(managed_oore.parent().unwrap()).unwrap();
        fs::create_dir_all(unrelated_oore.parent().unwrap()).unwrap();
        fs::write(&managed_oore, "managed").unwrap();
        fs::write(&unrelated_oore, "unrelated").unwrap();
        std::os::unix::fs::symlink(&managed_oore, &managed_oore_alias).unwrap();

        assert_eq!(
            managed_runner_update_service_from_program_arguments(
                &install,
                &[
                    managed_oore_alias.display().to_string(),
                    "runner".to_string(),
                    "start".to_string(),
                ],
            ),
            Some(RUNNER_SERVICE_LABEL)
        );
        assert_eq!(
            managed_runner_update_service_from_program_arguments(
                &install,
                &[unrelated_oore.display().to_string(), "runner".to_string()],
            ),
            None
        );
        assert_eq!(
            managed_runner_update_service_from_program_arguments(
                &install,
                &[managed_oore.display().to_string(), "runner".to_string()],
            ),
            None
        );
        assert_eq!(
            runner_config_from_program_arguments(&[
                managed_oore.display().to_string(),
                "runner".to_string(),
                "start".to_string(),
                "--config".to_string(),
                "/Users/me/.oore/runner.json".to_string(),
            ]),
            Some(PathBuf::from("/Users/me/.oore/runner.json"))
        );
    }

    #[test]
    fn runner_config_is_replaced_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("runner.json");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let config = RunnerConfig {
            runner_id: "runner-1".to_string(),
            runner_token: "secret".to_string(),
            daemon_url: "http://127.0.0.1:8787".to_string(),
            name: "local".to_string(),
        };

        write_runner_config(&path, &config).unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            read_runner_config(&path).unwrap().unwrap().runner_id,
            "runner-1"
        );
    }

    async fn test_runner_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE runners (\
                id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, \
                status TEXT NOT NULL, capabilities TEXT NOT NULL, last_heartbeat_at INTEGER, \
                registered_by TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL\
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn test_runner_transition_pool(path: &Path) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE builds (\
               id TEXT PRIMARY KEY, status TEXT NOT NULL, runner_id TEXT, \
               updated_at INTEGER NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE instance_preferences (\
               id INTEGER PRIMARY KEY, direct_macos_runner_paused INTEGER NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn runner_service_transition_barrier_blocks_new_claims_until_release() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("oore.db");
        let pool = test_runner_transition_pool(&database).await;
        sqlx::query("INSERT INTO builds (id, status, updated_at) VALUES ('queued', 'queued', 0)")
            .execute(&pool)
            .await
            .unwrap();

        let barrier = acquire_runner_claim_barrier(&database).await.unwrap();
        let blocked = sqlx::query("UPDATE builds SET status = 'scheduled' WHERE id = 'queued'")
            .execute(&pool)
            .await
            .unwrap_err();
        assert!(blocked.to_string().contains("runner service transition"));

        barrier.release().await.unwrap();
        let changed = sqlx::query("UPDATE builds SET status = 'scheduled' WHERE id = 'queued'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(changed.rows_affected(), 1);
    }

    #[tokio::test]
    async fn runner_service_transition_barrier_renews_until_release() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("oore.db");
        let pool = test_runner_transition_pool(&database).await;
        let barrier = acquire_runner_claim_barrier(&database).await.unwrap();
        let short_expiry = now_epoch_secs() + 1;
        sqlx::query("UPDATE runner_service_transition_lease SET expires_at = ?1 WHERE id = 1")
            .bind(short_expiry)
            .execute(&pool)
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let expires_at: i64 = sqlx::query_scalar(
                    "SELECT expires_at FROM runner_service_transition_lease WHERE id = 1",
                )
                .fetch_one(&pool)
                .await
                .unwrap();
                if expires_at > short_expiry {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        barrier.release().await.unwrap();
        let lease_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runner_service_transition_lease WHERE id = 1)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!lease_exists);
    }

    #[tokio::test]
    async fn runner_service_transition_waits_for_the_legacy_assignment_to_drain() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("oore.db");
        let pool = test_runner_transition_pool(&database).await;
        sqlx::query(
            "INSERT INTO instance_preferences (id, direct_macos_runner_paused) VALUES (1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO builds (id, status, runner_id, updated_at) \
             VALUES ('active', 'running', 'legacy', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let updater = pool.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            sqlx::query("UPDATE builds SET status = 'succeeded' WHERE id = 'active'")
                .execute(&updater)
                .await
                .unwrap();
        });

        let barrier = acquire_runner_claim_barrier(&database).await.unwrap();
        barrier.wait_for_runner("legacy").await.unwrap();
        let paused: bool = sqlx::query_scalar(
            "SELECT direct_macos_runner_paused FROM instance_preferences WHERE id = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        barrier.release().await.unwrap();

        assert!(paused);
    }

    #[tokio::test]
    async fn expired_runner_service_transition_lease_fails_open_after_abrupt_exit() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("oore.db");
        let pool = test_runner_transition_pool(&database).await;
        sqlx::query("INSERT INTO builds (id, status, updated_at) VALUES ('queued', 'queued', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let barrier = acquire_runner_claim_barrier(&database).await.unwrap();
        sqlx::query("UPDATE runner_service_transition_lease SET expires_at = 0 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();

        let changed = sqlx::query("UPDATE builds SET status = 'scheduled' WHERE id = 'queued'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(changed.rows_affected(), 1);
        let error = barrier.ensure_healthy().await.unwrap_err();
        assert!(error.to_string().contains("ownership was lost"));
        let expires_at: i64 = sqlx::query_scalar(
            "SELECT expires_at FROM runner_service_transition_lease WHERE id = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(expires_at, 0);
    }

    #[tokio::test]
    async fn managed_local_service_migrates_only_a_database_matching_runner_config() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("oore.db");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&database)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE runners (\
                id TEXT PRIMARY KEY NOT NULL, token_hash TEXT NOT NULL, registered_by TEXT\
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO runners (id, token_hash, registered_by) VALUES \
             ('managed', ?1, NULL), ('manual', ?2, 'owner')",
        )
        .bind(runner_token_hash("managed-token"))
        .bind(runner_token_hash("manual-token"))
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let managed_path = temp.path().join("managed.json");
        let manual_path = temp.path().join("manual.json");
        write_runner_config(
            &managed_path,
            &RunnerConfig {
                runner_id: "managed".to_string(),
                runner_token: "managed-token".to_string(),
                daemon_url: "http://127.0.0.1:8787".to_string(),
                name: "managed".to_string(),
            },
        )
        .unwrap();
        write_runner_config(
            &manual_path,
            &RunnerConfig {
                runner_id: "manual".to_string(),
                runner_token: "manual-token".to_string(),
                daemon_url: "https://ci.example.com".to_string(),
                name: "manual".to_string(),
            },
        )
        .unwrap();

        let migrated_path = temp.path().join("managed-runner.json");
        let managed_before = fs::read(&managed_path).unwrap();
        migrate_managed_runner_service_config(&managed_path, &migrated_path, database.to_str())
            .await
            .unwrap();
        let migrated = read_runner_config(&migrated_path).unwrap().unwrap();
        assert_eq!(migrated.runner_id, "managed");
        assert_eq!(migrated.runner_token, "managed-token");
        assert_eq!(fs::read(&managed_path).unwrap(), managed_before);
        remove_migrated_managed_runner_config(&managed_path, &migrated_path).unwrap();
        assert!(!managed_path.exists());
        assert!(migrated_path.is_file());

        let manual_destination = temp.path().join("manual-destination.json");
        migrate_managed_runner_service_config(&manual_path, &manual_destination, database.to_str())
            .await
            .unwrap();
        assert_eq!(
            read_runner_config(&manual_destination)
                .unwrap()
                .unwrap()
                .runner_id,
            "manual"
        );

        let mismatched_path = temp.path().join("mismatched.json");
        write_runner_config(
            &mismatched_path,
            &RunnerConfig {
                runner_id: "manual".to_string(),
                runner_token: "wrong-token".to_string(),
                daemon_url: "https://ci.example.com".to_string(),
                name: "manual".to_string(),
            },
        )
        .unwrap();
        let error = migrate_managed_runner_service_config(
            &mismatched_path,
            &temp.path().join("mismatched-destination.json"),
            database.to_str(),
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("does not match"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn managed_local_runner_enrollment_is_stable_after_first_install() {
        let pool = test_runner_pool().await;
        let capabilities = serde_json::json!({"os": "macos", "arch": "arm64"});

        let (first, enrolled) = resolve_local_runner_config(
            &pool,
            None,
            "http://127.0.0.1:8787",
            "build-host",
            &capabilities,
            false,
        )
        .await
        .unwrap();
        assert!(enrolled);
        let (second, enrolled) = resolve_local_runner_config(
            &pool,
            Some(&first),
            "https://ci.example.com",
            "renamed-host",
            &serde_json::json!({"os": "macos", "arch": "arm64", "version": "next"}),
            false,
        )
        .await
        .unwrap();

        assert!(!enrolled);
        assert_eq!(second.runner_id, first.runner_id);
        assert_eq!(second.runner_token, first.runner_token);
        assert_eq!(second.daemon_url, "https://ci.example.com");
        assert_eq!(second.name, "renamed-host");
        let registered_by: Option<String> =
            sqlx::query_scalar("SELECT registered_by FROM runners WHERE id = ?1")
                .bind(&first.runner_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(registered_by.is_none());
        let (name, capabilities): (String, String) =
            sqlx::query_as("SELECT name, capabilities FROM runners WHERE id = ?1")
                .bind(&first.runner_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(name, "renamed-host");
        assert!(capabilities.contains("next"));
    }

    #[tokio::test]
    async fn enrollment_reuses_the_sole_legacy_internal_runner() {
        let pool = test_runner_pool().await;
        sqlx::query(
            "INSERT INTO runners \
             (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
             VALUES ('legacy', 'local-embedded-runner', 'old-hash', 'offline', '{}', NULL, 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (config, enrolled) = resolve_local_runner_config(
            &pool,
            None,
            "http://127.0.0.1:8787",
            "new-hostname",
            &serde_json::json!({}),
            false,
        )
        .await
        .unwrap();

        assert!(enrolled);
        assert_eq!(config.runner_id, "legacy");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runners")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn managed_enrollment_refuses_a_manually_registered_runner() {
        let pool = test_runner_pool().await;
        sqlx::query(
            "INSERT INTO runners \
             (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
             VALUES ('manual', 'manual', ?1, 'offline', '{}', 'owner', 1, 1)",
        )
        .bind(runner_token_hash("actual-token"))
        .execute(&pool)
        .await
        .unwrap();
        let manual = RunnerConfig {
            runner_id: "manual".to_string(),
            runner_token: "actual-token".to_string(),
            daemon_url: "http://127.0.0.1:8787".to_string(),
            name: "manual".to_string(),
        };

        let error = resolve_local_runner_config(
            &pool,
            Some(&manual),
            "http://127.0.0.1:8787",
            "manual",
            &serde_json::json!({}),
            false,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("manually registered runner"));
        let token_hash: String =
            sqlx::query_scalar("SELECT token_hash FROM runners WHERE id = 'manual'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(token_hash, runner_token_hash("actual-token"));

        let (adopted, enrolled) = resolve_local_runner_config(
            &pool,
            Some(&manual),
            "http://127.0.0.1:8787",
            "managed-host",
            &serde_json::json!({}),
            true,
        )
        .await
        .unwrap();
        assert!(!enrolled);
        assert_eq!(adopted.runner_id, "manual");
        assert_eq!(adopted.runner_token, "actual-token");
        let registered_by: Option<String> =
            sqlx::query_scalar("SELECT registered_by FROM runners WHERE id = 'manual'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(registered_by.as_deref(), Some("owner"));
    }

    #[tokio::test]
    async fn failed_config_publication_restores_the_previous_runner_token() {
        let pool = test_runner_pool().await;
        sqlx::query(
            "INSERT INTO runners \
             (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
             VALUES ('managed', 'managed', ?1, 'offline', '{}', NULL, 1, 1)",
        )
        .bind(runner_token_hash("new-token"))
        .execute(&pool)
        .await
        .unwrap();
        let previous = RunnerConfig {
            runner_id: "managed".to_string(),
            runner_token: "old-token".to_string(),
            daemon_url: "http://127.0.0.1:8787".to_string(),
            name: "managed".to_string(),
        };
        let current = RunnerConfig {
            runner_token: "new-token".to_string(),
            ..previous.clone()
        };

        publish_local_runner_config(
            &pool,
            Path::new("/dev/null/runner.json"),
            Some(&previous),
            &current,
            true,
        )
        .await
        .unwrap_err();

        let token_hash: String =
            sqlx::query_scalar("SELECT token_hash FROM runners WHERE id = 'managed'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(token_hash, runner_token_hash("old-token"));
    }

    #[tokio::test]
    async fn external_service_install_reuses_its_registered_config() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("external-runner.json");
        let external = RunnerConfig {
            runner_id: "external".to_string(),
            runner_token: "external-token".to_string(),
            daemon_url: "https://ci.example.com".to_string(),
            name: "external-host".to_string(),
        };
        write_runner_config(&path, &external).unwrap();
        let args = RunnerServiceArgs {
            config: None,
            managed_local: false,
            daemon_url: None,
            state_file: None,
            name: None,
        };

        let (_, resolved, enrolled) = ensure_runner_service_config(&args, path, false)
            .await
            .unwrap();

        assert!(!enrolled);
        assert_eq!(resolved.runner_id, "external");
        assert_eq!(resolved.daemon_url, "https://ci.example.com");
    }

    #[tokio::test]
    async fn external_service_install_requires_registration() {
        let temp = tempfile::tempdir().unwrap();
        let args = RunnerServiceArgs {
            config: None,
            managed_local: false,
            daemon_url: None,
            state_file: None,
            name: None,
        };

        let error = ensure_runner_service_config(&args, temp.path().join("missing.json"), false)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("runner is not registered"));
    }

    struct RecordingUpdateServiceControl {
        install_root: PathBuf,
        restarts: Vec<(String, String)>,
        markers_at_restart: Vec<Option<String>>,
        fail_service_once: Option<&'static str>,
    }

    impl UpdateServiceControl for RecordingUpdateServiceControl {
        fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
            let binary = fs::read_to_string(self.install_root.join("bin/oore"))?;
            self.restarts.push((label.to_string(), binary));
            self.markers_at_restart.push(
                read_runner_release_marker(&self.install_root)?
                    .map(String::from_utf8)
                    .transpose()
                    .context("runner release marker is not UTF-8")?,
            );
            if self.fail_service_once == Some(label) {
                self.fail_service_once = None;
                anyhow::bail!("injected {label} restart failure");
            }
            Ok(())
        }

        fn stop_managed_service(&mut self, _label: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn restart_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn stop_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn wait_for_endpoint(&mut self, _url: &str, _name: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MigrationFailureControl {
        install_root: PathBuf,
        database: PathBuf,
        expected_database: Vec<u8>,
        readiness_failed: bool,
        daemon_stopped: bool,
        rollback_saw_restored_database: bool,
    }

    struct SimulatedRunnerServiceRollback {
        plist: PathBuf,
        config: PathBuf,
        previous_plist: Vec<u8>,
        previous_config: Vec<u8>,
        was_loaded: bool,
        running: std::cell::Cell<bool>,
        ready: std::cell::Cell<bool>,
    }

    impl ManagedRunnerServiceRollback for SimulatedRunnerServiceRollback {
        fn stop_and_restore_files(&self) -> anyhow::Result<()> {
            self.running.set(false);
            self.ready.set(false);
            fs::write(&self.plist, &self.previous_plist)?;
            fs::write(&self.config, &self.previous_config)?;
            Ok(())
        }

        fn start(&self) -> anyhow::Result<()> {
            self.running.set(self.was_loaded);
            self.ready.set(self.was_loaded);
            Ok(())
        }
    }

    impl UpdateServiceControl for MigrationFailureControl {
        fn restart_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
            if label == DAEMON_SERVICE_LABEL
                && fs::read_to_string(self.install_root.join("bin/oore"))? == "old-binary"
            {
                self.rollback_saw_restored_database =
                    fs::read(&self.database)? == self.expected_database;
            }
            Ok(())
        }

        fn stop_managed_service(&mut self, label: &str) -> anyhow::Result<()> {
            if label == DAEMON_SERVICE_LABEL {
                self.daemon_stopped = true;
            }
            Ok(())
        }

        async fn restart_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn stop_unmanaged_daemon(&mut self, _listen: &str) -> anyhow::Result<()> {
            self.daemon_stopped = true;
            Ok(())
        }

        async fn wait_for_endpoint(&mut self, _url: &str, name: &str) -> anyhow::Result<()> {
            if name == "oored" && !self.readiness_failed {
                self.readiness_failed = true;
                anyhow::bail!("injected post-migration readiness failure");
            }
            Ok(())
        }
    }

    fn runner_update_plan(defer_daemon_restart: bool, web_is_managed: bool) -> UpdateServicePlan {
        UpdateServicePlan {
            defer_daemon_restart,
            daemon_is_managed: true,
            unmanaged_daemon_listen: None,
            daemon_should_be_running: true,
            daemon_ready_url: "http://127.0.0.1:8787/readyz".to_string(),
            installed_version: "2.0.0".to_string(),
            restored_version: "1.0.0".to_string(),
            runner_service: (!defer_daemon_restart).then_some(RUNNER_SERVICE_LABEL),
            runner_previous_pid: None,
            runner_ack: None,
            runner_installed_version: "2.0.0-release".to_string(),
            runner_restored_version: "1.0.0-release".to_string(),
            install_managed_runner: false,
            managed_runner_service_existed: false,
            managed_runner_config_existed: false,
            managed_runner_database: None,
            web_is_managed,
            web_was_running: false,
            web_ready_url: None,
        }
    }

    fn prepare_update_transaction() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let install = temp.path().join("install");
        let stage = temp.path().join("stage");
        let snapshot = temp.path().join("snapshot");
        fs::create_dir_all(install.join("bin")).unwrap();
        fs::create_dir_all(stage.join("bin")).unwrap();
        fs::write(install.join("bin/oore"), "old-binary").unwrap();
        fs::write(install.join("VERSION"), "1.0.0").unwrap();
        fs::write(stage.join("bin/oore"), "new-binary").unwrap();
        fs::write(stage.join("VERSION"), "2.0.0").unwrap();
        copy_release_snapshot(&install, &snapshot).unwrap();
        (temp, install, stage, snapshot)
    }

    #[tokio::test]
    async fn post_migration_readiness_failure_restores_database_before_old_daemon_restart() {
        let (temp, install, stage, snapshot) = prepare_update_transaction();
        let database = temp.path().join("custom/state.db");
        let key = temp.path().join("custom/key.bin");
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&database)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE state (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO state (value) VALUES ('before')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        fs::write(&key, [7_u8; 32]).unwrap();
        let backup = temp.path().join("backup.tar.gz");
        let backup_database = database.clone();
        let backup_key = key.clone();
        let backup_output = backup.clone();
        tokio::task::spawn_blocking(move || {
            create_backup_archive(&backup_database, &backup_key, &backup_output)
        })
        .await
        .unwrap()
        .unwrap();
        let unpacked = tempfile::tempdir().unwrap();
        let unpack_backup_path = backup.clone();
        let unpacked_path = unpacked.path().to_path_buf();
        tokio::task::spawn_blocking(move || unpack_backup(&unpack_backup_path, &unpacked_path))
            .await
            .unwrap()
            .unwrap();
        let expected_database = fs::read(unpacked.path().join(BACKUP_DATABASE_FILE)).unwrap();
        fs::write(&database, b"candidate migration").unwrap();

        let services = UpdateServicePlan {
            defer_daemon_restart: false,
            daemon_is_managed: true,
            unmanaged_daemon_listen: None,
            daemon_should_be_running: true,
            daemon_ready_url: "http://127.0.0.1:8787/readyz".to_string(),
            installed_version: "2.0.0".to_string(),
            restored_version: "1.0.0".to_string(),
            runner_service: None,
            runner_previous_pid: None,
            runner_ack: None,
            runner_installed_version: "2.0.0-release".to_string(),
            runner_restored_version: "1.0.0-release".to_string(),
            install_managed_runner: false,
            managed_runner_service_existed: false,
            managed_runner_config_existed: false,
            managed_runner_database: None,
            web_is_managed: false,
            web_was_running: false,
            web_ready_url: None,
        };
        let rollback = UpdateRollbackState {
            backup,
            database: database.clone(),
            key,
        };
        let mut control = MigrationFailureControl {
            install_root: install.clone(),
            database: database.clone(),
            expected_database: expected_database.clone(),
            readiness_failed: false,
            daemon_stopped: false,
            rollback_saw_restored_database: false,
        };

        let error = install_release_with_rollback(
            &stage,
            &install,
            &snapshot,
            ReleaseChannel::Stable,
            "oorebuild/oore",
            None,
            None,
            None,
            &services,
            Some(&rollback),
            &mut control,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("post-migration readiness"));
        assert!(control.daemon_stopped);
        assert!(control.rollback_saw_restored_database);
        assert_eq!(fs::read(database).unwrap(), expected_database);
    }

    #[tokio::test]
    async fn wrapper_repair_failure_restores_release_database_and_runner_service_snapshot() {
        let (temp, install, stage, release_snapshot) = prepare_update_transaction();
        let database = temp.path().join("state/oore.db");
        let key = temp.path().join("state/master.key");
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&database)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE state (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO state (value) VALUES ('before')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        fs::write(&key, [9_u8; 32]).unwrap();

        let backup = temp.path().join("backup.tar.gz");
        let backup_database = database.clone();
        let backup_key = key.clone();
        let backup_output = backup.clone();
        tokio::task::spawn_blocking(move || {
            create_backup_archive(&backup_database, &backup_key, &backup_output)
        })
        .await
        .unwrap()
        .unwrap();
        let unpacked = tempfile::tempdir().unwrap();
        let unpack_backup_path = backup.clone();
        let unpacked_path = unpacked.path().to_path_buf();
        tokio::task::spawn_blocking(move || unpack_backup(&unpack_backup_path, &unpacked_path))
            .await
            .unwrap()
            .unwrap();
        let previous_database = fs::read(unpacked.path().join(BACKUP_DATABASE_FILE)).unwrap();
        fs::write(&database, b"candidate database migration").unwrap();

        let plist = temp.path().join("build.oore.oore-runner.plist");
        let config = temp.path().join("managed-runner.json");
        let previous_plist = b"alpha.18 launchctl asuser sudo wrapper".to_vec();
        let previous_config = b"original runner identity and registration".to_vec();
        fs::write(&plist, b"candidate canonical UserName SessionCreate plist").unwrap();
        fs::write(&config, b"candidate config").unwrap();
        let runner_snapshot = SimulatedRunnerServiceRollback {
            plist: plist.clone(),
            config: config.clone(),
            previous_plist: previous_plist.clone(),
            previous_config: previous_config.clone(),
            was_loaded: true,
            running: std::cell::Cell::new(true),
            ready: std::cell::Cell::new(true),
        };
        let rollback = UpdateRollbackState {
            backup,
            database: database.clone(),
            key,
        };
        let services = UpdateServicePlan {
            runner_service: None,
            ..runner_update_plan(false, false)
        };
        let mut control = MigrationFailureControl {
            install_root: install.clone(),
            database: database.clone(),
            expected_database: previous_database.clone(),
            readiness_failed: false,
            daemon_stopped: false,
            rollback_saw_restored_database: false,
        };

        let error = install_release_with_rollback(
            &stage,
            &install,
            &release_snapshot,
            ReleaseChannel::Stable,
            "oorebuild/oore",
            None,
            Some(&runner_snapshot),
            None,
            &services,
            Some(&rollback),
            &mut control,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("post-migration readiness"));
        assert_eq!(
            fs::read_to_string(install.join("bin/oore")).unwrap(),
            "old-binary"
        );
        assert_eq!(fs::read(&database).unwrap(), previous_database);
        assert_eq!(fs::read(plist).unwrap(), previous_plist);
        assert_eq!(fs::read(config).unwrap(), previous_config);
        assert!(runner_snapshot.running.get());
        assert!(runner_snapshot.ready.get());
        assert!(control.rollback_saw_restored_database);
    }

    fn v2_marker_has_identity(marker: &str, identity: &str) -> bool {
        let Some(identity) = identity.strip_prefix("v1:") else {
            return false;
        };
        let mut values = marker.trim().splitn(3, ':');
        values.next() == Some("v2")
            && values.next().is_some_and(|generation| {
                generation.len() == 32 && generation.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            && values.next() == Some(identity)
    }

    #[test]
    fn successful_update_commits_before_restarting_the_runner() {
        let (_temp, install, stage, snapshot) = prepare_update_transaction();
        let previous_identity =
            oore_runner::runner_executable_identity_marker(&install.join("bin/oore")).unwrap();
        let mut control = RecordingUpdateServiceControl {
            install_root: install.clone(),
            restarts: Vec::new(),
            markers_at_restart: Vec::new(),
            fail_service_once: None,
        };

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(install_release_with_rollback(
                &stage,
                &install,
                &snapshot,
                ReleaseChannel::Stable,
                "oorebuild/oore",
                None,
                None,
                None,
                &runner_update_plan(false, false),
                None,
                &mut control,
            ))
            .unwrap();

        assert_eq!(
            control.restarts,
            vec![
                (DAEMON_SERVICE_LABEL.to_string(), "new-binary".to_string()),
                (RUNNER_SERVICE_LABEL.to_string(), "new-binary".to_string()),
            ]
        );
        let committed_identity =
            oore_runner::runner_executable_identity_marker(&install.join("bin/oore")).unwrap();
        let baseline = control.markers_at_restart[0].as_deref().unwrap();
        let committed = control.markers_at_restart[1].as_deref().unwrap();
        assert!(v2_marker_has_identity(baseline, &previous_identity));
        assert!(v2_marker_has_identity(committed, &committed_identity));
        assert_ne!(baseline, committed);
    }

    #[test]
    fn failed_deferred_web_update_does_not_commit_or_interrupt_the_runner() {
        let (_temp, install, stage, snapshot) = prepare_update_transaction();
        let previous_identity =
            oore_runner::runner_executable_identity_marker(&install.join("bin/oore")).unwrap();
        let mut control = RecordingUpdateServiceControl {
            install_root: install.clone(),
            restarts: Vec::new(),
            markers_at_restart: Vec::new(),
            fail_service_once: Some(WEB_SERVICE_LABEL),
        };

        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(install_release_with_rollback(
                &stage,
                &install,
                &snapshot,
                ReleaseChannel::Stable,
                "oorebuild/oore",
                None,
                None,
                None,
                &runner_update_plan(true, true),
                None,
                &mut control,
            ))
            .unwrap_err();

        assert!(error.to_string().contains("injected"));
        assert_eq!(
            control.restarts,
            vec![
                (WEB_SERVICE_LABEL.to_string(), "new-binary".to_string()),
                (WEB_SERVICE_LABEL.to_string(), "old-binary".to_string()),
            ]
        );
        let baseline = control.markers_at_restart[0].as_deref().unwrap();
        let rollback = control.markers_at_restart[1].as_deref().unwrap();
        let restored_identity =
            oore_runner::runner_executable_identity_marker(&install.join("bin/oore")).unwrap();
        assert!(v2_marker_has_identity(baseline, &previous_identity));
        assert!(v2_marker_has_identity(rollback, &restored_identity));
        assert_ne!(baseline, rollback);
        assert_eq!(
            String::from_utf8(read_runner_release_marker(&install).unwrap().unwrap()).unwrap(),
            rollback
        );
        assert_eq!(
            fs::read_to_string(install.join("bin/oore")).unwrap(),
            "old-binary"
        );
    }

    #[test]
    fn failed_runner_restart_normalizes_and_restores_a_corrupt_release_marker() {
        let (_temp, install, stage, snapshot) = prepare_update_transaction();
        fs::write(runner_release_marker_path(&install), b"corrupt marker").unwrap();
        let previous_identity =
            oore_runner::runner_executable_identity_marker(&install.join("bin/oore")).unwrap();
        let mut control = RecordingUpdateServiceControl {
            install_root: install.clone(),
            restarts: Vec::new(),
            markers_at_restart: Vec::new(),
            fail_service_once: Some(RUNNER_SERVICE_LABEL),
        };

        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(install_release_with_rollback(
                &stage,
                &install,
                &snapshot,
                ReleaseChannel::Stable,
                "oorebuild/oore",
                None,
                None,
                None,
                &runner_update_plan(false, false),
                None,
                &mut control,
            ))
            .unwrap_err();

        assert!(error.to_string().contains("injected"));
        assert_eq!(
            control.restarts,
            vec![
                (DAEMON_SERVICE_LABEL.to_string(), "new-binary".to_string()),
                (RUNNER_SERVICE_LABEL.to_string(), "new-binary".to_string()),
                (DAEMON_SERVICE_LABEL.to_string(), "old-binary".to_string()),
                (RUNNER_SERVICE_LABEL.to_string(), "old-binary".to_string()),
            ]
        );
        let baseline = control.markers_at_restart[0].as_deref().unwrap();
        let committed = control.markers_at_restart[1].as_deref().unwrap();
        let rollback = control.markers_at_restart[2].as_deref().unwrap();
        let rollback_runner = control.markers_at_restart[3].as_deref().unwrap();
        let restored_identity =
            oore_runner::runner_executable_identity_marker(&install.join("bin/oore")).unwrap();
        assert!(v2_marker_has_identity(baseline, &previous_identity));
        assert!(v2_marker_has_identity(rollback, &restored_identity));
        assert_ne!(baseline, committed);
        assert_ne!(baseline, rollback);
        assert_ne!(committed, rollback);
        assert_eq!(rollback, rollback_runner);
        assert_eq!(
            String::from_utf8(read_runner_release_marker(&install).unwrap().unwrap()).unwrap(),
            rollback
        );
        assert_eq!(
            fs::read_to_string(install.join("bin/oore")).unwrap(),
            "old-binary"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn android_doctor_runtime_helper() {
        let Some(expected_sdk) = std::env::var_os("OORE_DOCTOR_EXPECTED_SDK_ROOT") else {
            return;
        };
        let expected_java_home =
            PathBuf::from(std::env::var_os("OORE_DOCTOR_EXPECTED_JAVA_HOME").unwrap());
        let mut checks = Vec::new();
        add_android_checks(&mut checks);

        let java = checks.iter().find(|check| check.name == "java").unwrap();
        assert_eq!(java.status, "ok");
        assert!(
            java.detail
                .as_deref()
                .unwrap_or_default()
                .contains(&expected_java_home.join("bin/java").display().to_string()),
            "unexpected Java detail: {:?}",
            java.detail
        );
        let sdk = checks
            .iter()
            .find(|check| check.name == "android_sdk")
            .unwrap();
        assert_eq!(sdk.status, "ok");
        assert_eq!(sdk.detail.as_deref(), Path::new(&expected_sdk).to_str());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn android_doctor_supports_launchd_defaults_and_valid_explicit_overrides() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let default_sdk = home.join("Library/Android/sdk");
        let default_java_home =
            home.join("Applications/Android Studio.app/Contents/jbr/Contents/Home");
        let explicit_sdk = temp.path().join("explicit-sdk");
        let explicit_java_home = temp.path().join("explicit-jdk");

        for sdk in [&default_sdk, &explicit_sdk] {
            let adb = sdk.join("platform-tools/adb");
            fs::create_dir_all(adb.parent().unwrap()).unwrap();
            fs::write(adb, "synthetic adb").unwrap();
        }
        for java_home in [&default_java_home, &explicit_java_home] {
            for (tool, contents) in [
                ("java", "#!/bin/sh\necho 'synthetic Java 21' >&2\n"),
                ("jarsigner", "#!/bin/sh\nexit 0\n"),
            ] {
                let path = java_home.join("bin").join(tool);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, contents).unwrap();
                set_executable(&path).unwrap();
            }
        }

        for (configured, expected_sdk, expected_java_home) in [
            (false, &default_sdk, &default_java_home),
            (true, &explicit_sdk, &explicit_java_home),
        ] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "tests::android_doctor_runtime_helper"])
                .env_remove("ANDROID_HOME")
                .env_remove("ANDROID_SDK_ROOT")
                .env_remove("JAVA_HOME")
                .env("HOME", &home)
                .env("PATH", "/usr/bin:/bin")
                .env("OORE_DOCTOR_EXPECTED_SDK_ROOT", expected_sdk)
                .env("OORE_DOCTOR_EXPECTED_JAVA_HOME", expected_java_home);
            if configured {
                command
                    .env("ANDROID_HOME", &explicit_sdk)
                    .env("JAVA_HOME", &explicit_java_home);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "isolated doctor helper failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn command_timeout_stops_stalled_service_commands() {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "sleep 1"]);

        let error = run_command_with_timeout(
            &mut command,
            "test service command",
            Duration::from_millis(20),
        )
        .unwrap_err();

        assert!(error.to_string().contains("timed out"));
    }

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
    fn legacy_release_repository_is_normalized() {
        assert_eq!(
            normalize_github_repo(LEGACY_GITHUB_REPO),
            DEFAULT_GITHUB_REPO
        );
        assert_eq!(
            normalize_github_repo(DEFAULT_GITHUB_REPO),
            DEFAULT_GITHUB_REPO
        );
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
        assert_eq!(
            fs::read_to_string(install.join("WEB_VERSION")).unwrap(),
            "2.0.0"
        );
        assert_eq!(
            fs::read_to_string(install.join("WEB_CHANNEL")).unwrap(),
            "stable"
        );
        assert_eq!(
            fs::read_to_string(install.join("WEB_GITHUB_REPO")).unwrap(),
            "oorebuild/oore"
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
        assert!(!install.join("WEB_VERSION").exists());
        assert!(!install.join("WEB_CHANNEL").exists());
        assert!(!install.join("WEB_GITHUB_REPO").exists());
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
            "http://127.0.0.1:9876"
        );
    }

    #[test]
    fn launchd_stop_waits_for_delayed_unload() {
        let mut states = [true, true, false].into_iter();
        assert!(wait_for_launchd_service_unloaded(|| Ok(states.next().unwrap_or(false))).unwrap());
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
