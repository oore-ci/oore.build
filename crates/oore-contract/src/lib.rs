use std::fmt;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
#[cfg(unix)]
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

pub mod data_paths;
pub mod install_root;
pub mod pipeline_config;
pub mod runtime_update_preflight;

pub use pipeline_config::{
    PipelinePlan, PlanRequest, PlanStep, RepositoryConfig, StepPhase, parse_repository_config,
    plan_execution_config,
};

// ── Setup state machine ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SetupState {
    Uninitialized,
    BootstrapPending,
    IdpConfigured,
    OwnerCreated,
    Ready,
}

impl fmt::Display for SetupState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Uninitialized => "uninitialized",
            Self::BootstrapPending => "bootstrap_pending",
            Self::IdpConfigured => "idp_configured",
            Self::OwnerCreated => "owner_created",
            Self::Ready => "ready",
        };
        f.write_str(s)
    }
}

impl FromStr for SetupState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "uninitialized" => Ok(Self::Uninitialized),
            "bootstrap_pending" => Ok(Self::BootstrapPending),
            "idp_configured" => Ok(Self::IdpConfigured),
            "owner_created" => Ok(Self::OwnerCreated),
            "ready" => Ok(Self::Ready),
            other => Err(format!("unknown setup state: {other}")),
        }
    }
}

impl SetupState {
    pub fn next(self) -> Option<SetupState> {
        match self {
            Self::BootstrapPending => Some(Self::IdpConfigured),
            Self::IdpConfigured => Some(Self::OwnerCreated),
            Self::OwnerCreated => Some(Self::Ready),
            _ => None,
        }
    }
}

// ── Public setup status (non-sensitive) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct SetupStatus {
    pub instance_id: String,
    pub state: SetupState,
    pub runtime_mode: RuntimeMode,
    pub remote_auth_mode: RemoteAuthMode,
    pub setup_mode: bool,
    pub is_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct FrontendPairRequest {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct FrontendPairResponse {
    pub backend_proof: String,
    pub user_email_header: String,
}

impl SetupStatus {
    pub fn from_state(
        instance_id: impl Into<String>,
        state: SetupState,
        runtime_mode: RuntimeMode,
        remote_auth_mode: RemoteAuthMode,
    ) -> Self {
        let is_configured = state == SetupState::Ready;
        let setup_mode = !is_configured;

        Self {
            instance_id: instance_id.into(),
            state,
            runtime_mode,
            remote_auth_mode,
            setup_mode,
            is_configured,
        }
    }
}

// ── API request/response types ──────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BootstrapTokenVerifyRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BootstrapTokenVerifyResponse {
    pub session_token: String,
    pub expires_at: i64,
}

/// Closed request protocol for host-authorized local operator actions.
/// The daemon accepts this protocol only through its private Unix socket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields, rename_all = "snake_case")]
pub enum OperatorRequest {
    MintBootstrapToken {
        ttl_secs: u64,
    },
    CreateFrontendInvite {
        ttl_secs: u64,
    },
    EnsureManagedRunner {
        operation_id: String,
        name: String,
        capabilities: ManagedRunnerCapabilities,
        runner_id: Option<String>,
        runner_token: Option<String>,
        proposed_runner_id: String,
        proposed_runner_token: String,
        adopt_installed_registration: bool,
    },
    RunnerRegistrationMatches {
        runner_id: String,
        runner_token: String,
        allow_manual_for_adoption: bool,
    },
    RunnerStatus {
        runner_id: String,
    },
    RestoreManagedRunner {
        rollback: ManagedRunnerRollback,
    },
    RunnerBarrierAcquire,
    RunnerBarrierRenew {
        lease_token: String,
    },
    RunnerBarrierWait {
        lease_token: String,
        runner_id: Option<String>,
    },
    RunnerBarrierRelease {
        lease_token: String,
    },
}

/// Instance-bound envelope for every local operator request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorRequestEnvelope {
    pub expected_instance_id: String,
    pub request: OperatorRequest,
}

/// Closed capability document for daemon-managed local runners.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedRunnerCapabilities {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub xcode_version: String,
    pub version: String,
    pub protocol_version: u32,
}

/// Compare-and-restore payload for one daemon-managed runner mutation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedRunnerRollback {
    pub runner_id: String,
    pub issued_token_hash: String,
    pub previous: Option<ManagedRunnerRecord>,
}

/// Closed runner row snapshot used only by the local rollback protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedRunnerRecord {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub status: String,
    pub capabilities: String,
    pub last_heartbeat_at: Option<i64>,
    pub registered_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "response", deny_unknown_fields, rename_all = "snake_case")]
