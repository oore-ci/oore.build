use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ── Setup state machine ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupStatus {
    pub instance_id: String,
    pub state: SetupState,
    pub setup_mode: bool,
    pub is_configured: bool,
}

impl SetupStatus {
    pub fn from_state(instance_id: impl Into<String>, state: SetupState) -> Self {
        let is_configured = state == SetupState::Ready;
        let setup_mode = !is_configured;

        Self {
            instance_id: instance_id.into(),
            state,
            setup_mode,
            is_configured,
        }
    }
}

// ── API request/response types ──────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct BootstrapTokenVerifyRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BootstrapTokenVerifyResponse {
    pub session_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OidcConfigureRequest {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OidcConfigureResponse {
    pub state: SetupState,
    pub discovered_issuer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupOidcStartRequest {
    pub redirect_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupOidcStartResponse {
    pub authorization_url: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupOidcVerifyRequest {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupOidcVerifyResponse {
    pub state: SetupState,
    pub owner_email: String,
    pub oidc_subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupCompleteResponse {
    pub state: SetupState,
    pub instance_id: String,
}

// ── Structured API error ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapTokenRecord {
    pub hash: String,
    pub expires_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupSessionRecord {
    pub hash: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcSecretRecord {
    pub encrypted_client_secret: String,
    pub stored_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerRecord {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc_subject: Option<String>,
    pub created_at: i64,
}

// ── Auth response types ─────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct OidcStartResponse {
    pub authorization_url: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OidcCallbackResponse {
    pub session_token: String,
    pub expires_at: i64,
    pub user: AuthenticatedUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct LogoutResponse {
    pub ok: bool,
}

// ── User management types ───────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct InviteUserRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InviteUserResponse {
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserRoleRequest {
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserRoleResponse {
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReEnableUserResponse {
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListUsersResponse {
    pub users: Vec<User>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserProfileResponse {
    pub user: User,
}

// ── SCM Integration types ──────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScmProvider {
    Github,
    Gitlab,
}

impl fmt::Display for ScmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github => f.write_str("github"),
            Self::Gitlab => f.write_str("gitlab"),
        }
    }
}

impl FromStr for ScmProvider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            other => Err(format!("unknown SCM provider: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationAuthMode {
    GithubApp,
    OauthApp,
    PersonalToken,
}

impl fmt::Display for IntegrationAuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GithubApp => f.write_str("github_app"),
            Self::OauthApp => f.write_str("oauth_app"),
            Self::PersonalToken => f.write_str("personal_token"),
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
            other => Err(format!("unknown integration auth mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationInstallation {
    pub id: String,
    pub integration_id: String,
    pub external_id: String,
    pub account_name: String,
    pub account_type: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubAppStartRequest {
    pub webhook_url: String,
    /// Frontend URL to redirect to after GitHub App creation completes.
    pub redirect_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubAppStartResponse {
    /// URL to navigate the browser to — serves an auto-submitting form that POSTs the manifest to GitHub.
    pub create_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubAppCompleteRequest {
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubAppCompleteResponse {
    pub integration: Integration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncInstallationsRequest {
    pub installation_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncInstallationsResponse {
    pub installations: Vec<IntegrationInstallation>,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct GitLabCompleteResponse {
    pub integration: Integration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitLabAuthorizeRequest {
    pub integration_id: String,
    pub redirect_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitLabAuthorizeResponse {
    pub authorize_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListIntegrationsResponse {
    pub integrations: Vec<Integration>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntegrationDetailResponse {
    pub integration: Integration,
    pub installation_count: i64,
    pub repository_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_webhook_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListInstallationsResponse {
    pub installations: Vec<IntegrationInstallation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListRepositoriesResponse {
    pub repositories: Vec<IntegrationRepository>,
}

// ── Build domain types ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
            // Terminal states have no valid transitions
            Self::Succeeded | Self::Failed | Self::Canceled | Self::TimedOut | Self::Expired => &[],
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyPolicy {
    #[serde(default)]
    pub cancel_previous: bool,
    #[serde(default)]
    pub max_concurrent: Option<u32>,
}

impl Default for ConcurrencyPolicy {
    fn default() -> Self {
        Self {
            cancel_previous: false,
            max_concurrent: None,
        }
    }
}

// ── Trigger config ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub branches: Vec<String>,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            branches: Vec::new(),
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBuildRequest {
    pub pipeline_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_ref: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBuildResponse {
    pub build: Build,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildDetailResponse {
    pub build: Build,
    pub events: Vec<BuildEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListBuildsResponse {
    pub builds: Vec<Build>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CancelBuildResponse {
    pub build: Build,
}

// ── Runner domain types ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runner {
    pub id: String,
    pub name: String,
    pub status: String,
    pub capabilities: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Runner API types ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRunnerRequest {
    pub name: String,
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRunnerResponse {
    pub runner: Runner,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunnerHeartbeatRequest {
    pub status: String,
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRunnerRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRunnerResponse {
    pub runner: Runner,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaimJobResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ClaimedJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedJob {
    pub build_id: String,
    pub project_id: String,
    pub pipeline_id: String,
    pub build_number: i64,
    pub config_snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub lease_expires_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateJobStatusRequest {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default)]
    pub steps: Vec<StepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub started_at: i64,
    pub finished_at: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListRunnersResponse {
    pub runners: Vec<Runner>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobStatusResponse {
    pub status: String,
}

// ── Artifact domain types ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub build_id: String,
    pub name: String,
    pub artifact_type: String,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub checksum: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateArtifactRequest {
    pub name: String,
    pub artifact_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateArtifactResponse {
    pub artifact: Artifact,
    pub upload_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListArtifactsResponse {
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactDownloadLinkResponse {
    pub download_url: String,
    pub expires_at: i64,
}

// ── Artifact storage settings types ─────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStorageSource {
    Database,
    Environment,
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactStorageSettingsResponse {
    pub settings: ArtifactStorageSettings,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstancePreferences {
    pub key_storage_mode: KeyStorageMode,
    pub restart_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstancePreferencesResponse {
    pub preferences: InstancePreferences,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateInstancePreferencesRequest {
    pub key_storage_mode: KeyStorageMode,
}

// ── Project API types ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    pub settings: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
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
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectResponse {
    pub project: Project,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectDetailResponse {
    pub project: Project,
    pub pipeline_count: i64,
    pub build_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<Project>,
    pub total: i64,
}

// ── Pipeline API types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BuildPlatform {
    #[default]
    Android,
    Ios,
    Macos,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineCommandStages {
    #[serde(default)]
    pub pre_build: Vec<String>,
    #[serde(default)]
    pub build: Vec<String>,
    #[serde(default)]
    pub post_build: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformBuildArgs {
    #[serde(default)]
    pub android: Vec<String>,
    #[serde(default)]
    pub ios: Vec<String>,
    #[serde(default)]
    pub macos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformBuildCommands {
    #[serde(default)]
    pub android: Option<String>,
    #[serde(default)]
    pub ios: Option<String>,
    #[serde(default)]
    pub macos: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePipelineResponse {
    pub pipeline: Pipeline,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineDetailResponse {
    pub pipeline: Pipeline,
    pub build_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListPipelinesResponse {
    pub pipelines: Vec<Pipeline>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatePipelineResponse {
    pub valid: bool,
    pub errors: Vec<String>,
}

// ── Build log types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildLogChunk {
    pub sequence: i64,
    pub content: String,
    pub stream: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppendBuildLogsRequest {
    pub chunks: Vec<BuildLogChunk>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppendBuildLogsResponse {
    pub appended: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildLogsResponse {
    pub logs: Vec<BuildLogChunk>,
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
}
