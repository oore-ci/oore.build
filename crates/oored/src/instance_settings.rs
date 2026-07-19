use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, header::HeaderName};
use ipnet::IpNet;
use oore_contract::{
    ApiError, ArtifactStorageProvider, ArtifactStorageSettingsResponse,
    ConfigureExternalAccessOidcRequest, ConfigureExternalAccessOidcResponse,
    ExternalAccessNetworkSettings, ExternalAccessNetworkSettingsResponse,
    ExternalAccessNetworkSource, ExternalAccessPreflightCheck, ExternalAccessPreflightResponse,
    GetExternalAccessOidcResponse, InstancePreferences, InstancePreferencesResponse,
    KeyStorageMode, OidcConfigRecord, OidcSecretRecord, RemoteAuthMode, RuntimeMode, SetupState,
    TestOidcConnectionRequest, TestOidcConnectionResponse, TrustedProxySettingsPublic,
    TrustedProxySettingsResponse, UpdateArtifactStorageSettingsRequest,
    UpdateExternalAccessNetworkSettingsRequest, UpdateInstancePreferencesRequest,
    UpdateTrustedProxySettingsRequest, WarpgateTicketSource,
};
use sqlx::Row;
use tracing::{error, info};

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::storage;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

fn trim_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn warpgate_ticket_from_env() -> anyhow::Result<Option<String>> {
    let ticket = std::env::var(WARPGATE_TICKET_ENV)
        .ok()
        .and_then(|value| trim_opt(Some(value)));
    if ticket
        .as_ref()
        .is_some_and(|value| value.len() > MAX_WARPGATE_TICKET_LEN)
    {
        anyhow::bail!(
            "{WARPGATE_TICKET_ENV} must be {MAX_WARPGATE_TICKET_LEN} characters or fewer"
        );
    }
    Ok(ticket)
}

pub const DEFAULT_ALLOWED_ORIGINS: [&str; 4] = [
    "http://localhost:3000",
    "http://127.0.0.1:3000",
    "http://localhost:4173",
    "http://127.0.0.1:4173",
];
pub const DEFAULT_TRUSTED_PROXY_EMAIL_HEADER: &str = "x-oore-user-email";
pub const TRUSTED_PROXY_SHARED_SECRET_HEADER: &str = "x-oore-trusted-proxy-secret";
pub const WARPGATE_USER_EMAIL_HEADER: &str = "x-warpgate-username";
const WARPGATE_TICKET_ENV: &str = "OORE_WARPGATE_TICKET";
const MAX_WARPGATE_TICKET_LEN: usize = 1024;

#[derive(Debug, Clone)]
pub struct EffectiveExternalAccessNetworkSettings {
    pub public_url: Option<String>,
    pub artifact_delivery_url: Option<String>,
    pub allowed_origins: Vec<String>,
    pub source: ExternalAccessNetworkSource,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct EffectiveTrustedProxySettings {
    pub user_email_header: String,
    pub setup_owner_email: Option<String>,
    pub trusted_proxy_cidrs: Vec<String>,
    pub trusted_proxy_networks: Vec<IpNet>,
    pub has_shared_secret: bool,
    pub encrypted_shared_secret: Option<String>,
    pub has_warpgate_ticket: bool,
    pub warpgate_ticket_source: Option<WarpgateTicketSource>,
    pub encrypted_warpgate_ticket: Option<String>,
    pub configured: bool,
    pub updated_at: Option<i64>,
}

pub fn default_allowed_origins() -> Vec<String> {
    DEFAULT_ALLOWED_ORIGINS
        .iter()
        .map(|value| value.to_string())
        .collect()
}

fn dedupe_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.iter().any(|existing| existing == &value) {
            deduped.push(value);
        }
    }
    deduped
}

fn normalize_origin_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed = url::Url::parse(trimmed).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return None;
    }

    let origin = parsed.origin().ascii_serialization();
    if origin == "null" {
        return None;
    }

    Some(origin)
}

fn parse_allowed_origins_raw(raw: &str) -> Vec<String> {
    dedupe_preserve_order(
        raw.split([',', '\n'])
            .filter_map(normalize_origin_value)
            .collect(),
    )
}

fn with_local_defaults(mut origins: Vec<String>) -> Vec<String> {
    for default_origin in DEFAULT_ALLOWED_ORIGINS {
        if !origins.iter().any(|existing| existing == default_origin) {
            origins.push(default_origin.to_string());
        }
    }
    dedupe_preserve_order(origins)
}

fn parse_db_allowed_origins(raw_json: &str) -> Vec<String> {
    let parsed: Vec<String> = serde_json::from_str(raw_json).unwrap_or_default();
    dedupe_preserve_order(
        parsed
            .into_iter()
            .filter_map(|value| normalize_origin_value(&value))
            .collect(),
    )
}

fn parse_db_trusted_proxy_cidrs(raw_json: &str) -> Vec<String> {
    let parsed: Vec<String> = serde_json::from_str(raw_json).unwrap_or_default();
    dedupe_preserve_order(
        parsed
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect(),
    )
}

pub(crate) fn normalize_header_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    HeaderName::from_bytes(trimmed.as_bytes())
        .ok()
        .map(|header| header.as_str().to_string())
}

pub(crate) fn normalize_email_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() || trimmed.len() > 256 || !trimmed.contains('@') {
        return None;
    }
    Some(trimmed)
}

fn parse_cidr_list(cidr_strings: &[String]) -> Vec<IpNet> {
    cidr_strings
        .iter()
        .filter_map(|cidr| cidr.parse::<IpNet>().ok())
        .collect()
}

pub(crate) fn normalize_requested_trusted_proxy_cidrs(
    values: Vec<String>,
) -> Result<Vec<String>, (StatusCode, Json<ApiError>)> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cidr = trimmed.parse::<IpNet>().map_err(|_| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                format!("invalid trusted proxy CIDR: {}", trimmed),
            )
        })?;
        let canonical = cidr.to_string();
        if !normalized.iter().any(|existing| existing == &canonical) {
            normalized.push(canonical);
        }
    }
    Ok(normalized)
}

pub async fn load_effective_external_access_network_settings(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<EffectiveExternalAccessNetworkSettings> {
    let env_public_url = std::env::var("OORE_PUBLIC_URL")
        .ok()
        .and_then(|value| trim_opt(Some(value)));
    let env_artifact_delivery_url = std::env::var("OORE_ARTIFACT_DELIVERY_URL")
        .ok()
        .and_then(|value| trim_opt(Some(value)));
    let row = sqlx::query(
        "SELECT public_url, artifact_delivery_url, allowed_origins_json, updated_at \
         FROM external_access_network_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let public_url = row
            .try_get::<Option<String>, _>("public_url")
            .ok()
            .flatten()
            .and_then(|value| trim_opt(Some(value)));
        let artifact_delivery_url = row
            .try_get::<Option<String>, _>("artifact_delivery_url")
            .ok()
            .flatten()
            .and_then(|value| trim_opt(Some(value)))
            .or_else(|| env_artifact_delivery_url.clone());
        let allowed_origins = row
            .try_get::<String, _>("allowed_origins_json")
            .ok()
            .map(|value| parse_db_allowed_origins(&value))
            .unwrap_or_default();

        return Ok(EffectiveExternalAccessNetworkSettings {
            public_url,
            artifact_delivery_url,
            allowed_origins: with_local_defaults(allowed_origins),
            source: ExternalAccessNetworkSource::Database,
            updated_at: row.try_get::<Option<i64>, _>("updated_at").ok().flatten(),
        });
    }

    let env_origins_raw = std::env::var("OORE_CORS_ORIGINS")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("OORE_CORS_ORIGIN").ok());

    if let Some(raw) = env_origins_raw {
        let parsed = parse_allowed_origins_raw(&raw);
        return Ok(EffectiveExternalAccessNetworkSettings {
            public_url: env_public_url,
            artifact_delivery_url: env_artifact_delivery_url,
            allowed_origins: with_local_defaults(parsed),
            source: ExternalAccessNetworkSource::Environment,
            updated_at: None,
        });
    }

    Ok(EffectiveExternalAccessNetworkSettings {
        public_url: env_public_url,
        artifact_delivery_url: env_artifact_delivery_url,
        allowed_origins: default_allowed_origins(),
        source: ExternalAccessNetworkSource::Default,
        updated_at: None,
    })
}

