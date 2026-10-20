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
    #[serde(default)]
    #[schema(required = true)]
    pub branches: Vec<String>,
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
    /// Empty `events` list means all events match. Empty `branches` list
    /// means all branches match. `None` branch with a non-empty branch
    /// filter results in rejection.
    pub fn should_trigger(&self, event_type: &str, branch: Option<&str>) -> bool {
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

        // Check branch filter
        if !self.branches.is_empty() {
            match branch {
                None => return false,
                Some(b) => {
                    if !self.branches.iter().any(|pat| Self::glob_match(pat, b)) {
                        return false;
                    }
                }
            }
        }

        true
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
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListBuildsResponse {
    pub builds: Vec<Build>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CancelBuildResponse {
    pub build: Build,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RerunBuildResponse {
    pub build: Build,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Runner API types ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRunnerRequest {
    pub name: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub capabilities: serde_json::Value,
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
}

pub const RUNNER_PROTOCOL_VERSION: u32 = 5;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ClaimJobRequest {
    pub protocol_version: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateRunnerRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListRunnersResponse {
    pub runners: Vec<Runner>,
    pub total: i64,
    pub online_total: i64,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryArtifacts {
    #[serde(default)]
    patterns: Vec<String>,
}

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
    };
    let errors = validate_repository_execution_config(&config);
    if errors.is_empty() {
        Ok(config)
    } else {
        Err(errors.join("\n"))
    }
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
}

/// Publish work for the runner, fetched with the job-scoped grant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunnerPublishPlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    /// Present only when Google Play publishing is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_play: Option<GooglePlayPublishConfig>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublishStore {
    GooglePlay,
    AppStore,
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
    pub export_method: String,
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
    pub has_api_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_issuer_id: Option<String>,
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
    pub export_method: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
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
    use super::parse_repository_pipeline_yaml;

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
