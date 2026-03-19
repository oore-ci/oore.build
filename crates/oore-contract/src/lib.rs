use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OidcConfigureRequest {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
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
    #[serde(default)]
    pub trusted_proxy_cidrs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetupTrustedProxyConfigureResponse {
    pub state: SetupState,
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
    pub owner_email: Option<String>,
}

// ── Structured API error ────────────────────────────────────────

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
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
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LogoutResponse {
    pub ok: bool,
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
    pub role: String,
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
    pub auth_mode: String,
    pub webhook_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
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
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IntegrationDetailResponse {
    pub integration: Integration,
    pub installation_count: i64,
    pub repository_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_webhook_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListInstallationsResponse {
    pub installations: Vec<IntegrationInstallation>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListRepositoriesResponse {
    pub repositories: Vec<IntegrationRepository>,
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
            Self::Scheduled => &[Self::Assigned, Self::Canceled, Self::Expired],
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
    pub cancel_previous: bool,
    #[serde(default)]
    pub max_concurrent: Option<u32>,
}

// ── Trigger config ──────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct TriggerConfig {
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
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
    pub status: String,
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
    pub source_build_id: Option<String>,
    #[schema(value_type = Object)]
    pub config_snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_ref: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateBuildResponse {
    pub build: Build,
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
    pub artifact_type: String,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub checksum: Option<String>,
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
    pub created_at: i64,
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
pub struct ListArtifactsResponse {
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ArtifactDownloadLinkResponse {
    pub download_url: String,
    pub expires_at: i64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InstancePreferences {
    pub key_storage_mode: KeyStorageMode,
    pub runtime_mode: RuntimeMode,
    pub remote_auth_mode: RemoteAuthMode,
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
    #[schema(value_type = Object)]
    pub settings: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_user_role: Option<String>,
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
pub struct ListProjectMembersResponse {
    pub members: Vec<ProjectMember>,
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
pub struct PipelineCommandStages {
    #[serde(default)]
    pub pre_build: Vec<String>,
    #[serde(default)]
    pub build: Vec<String>,
    #[serde(default)]
    pub post_build: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct PlatformBuildArgs {
    #[serde(default)]
    pub android: Vec<String>,
    #[serde(default)]
    pub ios: Vec<String>,
    #[serde(default)]
    pub macos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct PlatformBuildCommands {
    #[serde(default)]
    pub android: Option<String>,
    #[serde(default)]
    pub ios: Option<String>,
    #[serde(default)]
    pub macos: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineEnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineExecutionConfig {
    #[serde(default = "default_platforms")]
    pub platforms: Vec<BuildPlatform>,
    #[serde(default)]
    pub flutter_version: Option<String>,
    #[serde(default)]
    pub commands: PipelineCommandStages,
    #[serde(default)]
    pub platform_build_args: PlatformBuildArgs,
    #[serde(default)]
    pub platform_commands: PlatformBuildCommands,
    #[serde(default)]
    pub env: Vec<PipelineEnvVar>,
    #[serde(default = "default_artifact_patterns")]
    pub artifact_patterns: Vec<String>,
}

fn default_platforms() -> Vec<BuildPlatform> {
    vec![BuildPlatform::Android]
}

fn default_artifact_patterns() -> Vec<String> {
    vec!["*.apk".to_string()]
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
    pub config_path_explicit: bool,
    #[serde(default)]
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
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ValidatePipelineResponse {
    pub valid: bool,
    pub errors: Vec<String>,
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
    pub synced_bundle_ids: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

// ── Build log types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildLogChunk {
    pub sequence: i64,
    pub content: String,
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
    pub keep_statuses: Vec<String>,
    pub dry_run: bool,
    pub cleanup_interval_secs: i64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_config_empty_matches_everything() {
        let tc = TriggerConfig::default();
        assert!(tc.should_trigger("push", Some("main")));
        assert!(tc.should_trigger("pull_request", Some("feature/x")));
        assert!(tc.should_trigger("Push Hook", None));
    }

    #[test]
    fn trigger_config_event_filter_github() {
        let tc = TriggerConfig {
            events: vec!["push".to_string()],
            branches: Vec::new(),
        };
        assert!(tc.should_trigger("push", Some("main")));
        assert!(!tc.should_trigger("pull_request", Some("main")));
    }

    #[test]
    fn trigger_config_event_filter_gitlab_normalization() {
        let tc = TriggerConfig {
            events: vec!["push".to_string(), "pull_request".to_string()],
            branches: Vec::new(),
        };
        assert!(tc.should_trigger("Push Hook", Some("main")));
        assert!(tc.should_trigger("Merge Request Hook", Some("feature")));
        assert!(!tc.should_trigger("Tag Push Hook", Some("v1.0")));
    }

    #[test]
    fn trigger_config_exact_branch() {
        let tc = TriggerConfig {
            events: Vec::new(),
            branches: vec!["main".to_string()],
        };
        assert!(tc.should_trigger("push", Some("main")));
        assert!(!tc.should_trigger("push", Some("develop")));
    }

    #[test]
    fn trigger_config_glob_star() {
        let tc = TriggerConfig {
            events: Vec::new(),
            branches: vec!["release/*".to_string()],
        };
        assert!(tc.should_trigger("push", Some("release/1.0")));
        assert!(tc.should_trigger("push", Some("release/2.0.1")));
        assert!(!tc.should_trigger("push", Some("main")));
    }

    #[test]
    fn trigger_config_glob_question_mark() {
        let tc = TriggerConfig {
            events: Vec::new(),
            branches: vec!["release-?.x".to_string()],
        };
        assert!(tc.should_trigger("push", Some("release-1.x")));
        assert!(tc.should_trigger("push", Some("release-3.x")));
        assert!(!tc.should_trigger("push", Some("release-10.x")));
    }

    #[test]
    fn trigger_config_combined_filters() {
        let tc = TriggerConfig {
            events: vec!["push".to_string()],
            branches: vec!["main".to_string(), "release/*".to_string()],
        };
        assert!(tc.should_trigger("push", Some("main")));
        assert!(tc.should_trigger("push", Some("release/1.0")));
        assert!(!tc.should_trigger("pull_request", Some("main")));
        assert!(!tc.should_trigger("push", Some("develop")));
    }

    #[test]
    fn trigger_config_none_branch_rejected_by_filter() {
        let tc = TriggerConfig {
            events: Vec::new(),
            branches: vec!["main".to_string()],
        };
        assert!(!tc.should_trigger("push", None));
    }

    #[test]
    fn pipeline_execution_config_default_is_android_fallback() {
        let cfg = PipelineExecutionConfig::default();
        assert_eq!(cfg.platforms, vec![BuildPlatform::Android]);
        assert!(cfg.flutter_version.is_none());
        assert!(cfg.commands.pre_build.is_empty());
        assert!(cfg.commands.build.is_empty());
        assert!(cfg.commands.post_build.is_empty());
        assert!(cfg.platform_build_args.android.is_empty());
        assert!(cfg.platform_commands.android.is_none());
        assert!(cfg.env.is_empty());
        assert_eq!(cfg.artifact_patterns, vec!["*.apk".to_string()]);
    }

    #[test]
    fn build_platform_round_trip_json() {
        let json = serde_json::to_string(&BuildPlatform::Ios).expect("serialize");
        assert_eq!(json, "\"ios\"");
        let parsed: BuildPlatform = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, BuildPlatform::Ios);
    }

    #[test]
    fn pipeline_execution_config_supports_env_and_platform_overrides() {
        let cfg = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android, BuildPlatform::Ios],
            flutter_version: Some("3.24.0".to_string()),
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs {
                android: vec!["--flavor=dev".to_string()],
                ios: vec!["--dart-define-from-file=config/dev.json".to_string()],
                macos: Vec::new(),
            },
            platform_commands: PlatformBuildCommands {
                android: Some("flutter build appbundle --release".to_string()),
                ios: None,
                macos: None,
            },
            env: vec![PipelineEnvVar {
                key: "PROJECT_BUILD_NUMBER".to_string(),
                value: "42".to_string(),
            }],
            artifact_patterns: vec!["*.apk".to_string()],
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let parsed: PipelineExecutionConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.flutter_version.as_deref(), Some("3.24.0"));
        assert_eq!(parsed.platform_build_args.android.len(), 1);
        assert_eq!(
            parsed.platform_commands.android.as_deref(),
            Some("flutter build appbundle --release")
        );
        assert_eq!(parsed.env[0].key, "PROJECT_BUILD_NUMBER");
    }

    #[test]
    fn artifact_storage_provider_round_trip_json() {
        let json = serde_json::to_string(&ArtifactStorageProvider::R2).expect("serialize");
        assert_eq!(json, "\"r2\"");
        let parsed: ArtifactStorageProvider = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, ArtifactStorageProvider::R2);
    }

    #[test]
    fn key_storage_mode_round_trip_json() {
        let json = serde_json::to_string(&KeyStorageMode::Keychain).expect("serialize");
        assert_eq!(json, "\"keychain\"");
        let parsed: KeyStorageMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, KeyStorageMode::Keychain);
    }

    #[test]
    fn runtime_mode_round_trip_json() {
        let json = serde_json::to_string(&RuntimeMode::Local).expect("serialize");
        assert_eq!(json, "\"local\"");
        let parsed: RuntimeMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, RuntimeMode::Local);
    }

    #[test]
    fn remote_auth_mode_round_trip_json() {
        let json = serde_json::to_string(&RemoteAuthMode::TrustedProxy).expect("serialize");
        assert_eq!(json, "\"trusted_proxy\"");
        let parsed: RemoteAuthMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, RemoteAuthMode::TrustedProxy);
    }

    #[test]
    fn external_access_preflight_response_round_trip_json() {
        let response = ExternalAccessPreflightResponse {
            ready: false,
            checks: vec![ExternalAccessPreflightCheck {
                id: "public_url_https".to_string(),
                label: "Public URL uses HTTPS".to_string(),
                ok: false,
                message: "OORE_PUBLIC_URL must use https for External Access".to_string(),
                failure_code: Some("external_access_https_required".to_string()),
            }],
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let parsed: ExternalAccessPreflightResponse =
            serde_json::from_str(&json).expect("deserialize");
        assert!(!parsed.ready);
        assert_eq!(parsed.checks.len(), 1);
        assert_eq!(parsed.checks[0].id, "public_url_https");
        assert_eq!(
            parsed.checks[0].failure_code.as_deref(),
            Some("external_access_https_required")
        );
    }

    #[test]
    fn android_signing_profile_input_round_trip_json() {
        let request = UpdatePipelineAndroidSigningRequest {
            debug: Some(AndroidSigningProfileInput {
                enabled: true,
                keystore_filename: Some("debug.jks".to_string()),
                keystore_base64: Some("ZmFrZQ==".to_string()),
                store_password: Some("store-pass".to_string()),
                key_alias: Some("debugAlias".to_string()),
                key_password: Some("key-pass".to_string()),
            }),
            release: None,
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: UpdatePipelineAndroidSigningRequest =
            serde_json::from_str(&json).expect("deserialize");
        let debug = parsed.debug.expect("debug profile");
        assert!(debug.enabled);
        assert_eq!(debug.keystore_filename.as_deref(), Some("debug.jks"));
        assert_eq!(debug.key_alias.as_deref(), Some("debugAlias"));
    }

    #[test]
    fn ios_signing_request_round_trip_json() {
        let request = UpdatePipelineIosSigningRequest {
            enabled: true,
            mode: IosSigningMode::Hybrid,
            team_id: Some("TEAM1234".to_string()),
            bundle_ids: vec!["com.example.app".to_string()],
            certificate: Some(IosCertificateInput {
                p12_filename: Some("dist.p12".to_string()),
                p12_base64: Some("ZmFrZS1wMTI=".to_string()),
                p12_password: Some("secret-pass".to_string()),
            }),
            provisioning_profiles: vec![IosProvisioningProfileInput {
                bundle_id: "com.example.app".to_string(),
                profile_filename: Some("app.mobileprovision".to_string()),
                profile_base64: Some("ZmFrZS1wcm9maWxl".to_string()),
            }],
            api_credentials: Some(IosApiCredentialInput {
                key_id: Some("ABC123XYZ".to_string()),
                issuer_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
                private_key_base64: Some("ZmFrZS1wOA==".to_string()),
            }),
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: UpdatePipelineIosSigningRequest =
            serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.enabled);
        assert_eq!(parsed.mode, IosSigningMode::Hybrid);
        assert_eq!(parsed.team_id.as_deref(), Some("TEAM1234"));
        assert_eq!(parsed.bundle_ids, vec!["com.example.app".to_string()]);
        assert_eq!(parsed.provisioning_profiles.len(), 1);
        let api = parsed.api_credentials.expect("api credentials");
        assert_eq!(api.key_id.as_deref(), Some("ABC123XYZ"));
    }
}

// ── Notification channel types ──────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannelType {
    Webhook,
    Mattermost,
}

impl fmt::Display for NotificationChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Webhook => f.write_str("webhook"),
            Self::Mattermost => f.write_str("mattermost"),
        }
    }
}

impl FromStr for NotificationChannelType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "webhook" => Ok(Self::Webhook),
            "mattermost" => Ok(Self::Mattermost),
            other => Err(format!("unknown notification channel type: {other}")),
        }
    }
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
    /// Which terminal build statuses trigger this channel. Empty means all.
    #[serde(default)]
    pub events: Vec<String>,
    /// True if the channel has a URL configured (the actual URL is never exposed).
    pub has_url: bool,
    /// True if the channel has an HMAC secret configured (webhook only).
    pub has_secret: bool,
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
    /// Terminal build statuses to notify on. Empty or omitted means all terminal events.
    #[serde(default)]
    pub events: Vec<String>,
    /// Webhook/Mattermost incoming webhook URL (required).
    pub url: String,
    /// HMAC secret for signing webhook payloads (webhook only, optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
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
    /// New URL. Omit to keep existing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// New HMAC secret. Omit to keep existing; pass empty string to clear.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
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
    pub build_id: String,
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