pub async fn load_effective_trusted_proxy_settings(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<EffectiveTrustedProxySettings> {
    let row = sqlx::query(
        "SELECT user_email_header, setup_owner_email, trusted_proxy_cidrs_json, encrypted_shared_secret, encrypted_warpgate_ticket, updated_at \
         FROM trusted_proxy_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let user_email_header = row
            .try_get::<String, _>("user_email_header")
            .ok()
            .and_then(|value| normalize_header_name(&value))
            .unwrap_or_else(|| DEFAULT_TRUSTED_PROXY_EMAIL_HEADER.to_string());
        let environment_warpgate_ticket = if user_email_header == WARPGATE_USER_EMAIL_HEADER {
            warpgate_ticket_from_env()?
        } else {
            None
        };
        let trusted_proxy_cidrs = row
            .try_get::<String, _>("trusted_proxy_cidrs_json")
            .ok()
            .map(|value| parse_db_trusted_proxy_cidrs(&value))
            .unwrap_or_default();
        let setup_owner_email = row
            .try_get::<Option<String>, _>("setup_owner_email")
            .ok()
            .flatten()
            .and_then(|value| normalize_email_value(&value));
        let trusted_proxy_networks = parse_cidr_list(&trusted_proxy_cidrs);
        let encrypted_shared_secret = row
            .try_get::<Option<String>, _>("encrypted_shared_secret")
            .ok()
            .flatten();
        let has_shared_secret = encrypted_shared_secret.is_some();
        let encrypted_warpgate_ticket = row
            .try_get::<Option<String>, _>("encrypted_warpgate_ticket")
            .ok()
            .flatten();
        let warpgate_ticket_source = if encrypted_warpgate_ticket.is_some() {
            Some(WarpgateTicketSource::Database)
        } else if environment_warpgate_ticket.is_some() {
            Some(WarpgateTicketSource::Environment)
        } else {
            None
        };

        return Ok(EffectiveTrustedProxySettings {
            user_email_header,
            setup_owner_email,
            trusted_proxy_cidrs,
            trusted_proxy_networks,
            has_shared_secret,
            encrypted_shared_secret,
            has_warpgate_ticket: warpgate_ticket_source.is_some(),
            warpgate_ticket_source,
            encrypted_warpgate_ticket,
            configured: true,
            updated_at: row.try_get::<Option<i64>, _>("updated_at").ok().flatten(),
        });
    }

    let trusted_proxy_cidrs = Vec::new();
    Ok(EffectiveTrustedProxySettings {
        user_email_header: DEFAULT_TRUSTED_PROXY_EMAIL_HEADER.to_string(),
        setup_owner_email: None,
        trusted_proxy_networks: parse_cidr_list(&trusted_proxy_cidrs),
        trusted_proxy_cidrs,
        has_shared_secret: false,
        encrypted_shared_secret: None,
        has_warpgate_ticket: false,
        warpgate_ticket_source: None,
        encrypted_warpgate_ticket: None,
        configured: false,
        updated_at: None,
    })
}

pub fn validate_external_access_public_url(raw: &str) -> Result<url::Url, &'static str> {
    let parsed = url::Url::parse(raw).map_err(|_| "invalid_url")?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("invalid_url");
    }

    let host = parsed.host_str().unwrap_or_default();
    if host.is_empty() || crate::is_loopback_host(host) {
        return Err("loopback_host");
    }
    if parsed.scheme() != "https" {
        return Err("https_required");
    }

    Ok(parsed)
}

async fn runtime_network_settings(state: &Arc<AppState>) -> EffectiveExternalAccessNetworkSettings {
    let public_url = state.public_url.read().await.clone();
    let allowed_origins = state.allowed_origins.read().await.clone();
    EffectiveExternalAccessNetworkSettings {
        public_url,
        artifact_delivery_url: None,
        allowed_origins,
        source: ExternalAccessNetworkSource::Database,
        updated_at: None,
    }
}

fn network_settings_response(
    settings: EffectiveExternalAccessNetworkSettings,
) -> ExternalAccessNetworkSettingsResponse {
    ExternalAccessNetworkSettingsResponse {
        settings: ExternalAccessNetworkSettings {
            public_url: settings.public_url,
            artifact_delivery_url: settings.artifact_delivery_url,
            allowed_origins: settings.allowed_origins,
            source: settings.source,
            updated_at: settings.updated_at,
        },
    }
}

fn normalize_requested_allowed_origins(
    values: Vec<String>,
) -> Result<Vec<String>, (StatusCode, Json<ApiError>)> {
    let mut normalized = Vec::new();
    for value in values {
        let Some(origin) = normalize_origin_value(&value) else {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                format!("invalid allowed origin: {}", value.trim()),
            ));
        };
        if !normalized.iter().any(|existing| existing == &origin) {
            normalized.push(origin);
        }
    }

    Ok(with_local_defaults(normalized))
}

pub async fn load_key_storage_mode(pool: &sqlx::SqlitePool) -> anyhow::Result<KeyStorageMode> {
    let _ = pool;
    Ok(KeyStorageMode::File)
}

pub async fn load_runtime_mode(pool: &sqlx::SqlitePool) -> anyhow::Result<RuntimeMode> {
    let row = sqlx::query("SELECT runtime_mode FROM instance_preferences WHERE id = 1")
        .fetch_optional(pool)
        .await?;

    let mode = match row {
        Some(row) => {
            let raw: Option<String> = row.try_get("runtime_mode").ok();
            raw.and_then(|value| value.parse::<RuntimeMode>().ok())
                .unwrap_or(RuntimeMode::Local)
        }
        None => RuntimeMode::Local,
    };

    Ok(mode)
}

pub async fn load_remote_auth_mode(pool: &sqlx::SqlitePool) -> anyhow::Result<RemoteAuthMode> {
    let row = sqlx::query("SELECT remote_auth_mode FROM instance_preferences WHERE id = 1")
        .fetch_optional(pool)
        .await?;

    let mode = match row {
        Some(row) => {
            let raw: Option<String> = row.try_get("remote_auth_mode").ok();
            raw.and_then(|value| value.parse::<RemoteAuthMode>().ok())
                .unwrap_or(RemoteAuthMode::Oidc)
        }
        None => RemoteAuthMode::Oidc,
    };

    Ok(mode)
}

pub async fn load_direct_macos_runner_enabled(pool: &sqlx::SqlitePool) -> anyhow::Result<bool> {
    let row =
        sqlx::query("SELECT direct_macos_runner_enabled FROM instance_preferences WHERE id = 1")
            .fetch_optional(pool)
            .await?;

    Ok(row
        .and_then(|row| row.try_get::<i32, _>("direct_macos_runner_enabled").ok())
        .is_some_and(|enabled| enabled != 0))
}

