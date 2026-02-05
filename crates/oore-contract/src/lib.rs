use serde::{Deserialize, Serialize};

// ── Setup state machine ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetupState {
    Uninitialized,
    BootstrapPending,
    IdpConfigured,
    OwnerCreated,
    Ready,
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
        let state = state;
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OwnerFinalizeRequest {
    pub owner_email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OwnerFinalizeResponse {
    pub state: SetupState,
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
    pub configured_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerRecord {
    pub email: String,
    pub created_at: i64,
}
