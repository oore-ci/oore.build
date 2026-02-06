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