pub async fn load_warpgate_install_ticket(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
) -> anyhow::Result<Option<String>> {
    if load_runtime_mode(pool).await? != RuntimeMode::Remote
        || load_remote_auth_mode(pool).await? != RemoteAuthMode::TrustedProxy
    {
        return Ok(None);
    }

    let settings = load_effective_trusted_proxy_settings(pool).await?;
    if settings.user_email_header != WARPGATE_USER_EMAIL_HEADER {
        return Ok(None);
    }

    match settings.warpgate_ticket_source {
        Some(WarpgateTicketSource::Database) => settings
            .encrypted_warpgate_ticket
            .as_deref()
            .map(|encrypted| crypto::decrypt(encrypted, encryption_key))
            .transpose(),
        Some(WarpgateTicketSource::Environment) => warpgate_ticket_from_env(),
        None => Ok(None),
    }
}

fn preferences_response(
    mode: KeyStorageMode,
    runtime_mode: RuntimeMode,
    remote_auth_mode: RemoteAuthMode,
    direct_macos_runner_enabled: bool,
    updated_at: Option<i64>,
) -> InstancePreferencesResponse {
    InstancePreferencesResponse {
        preferences: InstancePreferences {
            key_storage_mode: mode,
            runtime_mode,
            remote_auth_mode,
            direct_macos_runner_enabled,
            restart_required: false,
            updated_at,
        },
    }
}

fn trusted_proxy_settings_response(
    settings: EffectiveTrustedProxySettings,
) -> TrustedProxySettingsResponse {
    TrustedProxySettingsResponse {
        settings: TrustedProxySettingsPublic {
            user_email_header: settings.user_email_header,
            trusted_proxy_cidrs: settings.trusted_proxy_cidrs,
            has_shared_secret: settings.has_shared_secret,
            has_warpgate_ticket: settings.has_warpgate_ticket,
            warpgate_ticket_source: settings.warpgate_ticket_source,
            updated_at: settings.updated_at,
        },
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub fn is_trusted_proxy_peer(peer_ip: IpAddr, settings: &EffectiveTrustedProxySettings) -> bool {
    peer_ip.is_loopback()
        || settings
            .trusted_proxy_networks
            .iter()
            .any(|cidr| cidr.contains(&peer_ip))
}

pub fn verify_trusted_proxy_shared_secret(
    headers: &HeaderMap,
    settings: &EffectiveTrustedProxySettings,
    encryption_key: &[u8],
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let Some(encrypted_shared_secret) = settings.encrypted_shared_secret.as_deref() else {
        return Err(api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "trusted_proxy_config_invalid",
            "Trusted proxy shared secret is not configured",
        ));
    };

    let expected = crypto::decrypt(encrypted_shared_secret, encryption_key).map_err(|e| {
        error!(error = %e, "failed to decrypt trusted proxy shared secret");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to verify trusted proxy shared secret",
        )
    })?;

    let provided = headers
        .get(TRUSTED_PROXY_SHARED_SECRET_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "trusted_proxy_shared_secret_missing",
                "Trusted proxy shared secret header is missing",
            )
        })?;

    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "trusted_proxy_shared_secret_invalid",
            "Trusted proxy shared secret header is invalid",
        ));
    }

    Ok(())
}

pub fn extract_trusted_proxy_email(
    headers: &HeaderMap,
    settings: &EffectiveTrustedProxySettings,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let header_name =
        HeaderName::from_bytes(settings.user_email_header.as_bytes()).map_err(|_| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "trusted_proxy_config_invalid",
                "Trusted proxy email header configuration is invalid",
            )
        })?;

    let raw_identity = headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "trusted_proxy_identity_missing",
                "Trusted proxy identity header is missing. Configure the upstream proxy to forward user identity.",
            )
        })?;

    normalize_email_value(raw_identity).ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "trusted_proxy_identity_invalid",
            "Trusted proxy identity must be an email address. Configure the upstream proxy to forward an email identity.",
        )
    })
}

fn build_external_access_check(
    id: &str,
    label: &str,
    ok: bool,
    message: impl Into<String>,
    failure_code: Option<&str>,
) -> ExternalAccessPreflightCheck {
    ExternalAccessPreflightCheck {
        id: id.to_string(),
        label: label.to_string(),
        ok,
        message: message.into(),
        failure_code: failure_code.map(str::to_string),
    }
}

fn first_preflight_failure_code(result: &ExternalAccessPreflightResponse) -> &str {
    result
        .checks
        .iter()
        .find(|check| !check.ok)
        .and_then(|check| check.failure_code.as_deref())
        .unwrap_or("external_access_preflight_failed")
}

fn preflight_failure_summary(result: &ExternalAccessPreflightResponse) -> Vec<String> {
    result
        .checks
        .iter()
        .filter(|check| !check.ok)
        .map(|check| check.id.clone())
        .collect()
}

