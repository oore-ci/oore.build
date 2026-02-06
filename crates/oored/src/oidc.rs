use std::fmt;
use std::time::Duration;

use openidconnect::core::CoreProviderMetadata;
use openidconnect::IssuerUrl;

// ── Discovery result ────────────────────────────────────────────

/// Metadata returned after successful OIDC provider discovery.
#[derive(Debug, Clone)]
pub struct DiscoveredProvider {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
    pub scopes_supported: Vec<String>,
}

// ── Discovery error ─────────────────────────────────────────────

/// Errors that can occur during OIDC provider discovery.
#[derive(Debug)]
pub enum OidcDiscoveryError {
    /// The issuer URL could not be parsed.
    InvalidIssuerUrl(String),
    /// The HTTP request to .well-known/openid-configuration failed.
    DiscoveryFailed(String),
    /// Required endpoints (authorization, token) were not found in provider metadata.
    MissingEndpoints(String),
}

impl fmt::Display for OidcDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIssuerUrl(msg) => write!(f, "Invalid issuer URL: {}", msg),
            Self::DiscoveryFailed(msg) => write!(f, "OIDC discovery failed: {}", msg),
            Self::MissingEndpoints(msg) => write!(f, "Missing required endpoints: {}", msg),
        }
    }
}

impl std::error::Error for OidcDiscoveryError {}

// ── Discovery function ──────────────────────────────────────────

/// Perform OIDC discovery for the given issuer URL.
///
/// Fetches `.well-known/openid-configuration` and validates the provider.
/// Returns discovered metadata or an error.
///
/// The HTTP client is configured with a 10-second timeout and does not
/// follow redirects (to prevent SSRF vulnerabilities, per openidconnect
/// crate recommendations).
pub async fn discover_provider(issuer_url: &str) -> Result<DiscoveredProvider, OidcDiscoveryError> {
    // Parse the issuer URL
    let issuer = IssuerUrl::new(issuer_url.to_string())
        .map_err(|e| OidcDiscoveryError::InvalidIssuerUrl(e.to_string()))?;

    // Build an HTTP client with timeout and no-redirect policy
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| OidcDiscoveryError::DiscoveryFailed(format!("failed to build HTTP client: {}", e)))?;

    // Perform OIDC discovery
    let provider_metadata =
        CoreProviderMetadata::discover_async(issuer, &http_client)
            .await
            .map_err(|e| OidcDiscoveryError::DiscoveryFailed(e.to_string()))?;

    // Extract the authorization endpoint (required)
    let authorization_endpoint = provider_metadata.authorization_endpoint().to_string();

    // Extract the token endpoint (required for code flow)
    let token_endpoint = provider_metadata
        .token_endpoint()
        .ok_or_else(|| {
            OidcDiscoveryError::MissingEndpoints(
                "token_endpoint is required but not present in provider metadata".to_string(),
            )
        })?
        .to_string();

    // Extract optional userinfo endpoint
    let userinfo_endpoint = provider_metadata
        .userinfo_endpoint()
        .map(|u| u.to_string());

    // Extract JWKS URI (required)
    let jwks_uri = provider_metadata.jwks_uri().to_string();

    // Extract supported scopes (optional, default to empty)
    let scopes_supported = provider_metadata
        .scopes_supported()
        .map(|scopes| scopes.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    // Extract the validated issuer (as returned by the provider)
    let issuer = provider_metadata.issuer().to_string();

    Ok(DiscoveredProvider {
        issuer,
        authorization_endpoint,
        token_endpoint,
        userinfo_endpoint,
        jwks_uri,
        scopes_supported,
    })
}