// Keep the local wire protocol direct. Boxing this one-shot response would add
// conversion branches without reducing serialized bytes or retained state.
#[allow(clippy::large_enum_variant)]
pub enum OperatorResponse {
    BootstrapToken {
        token: String,
        expires_at: i64,
        state: SetupState,
        instance_id: String,
    },
    FrontendInvite {
        code: String,
        expires_at: i64,
        instance_id: String,
    },
    ManagedRunner {
        runner_id: String,
        runner_name: String,
        runner_token: String,
        enrolled: bool,
        rollback: Option<ManagedRunnerRollback>,
        instance_id: String,
    },
    ManagedRunnerRestored {
        runner_id: String,
        instance_id: String,
    },
    RegistrationMatch {
        matches: bool,
        instance_id: String,
    },
    RunnerStatus {
        status: String,
        last_heartbeat_at: Option<i64>,
        capabilities: String,
        instance_id: String,
    },
    Barrier {
        lease_token: String,
        instance_id: String,
    },
    BarrierWait {
        active: bool,
        instance_id: String,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcConfigureRequest {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    #[serde(default)]
    pub clear_client_secret: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcConfigureResponse {
    pub state: SetupState,
    pub discovered_issuer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupOidcStartRequest {
    pub redirect_uri: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupOidcStartResponse {
    pub authorization_url: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupOidcVerifyRequest {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupOidcVerifyResponse {
    pub state: SetupState,
    pub owner_email: String,
    pub oidc_subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupLocalOwnerCreateRequest {
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupLocalOwnerCreateResponse {
    pub state: SetupState,
    pub owner_email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupPreferencesRequest {
    pub runtime_mode: RuntimeMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_auth_mode: Option<RemoteAuthMode>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupPreferencesResponse {
    pub runtime_mode: RuntimeMode,
    pub remote_auth_mode: RemoteAuthMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupTrustedProxyConfigureRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_email_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_owner_email: Option<String>,
    #[serde(default)]
    pub trusted_proxy_cidrs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupTrustedProxyConfigureResponse {
    pub state: SetupState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_owner_email: Option<String>,
    pub has_shared_secret: bool,
    pub configured_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupTrustedProxyClaimOwnerResponse {
    pub state: SetupState,
    pub owner_email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupCompleteResponse {
    pub state: SetupState,
    pub instance_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupSummaryResponse {
    pub instance_id: String,
    pub state: SetupState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    pub has_client_secret: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_email: Option<String>,
}

// ── Structured API error ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct HealthResponse {
    pub version: String,
    #[schema(required = true)]
    pub channel: Option<String>,
    #[schema(required = true)]
    pub github_repo: Option<String>,
    pub package_version: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct StreamTokenResponse {
    pub token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct WebhookResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate: Option<bool>,
}

#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct BinaryPayload(pub Vec<u8>);

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

// ── State file model (shared between CLI and daemon) ────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SetupStateFile {
    pub schema_version: u32,
    pub instance_id: String,
    pub setup_state: SetupState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_token: Option<BootstrapTokenRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_session: Option<SetupSessionRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc_config: Option<OidcConfigRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc_secret: Option<OidcSecretRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerRecord>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl SetupStateFile {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BootstrapTokenRecord {
    pub hash: String,
    pub expires_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SetupSessionRecord {
    pub hash: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OidcConfigRecord {
    pub issuer_url: String,
    pub client_id: String,
    pub has_client_secret: bool,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
    pub configured_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OidcSecretRecord {
    pub encrypted_client_secret: String,
    pub stored_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OwnerRecord {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc_subject: Option<String>,
    pub created_at: i64,
}

// ── Auth response types ─────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcStartResponse {
    pub authorization_url: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcCallbackResponse {
    pub session_token: String,
    pub expires_at: i64,
    pub user: AuthenticatedUser,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LocalLoginRequest {
    /// Optional account selector for Local Only login. In Ready Remote recovery,
    /// it may only confirm the account already bound to the capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Short-lived, single-use capability required for Ready Remote recovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_capability: Option<String>,
}

pub const LOCAL_RECOVERY_MIN_TTL_SECS: u64 = 1;
pub const LOCAL_RECOVERY_MAX_TTL_SECS: u64 = 5 * 60;
pub const LOCAL_RECOVERY_SOCKET_DIR: &str = "run";
pub const LOCAL_RECOVERY_SOCKET_FILE: &str = "oored-management.sock";

#[cfg(all(unix, any(target_os = "android", target_os = "linux")))]
const LOCAL_RECOVERY_SOCKET_PATH_MAX_BYTES: usize = 107;
#[cfg(all(unix, not(any(target_os = "android", target_os = "linux"))))]
const LOCAL_RECOVERY_SOCKET_PATH_MAX_BYTES: usize = 103;
#[cfg(unix)]
const LOCAL_RECOVERY_SOCKET_HASH_DOMAIN: &[u8] = b"oore-local-recovery-socket-v1\0";

/// Resolves the shared local recovery socket path for the daemon and CLI.
///
/// The existing database-adjacent path stays unchanged when it fits a Unix
/// socket address. Longer paths use a per-user directory under `/tmp`. Callers
/// must enforce that the socket directory belongs to `effective_uid` with mode
/// `0700` before they create or use the socket.
///
/// # Errors
///
/// Returns an error when an existing long database path cannot be resolved to
/// its canonical identity, or when a missing path cannot be made absolute.
#[cfg(unix)]
pub fn local_recovery_socket_path(
    database_path: &Path,
    effective_uid: u32,
) -> std::io::Result<PathBuf> {
    let parent = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let preferred = parent
        .join(LOCAL_RECOVERY_SOCKET_DIR)
        .join(LOCAL_RECOVERY_SOCKET_FILE);
    if preferred.as_os_str().as_bytes().len() <= LOCAL_RECOVERY_SOCKET_PATH_MAX_BYTES {
        return Ok(preferred);
    }

    let database_identity = match std::fs::canonicalize(database_path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::path::absolute(database_path)?
        }
        Err(error) => return Err(error),
    };
    let mut hasher = Sha256::new();
    hasher.update(LOCAL_RECOVERY_SOCKET_HASH_DOMAIN);
    hasher.update(database_identity.as_os_str().as_bytes());
    let database_hash = hex::encode(hasher.finalize());
    Ok(Path::new("/tmp")
        .join(format!("oore-{effective_uid}"))
        .join(format!("oore-{database_hash}.sock")))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalRecoveryMintRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub ttl_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LocalRecoveryMintResponse {
    Success {
        capability: String,
        expires_at: i64,
        user_email: String,
    },
    Error {
        error: ApiError,
    },
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LocalLoginResponse {
    pub session_token: String,
    pub expires_at: i64,
    pub user: AuthenticatedUser,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthenticatedUser {
    pub email: String,
    pub oidc_subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<UserRole>)]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LogoutResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeUpdatePhase {
    Idle,
    Updating,
    Restarting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RuntimeUpdateStatus {
    pub phase: RuntimeUpdatePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub managed_service: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeUpdateCheckStatus {
    Pass,
    /// Does not block the one-click update, but limits recovery.
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RuntimeUpdateCheck {
    /// Stable identifier, for example `updater_definition`.
    pub id: String,
    pub label: String,
    pub status: RuntimeUpdateCheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What to run or change on the backend Mac to make the check pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RuntimeUpdatePreflight {
    /// True when no check failed, so a one-click backend update can start.
    pub ready: bool,
    /// Unix timestamp, in seconds, of when the checks ran.
    pub checked_at: i64,
    pub checks: Vec<RuntimeUpdateCheck>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeferredRuntimeUpdateRequest {
    pub parent_pid: u32,
    pub database: PathBuf,
    pub key: PathBuf,
    pub daemon_url: String,
    pub status: PathBuf,
}

// ── User management types ───────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Owner,
    Admin,
    Developer,
    QaViewer,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Developer => "developer",
            Self::QaViewer => "qa_viewer",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for UserRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "developer" => Ok(Self::Developer),
            "qa_viewer" => Ok(Self::QaViewer),
            other => Err(format!("unknown user role: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Active,
    Disabled,
    Invited,
}

impl std::fmt::Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Invited => "invited",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[schema(value_type = UserRole)]
    pub role: String,
    #[schema(value_type = UserStatus)]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InviteUserRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InviteUserResponse {
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateUserRoleRequest {
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateUserRoleResponse {
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReEnableUserResponse {
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListUsersResponse {
    pub users: Vec<User>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserProfileResponse {
    pub user: User,
}

// ── SCM Integration types ──────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScmProvider {
    Github,
    Gitlab,
    LocalGit,
}

impl fmt::Display for ScmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github => f.write_str("github"),
            Self::Gitlab => f.write_str("gitlab"),
            Self::LocalGit => f.write_str("local_git"),
        }
    }
}

impl FromStr for ScmProvider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            "local_git" => Ok(Self::LocalGit),
            other => Err(format!("unknown SCM provider: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationAuthMode {
    GithubApp,
    OauthApp,
    PersonalToken,
    LocalPath,
}

impl fmt::Display for IntegrationAuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GithubApp => f.write_str("github_app"),
            Self::OauthApp => f.write_str("oauth_app"),
            Self::PersonalToken => f.write_str("personal_token"),
            Self::LocalPath => f.write_str("local_path"),
        }
    }
}

impl FromStr for IntegrationAuthMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github_app" => Ok(Self::GithubApp),
            "oauth_app" => Ok(Self::OauthApp),
            "personal_token" => Ok(Self::PersonalToken),
            "local_path" => Ok(Self::LocalPath),
            other => Err(format!("unknown integration auth mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStatus {
    Active,
    Inactive,
    Error,
}

impl fmt::Display for IntegrationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Inactive => f.write_str("inactive"),
            Self::Error => f.write_str("error"),
        }
    }
}

impl FromStr for IntegrationStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "error" => Ok(Self::Error),
            other => Err(format!("unknown integration status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Integration {
    pub id: String,
    #[schema(value_type = ScmProvider)]
    pub provider: String,
    pub host_url: String,
    pub auth_mode: String,
    pub status: String,
    /// Why the source has its current status: `expired`, `rejected` or
    /// `refresh_failed` when credentials stopped working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    /// When Oore last checked the source's credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_checked_at: Option<i64>,
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_slug: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationInstallation {
    pub id: String,
    pub integration_id: String,
    pub external_id: String,
    pub account_name: String,
    pub account_type: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationRepository {
    pub id: String,
    pub installation_id: String,
    pub external_id: String,
    pub full_name: String,
    pub default_branch: Option<String>,
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── SCM Integration API types ──────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitHubAppStartRequest {
    pub webhook_url: String,
    /// Frontend URL to redirect to after GitHub App creation completes.
    pub redirect_url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitHubAppStartResponse {
    /// URL to navigate the browser to — serves an auto-submitting form that POSTs the manifest to GitHub.
    pub create_url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitHubAppCompleteRequest {
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitHubAppCompleteResponse {
    pub integration: Integration,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncInstallationsRequest {
    pub installation_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SyncInstallationsResponse {
    pub installations: Vec<IntegrationInstallation>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitLabStartRequest {
    pub host_url: String,
    pub auth_mode: IntegrationAuthMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitLabRepositoryWebhookSecretResponse {
    pub repository_id: String,
    pub webhook_secret: String,
    pub rotated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitLabCompleteResponse {
    pub integration: Integration,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitLabAuthorizeRequest {
    pub integration_id: String,
    pub redirect_url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitLabAuthorizeResponse {
    pub authorize_url: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitLabCredentialStatus {
    Valid,
    Expiring,
    Expired,
    Rejected,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitLabCredentialStatusResponse {
    #[schema(value_type = GitLabCredentialStatus)]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub checked_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReplaceGitLabTokenRequest {
    pub access_token: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateLocalGitIntegrationRequest {
    pub repository_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateLocalGitIntegrationResponse {
    pub integration: Integration,
    pub repository: IntegrationRepository,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LocalGitDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_git_repository: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LocalGitPathSuggestion {
    pub label: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BrowseLocalGitDirectoriesResponse {
    pub current_path: String,
    pub current_is_git_repository: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    pub directories: Vec<LocalGitDirectoryEntry>,
    pub suggestions: Vec<LocalGitPathSuggestion>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListIntegrationsResponse {
    pub integrations: Vec<Integration>,
    pub total: i64,
    pub active_total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IntegrationDetailResponse {
    pub integration: Integration,
    pub installation_count: i64,
    pub repository_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_webhook_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorIncidentStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorIncidentSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OperatorIncident {
    pub id: String,
    #[schema(value_type = OperatorIncidentStatus)]
    pub status: String,
    #[schema(value_type = OperatorIncidentSeverity)]
    pub severity: String,
    pub reason: String,
    pub first_occurrence_at: i64,
    pub latest_occurrence_at: i64,
    pub occurrence_count: i64,
    pub resource_kind: String,
    pub resource_id: String,
    pub resource_name: String,
    pub repair_action: String,
    pub repair_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListOperatorIncidentsResponse {
    pub incidents: Vec<OperatorIncident>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListInstallationsResponse {
    pub installations: Vec<IntegrationInstallation>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListRepositoriesResponse {
    pub repositories: Vec<IntegrationRepository>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SourceRepository {
    #[serde(flatten)]
    pub repository: IntegrationRepository,
    pub integration_id: String,
    pub provider: ScmProvider,
    pub host_url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListSourceRepositoriesResponse {
    pub repositories: Vec<SourceRepository>,
    pub total: i64,
}

// ── Build domain types ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildStatus {
    Queued,
    Scheduled,
    Assigned,
    Running,
    Succeeded,
    Failed,
    Canceled,
    TimedOut,
    Expired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerPolicyBlockReason {
    InstancePaused,
    RepositoryUnavailable,
}

impl FromStr for RunnerPolicyBlockReason {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "instance_paused" => Ok(Self::InstancePaused),
            "repository_unavailable" => Ok(Self::RepositoryUnavailable),
            other => Err(format!("unknown runner policy block reason: {other}")),
        }
    }
}

impl BuildStatus {
    /// Returns true if this status is a terminal state (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Canceled | Self::TimedOut | Self::Expired
        )
    }

    /// Returns the set of valid statuses this status can transition to.
    pub fn valid_transitions(self) -> &'static [BuildStatus] {
        match self {
            Self::Queued => &[Self::Scheduled, Self::Canceled, Self::Expired],
            Self::Scheduled => &[Self::Assigned, Self::Queued, Self::Canceled, Self::Expired],
            Self::Assigned => &[Self::Running, Self::Queued, Self::Canceled, Self::TimedOut],
            Self::Running => &[
                Self::Succeeded,
                Self::Failed,
                Self::Canceled,
                Self::TimedOut,
            ],
            // Terminal states can transition to Expired via retention cleanup
            Self::Succeeded | Self::Failed | Self::Canceled | Self::TimedOut => &[Self::Expired],
            Self::Expired => &[],
        }
    }

    /// Check if transitioning from this status to `target` is valid.
    pub fn can_transition_to(self, target: BuildStatus) -> bool {
        self.valid_transitions().contains(&target)
    }
}

impl fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Queued => "queued",
            Self::Scheduled => "scheduled",
            Self::Assigned => "assigned",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::TimedOut => "timed_out",
            Self::Expired => "expired",
        };
        f.write_str(s)
    }
}

impl FromStr for BuildStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "scheduled" => Ok(Self::Scheduled),
            "assigned" => Ok(Self::Assigned),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "canceled" => Ok(Self::Canceled),
            "timed_out" => Ok(Self::TimedOut),
            "expired" => Ok(Self::Expired),
            other => Err(format!("unknown build status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    Manual,
    Api,
    Webhook,
    Schedule,
}

impl fmt::Display for TriggerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manual => f.write_str("manual"),
            Self::Api => f.write_str("api"),
            Self::Webhook => f.write_str("webhook"),
            Self::Schedule => f.write_str("schedule"),
        }
    }
}

impl FromStr for TriggerType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(Self::Manual),
            "api" => Ok(Self::Api),
            "webhook" => Ok(Self::Webhook),
            "schedule" => Ok(Self::Schedule),
            other => Err(format!("unknown trigger type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ConcurrencyPolicy {
    #[serde(default)]
    #[schema(required = true)]
    pub cancel_previous: bool,
    #[serde(default)]
    pub max_concurrent: Option<u32>,
}

// ── Trigger config ──────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct TriggerConfig {
    #[serde(default)]
    #[schema(required = true)]
    pub events: Vec<String>,
    /// Branch globs for push and pull request events. When only `tags` is
    /// set, branch events do not trigger.
    #[serde(default)]
    #[schema(required = true)]
    pub branches: Vec<String>,
    /// Tag globs for tag push events. When only `branches` is set, tag
    /// events do not trigger.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Push and pull request events trigger only when a changed file matches
    /// one of these globs (`**` spans directories).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Push and pull request events are skipped when every changed file
    /// matches one of these globs.
    #[serde(default)]
    pub paths_ignore: Vec<String>,
    /// Cron schedules, evaluated in UTC.
    #[serde(default)]
    pub schedules: Vec<ScheduleTrigger>,
}

/// A cron schedule that starts a build on a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ScheduleTrigger {
    /// Five-field cron expression (`minute hour day month weekday`) in UTC,
    /// or `@hourly`, `@daily`, `@weekly`, `@monthly`.
    pub cron: String,
    /// Branch to build. Defaults to the project's default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// Which builds of a pipeline wait for a maintainer before a runner may
/// claim them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalPolicy {
    #[serde(default)]
    #[schema(required = true)]
    pub required: bool,
    /// Trigger events that need approval: `push`, `pull_request`,
    /// `tag_push`, `manual` or `schedule`. Empty means every build.
    #[serde(default)]
    pub events: Vec<String>,
    /// Hours a build waits for a decision before it is canceled. Defaults
    /// to 168 (7 days).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_after_hours: Option<u32>,
}

impl ApprovalPolicy {
    pub const EVENTS: &'static [&'static str] =
        &["push", "pull_request", "tag_push", "manual", "schedule"];
    pub const DEFAULT_EXPIRES_AFTER_HOURS: u32 = 7 * 24;
    pub const MAX_EXPIRES_AFTER_HOURS: u32 = 365 * 24;

    /// Whether a build started by `event` (a canonical trigger event, or
    /// `manual` / `schedule`) must wait for approval.
    pub fn applies_to(&self, event: &str) -> bool {
        self.required && (self.events.is_empty() || self.events.iter().any(|e| e == event))
    }
}

/// Build approval state. Pending builds stay `queued` but no runner claims them.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildApprovalState {
    Pending,
    Approved,
    Rejected,
}

impl fmt::Display for BuildApprovalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        })
    }
}

impl FromStr for BuildApprovalState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            other => Err(format!("unknown build approval state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildApproval {
    pub state: BuildApprovalState,
    pub requested_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct BuildApprovalRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl TriggerConfig {
    /// Map provider-specific event names to canonical names.
    ///
    /// Canonical names: `"push"`, `"pull_request"`, `"tag_push"`.
    pub fn normalize_event(event_type: &str) -> String {
        match event_type {
            "Push Hook" => "push".to_string(),
            "Merge Request Hook" => "pull_request".to_string(),
            "Tag Push Hook" => "tag_push".to_string(),
            other => other.to_lowercase(),
        }
    }

    /// Simple glob match supporting `*` (any chars) and `?` (single char).
    pub fn glob_match(pattern: &str, value: &str) -> bool {
        Self::glob_match_inner(pattern.as_bytes(), value.as_bytes())
    }

    fn glob_match_inner(pattern: &[u8], value: &[u8]) -> bool {
        let (mut pi, mut vi) = (0, 0);
        let (mut star_pi, mut star_vi) = (usize::MAX, 0);

        while vi < value.len() {
            if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == value[vi]) {
                pi += 1;
                vi += 1;
            } else if pi < pattern.len() && pattern[pi] == b'*' {
                star_pi = pi;
                star_vi = vi;
                pi += 1;
            } else if star_pi != usize::MAX {
                pi = star_pi + 1;
                star_vi += 1;
                vi = star_vi;
            } else {
                return false;
            }
        }

        while pi < pattern.len() && pattern[pi] == b'*' {
            pi += 1;
        }

        pi == pattern.len()
    }

    /// Determine whether a webhook event should trigger this pipeline.
    ///
    /// Empty `events` list means all events match. A tag event (`tag` is
    /// set) is checked against `tags`, any other event against `branches`.
    /// When neither list is set every ref matches; when only one is set,
    /// events for the other kind of ref do not trigger.
    pub fn should_trigger(
        &self,
        event_type: &str,
        branch: Option<&str>,
        tag: Option<&str>,
    ) -> bool {
        // Check event filter
        if !self.events.is_empty() {
            let canonical = Self::normalize_event(event_type);
            if !self
                .events
                .iter()
                .any(|e| Self::normalize_event(e) == canonical)
            {
                return false;
            }
        }

        if self.branches.is_empty() && self.tags.is_empty() {
            return true;
        }
        let (patterns, reference) = match tag {
            Some(tag) => (&self.tags, Some(tag)),
            None => (&self.branches, branch),
        };
        reference.is_some_and(|reference| {
            patterns
                .iter()
                .any(|pattern| Self::glob_match(pattern, reference))
        })
    }

    /// Whether the changed files pass `paths` and `paths_ignore`.
    ///
    /// `None` means the change set is unknown (for example a truncated
    /// comparison), which never suppresses a build.
    pub fn paths_match(&self, changed_files: Option<&[String]>) -> bool {
        if self.paths.is_empty() && self.paths_ignore.is_empty() {
            return true;
        }
        let Some(files) = changed_files else {
            return true;
        };
        let matches_any = |patterns: &[String], file: &str| {
            patterns
                .iter()
                .any(|pattern| changed_path_matches(pattern, file))
        };
        files.iter().any(|file| {
            (self.paths.is_empty() || matches_any(&self.paths, file))
                && !matches_any(&self.paths_ignore, file)
        })
    }

    /// Whether path filters are configured, so callers can skip fetching
    /// the change set.
    pub fn has_path_filters(&self) -> bool {
        !self.paths.is_empty() || !self.paths_ignore.is_empty()
    }
}

/// Match a repository-relative path against a trigger path glob with the
/// artifact pattern rules: `*` and `?` stay within one segment, `**` spans
/// segments, and a pattern without `/` matches the file name anywhere. A
/// pattern ending in `/` matches everything below that directory.
pub fn changed_path_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim();
    match pattern.strip_suffix('/') {
        Some(directory) => artifact_pattern_matches(&format!("{directory}/**"), path),
        None => artifact_pattern_matches(pattern, path),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Build {
    pub id: String,
    pub project_id: String,
    pub pipeline_id: String,
    pub build_number: i64,
    #[schema(value_type = BuildStatus)]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_policy_block_reason: Option<RunnerPolicyBlockReason>,
    #[schema(value_type = TriggerType)]
    pub trigger_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_build_id: Option<String>,
    /// Steps this build retries against `source_build_id`'s stored artifacts.
    /// Absent for a full build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_steps: Option<Vec<String>>,
    /// Present when the pipeline's approval policy gated this build.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<BuildApproval>,
    #[schema(value_type = Object)]
    pub config_snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<BuildContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_results: Option<Vec<StepResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub queued_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<ScmProvider>)]
    pub repository_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_host_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildEvent {
    pub id: String,
    pub build_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_status: Option<String>,
    pub to_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub created_at: i64,
}

// ── Build API types ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateBuildRequest {
    pub pipeline_id: String,
    /// Optional one-run platform selection. Omitted builds every platform configured by the pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<BuildPlatform>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateBuildResponse {
    pub build: Build,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BuildChangelogPreviewResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    pub target_commit: String,
    pub markdown: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BuildDetailResponse {
    pub build: Build,
    pub events: Vec<BuildEvent>,
    /// Steps of this finished build that `POST /v1/builds/{build_id}/retry-step`
    /// can run again without rebuilding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retryable_steps: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListBuildsResponse {
    pub builds: Vec<Build>,
    pub total: i64,
}

/// Distinct `trigger_actor` values on builds the caller can see, most
/// recently active first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListBuildActorsResponse {
    pub actors: Vec<String>,
}

/// Outcome counts for builds that finished in a window.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct BuildOutcomeTotals {
    pub finished: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub timed_out: i64,
    pub canceled: i64,
    pub expired: i64,
    /// `succeeded / (succeeded + failed + timed_out)`. Canceled and expired
    /// builds never ran to a verdict, so they are left out. Absent when no
    /// build reached a verdict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_rate: Option<f64>,
}

/// Median and 90th percentile of a duration, in seconds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct DurationPercentiles {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p90_seconds: Option<i64>,
    /// Builds the percentiles cover.
    pub samples: i64,
}

/// One day of finished builds, in the caller's UTC offset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct BuildInsightsDay {
    /// Unix seconds at the start of the day.
    pub day_start: i64,
    pub succeeded: i64,
    /// Failed and timed-out builds.
    pub failed: i64,
    /// Canceled and expired builds.
    pub other: i64,
    /// Median run time of the day's succeeded builds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50_duration_seconds: Option<i64>,
}

/// Builds waiting or running right now.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct BuildQueueSnapshot {
    /// Queued and scheduled builds that a runner can pick up.
    pub waiting: i64,
    /// Queued builds held for a maintainer's approval.
    pub awaiting_approval: i64,
    /// Assigned and running builds.
    pub running: i64,
    /// When the longest-waiting build was queued, in unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_waiting_since: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ProjectBuildInsights {
    pub project_id: String,
    pub project_name: String,
    pub totals: BuildOutcomeTotals,
    pub duration: DurationPercentiles,
    /// Status of the project's newest finished build in the window.
    #[schema(value_type = BuildStatus)]
    pub last_status: String,
    pub last_finished_at: i64,
}

/// A pipeline with failed or timed-out builds in the window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct FailingPipelineInsights {
    pub pipeline_id: String,
    pub pipeline_name: String,
    pub project_id: String,
    pub project_name: String,
    pub failures: i64,
    pub finished: i64,
    /// Failures in a row ending with the newest finished build; 0 once it passed again.
    pub current_streak: i64,
    pub last_failed_build_id: String,
    pub last_failed_build_number: i64,
    pub last_failed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct BuildInsightsResponse {
    pub window_days: i64,
    /// Start of the window, in unix seconds.
    pub since: i64,
    /// Builds that finished in the window.
    pub totals: BuildOutcomeTotals,
    /// The same window length immediately before `since`, for comparison.
    pub previous_totals: BuildOutcomeTotals,
    /// Run time (start to finish) of succeeded builds in the window.
    pub duration: DurationPercentiles,
    /// Run time of succeeded builds in the previous window.
    pub previous_duration: DurationPercentiles,
    /// Time from queued to started for builds that started in the window.
    pub queue_wait: DurationPercentiles,
    /// One entry per day of the window, oldest first.
    pub daily: Vec<BuildInsightsDay>,
    pub queue: BuildQueueSnapshot,
    /// Projects with finished builds in the window, most builds first.
    pub projects: Vec<ProjectBuildInsights>,
    /// Up to five pipelines with the most failures in the window.
    pub failing_pipelines: Vec<FailingPipelineInsights>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CancelBuildResponse {
    pub build: Build,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BuildApprovalResponse {
    pub build: Build,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RerunBuildResponse {
    pub build: Build,
}

/// Step that uploads the App Bundle to Google Play.
pub const PUBLISH_PLAY_STEP: &str = "publish-play";
/// Step that uploads the IPA to App Store Connect.
pub const PUBLISH_APP_STORE_STEP: &str = "publish-app-store";
/// Steps that can run again on their own, in the order a build runs them.
/// They only read artifacts the build already stored; every other step
/// depends on workspace state that is gone once the build ends.
pub const RETRYABLE_BUILD_STEPS: [&str; 2] = [PUBLISH_PLAY_STEP, PUBLISH_APP_STORE_STEP];

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RetryBuildStepRequest {
    /// One of the build's `retryable_steps`, such as `publish-play`.
    pub step: String,
}

// ── Runner domain types ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatus {
    Online,
    Offline,
    Busy,
    Draining,
}

impl fmt::Display for RunnerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Busy => "busy",
            Self::Draining => "draining",
        };
        f.write_str(s)
    }
}

impl FromStr for RunnerStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "online" => Ok(Self::Online),
            "offline" => Ok(Self::Offline),
            "busy" => Ok(Self::Busy),
            "draining" => Ok(Self::Draining),
            other => Err(format!("unknown runner status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Runner {
    pub id: String,
    pub name: String,
    #[schema(value_type = RunnerStatus)]
    pub status: String,
    #[schema(value_type = Object)]
    pub capabilities: serde_json::Value,
    /// Every label the scheduler matches `runs_on` against: the labels detected
    /// from capabilities plus `custom_labels`.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Labels an operator assigned to this runner.
    #[serde(default)]
    pub custom_labels: Vec<String>,
    /// Utilisation from the latest heartbeat that carried metrics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<RunnerMetrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_pressure: Option<RunnerDiskPressure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_by: Option<String>,
    /// Set while a dependency cache purge waits for the runner to pick it up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_purge_requested_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Host utilisation a runner reports with each heartbeat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RunnerMetrics {
    /// One-minute load average as a share of logical CPUs, capped at 100.
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    /// Free space on the volume that holds build workspaces.
    pub disk_free_bytes: u64,
    pub disk_total_bytes: u64,
    /// Size of the runner's dependency caches, when measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_bytes: Option<u64>,
    /// When the backend received these metrics. Set by the backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerDiskPressure {
    Warning,
    Critical,
}

/// Disk free below this raises a warning incident for the runner.
pub const RUNNER_DISK_WARNING_BYTES: u64 = 20 * 1024 * 1024 * 1024;
/// Disk free below this raises the incident to critical.
pub const RUNNER_DISK_CRITICAL_BYTES: u64 = 10 * 1024 * 1024 * 1024;

pub fn runner_disk_pressure(disk_free_bytes: u64) -> Option<RunnerDiskPressure> {
    if disk_free_bytes < RUNNER_DISK_CRITICAL_BYTES {
        Some(RunnerDiskPressure::Critical)
    } else if disk_free_bytes < RUNNER_DISK_WARNING_BYTES {
        Some(RunnerDiskPressure::Warning)
    } else {
        None
    }
}

pub const MAX_RUNNER_LABELS: usize = 32;
const MAX_RUNNER_LABEL_LENGTH: usize = 64;

/// Labels a pipeline schedules on when it sets no `runs_on`. Builds assume a
/// macOS host unless a pipeline asks for something else.
pub const DEFAULT_RUNS_ON: &[&str] = &["macos"];

/// Lowercase a label and check it uses only `a-z`, `0-9`, `.`, `_` and `-`.
pub fn normalize_runner_label(label: &str) -> Result<String, String> {
    let label = label.trim().to_ascii_lowercase();
    if label.is_empty() || label.len() > MAX_RUNNER_LABEL_LENGTH {
        return Err(format!(
            "label must be between 1 and {MAX_RUNNER_LABEL_LENGTH} characters"
        ));
    }
    if !label
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!(
            "label '{label}' may only contain letters, digits, '.', '_' and '-'"
        ));
    }
    Ok(label)
}

/// Normalize, de-duplicate and sort a label list.
pub fn normalize_runner_labels(labels: &[String]) -> Result<Vec<String>, String> {
    if labels.len() > MAX_RUNNER_LABELS {
        return Err(format!("at most {MAX_RUNNER_LABELS} labels are allowed"));
    }
    let mut normalized = labels
        .iter()
        .map(|label| normalize_runner_label(label))
        .collect::<Result<Vec<_>, _>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

/// Labels implied by what a runner reports about its host, such as `macos`,
/// `macos-15`, `xcode-16` and `arm64`.
pub fn detected_runner_labels(capabilities: &serde_json::Value) -> Vec<String> {
    let text = |key: &str| {
        capabilities
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let major = |version: &str| {
        version
            .split(|c: char| !c.is_ascii_digit())
            .find(|part| !part.is_empty())
            .map(str::to_string)
    };
    let mut labels = Vec::new();
    if let Some(os) = text("os").and_then(|os| normalize_runner_label(os).ok()) {
        if let Some(version) = text("os_version").and_then(major) {
            labels.push(format!("{os}-{version}"));
        }
        labels.push(os);
    }
    if let Some(version) = text("xcode_version").and_then(major) {
        labels.push(format!("xcode-{version}"));
    }
    match text("arch") {
        Some("aarch64" | "arm64") => labels.push("arm64".to_string()),
        Some("x86_64" | "amd64") => labels.push("x86_64".to_string()),
        _ => {}
    }
    labels.sort();
    labels.dedup();
    labels
}

/// The labels a runner advertises to the scheduler.
pub fn effective_runner_labels(
    capabilities: &serde_json::Value,
    custom_labels: &[String],
) -> Vec<String> {
    let mut labels = detected_runner_labels(capabilities);
    labels.extend(custom_labels.iter().cloned());
    labels.sort();
    labels.dedup();
    labels
}

/// The labels a build needs, falling back to [`DEFAULT_RUNS_ON`].
pub fn scheduling_labels(runs_on: &[String]) -> Vec<String> {
    if runs_on.is_empty() {
        DEFAULT_RUNS_ON
            .iter()
            .map(|label| label.to_string())
            .collect()
    } else {
        runs_on.to_vec()
    }
}

/// Labels in `required` that `available` lacks.
pub fn missing_runner_labels(required: &[String], available: &[String]) -> Vec<String> {
    required
        .iter()
        .filter(|label| !available.contains(label))
        .cloned()
        .collect()
}

/// Accept `runs_on: linux` as well as `runs_on: [macos, xcode-16]`.
fn deserialize_runs_on<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(label)) => vec![label],
        Some(OneOrMany::Many(labels)) => labels,
    })
}

// ── Runner API types ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRunnerRequest {
    pub name: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub capabilities: serde_json::Value,
    /// Operator labels for the new runner, added to the detected labels.
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRunnerResponse {
    pub runner: Runner,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RunnerHeartbeatRequest {
    pub status: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub capabilities: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<RunnerMetrics>,
}

/// Heartbeat answer. Runners purge their dependency caches when
/// `cache_purge_requested_at` is newer than their last purge, then report
/// `capabilities.caches.purged_at` to clear the request.
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct RunnerHeartbeatResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_purge_requested_at: Option<i64>,
}

pub const RUNNER_PROTOCOL_VERSION: u32 = 6;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ClaimJobRequest {
    pub protocol_version: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateRunnerRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Replaces the runner's operator labels. Detected labels are kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_labels: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateRunnerResponse {
    pub runner: Runner,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ClaimJobResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ClaimedJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClaimedJob {
    pub build_id: String,
    pub project_id: String,
    pub pipeline_id: String,
    pub build_number: i64,
    #[schema(value_type = Object)]
    pub config_snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub lease_expires_at: i64,
    /// Ephemeral capability required for runner-owned signing material fetches.
    /// It is scoped to this job and revoked when the job leaves active execution.
    pub signing_token: String,
    /// Set when the job only retries these steps with the source build's
    /// stored artifacts, instead of building.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_steps: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateJobStatusRequest {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default)]
    pub steps: Vec<StepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StepResult {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub started_at: i64,
    pub finished_at: i64,
    pub duration_ms: i64,
}

// ── Test results ────────────────────────────────────────────────

/// Test results parsed by the runner from one build step.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct RunnerTestReport {
    /// Step that produced the report, such as `build-2`.
    pub step: String,
    pub total: i64,
    pub passed: i64,
    pub failed: i64,
    pub skipped: i64,
    pub duration_ms: i64,
    /// Failed tests. The daemon keeps at most 500 per build.
    pub failures: Vec<RunnerTestFailure>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct RunnerTestFailure {
    /// Test file or suite, such as `test/login_test.dart`.
    pub suite: String,
    /// Full test name, including group names.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

/// Totals across every test report of a build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct BuildTestSummary {
    pub total: i64,
    pub passed: i64,
    pub failed: i64,
    pub skipped: i64,
    pub duration_ms: i64,
}

/// One earlier run of a test in the same pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct TestRunOutcome {
    pub build_id: String,
    pub build_number: i64,
    /// `failed`, or `passed` when the build reported tests and this one did not fail.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct BuildTestFailure {
    pub suite: String,
    pub name: String,
    pub step: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    /// The test failed, passed, then failed again within the history window.
    pub flaky: bool,
    /// Recent builds of this pipeline that reported tests, oldest first, ending with this build.
    pub history: Vec<TestRunOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct BuildTestResultsResponse {
    /// Absent when the build reported no test results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<BuildTestSummary>,
    pub failures: Vec<BuildTestFailure>,
    /// How many recent test-reporting builds the history and flaky check cover.
    pub history_window: i64,
}

// ── Tester feedback ─────────────────────────────────────────────

/// Largest accepted feedback message, in characters.
pub const BUILD_FEEDBACK_MAX_MESSAGE_CHARS: usize = 4000;
/// Largest accepted device, OS version or app version, in characters.
pub const BUILD_FEEDBACK_MAX_FIELD_CHARS: usize = 120;
/// Largest accepted screenshot after base64 decoding, in bytes.
pub const BUILD_FEEDBACK_MAX_SCREENSHOT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateBuildFeedbackRequest {
    /// What the tester saw. 1 to 4000 characters.
    pub message: String,
    /// Device model, such as `Pixel 8` or `iPhone`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Operating system and version, such as `Android 15` or `iOS 18.1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    /// App version the tester installed, such as `2.4.0+118`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    /// The build's artifact the tester installed. Must belong to this build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// Optional PNG, JPEG or WebP screenshot, base64 encoded, at most 5 MiB
    /// decoded. Requires artifact storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct BuildFeedback {
    pub id: String,
    pub build_id: String,
    /// Email of the person who sent it.
    pub author_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// Fetch it from `GET /v1/builds/{build_id}/feedback/{feedback_id}/screenshot`.
    pub has_screenshot: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ListBuildFeedbackResponse {
    /// Oldest first.
    pub feedback: Vec<BuildFeedback>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListRunnersResponse {
    pub runners: Vec<Runner>,
    pub total: i64,
    pub online_total: i64,
    /// Builds waiting for a runner.
    #[serde(default)]
    pub queued_total: i64,
    /// Queued builds whose `runs_on` labels no online runner has.
    #[serde(default)]
    pub unmatched_queued_total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct JobStatusResponse {
    pub status: String,
}

// ── Artifact domain types ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Artifact {
    pub id: String,
    pub build_id: String,
    pub name: String,
    #[schema(value_type = ArtifactType)]
    pub artifact_type: String,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub checksum: Option<String>,
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
    pub created_at: i64,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Apk,
    Ipa,
    App,
    /// An Android App Bundle. Not installable on a device; Google Play takes it.
    Aab,
    Generic,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateArtifactRequest {
    pub name: String,
    pub artifact_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateArtifactResponse {
    pub artifact: Artifact,
    pub upload_url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompleteArtifactRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CompleteArtifactResponse {
    pub artifact: Artifact,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListBuildArtifactsRequest {
    pub build_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListArtifactsResponse {
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArtifactDownloadLinkResponse {
    pub download_url: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactInstallPlatform {
    Android,
    Ios,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArtifactInstallLinkResponse {
    pub platform: ArtifactInstallPlatform,
    pub install_url: String,
    pub download_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_url: Option<String>,
    pub expires_at: i64,
}

// ── Scoped download token types ─────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateScopedDownloadTokenRequest {
    /// TTL in seconds (default: 86400 = 24 hours, max: 604800 = 7 days).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<i64>,
    /// If true, token is consumed after first download.
    #[serde(default)]
    pub single_use: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateScopedDownloadTokenResponse {
    pub id: String,
    pub download_url: String,
    pub token: String,
    pub prefix: String,
    pub expires_at: i64,
    pub single_use: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArtifactDownloadTokenSummary {
    pub id: String,
    pub artifact_id: String,
    pub prefix: String,
    pub created_by: String,
    pub created_by_email: String,
    pub expires_at: i64,
    pub single_use: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<i64>,
    pub is_expired: bool,
    pub is_used: bool,
    pub is_revoked: bool,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListArtifactDownloadTokensResponse {
    pub tokens: Vec<ArtifactDownloadTokenSummary>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RevokeArtifactDownloadTokenResponse {
    pub revoked: bool,
}

// ── Artifact storage settings types ─────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStorageProvider {
    Disabled,
    Local,
    S3,
    R2,
}

impl fmt::Display for ArtifactStorageProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Disabled => "disabled",
            Self::Local => "local",
            Self::S3 => "s3",
            Self::R2 => "r2",
        };
        f.write_str(s)
    }
}

impl FromStr for ArtifactStorageProvider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "disabled" => Ok(Self::Disabled),
            "local" => Ok(Self::Local),
            "s3" => Ok(Self::S3),
            "r2" => Ok(Self::R2),
            other => Err(format!("unknown artifact storage provider: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStorageSource {
    Database,
    Environment,
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArtifactStorageSettings {
    pub provider: ArtifactStorageProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_base_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_bucket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_endpoint: Option<String>,
    pub has_access_key_id: bool,
    pub has_secret_access_key: bool,
    pub source: ArtifactStorageSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArtifactStorageSettingsResponse {
    pub settings: ArtifactStorageSettings,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateArtifactStorageSettingsRequest {
    pub provider: ArtifactStorageProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_base_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_bucket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyStorageMode {
    Keychain,
    File,
}

impl fmt::Display for KeyStorageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Keychain => "keychain",
            Self::File => "file",
        };
        f.write_str(s)
    }
}

impl FromStr for KeyStorageMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "keychain" => Ok(Self::Keychain),
            "file" => Ok(Self::File),
            other => Err(format!("unknown key storage mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Local,
    Remote,
}

impl fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Local => "local",
            Self::Remote => "remote",
        };
        f.write_str(s)
    }
}

impl FromStr for RuntimeMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            other => Err(format!("unknown runtime mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemoteAuthMode {
    Oidc,
    TrustedProxy,
}

impl fmt::Display for RemoteAuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Oidc => "oidc",
            Self::TrustedProxy => "trusted_proxy",
        };
        f.write_str(s)
    }
}

impl FromStr for RemoteAuthMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "oidc" => Ok(Self::Oidc),
            "trusted_proxy" => Ok(Self::TrustedProxy),
            other => Err(format!("unknown remote auth mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ExternalAccessPreflightCheck {
    pub id: String,
    pub label: String,
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ExternalAccessPreflightResponse {
    pub ready: bool,
    pub checks: Vec<ExternalAccessPreflightCheck>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAccessNetworkSource {
    Database,
    Environment,
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ExternalAccessNetworkSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_delivery_url: Option<String>,
    pub allowed_origins: Vec<String>,
    pub source: ExternalAccessNetworkSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ExternalAccessNetworkSettingsResponse {
    pub settings: ExternalAccessNetworkSettings,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateExternalAccessNetworkSettingsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_delivery_url: Option<String>,
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConfigureExternalAccessOidcRequest {
    pub issuer_url: String,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConfigureExternalAccessOidcResponse {
    pub discovered_issuer: String,
    pub has_client_secret: bool,
    pub configured_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetExternalAccessOidcResponse {
    pub issuer_url: String,
    pub client_id: String,
    pub has_client_secret: bool,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
    pub configured_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TestOidcConnectionRequest {
    pub issuer_url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TestOidcConnectionResponse {
    pub success: bool,
    pub discovered_issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
    pub scopes_supported: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct TrustedProxySettingsPublic {
    pub user_email_header: String,
    pub trusted_proxy_cidrs: Vec<String>,
    pub has_shared_secret: bool,
    pub has_warpgate_ticket: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warpgate_ticket_source: Option<WarpgateTicketSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarpgateTicketSource {
    Database,
    Environment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct TrustedProxySettingsResponse {
    pub settings: TrustedProxySettingsPublic,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateTrustedProxySettingsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_email_header: Option<String>,
    #[serde(default)]
    pub trusted_proxy_cidrs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warpgate_ticket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InstancePreferences {
    pub key_storage_mode: KeyStorageMode,
    pub runtime_mode: RuntimeMode,
    pub remote_auth_mode: RemoteAuthMode,
    pub direct_macos_runner_paused: bool,
    pub restart_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InstancePreferencesResponse {
    pub preferences: InstancePreferences,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateInstancePreferencesRequest {
    pub key_storage_mode: KeyStorageMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_mode: Option<RuntimeMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_auth_mode: Option<RemoteAuthMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_macos_runner_paused: Option<bool>,
}

// ── Project API types ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_provider: Option<ScmProvider>,
    #[schema(value_type = Object)]
    pub settings: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub current_user_role: ProjectRole,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_repository_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateProjectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProjectResponse {
    pub project: Project,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectDetailResponse {
    pub project: Project,
    pub pipeline_count: i64,
    pub build_count: i64,
    pub current_user_role: ProjectRole,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListProjectsResponse {
    pub projects: Vec<Project>,
    pub total: i64,
}

// ── Project member / per-project RBAC types ─────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRole {
    Maintainer,
    Developer,
    Viewer,
}

impl fmt::Display for ProjectRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Maintainer => "maintainer",
            Self::Developer => "developer",
            Self::Viewer => "viewer",
        };
        f.write_str(s)
    }
}

impl FromStr for ProjectRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "maintainer" => Ok(Self::Maintainer),
            "developer" => Ok(Self::Developer),
            "viewer" => Ok(Self::Viewer),
            other => Err(format!("unknown project role: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectMember {
    pub id: String,
    pub project_id: String,
    pub user_id: String,
    pub role: ProjectRole,
    pub user_email: String,
    pub user_role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_avatar_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddProjectMemberRequest {
    pub user_id: String,
    pub role: ProjectRole,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddProjectMemberResponse {
    pub member: ProjectMember,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateProjectMemberRequest {
    pub role: ProjectRole,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateProjectMemberResponse {
    pub member: ProjectMember,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RemoveProjectMemberResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListProjectMembersResponse {
    pub members: Vec<ProjectMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectMemberCandidate {
    pub id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListProjectMemberCandidatesResponse {
    pub candidates: Vec<ProjectMemberCandidate>,
}

// ── Pipeline API types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildPlatform {
    #[default]
    Android,
    Ios,
    Macos,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PipelineCommandStages {
    #[serde(default)]
    #[schema(required = true)]
    pub pre_build: Vec<String>,
    #[serde(default)]
    #[schema(required = true)]
    pub build: Vec<String>,
    #[serde(default)]
    #[schema(required = true)]
    pub post_build: Vec<String>,
}

/// Where a build's steps run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildIsolation {
    /// Directly on the runner host, with the host's toolchains.
    #[default]
    Host,
    /// In a fresh Tart VM cloned from the image for the build's Flutter version.
    Vm,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformBuildArgs {
    #[serde(default)]
    #[schema(required = true)]
    pub android: Vec<String>,
    #[serde(default)]
    #[schema(required = true)]
    pub ios: Vec<String>,
    #[serde(default)]
    #[schema(required = true)]
    pub macos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformBuildCommands {
    #[serde(default)]
    pub android: Option<String>,
    #[serde(default)]
    pub ios: Option<String>,
    #[serde(default)]
    pub macos: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PipelineEnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineExecutionConfig {
    #[serde(default = "default_platforms")]
    #[schema(required = true)]
    pub platforms: Vec<BuildPlatform>,
    #[serde(default)]
    pub flutter_version: Option<String>,
    #[serde(default)]
    #[schema(required = true)]
    pub commands: PipelineCommandStages,
    #[serde(default)]
    pub platform_build_args: PlatformBuildArgs,
    #[serde(default)]
    pub platform_commands: PlatformBuildCommands,
    #[serde(default)]
    pub env: Vec<PipelineEnvVar>,
    #[serde(default = "default_artifact_patterns")]
    #[schema(required = true)]
    pub artifact_patterns: Vec<String>,
    /// Added to the project build number to form `OORE_VERSION_CODE`, so the
    /// first store upload can start above the store's existing version code.
    #[serde(default)]
    pub build_number_offset: i64,
    /// Runner labels a build needs, all of them. Empty means `macos`.
    #[serde(default, deserialize_with = "deserialize_runs_on")]
    pub runs_on: Vec<String>,
    #[serde(default)]
    pub isolation: BuildIsolation,
}

/// Secret-free representation of a repository-owned pipeline config.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RepositoryWorkflowPreview {
    pub path: String,
    pub valid: bool,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<RepositoryWorkflowExecutionPreview>,
}

/// Pipeline behavior safe to show before import. Environment values are never returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RepositoryWorkflowExecutionPreview {
    pub platforms: Vec<BuildPlatform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flutter_version: Option<String>,
    pub commands: PipelineCommandStages,
    pub platform_build_args: PlatformBuildArgs,
    pub platform_commands: PlatformBuildCommands,
    pub env_keys: Vec<String>,
    pub artifact_patterns: Vec<String>,
    #[serde(default)]
    pub build_number_offset: i64,
    #[serde(default)]
    pub runs_on: Vec<String>,
    pub isolation: BuildIsolation,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoverRepositoryWorkflowsResponse {
    pub project_id: String,
    pub provider: ScmProvider,
    /// Branch, tag, or commit requested for discovery.
    pub reference: String,
    pub workflows: Vec<RepositoryWorkflowPreview>,
    /// True when more matching files existed than the bounded response could inspect.
    pub truncated: bool,
}

/// Build timeout when a pipeline does not set one.
pub const DEFAULT_BUILD_TIMEOUT_MINUTES: u32 = 60;
/// Longest build timeout a pipeline may set (24 hours).
pub const MAX_BUILD_TIMEOUT_MINUTES: u32 = 1440;

pub fn default_build_timeout_minutes() -> u32 {
    DEFAULT_BUILD_TIMEOUT_MINUTES
}

fn default_platforms() -> Vec<BuildPlatform> {
    vec![BuildPlatform::Android]
}

fn default_artifact_patterns() -> Vec<String> {
    vec!["build/app/outputs/flutter-apk/*.apk".to_string()]
}

pub fn default_artifact_patterns_for_platforms(platforms: &[BuildPlatform]) -> Vec<String> {
    let mut patterns = Vec::new();
    if platforms.contains(&BuildPlatform::Android) {
        patterns.extend([
            "build/app/outputs/flutter-apk/*.apk".to_string(),
            "build/app/outputs/bundle/**/*.aab".to_string(),
        ]);
    }
    if platforms.contains(&BuildPlatform::Ios) {
        patterns.push("build/ios/ipa/*.ipa".to_string());
    }
    if platforms.contains(&BuildPlatform::Macos) {
        patterns.push("build/macos/Build/Products/Release/*.app".to_string());
    }
    patterns
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryPipelineConfig {
    version: u32,
    platforms: Vec<BuildPlatform>,
    #[serde(default)]
    flutter_version: Option<String>,
    #[serde(default)]
    commands: PipelineCommandStages,
    #[serde(default)]
    platform_build_args: PlatformBuildArgs,
    #[serde(default)]
    platform_commands: PlatformBuildCommands,
    #[serde(default)]
    env: Vec<PipelineEnvVar>,
    #[serde(default)]
    artifacts: Option<RepositoryArtifacts>,
    #[serde(default)]
    build_number_offset: i64,
    #[serde(default, deserialize_with = "deserialize_runs_on")]
    runs_on: Vec<String>,
    #[serde(default)]
    isolation: BuildIsolation,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryArtifacts {
    #[serde(default)]
    patterns: Vec<String>,
}

/// Parse a `version: 1` file. [`parse_repository_config`] accepts both versions.
pub fn parse_repository_pipeline_yaml(raw: &str) -> Result<PipelineExecutionConfig, String> {
    let parsed: RepositoryPipelineConfig =
        serde_yaml::from_str(raw).map_err(|error| format!("YAML parse error: {error}"))?;
    if parsed.version != 1 {
        return Err(format!(
            "unsupported config version {}, expected 1",
            parsed.version
        ));
    }

    let artifact_patterns = parsed.artifacts.map_or_else(
        || default_artifact_patterns_for_platforms(&parsed.platforms),
        |artifacts| artifacts.patterns,
    );
    let config = PipelineExecutionConfig {
        platforms: parsed.platforms,
        flutter_version: parsed.flutter_version,
        commands: parsed.commands,
        platform_build_args: parsed.platform_build_args,
        platform_commands: parsed.platform_commands,
        env: parsed.env,
        artifact_patterns,
        build_number_offset: parsed.build_number_offset,
        runs_on: parsed.runs_on,
        isolation: parsed.isolation,
    };
    let errors = validate_repository_execution_config(&config);
    if errors.is_empty() {
        let runs_on = normalize_runner_labels(&config.runs_on)?;
        Ok(PipelineExecutionConfig { runs_on, ..config })
    } else {
        Err(errors.join("\n"))
    }
}

/// Repository-owned VM image customisation, read from `oore.image.yaml`.
/// The Flutter version still comes from `.fvmrc` or the pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmImageConfig {
    pub version: u32,
    /// Tart image to start from, overriding the runner's base image.
    #[serde(default)]
    pub base: Option<String>,
    /// Extra `sdkmanager` packages, such as `platforms;android-36`.
    #[serde(default)]
    pub android_packages: Vec<String>,
    /// Extra Homebrew formulae.
    #[serde(default)]
    pub brew: Vec<String>,
    /// Shell commands run last, after Flutter is installed.
    #[serde(default)]
    pub setup: Vec<String>,
}

pub const VM_IMAGE_CONFIG_PATH: &str = "oore.image.yaml";

fn is_plain_package_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ';' | '@' | '/' | '+')
        })
}

pub fn parse_vm_image_yaml(raw: &str) -> Result<VmImageConfig, String> {
    let config: VmImageConfig =
        serde_yaml::from_str(raw).map_err(|error| format!("YAML parse error: {error}"))?;
    let mut errors = Vec::new();
    if config.version != 1 {
        errors.push(format!(
            "unsupported image config version {}, expected 1",
            config.version
        ));
    }
    if let Some(base) = &config.base
        && (base.trim().is_empty() || base.chars().any(char::is_whitespace))
    {
        errors.push("base must be a Tart image name or OCI reference".to_string());
    }
    for (field, values) in [
        ("android_packages", &config.android_packages),
        ("brew", &config.brew),
    ] {
        for (index, value) in values.iter().enumerate() {
            if !is_plain_package_name(value) {
                errors.push(format!("{field}[{index}] is not a package name: '{value}'"));
            }
        }
    }
    for (index, command) in config.setup.iter().enumerate() {
        if command.trim().is_empty() {
            errors.push(format!("setup[{index}] must not be empty"));
        }
    }
    if errors.is_empty() {
        Ok(config)
    } else {
        Err(errors.join("\n"))
    }
}

/// Highest `versionCode` Google Play accepts. Apple build numbers have no
/// lower ceiling that matters here, so this bounds every store.
pub const MAX_STORE_VERSION_CODE: i64 = 2_100_000_000;

pub fn validate_build_number_offset(offset: i64) -> Result<(), String> {
    if !(0..MAX_STORE_VERSION_CODE).contains(&offset) {
        return Err(format!(
            "must be between 0 and {}",
            MAX_STORE_VERSION_CODE - 1
        ));
    }
    Ok(())
}

/// The version code a build sends to stores: its project build number plus the
/// pipeline offset, rejected when it exceeds what Google Play accepts.
pub fn store_version_code(build_number: i64, offset: i64) -> Result<i64, String> {
    validate_build_number_offset(offset).map_err(|error| format!("build_number_offset {error}"))?;
    let code = build_number.saturating_add(offset);
    if !(1..=MAX_STORE_VERSION_CODE).contains(&code) {
        return Err(format!(
            "version code {code} (build {build_number} + offset {offset}) is outside the store range 1..={MAX_STORE_VERSION_CODE}"
        ));
    }
    Ok(code)
}

pub fn validate_artifact_pattern(pattern: &str) -> Result<(), String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err("must not be empty".to_string());
    }
    if pattern.len() > 512 {
        return Err("is too long (max 512 chars)".to_string());
    }
    if !pattern.contains(['*', '?']) {
        return Err("must contain '*' or '?'".to_string());
    }
    if pattern.contains('\\') || pattern.starts_with('/') {
        return Err("must be a workspace-relative path using '/' separators".to_string());
    }
    if pattern
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("must not contain empty, '.' or '..' path segments".to_string());
    }
    Ok(())
}

pub fn validate_repository_config_path(path: &str) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("must not be empty".to_string());
    }
    if path.len() > 512 {
        return Err("is too long (max 512 chars)".to_string());
    }
    if path.starts_with('/') {
        return Err("must be a repository-relative path".to_string());
    }
    if path.contains('\\') {
        return Err("must use '/' separators".to_string());
    }
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("must not contain empty, '.' or '..' path segments".to_string());
    }
    Ok(())
}

pub fn artifact_pattern_matches(pattern: &str, relative_path: &str) -> bool {
    fn segment_matches(pattern: &[u8], value: &[u8]) -> bool {
        let (mut p, mut v, mut star, mut retry) = (0, 0, None, 0);
        while v < value.len() {
            if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == value[v]) {
                p += 1;
                v += 1;
            } else if p < pattern.len() && pattern[p] == b'*' {
                star = Some(p);
                p += 1;
                retry = v;
            } else if let Some(star_pos) = star {
                p = star_pos + 1;
                retry += 1;
                v = retry;
            } else {
                return false;
            }
        }
        while p < pattern.len() && pattern[p] == b'*' {
            p += 1;
        }
        p == pattern.len()
    }

    fn path_matches(pattern: &[&str], path: &[&str]) -> bool {
        match pattern.split_first() {
            None => path.is_empty(),
            Some((&"**", rest)) => {
                path_matches(rest, path) || (!path.is_empty() && path_matches(pattern, &path[1..]))
            }
            Some((segment, rest)) => {
                !path.is_empty()
                    && segment_matches(segment.as_bytes(), path[0].as_bytes())
                    && path_matches(rest, &path[1..])
            }
        }
    }

    let pattern = pattern.trim();
    if !pattern.contains('/') {
        return relative_path
            .rsplit('/')
            .next()
            .is_some_and(|name| segment_matches(pattern.as_bytes(), name.as_bytes()));
    }
    path_matches(
        &pattern.split('/').collect::<Vec<_>>(),
        &relative_path.split('/').collect::<Vec<_>>(),
    )
}

/// Whether a pipeline environment key matches `[A-Za-z_][A-Za-z0-9_]*`.
/// Callers trim input and report errors according to their configuration source.
pub fn is_valid_pipeline_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn validate_repository_execution_config(config: &PipelineExecutionConfig) -> Vec<String> {
    let mut errors = Vec::new();
    if config.platforms.is_empty() {
        errors.push("platforms must include at least one target".to_string());
    }
    for (stage, commands) in [
        ("pre_build", &config.commands.pre_build),
        ("build", &config.commands.build),
        ("post_build", &config.commands.post_build),
    ] {
        for (index, command) in commands.iter().enumerate() {
            if command.trim().is_empty() {
                errors.push(format!("commands.{stage}[{index}] must not be empty"));
            }
        }
    }
    for (index, pattern) in config.artifact_patterns.iter().enumerate() {
        if let Err(error) = validate_artifact_pattern(pattern) {
            errors.push(format!("artifacts.patterns[{index}] {error}"));
        }
    }
    if let Err(error) = validate_build_number_offset(config.build_number_offset) {
        errors.push(format!("build_number_offset {error}"));
    }
    if let Err(error) = normalize_runner_labels(&config.runs_on) {
        errors.push(format!("runs_on {error}"));
    }
    let mut env_keys = std::collections::HashSet::new();
    for (index, entry) in config.env.iter().enumerate() {
        let key = entry.key.trim();
        if !is_valid_pipeline_env_key(key) {
            errors.push(format!(
                "env[{index}].key must match [A-Za-z_][A-Za-z0-9_]*"
            ));
        } else if !env_keys.insert(key) {
            errors.push(format!("env contains duplicate key '{key}'"));
        }
    }
    errors.extend(secret_expression_errors(config));
    errors
}

impl Default for PipelineExecutionConfig {
    fn default() -> Self {
        Self {
            platforms: default_platforms(),
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands::default(),
            env: Vec::new(),
            artifact_patterns: default_artifact_patterns(),
            build_number_offset: 0,
            runs_on: Vec::new(),
            isolation: BuildIsolation::Host,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pipeline {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub config_path: String,
    #[serde(default)]
    #[schema(required = true)]
    pub config_path_explicit: bool,
    #[serde(default)]
    #[schema(required = true)]
    pub execution_config: PipelineExecutionConfig,
    pub trigger_config: TriggerConfig,
    pub concurrency: ConcurrencyPolicy,
    /// Minutes a running build may take before it is marked `timed_out`.
    #[serde(default = "default_build_timeout_minutes")]
    #[schema(required = true)]
    pub timeout_minutes: u32,
    #[serde(default)]
    #[schema(required = true)]
    pub approval: ApprovalPolicy,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreatePipelineRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path_explicit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_config: Option<PipelineExecutionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_config: Option<TriggerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<ConcurrencyPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalPolicy>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdatePipelineRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path_explicit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_config: Option<PipelineExecutionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_config: Option<TriggerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<ConcurrencyPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreatePipelineResponse {
    pub pipeline: Pipeline,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PipelineDetailResponse {
    pub pipeline: Pipeline,
    pub build_count: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListPipelinesResponse {
    pub pipelines: Vec<Pipeline>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ValidatePipelineRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path_explicit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_config: Option<PipelineExecutionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_config: Option<TriggerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<ConcurrencyPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_yaml: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ValidatePipelineResponse {
    pub valid: bool,
    pub errors: Vec<String>,
}

// ── Secret types ────────────────────────────────────────────────

/// Maximum secret value size. Large enough for a Google Play service account JSON.
pub const MAX_SECRET_VALUE_BYTES: usize = 64 * 1024;
/// Maximum secret name length.
pub const MAX_SECRET_NAME_LENGTH: usize = 128;

/// Where a secret is defined. A job sees instance, project and pipeline secrets;
/// on a name clash the narrower scope wins.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretScope {
    Instance,
    Project,
    Pipeline,
}

impl SecretScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instance => "instance",
            Self::Project => "project",
            Self::Pipeline => "pipeline",
        }
    }
}

/// Secret metadata. Values are write-only and never returned by the user API.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SecretSummary {
    pub name: String,
    pub scope: SecretScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListSecretsResponse {
    pub secrets: Vec<SecretSummary>,
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSecretRequest {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for CreateSecretRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateSecretRequest")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

/// Replace a secret's value (rotation).
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RotateSecretRequest {
    pub value: String,
}

impl fmt::Debug for RotateSecretRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RotateSecretRequest")
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SecretResponse {
    pub secret: SecretSummary,
}

/// A resolved secret delivered to the runner assigned to an active job.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerJobSecret {
    pub name: String,
    pub value: String,
    pub scope: SecretScope,
}

impl fmt::Debug for RunnerJobSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunnerJobSecret")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .field("scope", &self.scope)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerJobSecretsResponse {
    pub secrets: Vec<RunnerJobSecret>,
}

/// Validate a secret name. Names follow environment variable rules.
pub fn validate_secret_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("must not be empty".to_string());
    }
    if name.len() > MAX_SECRET_NAME_LENGTH {
        return Err(format!("is too long (max {MAX_SECRET_NAME_LENGTH} chars)"));
    }
    if !is_valid_pipeline_env_key(name) {
        return Err("must match [A-Za-z_][A-Za-z0-9_]*".to_string());
    }
    Ok(())
}

/// A `${{ secrets.NAME }}` reference found in a command or env value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretReference {
    /// Byte offset of `$`.
    pub start: usize,
    /// Byte offset just past the closing `}}`.
    pub end: usize,
    pub name: String,
}

/// Parse every `${{ ... }}` expression in `text`. Only `secrets.NAME` is
/// supported; any other expression is an error so it never reaches a shell.
pub fn parse_secret_references(text: &str) -> Result<Vec<SecretReference>, String> {
    let mut references = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("${{") {
        let start = cursor + offset;
        let body_start = start + 3;
        let Some(close) = text[body_start..].find("}}") else {
            return Err("has an unterminated ${{ expression".to_string());
        };
        let end = body_start + close + 2;
        let expression = text[body_start..body_start + close].trim();
        let name = expression
            .strip_prefix("secrets.")
            .filter(|name| is_valid_pipeline_env_key(name))
            .ok_or_else(|| {
                format!(
                    "has unsupported expression '${{{{ {expression} }}}}'; only ${{{{ secrets.NAME }}}} is supported"
                )
            })?;
        references.push(SecretReference {
            start,
            end,
            name: name.to_string(),
        });
        cursor = end;
    }
    Ok(references)
}

/// Report malformed `${{ }}` expressions in commands and env values.
pub fn secret_expression_errors(config: &PipelineExecutionConfig) -> Vec<String> {
    let mut errors = Vec::new();
    let mut check = |path: String, text: &str| {
        if let Err(error) = parse_secret_references(text) {
            errors.push(format!("{path} {error}"));
        }
    };
    for (stage, commands) in [
        ("pre_build", &config.commands.pre_build),
        ("build", &config.commands.build),
        ("post_build", &config.commands.post_build),
    ] {
        for (index, command) in commands.iter().enumerate() {
            check(format!("commands.{stage}[{index}]"), command);
        }
    }
    for (platform, args) in [
        ("android", &config.platform_build_args.android),
        ("ios", &config.platform_build_args.ios),
        ("macos", &config.platform_build_args.macos),
    ] {
        for (index, arg) in args.iter().enumerate() {
            check(format!("platform_build_args.{platform}[{index}]"), arg);
        }
    }
    for (platform, command) in [
        ("android", &config.platform_commands.android),
        ("ios", &config.platform_commands.ios),
        ("macos", &config.platform_commands.macos),
    ] {
        if let Some(command) = command {
            check(format!("platform_commands.{platform}"), command);
        }
    }
    for (index, entry) in config.env.iter().enumerate() {
        check(format!("env[{index}].value"), &entry.value);
    }
    errors
}

/// Replace each `${{ secrets.NAME }}` reference with `replacement(NAME)`.
pub fn replace_secret_references(
    text: &str,
    mut replacement: impl FnMut(&str) -> String,
) -> Result<String, String> {
    let references = parse_secret_references(text)?;
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for reference in references {
        out.push_str(&text[cursor..reference.start]);
        out.push_str(&replacement(&reference.name));
        cursor = reference.end;
    }
    out.push_str(&text[cursor..]);
    Ok(out)
}

// ── Pipeline publish types ──────────────────────────────────────

/// Where Google Play release notes come from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GooglePlayReleaseNotesSource {
    /// No release notes.
    #[default]
    None,
    /// The build changelog, under `release_notes_locale`.
    Changelog,
    /// One `<locale>.txt` file per language in `release_notes_dir`.
    RepoFiles,
}

/// Per-pipeline Google Play upload settings. Credentials are referenced by
/// secret name and never stored here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GooglePlayPublishConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Name of the secret holding the service account JSON key.
    pub service_account_secret: String,
    /// Expected application id. When omitted, the AAB's package is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    /// `internal`, `closed`, `open`, `production`, or a custom track name.
    pub track: String,
    /// Initial staged rollout, 0 < percent <= 100. Omitted means 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_percent: Option<f64>,
    /// Create a draft release instead of rolling out.
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub release_notes: GooglePlayReleaseNotesSource,
    #[serde(default = "default_release_notes_locale")]
    pub release_notes_locale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_notes_dir: Option<String>,
    /// Upload R8/ProGuard `mapping.txt` when the build produced one.
    #[serde(default = "default_true")]
    pub upload_mapping: bool,
    /// In-app update priority, 0 to 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_app_update_priority: Option<u8>,
    /// Staged rollout schedule, crash-rate halt and approval gate for the
    /// release record. Omitted means the default schedule with no halt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_policy: Option<ReleasePolicy>,
}

fn default_release_notes_locale() -> String {
    "en-US".to_string()
}

/// Validate Google Play publish settings, returning every problem found.
pub fn validate_google_play_publish_config(config: &GooglePlayPublishConfig) -> Vec<String> {
    let mut errors = Vec::new();
    if let Err(error) = validate_secret_name(config.service_account_secret.trim()) {
        errors.push(format!("service_account_secret {error}"));
    }
    if let Some(package) = &config.package_name {
        let valid = package.split('.').count() >= 2
            && package.split('.').all(|part| {
                part.chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic())
                    && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
        if !valid {
            errors.push(
                "package_name must be an Android application id such as com.example.app"
                    .to_string(),
            );
        }
    }
    let track = config.track.trim();
    if track.is_empty()
        || track.len() > 128
        || !track
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
    {
        errors.push(
            "track must be internal, closed, open, production, or a custom track name".to_string(),
        );
    }
    if let Some(percent) = config.rollout_percent {
        if !(percent > 0.0 && percent <= 100.0) {
            errors.push("rollout_percent must be greater than 0 and at most 100".to_string());
        } else if percent < 100.0 && track == "internal" {
            errors.push("the internal track does not support staged rollouts".to_string());
        } else if percent < 100.0 && config.draft {
            errors.push("a draft release cannot have a staged rollout".to_string());
        }
    }
    if let Some(policy) = &config.rollout_policy {
        if let Err(error) = policy.validate() {
            errors.push(error);
        }
        if let Err(error) = policy.validate_gate() {
            errors.push(error);
        }
    }
    if config.release_notes_locale.trim().is_empty() || config.release_notes_locale.len() > 16 {
        errors.push("release_notes_locale must be a Play language code such as en-US".to_string());
    }
    if config.release_notes == GooglePlayReleaseNotesSource::RepoFiles {
        match config.release_notes_dir.as_deref() {
            Some(dir) => {
                if let Err(error) = validate_repository_config_path(dir) {
                    errors.push(format!("release_notes_dir {error}"));
                }
            }
            None => errors
                .push("release_notes_dir is required for repo_files release notes".to_string()),
        }
    }
    if config
        .in_app_update_priority
        .is_some_and(|priority| priority > 5)
    {
        errors.push("in_app_update_priority must be between 0 and 5".to_string());
    }
    errors
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineGooglePlayPublishResponse {
    pub pipeline_id: String,
    /// Absent until publish settings are saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<GooglePlayPublishConfig>,
    /// Whether the referenced service account secret exists in a visible scope.
    pub service_account_secret_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    /// The latest credentials check, while the key it checked is still the
    /// one its secret name resolves to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_credentials_check: Option<GooglePlayCredentialsTestResponse>,
}

/// Which service account key to check against Google Play. The values come
/// from the form, so a key can be checked before the settings are saved.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GooglePlayCredentialsTestRequest {
    /// Name of the secret holding the service account JSON key.
    pub service_account_secret: String,
    /// The app to open. When omitted, the saved expected package is used,
    /// then the package of the pipeline's latest Google Play release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
}

/// Outcome of a Google Play credentials check. A failed check is a normal
/// answer, with Google's reason in `message`, not an HTTP error.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GooglePlayCredentialsTestResponse {
    pub ok: bool,
    /// The secret that was checked.
    pub service_account_secret: String,
    /// What passed, or the exact failure.
    pub message: String,
    /// The app the check opened, when one was known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    /// The key's `client_email`, when the secret is a service account key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_account_email: Option<String>,
    pub checked_at: i64,
}

/// Publish work for the runner, fetched with the job-scoped grant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerPublishPlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    /// Present only when Google Play publishing is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_play: Option<GooglePlayPublishConfig>,
    /// Present only when App Store publishing is enabled for an App Store export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_store: Option<RunnerAppStorePublishPlan>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublishStore {
    GooglePlay,
    AppStore,
}

/// A stored artifact of the build a retry job reuses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerRetryArtifact {
    pub id: String,
    pub name: String,
    pub artifact_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
    /// Short-lived download URL, relative to the daemon for local storage.
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerRetryArtifactsResponse {
    pub artifacts: Vec<RunnerRetryArtifact>,
}

/// Outcome of a store upload, reported by the runner.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerPublishResult {
    pub store: PublishStore,
    /// Store track as sent to the store (for Play: internal, alpha, beta, production or custom).
    pub track: String,
    pub package_name: String,
    pub version_code: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_name: Option<String>,
    /// Staged rollout fraction, 0 < f <= 1. Absent for drafts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollout_fraction: Option<f64>,
    /// `completed`, `in_progress` or `draft`.
    pub status: String,
}

// ── Pipeline Android signing types ──────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AndroidSigningBuildType {
    Debug,
    Release,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct AndroidSigningProfileInput {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystore_filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystore_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct UpdatePipelineAndroidSigningRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<AndroidSigningProfileInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<AndroidSigningProfileInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AndroidSigningProfile {
    pub build_type: AndroidSigningBuildType,
    pub enabled: bool,
    pub has_keystore: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystore_filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystore_checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_alias: Option<String>,
    pub has_store_password: bool,
    pub has_key_password: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineAndroidSigningResponse {
    pub pipeline_id: String,
    pub debug: AndroidSigningProfile,
    pub release: AndroidSigningProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerAndroidSigningProfile {
    pub build_type: AndroidSigningBuildType,
    pub enabled: bool,
    pub keystore_filename: String,
    pub keystore_base64: String,
    pub store_password: String,
    pub key_alias: String,
    pub key_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerAndroidSigningResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<RunnerAndroidSigningProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<RunnerAndroidSigningProfile>,
}

// ── Pipeline iOS signing types ──────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IosSigningMode {
    #[default]
    Manual,
    Api,
    Hybrid,
}

/// How a signed iOS archive is distributed.
///
/// `ad_hoc` IPAs install on registered devices through Oore. `app_store` IPAs
/// are signed with an App Store distribution profile and can only be uploaded
/// to App Store Connect for TestFlight or App Store release.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IosExportMethod {
    #[default]
    AdHoc,
    AppStore,
}

impl IosExportMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdHoc => "ad_hoc",
            Self::AppStore => "app_store",
        }
    }

    /// Parses a stored value. Unknown values fall back to ad hoc so a newer
    /// database never makes an older daemon produce App Store builds.
    pub fn from_stored(raw: &str) -> Self {
        match raw {
            "app_store" => Self::AppStore,
            _ => Self::AdHoc,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct IosCertificateInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p12_filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p12_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p12_password: Option<String>,
}

impl fmt::Debug for IosCertificateInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IosCertificateInput")
            .field("p12_filename", &self.p12_filename)
            .field("p12_base64", &"[REDACTED]")
            .field("p12_password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct IosProvisioningProfileInput {
    pub bundle_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_base64: Option<String>,
}

impl fmt::Debug for IosProvisioningProfileInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IosProvisioningProfileInput")
            .field("bundle_id", &self.bundle_id)
            .field("profile_filename", &self.profile_filename)
            .field("profile_base64", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct IosApiCredentialInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key_base64: Option<String>,
}

impl fmt::Debug for IosApiCredentialInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IosApiCredentialInput")
            .field("key_id", &self.key_id)
            .field("issuer_id", &self.issuer_id)
            .field("private_key_base64", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct IosBundleProfileMappingInput {
    pub bundle_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct UpdatePipelineIosSigningRequest {
    #[serde(default)]
    pub enabled: bool,
    pub mode: IosSigningMode,
    /// Omit to keep the stored export method. Changing it discards stored
    /// provisioning profiles because ad hoc and App Store profiles differ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_method: Option<IosExportMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(default)]
    pub bundle_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<IosCertificateInput>,
    #[serde(default)]
    pub provisioning_profiles: Vec<IosProvisioningProfileInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_credentials: Option<IosApiCredentialInput>,
    /// Use a shared App Store Connect key instead of the pipeline's own.
    /// Omit to keep the current choice; an empty string switches back to the
    /// pipeline's own key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_api_key_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IosProvisioningProfileSummary {
    pub bundle_id: String,
    pub has_profile: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineIosSigningResponse {
    pub pipeline_id: String,
    pub enabled: bool,
    pub mode: IosSigningMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    pub export_method: IosExportMethod,
    #[serde(default)]
    #[schema(required = true)]
    pub bundle_ids: Vec<String>,
    pub has_p12: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p12_filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p12_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p12_expires_at: Option<i64>,
    pub has_p12_password: bool,
    /// Whether the pipeline stores its own App Store Connect API key.
    pub has_api_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_issuer_id: Option<String>,
    /// The shared App Store Connect key in use. When set it takes the place of
    /// the pipeline's own key, which stays stored for switching back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_api_key: Option<AppStoreConnectKeySummary>,
    #[serde(default)]
    #[schema(required = true)]
    pub provisioning_profiles: Vec<IosProvisioningProfileSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerIosProvisioningProfile {
    pub bundle_id: String,
    pub profile_filename: String,
    pub profile_base64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerIosSigningBundle {
    pub enabled: bool,
    pub mode: IosSigningMode,
    pub team_id: String,
    pub export_method: IosExportMethod,
    pub p12_filename: String,
    pub p12_base64: String,
    pub p12_password: String,
    #[serde(default)]
    pub provisioning_profiles: Vec<RunnerIosProvisioningProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerIosSigningResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<RunnerIosSigningBundle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisteredIosDevice {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub udid: String,
    pub name: String,
    pub platform: String,
    pub status: String,
    pub added_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListPipelineIosDevicesResponse {
    pub pipeline_id: String,
    #[serde(default)]
    #[schema(required = true)]
    pub devices: Vec<RegisteredIosDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterIosDeviceRequest {
    pub udid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterIosDeviceResponse {
    pub pipeline_id: String,
    pub device: RegisteredIosDevice,
    pub profile_sync_triggered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SyncPipelineIosSigningResponse {
    pub pipeline_id: String,
    pub ok: bool,
    pub updated_profiles: usize,
    #[serde(default)]
    #[schema(required = true)]
    pub synced_bundle_ids: Vec<String>,
    #[serde(default)]
    #[schema(required = true)]
    pub warnings: Vec<String>,
}

// ── Shared App Store Connect API keys ──────────────────────────

/// An App Store Connect API key stored once for the instance. Pipelines that
/// sign or publish for the same Apple team can use it instead of their own key.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppStoreConnectKeySummary {
    pub id: String,
    pub name: String,
    pub key_id: String,
    pub issuer_id: String,
    /// Pipelines whose iOS signing uses this key.
    pub pipeline_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListAppStoreConnectKeysResponse {
    pub keys: Vec<AppStoreConnectKeySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppStoreConnectKeyResponse {
    pub key: AppStoreConnectKeySummary,
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateAppStoreConnectKeyRequest {
    pub name: String,
    pub key_id: String,
    pub issuer_id: String,
    /// Base64 of the `.p8` file downloaded from App Store Connect.
    pub private_key_base64: String,
}

impl fmt::Debug for CreateAppStoreConnectKeyRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateAppStoreConnectKeyRequest")
            .field("name", &self.name)
            .field("key_id", &self.key_id)
            .field("issuer_id", &self.issuer_id)
            .field("private_key_base64", &"[REDACTED]")
            .finish()
    }
}

/// Rename a shared key or replace it. Omitted fields keep their stored value;
/// a new `key_id` requires a new private key.
#[derive(Clone, Serialize, Deserialize, Default, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateAppStoreConnectKeyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_base64: Option<String>,
}

impl fmt::Debug for UpdateAppStoreConnectKeyRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateAppStoreConnectKeyRequest")
            .field("name", &self.name)
            .field("key_id", &self.key_id)
            .field("issuer_id", &self.issuer_id)
            .field(
                "private_key_base64",
                &self.private_key_base64.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

// ── Pipeline App Store publishing types ─────────────────────────

/// Where an App Store Connect upload goes once Apple finishes processing it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppStoreDestination {
    /// Add the build to the named TestFlight groups.
    #[default]
    Testflight,
    /// Attach the build to the App Store version matching its version string.
    AppStore,
}

impl AppStoreDestination {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Testflight => "testflight",
            Self::AppStore => "app_store",
        }
    }

    pub fn from_stored(raw: &str) -> Self {
        match raw {
            "app_store" => Self::AppStore,
            _ => Self::Testflight,
        }
    }
}

/// Release notes for one App Store Connect locale, such as `en-US`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct AppStoreReleaseNote {
    pub locale: String,
    pub whats_new: String,
}

/// App Store publishing settings for a pipeline, stored as the `app_store`
/// publish target. Uploads use the App Store Connect API key from the
/// pipeline's iOS signing settings and require the `app_store` export method.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema)]
pub struct AppStorePublishConfig {
    pub enabled: bool,
    #[serde(default)]
    pub destination: AppStoreDestination,
    /// TestFlight group names. Used when `destination` is `testflight`.
    #[serde(default)]
    #[schema(required = true)]
    pub testflight_groups: Vec<String>,
    /// TestFlight "What to Test" or App Store "What's New", per locale.
    #[serde(default)]
    #[schema(required = true)]
    pub release_notes: Vec<AppStoreReleaseNote>,
    /// Export compliance answer set on each processed build.
    #[serde(default)]
    pub uses_non_exempt_encryption: bool,
    /// Submit the App Store version for review once the build is attached.
    #[serde(default)]
    pub submit_for_review: bool,
    /// Release the approved version with Apple's seven-day phased release.
    #[serde(default)]
    pub phased_release: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineAppStorePublishResponse {
    pub pipeline_id: String,
    pub config: AppStorePublishConfig,
    /// Whether the pipeline's iOS signing has an App Store Connect API key,
    /// its own or a shared one.
    pub has_api_key: bool,
    /// Whether the pipeline's iOS signing exports App Store IPAs.
    pub app_store_export: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// App Store Connect API key handed to the runner for `altool` upload.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerAppStoreUploadCredentials {
    pub key_id: String,
    pub issuer_id: String,
    pub private_key_pem: String,
}

impl fmt::Debug for RunnerAppStoreUploadCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunnerAppStoreUploadCredentials")
            .field("key_id", &self.key_id)
            .field("issuer_id", &self.issuer_id)
            .field("private_key_pem", &"[REDACTED]")
            .finish()
    }
}

/// What the runner needs to upload the job's IPA to App Store Connect.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerAppStorePublishPlan {
    pub config: AppStorePublishConfig,
    pub credentials: RunnerAppStoreUploadCredentials,
}

// ── Build log types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildLogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildLogChunk {
    pub sequence: i64,
    pub content: String,
    #[schema(value_type = BuildLogStream)]
    pub stream: String,
}

// ── Live events (SSE) ───────────────────────────────────────────

/// `build` event on `GET /v1/events`: a build was created or changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct BuildChangedEvent {
    pub build_id: String,
    pub project_id: String,
    pub build_number: i64,
    #[schema(value_type = BuildStatus)]
    pub status: String,
    pub updated_at: i64,
}

/// `runner` event on `GET /v1/events`: a runner was registered, removed,
/// renamed or changed status. Sent only to users who can read runners.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct RunnerChangedEvent {
    pub runner_id: String,
    pub name: String,
    #[schema(value_type = RunnerStatus)]
    pub status: String,
    /// True when the runner no longer exists; `status` is its last known one.
    pub removed: bool,
}

/// `release` event on `GET /v1/events`: a release was created or its state
/// or rollout changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ReleaseChangedEvent {
    pub release_id: String,
    pub project_id: String,
    pub build_id: String,
    pub state: ReleaseState,
    /// Fraction of users, 0..1.
    pub rollout_fraction: f64,
    pub updated_at: i64,
}

/// `ready` event, the first event on every `GET /v1/events` connection.
/// Clients refetch once on `ready` after a reconnect, since events sent while
/// disconnected are not replayed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct EventStreamReady {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Whether this stream carries `runner` events.
    pub runners: bool,
}

/// `resync` event: events were dropped, so clients should refetch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct EventStreamResync {
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AppendBuildLogsRequest {
    pub chunks: Vec<BuildLogChunk>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AppendBuildLogsResponse {
    pub appended: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BuildLogsResponse {
    pub logs: Vec<BuildLogChunk>,
    pub total: i64,
}

// ── Retention policy types ───────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetentionCleanupTarget {
    ArtifactsOnly,
    Full,
}

impl fmt::Display for RetentionCleanupTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::ArtifactsOnly => "artifacts_only",
            Self::Full => "full",
        };
        f.write_str(s)
    }
}

impl FromStr for RetentionCleanupTarget {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "artifacts_only" => Ok(Self::ArtifactsOnly),
            "full" => Ok(Self::Full),
            other => Err(format!("unknown retention cleanup target: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RetentionPolicy {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_builds_per_project: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_artifact_size_bytes: Option<i64>,
    pub cleanup_target: RetentionCleanupTarget,
    #[serde(default)]
    #[schema(required = true)]
    pub keep_statuses: Vec<String>,
    pub dry_run: bool,
    pub cleanup_interval_secs: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_ttl_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RetentionPolicyResponse {
    pub policy: RetentionPolicy,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateRetentionPolicyRequest {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_builds_per_project: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_artifact_size_bytes: Option<i64>,
    pub cleanup_target: RetentionCleanupTarget,
    #[serde(default)]
    pub keep_statuses: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_secs: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_ttl_days: Option<i64>,
}

fn default_cleanup_interval() -> i64 {
    3600
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectRetentionOverride {
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_builds_per_project: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_artifact_size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_target: Option<RetentionCleanupTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_statuses: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_ttl_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EffectiveProjectRetentionResponse {
    pub effective: RetentionPolicy,
    pub has_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_fields: Option<ProjectRetentionOverride>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateProjectRetentionOverrideRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_builds_per_project: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_artifact_size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_target: Option<RetentionCleanupTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_statuses: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_ttl_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RetentionCleanupSummary {
    pub builds_expired: i64,
    pub artifacts_deleted: i64,
    pub bytes_reclaimed: i64,
    pub dry_run: bool,
    pub ran_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RetentionCleanupSummaryResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cleanup: Option<RetentionCleanupSummary>,
}

// ── Audit Logs ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuditLogEntry {
    pub id: i64,
    pub actor_id: Option<String>,
    pub actor_email: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListAuditLogsResponse {
    pub entries: Vec<AuditLogEntry>,
    pub total: i64,
}

// ── Tests ──────────────────────────────────────────────────────

// ── Notification channel types ──────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannelType {
    Webhook,
    Mattermost,
    Email,
    Slack,
}

impl fmt::Display for NotificationChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Webhook => f.write_str("webhook"),
            Self::Mattermost => f.write_str("mattermost"),
            Self::Email => f.write_str("email"),
            Self::Slack => f.write_str("slack"),
        }
    }
}

impl FromStr for NotificationChannelType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "webhook" => Ok(Self::Webhook),
            "mattermost" => Ok(Self::Mattermost),
            "email" => Ok(Self::Email),
            "slack" => Ok(Self::Slack),
            other => Err(format!("unknown notification channel type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SmtpTlsMode {
    None,
    StartTls,
    Tls,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub tls_mode: SmtpTlsMode,
    pub from_address: String,
    pub recipients: Vec<String>,
}

/// The SMTP settings an operator can read back. The password is never
/// returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SmtpConfigSummary {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub tls_mode: SmtpTlsMode,
    pub from_address: String,
    pub recipients: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateSmtpConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_mode: Option<SmtpTlsMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipients: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationDeliveryStatus {
    Pending,
    Delivered,
    Failed,
}

impl fmt::Display for NotificationDeliveryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Delivered => f.write_str("delivered"),
            Self::Failed => f.write_str("failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    pub channel_type: NotificationChannelType,
    pub enabled: bool,
    /// Which events trigger this channel. Empty means all.
    #[serde(default)]
    #[schema(required = true)]
    pub events: Vec<String>,
    /// Project this channel is scoped to. Absent means the channel receives
    /// events from every project, plus instance-wide events such as runner
    /// offline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// True if the channel has a URL configured (the actual URL is never exposed).
    pub has_url: bool,
    /// True if the channel has a secret configured: the HMAC secret for
    /// webhook channels, or the bot token used for mentions on Slack channels.
    pub has_secret: bool,
    /// True if the channel has SMTP configuration (email only).
    pub has_smtp_config: bool,
    /// Host of the configured URL, so operators can tell which endpoint
    /// receives alerts. The full URL can carry a token and is never exposed.
    /// Returned when reading one channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_host: Option<String>,
    /// SMTP settings without the password (email only). Returned when
    /// reading one channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smtp_summary: Option<SmtpConfigSummary>,
    /// Slack member IDs to mention, matched against the person behind each
    /// event (Slack only). Checked before the bot token lookup. Absent from
    /// daemons that predate it, which is the same as none.
    #[serde(default)]
    pub slack_mentions: Vec<SlackMention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Maps a person to the Slack member mentioned in their notifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SlackMention {
    /// Oore sign-in email (manual and API builds, releases) or Git username
    /// (push and merge request builds). Matched without regard to case.
    pub identity: String,
    /// Slack member ID, such as `U0123ABCD`.
    pub member_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateNotificationChannelRequest {
    pub name: String,
    pub channel_type: NotificationChannelType,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Events to notify on. Empty or omitted means all events.
    #[serde(default)]
    pub events: Vec<String>,
    /// Scope the channel to one project. Omit for an instance-wide channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Incoming webhook URL (required for webhook, mattermost and slack).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Webhook: HMAC secret for signing payloads. Slack: bot token with
    /// `users:read.email`, used to mention the triggering user. Optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// SMTP configuration (required for email channel type).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smtp_config: Option<SmtpConfig>,
    /// Slack member IDs to mention (Slack only). Works without a bot token.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slack_mentions: Vec<SlackMention>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateNotificationChannelRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    /// New project scope. Omit to keep existing; pass empty string to make
    /// the channel instance-wide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// New URL. Omit to keep existing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// New HMAC secret (webhook) or bot token (slack). Omit to keep existing;
    /// pass empty string to clear.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Partial SMTP configuration update (email only). Omit fields to keep existing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smtp_config: Option<UpdateSmtpConfig>,
    /// New Slack member mappings (Slack only). Omit to keep existing; pass an
    /// empty list to clear.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack_mentions: Option<Vec<SlackMention>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NotificationChannelResponse {
    pub channel: NotificationChannel,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListNotificationChannelsResponse {
    pub channels: Vec<NotificationChannel>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeleteNotificationChannelResponse {
    pub deleted: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TestNotificationChannelResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotificationDelivery {
    pub id: String,
    pub channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    pub event_type: String,
    pub status: NotificationDeliveryStatus,
    pub attempt_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListNotificationDeliveriesResponse {
    pub deliveries: Vec<NotificationDelivery>,
    pub total: i64,
}

// ── Release domain types ────────────────────────────────────────

/// A store health metric shown on Home.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StoreVitalKind {
    /// Google Play user-perceived crash rate, 0..1.
    CrashRate,
    /// Google Play user-perceived ANR rate, 0..1.
    AnrRate,
    /// App Store hang rate, in the unit Apple reports (seconds of hang per hour).
    HangRate,
    /// App Store time to first frame at launch, in the unit Apple reports.
    LaunchTime,
}

/// One point of a store metric: a day for Google Play, an app version for the App Store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct StoreVitalPoint {
    /// `YYYY-MM-DD` for Google Play, the app version for the App Store.
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct StoreVital {
    pub kind: StoreVitalKind,
    /// Headline value: Play's 28-day user-weighted rate, or the latest App Store version's value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Unit as the store names it; `ratio` for Play rates.
    pub unit: String,
    /// Where the store starts treating the value as bad: Play's bad-behavior
    /// threshold, or Apple's recommended goal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// Oldest first.
    pub points: Vec<StoreVitalPoint>,
}

/// A grouped crash or ANR reported by the store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct StoreErrorIssue {
    /// `crash` or `anr`.
    pub kind: String,
    pub cause: String,
    pub location: String,
    pub reports: i64,
    pub users: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reported_at: Option<i64>,
    /// The issue in the store's console.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Store health of one app: a project shipped to one store under one package name or bundle id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct StoreAppVitals {
    pub project_id: String,
    pub project_name: String,
    pub store: ReleaseStore,
    /// Package name on Google Play, bundle id on the App Store.
    pub app_identifier: String,
    pub vitals: Vec<StoreVital>,
    /// Most-reported issues of the last 28 days. Google Play only.
    pub issues: Vec<StoreErrorIssue>,
    /// When Oore last read the store; absent until the first read finishes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<i64>,
    /// Why the last read failed, such as missing credentials or access.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct StoreVitalsResponse {
    pub apps: Vec<StoreAppVitals>,
}

/// Store a release is published to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStore {
    GooglePlay,
    AppStore,
}

impl ReleaseStore {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GooglePlay => "google_play",
            Self::AppStore => "app_store",
        }
    }

    /// Build platform the store distributes.
    pub fn platform(self) -> &'static str {
        match self {
            Self::GooglePlay => "android",
            Self::AppStore => "ios",
        }
    }
}

impl fmt::Display for ReleaseStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReleaseStore {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "google_play" => Ok(Self::GooglePlay),
            "app_store" => Ok(Self::AppStore),
            other => Err(format!("unknown release store: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    Uploading,
    Processing,
    InReview,
    Rolling,
    Paused,
    Halted,
    Complete,
    Failed,
}

impl ReleaseState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uploading => "uploading",
            Self::Processing => "processing",
            Self::InReview => "in_review",
            Self::Rolling => "rolling",
            Self::Paused => "paused",
            Self::Halted => "halted",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed)
    }

    /// States this state can move to. `Rolling -> Rolling` is a fraction change.
    /// A halted release only leaves through an explicit resume (or failure).
    pub fn valid_transitions(self) -> &'static [ReleaseState] {
        match self {
            Self::Uploading => &[
                Self::Processing,
                Self::InReview,
                Self::Rolling,
                Self::Failed,
            ],
            Self::Processing => &[Self::InReview, Self::Rolling, Self::Failed],
            Self::InReview => &[Self::Rolling, Self::Failed],
            Self::Rolling => &[
                Self::Rolling,
                Self::Paused,
                Self::Halted,
                Self::Complete,
                Self::Failed,
            ],
            Self::Paused => &[Self::Rolling, Self::Halted, Self::Complete, Self::Failed],
            Self::Halted => &[Self::Rolling, Self::Failed],
            Self::Complete | Self::Failed => &[],
        }
    }

    pub fn can_transition_to(self, target: ReleaseState) -> bool {
        self.valid_transitions().contains(&target)
    }
}

impl fmt::Display for ReleaseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReleaseState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "uploading" => Ok(Self::Uploading),
            "processing" => Ok(Self::Processing),
            "in_review" => Ok(Self::InReview),
            "rolling" => Ok(Self::Rolling),
            "paused" => Ok(Self::Paused),
            "halted" => Ok(Self::Halted),
            "complete" => Ok(Self::Complete),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown release state: {other}")),
        }
    }
}

/// Manual controls on a release.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseAction {
    Raise,
    Pause,
    Resume,
    Halt,
    Complete,
    Approve,
}

impl ReleaseAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raise => "raise",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Halt => "halt",
            Self::Complete => "complete",
            Self::Approve => "approve",
        }
    }
}

impl FromStr for ReleaseAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "raise" => Ok(Self::Raise),
            "pause" => Ok(Self::Pause),
            "resume" => Ok(Self::Resume),
            "halt" => Ok(Self::Halt),
            "complete" => Ok(Self::Complete),
            "approve" => Ok(Self::Approve),
            other => Err(format!("unknown release action: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseHealth {
    Unknown,
    Healthy,
    Warning,
    Breached,
}

impl ReleaseHealth {
    /// Warning starts at three quarters of the threshold.
    pub fn from_crash_rate(crash_rate: Option<f64>, threshold: Option<f64>) -> Self {
        match (crash_rate, threshold) {
            (Some(rate), Some(limit)) if rate >= limit => Self::Breached,
            (Some(rate), Some(limit)) if rate >= limit * 0.75 => Self::Warning,
            (Some(_), Some(_)) => Self::Healthy,
            _ => Self::Unknown,
        }
    }
}

/// Where a maintainer approval is required, if anywhere.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseApprovalGate {
    #[default]
    None,
    /// Not supported: rejected when a policy is saved. Use the pipeline's
    /// build approval to hold a build before it uploads.
    BeforeUpload,
    /// Approval before a `production` track release starts rolling out.
    BeforeProduction,
}

/// Staged rollout policy stored with each release.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ReleasePolicy {
    /// Ascending rollout fractions in (0, 1]; the last step must be 1.
    #[serde(default = "default_release_steps")]
    pub steps: Vec<f64>,
    /// Hours between automatic steps.
    #[serde(default = "default_release_interval_hours")]
    pub interval_hours: f64,
    /// Crash rate (0..1) at or above which the daemon halts the rollout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crash_rate_threshold: Option<f64>,
    #[serde(default)]
    pub approval: ReleaseApprovalGate,
}

fn default_release_steps() -> Vec<f64> {
    vec![0.01, 0.05, 0.2, 0.5, 1.0]
}

fn default_release_interval_hours() -> f64 {
    24.0
}

impl Default for ReleasePolicy {
    fn default() -> Self {
        Self {
            steps: default_release_steps(),
            interval_hours: default_release_interval_hours(),
            crash_rate_threshold: None,
            approval: ReleaseApprovalGate::None,
        }
    }
}

impl ReleasePolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.steps.is_empty() {
            return Err("policy.steps must not be empty".into());
        }
        if self
            .steps
            .iter()
            .any(|step| !step.is_finite() || *step <= 0.0 || *step > 1.0)
        {
            return Err("policy.steps must be fractions in (0, 1]".into());
        }
        if self.steps.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("policy.steps must be strictly ascending".into());
        }
        if self.steps.last() != Some(&1.0) {
            return Err("policy.steps must end at 1".into());
        }
        if !self.interval_hours.is_finite() || self.interval_hours <= 0.0 {
            return Err("policy.interval_hours must be greater than zero".into());
        }
        if self
            .crash_rate_threshold
            .is_some_and(|limit| !limit.is_finite() || limit <= 0.0 || limit > 1.0)
        {
            return Err("policy.crash_rate_threshold must be a fraction in (0, 1]".into());
        }
        Ok(())
    }

    /// Rejects approval gates Oore cannot enforce. The runner uploads before
    /// a release exists, so nothing could hold a `before_upload` release;
    /// gate the build with the pipeline's approval policy instead. Saved
    /// policies that already use it still load.
    pub fn validate_gate(&self) -> Result<(), String> {
        match self.approval {
            ReleaseApprovalGate::BeforeUpload => Err(
                "policy.approval before_upload is not supported; require build approval on the pipeline or use before_production".into(),
            ),
            ReleaseApprovalGate::None | ReleaseApprovalGate::BeforeProduction => Ok(()),
        }
    }

    /// First policy step above `fraction`.
    pub fn next_step(&self, fraction: f64) -> Option<f64> {
        self.steps.iter().copied().find(|step| *step > fraction)
    }

    pub fn interval_secs(&self) -> i64 {
        (self.interval_hours * 3600.0).round() as i64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Release {
    pub id: String,
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    pub build_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// `android` or `ios`, derived from the store.
    pub platform: String,
    pub store: ReleaseStore,
    /// Play track, or TestFlight group / `app_store`.
    pub track: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_code: Option<i64>,
    pub state: ReleaseState,
    /// Fraction of users, 0..1.
    pub rollout_fraction: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_fraction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step_at: Option<i64>,
    pub policy: ReleasePolicy,
    pub health: ReleaseHealth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash_rate_threshold: Option<f64>,
    pub awaiting_approval: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_email: Option<String>,
    /// Actions the caller may take now, from their project role and the state.
    pub allowed_actions: Vec<ReleaseAction>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReleaseHistoryEntry {
    pub id: String,
    pub at: i64,
    /// Null when the daemon or a store made the change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_email: Option<String>,
    /// Null for the entry that created the release.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_state: Option<ReleaseState>,
    pub to_state: ReleaseState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_fraction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_fraction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A history entry with the release it belongs to, for activity feeds.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReleaseActivityEvent {
    #[serde(flatten)]
    pub entry: ReleaseHistoryEntry,
    pub release_id: String,
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    pub build_id: String,
    pub store: ReleaseStore,
    pub track: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListReleasesResponse {
    pub releases: Vec<Release>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReleaseDetailResponse {
    pub release: Release,
    pub history: Vec<ReleaseHistoryEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListReleaseActivityResponse {
    pub events: Vec<ReleaseActivityEvent>,
}

#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct ReleaseActionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Raise only: target fraction (0..1]. Defaults to the next policy step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fraction: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateReleaseRequest {
    pub build_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    pub store: ReleaseStore,
    pub track: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_code: Option<i64>,
    /// Initial state; defaults to `rolling`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ReleaseState>,
    /// Initial fraction; defaults to the first policy step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_fraction: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<ReleasePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── API Tokens ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateApiTokenRequest {
    pub name: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateApiTokenResponse {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub role: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    /// The plaintext token. Only returned on creation, never again.
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiTokenSummary {
    pub id: String,
    pub name: String,
    pub prefix: String,
    #[schema(value_type = UserRole)]
    pub role: String,
    pub created_by: String,
    pub created_by_email: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub is_expired: bool,
    pub is_revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListApiTokensResponse {
    pub tokens: Vec<ApiTokenSummary>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevokeApiTokenResponse {
    pub revoked: bool,
}

#[cfg(test)]
mod tests {
    use super::{ApprovalPolicy, TriggerConfig};

    fn triggers(json: &str) -> TriggerConfig {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn branch_and_tag_filters_follow_the_ref_kind() {
        let any = triggers("{}");
        assert!(any.should_trigger("push", Some("main"), None));
        assert!(any.should_trigger("tag_push", None, Some("v1")));

        let branches_only = triggers(r#"{"branches":["main"]}"#);
        assert!(branches_only.should_trigger("push", Some("main"), None));
        assert!(!branches_only.should_trigger("tag_push", None, Some("v1")));

        let tags_only = triggers(r#"{"tags":["v*"]}"#);
        assert!(tags_only.should_trigger("tag_push", None, Some("v1.2.0")));
        assert!(!tags_only.should_trigger("tag_push", None, Some("nightly")));
        assert!(!tags_only.should_trigger("push", Some("main"), None));

        let gitlab = triggers(r#"{"events":["tag_push"]}"#);
        assert!(gitlab.should_trigger("Tag Push Hook", None, Some("v1")));
        assert!(!gitlab.should_trigger("Push Hook", Some("main"), None));
    }

    #[test]
    fn path_filters_need_a_relevant_change() {
        let config = triggers(r#"{"paths":["lib/","android/**"],"paths_ignore":["*.md"]}"#);
        let files = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        assert!(config.paths_match(Some(&files(&["lib/src/app.dart"]))));
        assert!(config.paths_match(Some(&files(&["docs/a.txt", "android/app/build.gradle"]))));
        assert!(!config.paths_match(Some(&files(&["docs/a.txt"]))));
        assert!(!config.paths_match(Some(&files(&["lib/README.md"]))));
        assert!(config.paths_match(None));

        let ignore_only = triggers(r#"{"paths_ignore":["docs/**"]}"#);
        assert!(!ignore_only.paths_match(Some(&files(&["docs/setup.md"]))));
        assert!(ignore_only.paths_match(Some(&files(&["docs/setup.md", "lib/main.dart"]))));
    }

    #[test]
    fn approval_policy_matches_listed_events() {
        let tags: ApprovalPolicy =
            serde_json::from_str(r#"{"required":true,"events":["tag_push"]}"#).unwrap();
        assert!(tags.applies_to("tag_push"));
        assert!(!tags.applies_to("push"));
        let all: ApprovalPolicy = serde_json::from_str(r#"{"required":true}"#).unwrap();
        assert!(all.applies_to("manual"));
        assert!(!ApprovalPolicy::default().applies_to("tag_push"));
    }

    #[test]
    fn vm_image_config_accepts_packages_and_rejects_injection() {
        let config = super::parse_vm_image_yaml(
            "version: 1\nbase: ghcr.io/acme/xcode:26.5\nandroid_packages: ['platforms;android-36']\nbrew: [graphicsmagick]\nsetup: ['gem install bundler']\n",
        )
        .unwrap();
        assert_eq!(config.base.as_deref(), Some("ghcr.io/acme/xcode:26.5"));
        assert_eq!(config.android_packages, vec!["platforms;android-36"]);
        assert!(super::parse_vm_image_yaml("version: 1\nbrew: ['jq; rm -rf /']\n").is_err());
        assert!(super::parse_vm_image_yaml("version: 2\n").is_err());
        assert!(super::parse_vm_image_yaml("version: 1\nflutter: 3.47.7\n").is_err());
    }

    #[test]
    fn runner_labels_come_from_capabilities_and_operators() {
        let capabilities = serde_json::json!({
            "os": "macos",
            "os_version": "15.3.1",
            "arch": "aarch64",
            "xcode_version": "Xcode 16.2",
        });
        assert_eq!(
            super::detected_runner_labels(&capabilities),
            ["arm64", "macos", "macos-15", "xcode-16"]
        );
        assert_eq!(
            super::effective_runner_labels(&capabilities, &["gpu".into(), "macos".into()]),
            ["arm64", "gpu", "macos", "macos-15", "xcode-16"]
        );
        assert_eq!(
            super::detected_runner_labels(&serde_json::json!({"os": "linux", "arch": "x86_64"})),
            ["linux", "x86_64"]
        );
        assert_eq!(
            super::normalize_runner_labels(&[" Linux ".into(), "linux".into()]).unwrap(),
            ["linux"]
        );
        assert!(super::normalize_runner_labels(&["has space".into()]).is_err());
    }

    #[test]
    fn runs_on_accepts_one_label_or_a_list() {
        let parse = |runs_on: &str| {
            super::parse_repository_pipeline_yaml(&format!(
                "version: 1\nplatforms: [android]\n{runs_on}"
            ))
            .map(|config| config.runs_on)
        };
        assert_eq!(parse("runs_on: Linux").unwrap(), ["linux"]);
        assert_eq!(
            parse("runs_on: [xcode-16, macos]").unwrap(),
            ["macos", "xcode-16"]
        );
        assert!(parse("").unwrap().is_empty());
        assert!(
            parse("runs_on: [\"bad label\"]")
                .unwrap_err()
                .contains("runs_on")
        );
        assert_eq!(super::scheduling_labels(&[]), ["macos"]);
    }

    #[test]
    fn google_play_publish_config_validation() {
        let parse = |value: serde_json::Value| -> super::GooglePlayPublishConfig {
            serde_json::from_value(value).unwrap()
        };
        let base = serde_json::json!({
            "enabled": true,
            "service_account_secret": "PLAY_SERVICE_ACCOUNT",
            "track": "production",
            "rollout_percent": 5.0,
        });
        let config = parse(base.clone());
        assert!(super::validate_google_play_publish_config(&config).is_empty());
        assert_eq!(config.release_notes_locale, "en-US");
        assert!(config.upload_mapping);

        for (patch, expected) in [
            (
                serde_json::json!({"service_account_secret": "bad name"}),
                "service_account_secret",
            ),
            (serde_json::json!({"track": "internal"}), "internal track"),
            (
                serde_json::json!({"rollout_percent": 0.0}),
                "rollout_percent",
            ),
            (serde_json::json!({"draft": true}), "draft"),
            (
                serde_json::json!({"package_name": "nodots"}),
                "package_name",
            ),
            (
                serde_json::json!({"release_notes": "repo_files"}),
                "release_notes_dir",
            ),
            (
                serde_json::json!({"in_app_update_priority": 6}),
                "in_app_update_priority",
            ),
            (
                serde_json::json!({"rollout_policy": {"approval": "before_upload"}}),
                "before_upload is not supported",
            ),
        ] {
            let mut value = base.clone();
            for (key, field) in patch.as_object().unwrap() {
                value[key] = field.clone();
            }
            let errors = super::validate_google_play_publish_config(&parse(value));
            assert!(
                errors.iter().any(|error| error.contains(expected)),
                "{expected}: {errors:?}"
            );
        }
        assert!(
            serde_json::from_value::<super::GooglePlayPublishConfig>(serde_json::json!({
                "service_account_secret": "X", "track": "internal", "unknown": 1
            }))
            .is_err()
        );
    }

    #[test]
    fn secret_references_parse_and_reject_other_expressions() {
        let text = "a ${{ secrets.ONE }} b ${{secrets.TWO}}";
        let names: Vec<String> = super::parse_secret_references(text)
            .unwrap()
            .into_iter()
            .map(|reference| reference.name)
            .collect();
        assert_eq!(names, vec!["ONE", "TWO"]);
        assert_eq!(
            super::replace_secret_references(text, |name| name.to_lowercase()).unwrap(),
            "a one b two"
        );
        assert_eq!(
            super::replace_secret_references("echo $HOME ${PATH}", |_| unreachable!()).unwrap(),
            "echo $HOME ${PATH}"
        );
        assert!(super::parse_secret_references("${{ env.X }}").is_err());
        assert!(super::parse_secret_references("${{ secrets.1BAD }}").is_err());
        assert!(super::parse_secret_references("${{ secrets.X").is_err());
        assert!(
            super::parse_repository_pipeline_yaml(
                "version: 1\nplatforms: [android]\ncommands:\n  build: ['echo ${{ vars.X }}']\n"
            )
            .unwrap_err()
            .contains("commands.build[0]")
        );
    }
    use super::{MAX_STORE_VERSION_CODE, parse_repository_pipeline_yaml, store_version_code};

    #[test]
    fn repository_config_reads_build_number_offset() {
        let config = parse_repository_pipeline_yaml(
            "version: 1\nplatforms: [android]\nbuild_number_offset: 4000\n",
        )
        .unwrap();
        assert_eq!(config.build_number_offset, 4000);

        let error = parse_repository_pipeline_yaml(
            "version: 1\nplatforms: [android]\nbuild_number_offset: -1\n",
        )
        .unwrap_err();
        assert!(error.starts_with("build_number_offset must be between 0 and"));
    }

    #[test]
    fn store_version_code_adds_offset_within_play_range() {
        assert_eq!(store_version_code(12, 0), Ok(12));
        assert_eq!(store_version_code(12, 4000), Ok(4012));
        assert_eq!(
            store_version_code(1, MAX_STORE_VERSION_CODE - 1),
            Ok(MAX_STORE_VERSION_CODE)
        );
        let error = store_version_code(2, MAX_STORE_VERSION_CODE - 1).unwrap_err();
        assert!(error.contains("outside the store range"), "{error}");
        assert!(store_version_code(1, -5).is_err());
    }

    #[test]
    fn repository_environment_keys_keep_shell_identifier_rules() {
        for (key, valid) in [
            ("NAME", true),
            ("_", true),
            ("foo_9", true),
            (" trimmed ", true),
            ("", false),
            ("9NAME", false),
            ("na-me", false),
            ("with space", false),
            ("名字", false),
            ("A\nB", false),
        ] {
            let key = serde_json::to_string(key).unwrap();
            let yaml = format!(
                "version: 1\nplatforms: [android]\nenv:\n  - key: {key}\n    value: test\n"
            );
            let result = parse_repository_pipeline_yaml(&yaml);
            assert_eq!(result.is_ok(), valid, "{key}");
            if !valid {
                assert_eq!(
                    result.unwrap_err(),
                    "env[0].key must match [A-Za-z_][A-Za-z0-9_]*"
                );
            }
        }
    }
}