async fn evaluate_external_access_preflight(
    state: &Arc<AppState>,
    remote_auth_mode_override: Option<RemoteAuthMode>,
) -> Result<ExternalAccessPreflightResponse, (StatusCode, Json<ApiError>)> {
    let setup_state = {
        let store = state.store.lock().await;
        let sf = store.load().await.map_err(|e| {
            error!(error = %e, "failed to load setup state for external access preflight");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?;
        sf.setup_state
    };

    let remote_auth_mode = if let Some(mode) = remote_auth_mode_override {
        mode
    } else {
        load_remote_auth_mode(&state.db).await.map_err(|e| {
            error!(error = %e, "failed to load remote auth mode for external access preflight");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load External Access auth mode",
            )
        })?
    };

    let mut checks = Vec::new();

    checks.push(build_external_access_check(
        "setup_ready",
        "Setup state is ready",
        setup_state == SetupState::Ready,
        if setup_state == SetupState::Ready {
            "Setup is complete."
        } else {
            "Complete setup before enabling External Access."
        },
        None,
    ));

    match remote_auth_mode {
        RemoteAuthMode::Oidc => {
            let oidc_check = match crate::auth::load_oidc_config_for_setup(state).await {
                Ok(_) => build_external_access_check(
                    "oidc_configured",
                    "OIDC configuration is valid",
                    true,
                    "OIDC configuration is present and valid for runtime auth.",
                    None,
                ),
                Err((_, Json(err))) => build_external_access_check(
                    "oidc_configured",
                    "OIDC configuration is valid",
                    false,
                    format!("OIDC is not ready for External Access: {}", err.error),
                    None,
                ),
            };
            checks.push(oidc_check);
        }
        RemoteAuthMode::TrustedProxy => {
            let proxy_settings = load_effective_trusted_proxy_settings(&state.db)
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to load trusted proxy settings for preflight");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to load trusted proxy settings",
                    )
                })?;

            let configured = proxy_settings.configured
                && proxy_settings.has_shared_secret
                && normalize_header_name(&proxy_settings.user_email_header).is_some();
            checks.push(build_external_access_check(
                "trusted_proxy_configured",
                "Trusted proxy settings are configured",
                configured,
                if configured {
                    "Trusted proxy settings are configured for runtime auth."
                } else {
                    "Configure Trusted Proxy settings before enabling External Access."
                },
                if configured {
                    None
                } else {
                    Some("external_access_trusted_proxy_not_configured")
                },
            ));
        }
    }

    let network_settings = runtime_network_settings(state).await;
    let public_url_raw = network_settings.public_url.clone();

    let mut parsed_public_url: Option<url::Url> = None;
    checks.push(match public_url_raw {
        None => build_external_access_check(
            "public_url_https",
            "Public URL is configured with HTTPS",
            false,
            "Set External Access public URL to a non-loopback HTTPS URL before enabling External Access.",
            Some("external_access_public_url_missing"),
        ),
        Some(raw) => match validate_external_access_public_url(&raw) {
            Ok(parsed) => {
                parsed_public_url = Some(parsed);
                build_external_access_check(
                    "public_url_https",
                    "Public URL is configured with HTTPS",
                    true,
                    "Public URL is HTTPS and non-loopback.",
                    None,
                )
            }
            Err("loopback_host") => build_external_access_check(
                "public_url_https",
                "Public URL is configured with HTTPS",
                false,
                "Public URL must resolve to a non-loopback host for External Access.",
                Some("external_access_public_url_missing"),
            ),
            Err("https_required") => build_external_access_check(
                "public_url_https",
                "Public URL is configured with HTTPS",
                false,
                "Public URL must use https for External Access.",
                Some("external_access_https_required"),
            ),
            Err(_) => build_external_access_check(
                "public_url_https",
                "Public URL is configured with HTTPS",
                false,
                "Public URL must be a valid URL.",
                Some("external_access_public_url_missing"),
            ),
        },
    });

    checks.push(if let Some(public_url) = parsed_public_url.as_ref() {
        let origin = public_url.origin().ascii_serialization();
        if network_settings
            .allowed_origins
            .iter()
            .any(|allowed| allowed == &origin)
        {
            build_external_access_check(
                "public_origin_allowed",
                "Public URL origin is allowlisted in CORS",
                true,
                "Public origin is present in allowed CORS origins.",
                None,
            )
        } else {
            build_external_access_check(
                "public_origin_allowed",
                "Public URL origin is allowlisted in CORS",
                false,
                format!(
                    "Add {} to allowed origins before enabling External Access.",
                    origin
                ),
                Some("external_access_origin_not_allowed"),
            )
        }
    } else {
        build_external_access_check(
            "public_origin_allowed",
            "Public URL origin is allowlisted in CORS",
            false,
            "Public URL check must pass before origin allowlist validation can run.",
            None,
        )
    });

    if remote_auth_mode == RemoteAuthMode::Oidc {
        checks.push(if let Some(public_url) = parsed_public_url {
            let redirect_uri = format!(
                "{}/auth/callback",
                public_url.origin().ascii_serialization()
            );
            match crate::validate_redirect_uri(&redirect_uri, &network_settings.allowed_origins) {
                Ok(()) => build_external_access_check(
                    "redirect_policy_consistent",
                    "Redirect policy is consistent with allowed origins",
                    true,
                    "Redirect URI policy is consistent with current origin rules.",
                    None,
                ),
                Err((_, Json(err))) => build_external_access_check(
                    "redirect_policy_consistent",
                    "Redirect policy is consistent with allowed origins",
                    false,
                    format!("Redirect/origin policy validation failed: {}", err.error),
                    None,
                ),
            }
        } else {
            build_external_access_check(
                "redirect_policy_consistent",
                "Redirect policy is consistent with allowed origins",
                false,
                "Public URL check must pass before redirect/origin consistency can be validated.",
                None,
            )
        });
    }

    let ready = checks.iter().all(|check| check.ok);
    Ok(ExternalAccessPreflightResponse { ready, checks })
}

pub async fn get_artifact_storage_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ArtifactStorageSettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let pool = state.db.clone();

    let cfg = storage::load_effective_config(&pool, &state.encryption_key)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load artifact storage settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load artifact storage settings",
            )
        })?;

    Ok(Json(ArtifactStorageSettingsResponse {
        settings: cfg.to_public_settings(),
    }))
}

pub async fn update_artifact_storage_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpdateArtifactStorageSettingsRequest>,
) -> ApiResult<ArtifactStorageSettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let now = now_unix();
    let provider = req.provider;

    let local_base_dir = trim_opt(req.local_base_dir);
    let s3_bucket = trim_opt(req.s3_bucket);
    let s3_region = trim_opt(req.s3_region).or(Some("us-east-1".to_string()));
    let s3_endpoint = trim_opt(req.s3_endpoint);
    let access_key_id = trim_opt(req.access_key_id);
    let secret_access_key = trim_opt(req.secret_access_key);

    let pool = state.db.clone();

    let existing = sqlx::query(
        "SELECT s3_access_key_encrypted, s3_secret_key_encrypted FROM artifact_storage_settings WHERE id = 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load existing artifact storage row");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update artifact storage settings",
        )
    })?;

    let existing_access_encrypted = existing
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("s3_access_key_encrypted"));
    let existing_secret_encrypted = existing
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("s3_secret_key_encrypted"));

    let (
        persist_local_base_dir,
        persist_s3_bucket,
        persist_s3_region,
        persist_s3_endpoint,
        persist_access_encrypted,
        persist_secret_encrypted,
    ) = match provider {
        ArtifactStorageProvider::Disabled => (None, None, None, None, None, None),
        ArtifactStorageProvider::Local => {
            let Some(dir) = local_base_dir else {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_local_base_dir",
                    "local_base_dir is required when provider is local",
                ));
            };

            if !Path::new(&dir).is_absolute() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_local_base_dir",
                    "local_base_dir must be an absolute path",
                ));
            }

            (Some(dir), None, None, None, None, None)
        }
        ArtifactStorageProvider::S3 | ArtifactStorageProvider::R2 => {
            let Some(bucket) = s3_bucket else {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_s3_bucket",
                    "s3_bucket is required for s3/r2 providers",
                ));
            };

            if provider == ArtifactStorageProvider::R2 && s3_endpoint.is_none() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_s3_endpoint",
                    "s3_endpoint is required for r2 provider",
                ));
            }

            let access_encrypted = if let Some(value) = access_key_id {
                Some(crypto::encrypt(&value, &state.encryption_key).map_err(|e| {
                    error!(error = %e, "failed to encrypt access key");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to encrypt access key",
                    )
                })?)
            } else {
                existing_access_encrypted
            };

            let secret_encrypted = if let Some(value) = secret_access_key {
                Some(crypto::encrypt(&value, &state.encryption_key).map_err(|e| {
                    error!(error = %e, "failed to encrypt secret key");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to encrypt secret key",
                    )
                })?)
            } else {
                existing_secret_encrypted
            };

            if access_encrypted.is_none() || secret_encrypted.is_none() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "missing_s3_credentials",
                    "access_key_id and secret_access_key are required for s3/r2 providers",
                ));
            }

            (
                None,
                Some(bucket),
                s3_region,
                s3_endpoint,
                access_encrypted,
                secret_encrypted,
            )
        }
    };

    sqlx::query(
        "INSERT INTO artifact_storage_settings (
            id, provider, local_base_dir, s3_bucket, s3_region, s3_endpoint,
            s3_access_key_encrypted, s3_secret_key_encrypted,
            updated_by, created_at, updated_at
         ) VALUES (
            1, ?1, ?2, ?3, ?4, ?5,
            ?6, ?7,
            ?8, ?9, ?9
         )
         ON CONFLICT(id) DO UPDATE SET
            provider = excluded.provider,
            local_base_dir = excluded.local_base_dir,
            s3_bucket = excluded.s3_bucket,
            s3_region = excluded.s3_region,
            s3_endpoint = excluded.s3_endpoint,
            s3_access_key_encrypted = excluded.s3_access_key_encrypted,
            s3_secret_key_encrypted = excluded.s3_secret_key_encrypted,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(provider.to_string())
    .bind(persist_local_base_dir)
    .bind(persist_s3_bucket)
    .bind(persist_s3_region)
    .bind(persist_s3_endpoint)
    .bind(persist_access_encrypted)
    .bind(persist_secret_encrypted)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist artifact storage settings");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update artifact storage settings",
        )
    })?;

    let details = serde_json::json!({
        "provider": provider.to_string(),
    })
    .to_string();

    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "artifact_storage_updated",
        "instance_settings",
        Some("artifact_storage"),
        Some(&details),
    )
    .await;

    // Hot-reload backend so changes apply without daemon restart.
    let runtime_public_url = state.public_url.read().await.clone();
    let backend = storage::load_backend(&pool, &state.encryption_key, runtime_public_url).await;
    {
        let mut guard = state.storage.write().await;
        *guard = backend;
    }

    let cfg = storage::load_effective_config(&pool, &state.encryption_key)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload artifact storage settings after update");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update artifact storage settings",
            )
        })?;

    info!(provider = %provider, user_id = %auth.0.user_id, "artifact storage settings updated");

    Ok(Json(ArtifactStorageSettingsResponse {
        settings: cfg.to_public_settings(),
    }))
}

pub async fn get_instance_preferences(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<InstancePreferencesResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let pool = state.db.clone();

    let row = sqlx::query(
        "SELECT runtime_mode, remote_auth_mode, direct_macos_runner_enabled, updated_at \
         FROM instance_preferences WHERE id = 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load instance preferences");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load instance preferences",
        )
    })?;

    if let Some(row) = row {
        let runtime_mode = row
            .try_get::<Option<String>, _>("runtime_mode")
            .ok()
            .flatten()
            .and_then(|raw| raw.parse::<RuntimeMode>().ok())
            .unwrap_or(RuntimeMode::Local);
        let remote_auth_mode = row
            .try_get::<Option<String>, _>("remote_auth_mode")
            .ok()
            .flatten()
            .and_then(|raw| raw.parse::<RemoteAuthMode>().ok())
            .unwrap_or(RemoteAuthMode::Oidc);
        let direct_macos_runner_enabled = row
            .try_get::<i32, _>("direct_macos_runner_enabled")
            .unwrap_or(0)
            != 0;
        let updated_at: Option<i64> = row.get("updated_at");
        return Ok(Json(preferences_response(
            KeyStorageMode::File,
            runtime_mode,
            remote_auth_mode,
            direct_macos_runner_enabled,
            updated_at,
        )));
    }

    Ok(Json(preferences_response(
        KeyStorageMode::File,
        RuntimeMode::Local,
        RemoteAuthMode::Oidc,
        false,
        None,
    )))
}

pub async fn get_external_access_network_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ExternalAccessNetworkSettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let pool = state.db.clone();

    let settings = load_effective_external_access_network_settings(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load external access network settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load External Access network settings",
            )
        })?;

    Ok(Json(network_settings_response(settings)))
}

pub async fn get_external_access_trusted_proxy_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<TrustedProxySettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let pool = state.db.clone();

    let settings = load_effective_trusted_proxy_settings(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load trusted proxy settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load trusted proxy settings",
            )
        })?;

    Ok(Json(trusted_proxy_settings_response(settings)))
}

pub async fn update_external_access_trusted_proxy_settings(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    auth: AuthUser,
    Json(req): Json<UpdateTrustedProxySettingsRequest>,
) -> ApiResult<TrustedProxySettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    if auth.0.role != "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_owner_required",
            "Only the owner can update Trusted Proxy settings",
        ));
    }

    let pool = state.db.clone();

    let runtime_mode = load_runtime_mode(&pool).await.map_err(|e| {
        error!(error = %e, "failed to load runtime mode for trusted proxy update");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to determine runtime mode",
        )
    })?;
    let effective_ip = crate::effective_client_ip(peer_addr, &headers);
    if runtime_mode == RuntimeMode::Local && !effective_ip.is_loopback() {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_loopback_required",
            "Trusted Proxy settings can only be changed from loopback while in Local Only mode",
        ));
    }

    let existing_row =
        sqlx::query("SELECT encrypted_shared_secret, encrypted_warpgate_ticket FROM trusted_proxy_settings WHERE id = 1")
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to read existing trusted proxy settings");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to load existing trusted proxy settings",
                )
            })?;

    let user_email_header = req
        .user_email_header
        .as_deref()
        .and_then(normalize_header_name)
        .unwrap_or_else(|| DEFAULT_TRUSTED_PROXY_EMAIL_HEADER.to_string());

    let trusted_proxy_cidrs = normalize_requested_trusted_proxy_cidrs(req.trusted_proxy_cidrs)?;
    let trusted_proxy_cidrs_json = serde_json::to_string(&trusted_proxy_cidrs).map_err(|e| {
        error!(error = %e, "failed to serialize trusted proxy CIDRs");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to persist trusted proxy CIDRs",
        )
    })?;

    let encrypted_shared_secret = match req.shared_secret {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                if trimmed.len() > 1024 {
                    return Err(api_err(
                        StatusCode::BAD_REQUEST,
                        "invalid_input",
                        "shared_secret must be 1024 characters or fewer",
                    ));
                }
                Some(
                    crypto::encrypt(trimmed, &state.encryption_key).map_err(|e| {
                        error!(error = %e, "failed to encrypt trusted proxy shared secret");
                        api_err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "encryption_error",
                            "Failed to persist trusted proxy shared secret",
                        )
                    })?,
                )
            }
        }
        None => existing_row.as_ref().and_then(|row| {
            row.try_get::<Option<String>, _>("encrypted_shared_secret")
                .ok()
                .flatten()
        }),
    };

    if runtime_mode == RuntimeMode::Remote
        && load_remote_auth_mode(&pool).await.map_err(|e| {
            error!(error = %e, "failed to load remote auth mode for trusted proxy update");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine remote auth mode",
            )
        })? == RemoteAuthMode::TrustedProxy
        && encrypted_shared_secret.is_none()
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "trusted_proxy_shared_secret_required",
            "A shared secret is required while Trusted Proxy authentication is active",
        ));
    }

    let encrypted_warpgate_ticket = match req.warpgate_ticket {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                if trimmed.len() > MAX_WARPGATE_TICKET_LEN {
                    return Err(api_err(
                        StatusCode::BAD_REQUEST,
                        "invalid_input",
                        format!(
                            "warpgate_ticket must be {MAX_WARPGATE_TICKET_LEN} characters or fewer"
                        ),
                    ));
                }
                Some(
                    crypto::encrypt(trimmed, &state.encryption_key).map_err(|e| {
                        error!(error = %e, "failed to encrypt Warpgate install ticket");
                        api_err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "encryption_error",
                            "Failed to persist Warpgate install ticket",
                        )
                    })?,
                )
            }
        }
        None => existing_row.as_ref().and_then(|row| {
            row.try_get::<Option<String>, _>("encrypted_warpgate_ticket")
                .ok()
                .flatten()
        }),
    };

    let now = now_unix();
    sqlx::query(
        "INSERT INTO trusted_proxy_settings (id, user_email_header, trusted_proxy_cidrs_json, encrypted_shared_secret, encrypted_warpgate_ticket, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
            user_email_header = excluded.user_email_header,
            trusted_proxy_cidrs_json = excluded.trusted_proxy_cidrs_json,
            encrypted_shared_secret = excluded.encrypted_shared_secret,
            encrypted_warpgate_ticket = excluded.encrypted_warpgate_ticket,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(&user_email_header)
    .bind(&trusted_proxy_cidrs_json)
    .bind(encrypted_shared_secret.clone())
    .bind(encrypted_warpgate_ticket.clone())
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist trusted proxy settings");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update trusted proxy settings",
        )
    })?;

    let details = serde_json::json!({
        "user_email_header": user_email_header,
        "trusted_proxy_cidrs": trusted_proxy_cidrs,
        "has_shared_secret": encrypted_shared_secret.is_some(),
        "has_warpgate_ticket": encrypted_warpgate_ticket.is_some()
            || (user_email_header == WARPGATE_USER_EMAIL_HEADER
                && warpgate_ticket_from_env().ok().flatten().is_some()),
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "external_access_trusted_proxy_settings_updated",
        "instance_settings",
        Some("external_access"),
        Some(&details),
    )
    .await;

    let settings = load_effective_trusted_proxy_settings(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload trusted proxy settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load updated trusted proxy settings",
            )
        })?;
    state.recovery_capabilities.clear().await;

    Ok(Json(trusted_proxy_settings_response(settings)))
}

pub async fn update_external_access_network_settings(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    auth: AuthUser,
    Json(req): Json<UpdateExternalAccessNetworkSettingsRequest>,
) -> ApiResult<ExternalAccessNetworkSettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    if auth.0.role != "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_owner_required",
            "Only the owner can update External Access network settings",
        ));
    }

    let pool = state.db.clone();

    let runtime_mode = load_runtime_mode(&pool).await.map_err(|e| {
        error!(error = %e, "failed to load runtime mode for external access network update");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to determine runtime mode",
        )
    })?;
    let effective_ip = crate::effective_client_ip(peer_addr, &headers);
    if runtime_mode == RuntimeMode::Local && !effective_ip.is_loopback() {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_loopback_required",
            "External Access network settings can only be changed from loopback while in Local Only mode",
        ));
    }

    let mut public_url = trim_opt(req.public_url);
    let mut artifact_delivery_url = trim_opt(req.artifact_delivery_url);
    let allowed_origins = normalize_requested_allowed_origins(req.allowed_origins)?;
    if let Some(raw) = public_url.as_ref() {
        let parsed = validate_external_access_public_url(raw).map_err(|reason| match reason {
            "https_required" => api_err(
                StatusCode::BAD_REQUEST,
                "external_access_https_required",
                "Public URL must use https for External Access",
            ),
            "loopback_host" => api_err(
                StatusCode::BAD_REQUEST,
                "external_access_public_url_missing",
                "Public URL must resolve to a non-loopback host",
            ),
            _ => api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Public URL is not valid",
            ),
        })?;

        let public_origin = parsed.origin().ascii_serialization();
        if !allowed_origins
            .iter()
            .any(|allowed| allowed == &public_origin)
        {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "external_access_origin_not_allowed",
                format!(
                    "Public URL origin {} must be included in allowed origins",
                    public_origin
                ),
            ));
        }

        public_url = Some(raw.trim_end_matches('/').to_string());
    }

    if let Some(raw) = artifact_delivery_url.as_ref() {
        validate_external_access_public_url(raw).map_err(|reason| match reason {
            "https_required" => api_err(
                StatusCode::BAD_REQUEST,
                "artifact_delivery_https_required",
                "Artifact delivery URL must use https",
            ),
            "loopback_host" => api_err(
                StatusCode::BAD_REQUEST,
                "artifact_delivery_public_url_required",
                "Artifact delivery URL must resolve to a non-loopback host",
            ),
            _ => api_err(
                StatusCode::BAD_REQUEST,
                "artifact_delivery_url_invalid",
                "Artifact delivery URL is not valid",
            ),
        })?;
        artifact_delivery_url = Some(raw.trim_end_matches('/').to_string());
    }

    let now = now_unix();
    let allowed_origins_json = serde_json::to_string(&allowed_origins).map_err(|e| {
        error!(error = %e, "failed to serialize allowed origins");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to serialize allowed origins",
        )
    })?;

    sqlx::query(
        "INSERT INTO external_access_network_settings (id, public_url, artifact_delivery_url, allowed_origins_json, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(id) DO UPDATE SET
            public_url = excluded.public_url,
            artifact_delivery_url = excluded.artifact_delivery_url,
            allowed_origins_json = excluded.allowed_origins_json,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(public_url.clone())
    .bind(artifact_delivery_url.clone())
    .bind(allowed_origins_json)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist external access network settings");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update External Access network settings",
        )
    })?;

    {
        let mut runtime_public_url = state.public_url.write().await;
        *runtime_public_url = public_url.clone();
    }
    {
        let mut runtime_allowed_origins = state.allowed_origins.write().await;
        *runtime_allowed_origins = allowed_origins.clone();
    }
    state.recovery_capabilities.clear().await;
    // Hot-reload storage backend because local artifact links depend on public_base_url.
    let backend = storage::load_backend(&pool, &state.encryption_key, public_url.clone()).await;
    {
        let mut guard = state.storage.write().await;
        *guard = backend;
    }

    let details = serde_json::json!({
        "public_url": public_url,
        "artifact_delivery_url": artifact_delivery_url,
        "allowed_origins": allowed_origins,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "external_access_network_settings_updated",
        "instance_settings",
        Some("external_access"),
        Some(&details),
    )
    .await;

    Ok(Json(network_settings_response(
        EffectiveExternalAccessNetworkSettings {
            public_url,
            artifact_delivery_url,
            allowed_origins,
            source: ExternalAccessNetworkSource::Database,
            updated_at: Some(now),
        },
    )))
}

pub async fn get_external_access_preflight(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ExternalAccessPreflightResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let result = evaluate_external_access_preflight(&state, None).await?;

    let pool = state.db.clone();
    let details = serde_json::json!({
        "ready": result.ready,
        "failed_checks": preflight_failure_summary(&result),
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "external_access_preflight_checked",
        "instance_settings",
        Some("external_access"),
        Some(&details),
    )
    .await;

    Ok(Json(result))
}

pub async fn get_external_access_oidc(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<GetExternalAccessOidcResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let sf = {
        let store = state.store.lock().await;
        store.load().await.map_err(|e| {
            error!(error = %e, "failed to load setup state for External Access OIDC read");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?
    };

    if sf.setup_state != SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            "External Access OIDC settings are only available after setup is ready",
        ));
    }

    let oidc = sf.oidc_config.ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "oidc_not_configured",
            "No OIDC provider is configured",
        )
    })?;

    Ok(Json(GetExternalAccessOidcResponse {
        issuer_url: oidc.issuer_url,
        client_id: oidc.client_id,
        has_client_secret: oidc.has_client_secret,
        authorization_endpoint: oidc.authorization_endpoint,
        token_endpoint: oidc.token_endpoint,
        userinfo_endpoint: oidc.userinfo_endpoint,
        jwks_uri: oidc.jwks_uri,
        configured_at: oidc.configured_at,
    }))
}

pub async fn test_oidc_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<TestOidcConnectionRequest>,
) -> ApiResult<TestOidcConnectionResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    if auth.0.role != "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_owner_required",
            "Only the owner can test OIDC connections",
        ));
    }

    let issuer_url = req.issuer_url.trim();
    if issuer_url.is_empty() || issuer_url.len() > 2048 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "issuer_url must be between 1 and 2048 characters",
        ));
    }
    if url::Url::parse(issuer_url).is_err() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "issuer_url is not a valid URL",
        ));
    }

    let discovered = {
        #[cfg(any(test, feature = "test-support"))]
        {
            if state.skip_oidc_discovery {
                let base = issuer_url.trim_end_matches('/').to_string();
                crate::oidc::DiscoveredProvider {
                    issuer: issuer_url.to_string(),
                    authorization_endpoint: format!("{base}/o/oauth2/v2/auth"),
                    token_endpoint: format!("{base}/token"),
                    userinfo_endpoint: Some(format!("{base}/userinfo")),
                    jwks_uri: format!("{base}/jwks"),
                    scopes_supported: vec![
                        "openid".to_string(),
                        "email".to_string(),
                        "profile".to_string(),
                    ],
                }
            } else {
                crate::oidc::discover_provider(issuer_url)
                    .await
                    .map_err(|e| {
                        error!(error = %e, "OIDC test-connection discovery failed");
                        api_err(
                            StatusCode::BAD_REQUEST,
                            "oidc_discovery_failed",
                            "Failed to discover OIDC provider",
                        )
                    })?
            }
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            crate::oidc::discover_provider(issuer_url)
                .await
                .map_err(|e| {
                    error!(error = %e, "OIDC test-connection discovery failed");
                    api_err(
                        StatusCode::BAD_REQUEST,
                        "oidc_discovery_failed",
                        "Failed to discover OIDC provider",
                    )
                })?
        }
    };

    Ok(Json(TestOidcConnectionResponse {
        success: true,
        discovered_issuer: discovered.issuer,
        authorization_endpoint: discovered.authorization_endpoint,
        token_endpoint: discovered.token_endpoint,
        userinfo_endpoint: discovered.userinfo_endpoint,
        jwks_uri: discovered.jwks_uri,
        scopes_supported: discovered.scopes_supported,
    }))
}

pub async fn configure_external_access_oidc(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ConfigureExternalAccessOidcRequest>,
) -> ApiResult<ConfigureExternalAccessOidcResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    if auth.0.role != "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_owner_required",
            "Only the owner can configure External Access OIDC settings",
        ));
    }

    let issuer_url = req.issuer_url.trim();
    if issuer_url.is_empty() || issuer_url.len() > 2048 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "issuer_url must be between 1 and 2048 characters",
        ));
    }
    if url::Url::parse(issuer_url).is_err() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "issuer_url is not a valid URL",
        ));
    }

    let client_id = req.client_id.trim();
    if client_id.is_empty() || client_id.len() > 256 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "client_id must be between 1 and 256 characters",
        ));
    }

    let client_secret = trim_opt(req.client_secret);
    if let Some(secret) = client_secret.as_ref()
        && secret.len() > 1024
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "client_secret must be 1024 characters or fewer",
        ));
    }

    let now = now_unix();

    #[derive(Debug)]
    struct OidcConfigFromDiscovery {
        issuer: String,
        authorization_endpoint: String,
        token_endpoint: String,
        userinfo_endpoint: Option<String>,
        jwks_uri: String,
    }

    let discovered = {
        #[cfg(any(test, feature = "test-support"))]
        {
            if state.skip_oidc_discovery {
                let base = issuer_url.trim_end_matches('/').to_string();
                OidcConfigFromDiscovery {
                    issuer: issuer_url.to_string(),
                    authorization_endpoint: format!("{base}/o/oauth2/v2/auth"),
                    token_endpoint: format!("{base}/token"),
                    userinfo_endpoint: Some(format!("{base}/userinfo")),
                    jwks_uri: format!("{base}/jwks"),
                }
            } else {
                let provider = crate::oidc::discover_provider(issuer_url)
                    .await
                    .map_err(|e| {
                        error!(error = %e, "external access OIDC discovery failed");
                        api_err(
                            StatusCode::BAD_REQUEST,
                            "oidc_discovery_failed",
                            "Failed to discover OIDC provider",
                        )
                    })?;
                OidcConfigFromDiscovery {
                    issuer: provider.issuer,
                    authorization_endpoint: provider.authorization_endpoint,
                    token_endpoint: provider.token_endpoint,
                    userinfo_endpoint: provider.userinfo_endpoint,
                    jwks_uri: provider.jwks_uri,
                }
            }
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            let provider = crate::oidc::discover_provider(issuer_url)
                .await
                .map_err(|e| {
                    error!(error = %e, "external access OIDC discovery failed");
                    api_err(
                        StatusCode::BAD_REQUEST,
                        "oidc_discovery_failed",
                        "Failed to discover OIDC provider",
                    )
                })?;
            OidcConfigFromDiscovery {
                issuer: provider.issuer,
                authorization_endpoint: provider.authorization_endpoint,
                token_endpoint: provider.token_endpoint,
                userinfo_endpoint: provider.userinfo_endpoint,
                jwks_uri: provider.jwks_uri,
            }
        }
    };

    let pool = state.db.clone();

    let has_client_secret;
    {
        let store = state.store.lock().await;
        let mut sf = store.load().await.map_err(|e| {
            error!(error = %e, "failed to load setup state for External Access OIDC update");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?;

        if sf.setup_state != SetupState::Ready {
            return Err(api_err(
                StatusCode::CONFLICT,
                "invalid_state",
                "External Access OIDC can only be configured after setup is ready",
            ));
        }

        let credential_identity_changed = sf.oidc_config.as_ref().is_some_and(|current| {
            current.issuer_url != discovered.issuer || current.client_id != client_id
        });
        if credential_identity_changed && client_secret.is_none() && sf.oidc_secret.is_some() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "oidc_secret_reentry_required",
                "Re-enter the OIDC client secret when changing issuer or client ID",
            ));
        }

        // Update secret: if provided, encrypt and store; if omitted, preserve existing
        if let Some(secret) = client_secret {
            let encrypted = crypto::encrypt(&secret, &state.encryption_key).map_err(|e| {
                error!(error = %e, "failed to encrypt External Access OIDC client secret");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "encryption_error",
                    "Failed to encrypt OIDC client secret",
                )
            })?;
            sf.oidc_secret = Some(OidcSecretRecord {
                encrypted_client_secret: encrypted,
                stored_at: now,
            });
        }
        // When client_secret is None, sf.oidc_secret retains whatever was loaded

        has_client_secret = sf.oidc_secret.is_some();

        sf.oidc_config = Some(OidcConfigRecord {
            issuer_url: discovered.issuer.clone(),
            client_id: client_id.to_string(),
            has_client_secret,
            authorization_endpoint: discovered.authorization_endpoint.clone(),
            token_endpoint: discovered.token_endpoint.clone(),
            userinfo_endpoint: discovered.userinfo_endpoint.clone(),
            jwks_uri: discovered.jwks_uri.clone(),
            configured_at: now,
        });

        sf.updated_at = now;

        store.save(&sf).await.map_err(|e| {
            error!(error = %e, "failed to persist External Access OIDC configuration");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update OIDC configuration",
            )
        })?;
    }

    // Clear pending OIDC auth entries — any in-flight auth flows are now invalid
    // because the provider config just changed.
    {
        let mut pending = state.pending_auth.lock().await;
        pending.clear();
    }
    state.recovery_capabilities.clear().await;

    let details = serde_json::json!({
        "issuer_url": discovered.issuer.clone(),
        "has_client_secret": has_client_secret,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "external_access_oidc_configured",
        "instance_settings",
        Some("external_access"),
        Some(&details),
    )
    .await;

    Ok(Json(ConfigureExternalAccessOidcResponse {
        discovered_issuer: discovered.issuer,
        has_client_secret,
        configured_at: now,
    }))
}

pub async fn update_instance_preferences(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpdateInstancePreferencesRequest>,
) -> ApiResult<InstancePreferencesResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let pool = state.db.clone();
    let now = now_unix();

    if req.key_storage_mode != KeyStorageMode::File {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "unsupported_key_storage_mode",
            "Keychain mode is disabled in this release. Use file mode.",
        ));
    }

    let existing_mode = load_runtime_mode(&pool).await.map_err(|e| {
        error!(error = %e, "failed to load existing runtime mode");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update instance preferences",
        )
    })?;
    let existing_remote_auth_mode = load_remote_auth_mode(&pool).await.map_err(|e| {
        error!(error = %e, "failed to load existing remote auth mode");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update instance preferences",
        )
    })?;
    let existing_direct_macos_runner_enabled =
        load_direct_macos_runner_enabled(&pool).await.map_err(|e| {
            error!(error = %e, "failed to load existing direct runner policy");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update instance preferences",
            )
        })?;

    let runtime_mode = req.runtime_mode.unwrap_or(existing_mode);
    let remote_auth_mode = req.remote_auth_mode.unwrap_or(existing_remote_auth_mode);
    let direct_macos_runner_enabled = req
        .direct_macos_runner_enabled
        .unwrap_or(existing_direct_macos_runner_enabled);
    let runtime_mode_changed = runtime_mode != existing_mode;
    let remote_auth_mode_changed = remote_auth_mode != existing_remote_auth_mode;
    let direct_macos_runner_policy_changed =
        direct_macos_runner_enabled != existing_direct_macos_runner_enabled;
    let auth_policy_changed = runtime_mode_changed || remote_auth_mode_changed;

    if auth_policy_changed && auth.0.role != "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_owner_required",
            "Only the owner can change External Access authentication settings",
        ));
    }

    let mut preflight_result: Option<ExternalAccessPreflightResponse> = None;
    if runtime_mode == RuntimeMode::Remote && auth_policy_changed {
        let result = evaluate_external_access_preflight(&state, Some(remote_auth_mode)).await?;
        let preflight_details = serde_json::json!({
            "ready": result.ready,
            "failed_checks": preflight_failure_summary(&result),
            "remote_auth_mode": remote_auth_mode.to_string(),
        })
        .to_string();
        let _ = write_audit_log(
            &pool,
            Some(&auth.0.user_id),
            "external_access_preflight_checked",
            "instance_settings",
            Some("external_access"),
            Some(&preflight_details),
        )
        .await;

        if !result.ready {
            let blocked_details = serde_json::json!({
                "failed_checks": preflight_failure_summary(&result),
            })
            .to_string();
            let _ = write_audit_log(
                &pool,
                Some(&auth.0.user_id),
                "external_access_enable_blocked",
                "instance_settings",
                Some("external_access"),
                Some(&blocked_details),
            )
            .await;

            let first_failed = result.checks.iter().find(|check| !check.ok);
            let message = first_failed
                .map(|check| check.message.clone())
                .unwrap_or_else(|| {
                    "External Access cannot be enabled until all required checks pass.".to_string()
                });
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                first_preflight_failure_code(&result),
                message,
            ));
        }

        preflight_result = Some(result);
    }

    // File mode is the only supported mode in this release. The runtime key was
    // already loaded from (or created in) file storage during daemon startup, so
    // updating unrelated preferences must not rewrite it through a global path.
    let active_source = crypto::KeySource::LegacyFile;

    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, remote_auth_mode, direct_macos_runner_enabled, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
            key_storage_mode = excluded.key_storage_mode,
            runtime_mode = excluded.runtime_mode,
            remote_auth_mode = excluded.remote_auth_mode,
            direct_macos_runner_enabled = excluded.direct_macos_runner_enabled,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(KeyStorageMode::File.to_string())
    .bind(runtime_mode.to_string())
    .bind(remote_auth_mode.to_string())
    .bind(direct_macos_runner_enabled)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist instance preferences");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update instance preferences",
        )
    })?;

    let mut details_value = serde_json::json!({
        "key_storage_mode": KeyStorageMode::File.to_string(),
        "runtime_mode": runtime_mode.to_string(),
        "remote_auth_mode": remote_auth_mode.to_string(),
        "direct_macos_runner_enabled": direct_macos_runner_enabled,
        "active_key_source": active_source.as_str(),
    });
    if let Some(result) = preflight_result.as_ref() {
        details_value["external_access_preflight_ready"] = serde_json::json!(result.ready);
    }
    let details = details_value.to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "instance_preferences_updated",
        "instance_settings",
        Some("preferences"),
        Some(&details),
    )
    .await;

    if direct_macos_runner_policy_changed {
        let details = serde_json::json!({
            "previous_direct_macos_runner_enabled": existing_direct_macos_runner_enabled,
            "direct_macos_runner_enabled": direct_macos_runner_enabled,
            "updated_by": auth.0.email,
        })
        .to_string();
        let _ = write_audit_log(
            &pool,
            Some(&auth.0.user_id),
            "direct_macos_runner_policy_updated",
            "instance_settings",
            Some("direct_macos_runner"),
            Some(&details),
        )
        .await;
    }

    if auth_policy_changed {
        state.recovery_capabilities.clear().await;
        let revoked_sessions = state.sessions.revoke_all_sessions().await.map_err(|e| {
            error!(error = %e, "failed to revoke sessions after auth policy change");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to revoke sessions after auth policy change",
            )
        })?;

        let change_details = serde_json::json!({
            "from_mode": existing_mode.to_string(),
            "to_mode": runtime_mode.to_string(),
            "from_remote_auth_mode": existing_remote_auth_mode.to_string(),
            "to_remote_auth_mode": remote_auth_mode.to_string(),
            "revoked_sessions": revoked_sessions,
        })
        .to_string();
        if runtime_mode_changed {
            let _ = write_audit_log(
                &pool,
                Some(&auth.0.user_id),
                "runtime_mode_changed",
                "instance_settings",
                Some("preferences"),
                Some(&change_details),
            )
            .await;
        }
        if remote_auth_mode_changed {
            let _ = write_audit_log(
                &pool,
                Some(&auth.0.user_id),
                "remote_auth_mode_changed",
                "instance_settings",
                Some("preferences"),
                Some(&change_details),
            )
            .await;
        }

        if runtime_mode == RuntimeMode::Remote && (runtime_mode_changed || remote_auth_mode_changed)
        {
            let _ = write_audit_log(
                &pool,
                Some(&auth.0.user_id),
                "external_access_enabled",
                "instance_settings",
                Some("external_access"),
                Some(&change_details),
            )
            .await;
        }

        info!(
            mode = %KeyStorageMode::File,
            from_mode = %existing_mode,
            to_mode = %runtime_mode,
            from_remote_auth_mode = %existing_remote_auth_mode,
            to_remote_auth_mode = %remote_auth_mode,
            source = %active_source.as_str(),
            revoked_sessions,
            user_id = %auth.0.user_id,
            "instance preferences updated and sessions revoked after auth policy change"
        );
    } else {
        info!(
            mode = %KeyStorageMode::File,
            runtime_mode = %runtime_mode,
            remote_auth_mode = %remote_auth_mode,
            source = %active_source.as_str(),
            user_id = %auth.0.user_id,
            "instance preferences updated"
        );
    }

    Ok(Json(preferences_response(
        KeyStorageMode::File,
        runtime_mode,
        remote_auth_mode,
        direct_macos_runner_enabled,
        Some(now),
    )))
}
