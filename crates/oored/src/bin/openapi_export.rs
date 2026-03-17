//! Standalone binary that prints the Oore CI OpenAPI 3.1 specification to stdout.
//!
//! Usage:
//!   cargo run --bin openapi-export > apps/docs-site/docs/public/openapi.json
//!
//! This is used in CI (`make gen-openapi`) to generate a static spec file that
//! the VitePress docs site bundles and serves.

use utoipa::OpenApi;

/// Root OpenAPI document — registers every path and schema.
///
/// Schemas are pulled from `oore_contract` types via `ToSchema`.
/// Paths are declared inline with the `paths(…)` list.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Oore CI API",
        version = "1.0.0",
        description = "REST API for Oore CI — a self-hosted, Flutter-first mobile CI and internal app distribution platform.\n\nThe backend daemon (`oored`) exposes this API on the configured listen address. All endpoints under `/v1/` use JSON request/response bodies unless noted otherwise.\n\n## Authentication\n\n- **Setup endpoints** (`/v1/setup/*`) are token-gated by a bootstrap session token and auto-disabled after setup completes.\n- **Auth endpoints** (`/v1/auth/*`) support local-mode login and OIDC login/logout flows.\n- **All other endpoints** require a valid session token via `Authorization: Bearer <token>` header.\n- **Runner endpoints** use a separate runner token for authentication.\n\n## Base URL\n\nSince Oore CI is self-hosted, the base URL is your daemon's listen address (e.g. `http://localhost:8787`).",
        license(name = "MIT", url = "https://github.com/devaryakjha/oore.build/blob/master/LICENSE"),
        contact(name = "Oore CI", url = "https://oore.build"),
    ),
    servers(
        (url = "http://localhost:8787", description = "Local development (default)"),
        (url = "https://ci.example.com", description = "Self-hosted instance (replace with your URL)"),
    ),
    paths(
        // ── Health ──
        paths::healthz,
        // ── Setup ──
        paths::get_setup_status,
        paths::verify_bootstrap_token,
        paths::setup_preferences,
        paths::configure_oidc,
        paths::setup_trusted_proxy_configure,
        paths::setup_oidc_start,
        paths::setup_oidc_verify,
        paths::setup_owner_claim_trusted_proxy,
        paths::setup_local_owner_create,
        paths::complete_setup,
        paths::get_setup_summary,
        // ── Auth ──
        paths::oidc_start,
        paths::oidc_callback,
        paths::local_login,
        paths::trusted_proxy_login,
        paths::logout,
        // ── Users ──
        paths::get_me,
        paths::list_users,
        paths::invite_user,
        paths::update_user_role,
        paths::delete_user,
        paths::re_enable_user,
        // ── Instance Settings ──
        paths::get_artifact_storage_settings,
        paths::update_artifact_storage_settings,
        paths::get_instance_preferences,
        paths::update_instance_preferences,
        paths::get_external_access_network_settings,
        paths::update_external_access_network_settings,
        paths::get_external_access_trusted_proxy_settings,
        paths::update_external_access_trusted_proxy_settings,
        paths::get_external_access_preflight,
        paths::configure_external_access_oidc,
        // ── Retention Policy ──
        paths::get_retention_policy,
        paths::update_retention_policy,
        paths::get_retention_last_cleanup,
        paths::get_project_retention,
        paths::update_project_retention,
        paths::delete_project_retention,
        // ── Integrations ──
        paths::list_integrations,
        paths::get_integration,
        paths::delete_integration,
        paths::list_repositories,
        paths::list_installations,
        paths::sync_installations,
        paths::github_start,
        paths::github_complete,
        paths::gitlab_start,
        paths::gitlab_authorize,
        paths::browse_local_git_directories,
        paths::create_local_git_integration,
        paths::list_local_git_integrations,
        paths::delete_local_git_integration,
        // ── Projects ──
        paths::create_project,
        paths::list_projects,
        paths::get_project,
        paths::update_project,
        paths::delete_project,
        // ── Pipelines ──
        paths::create_pipeline,
        paths::list_pipelines,
        paths::get_pipeline,
        paths::update_pipeline,
        paths::delete_pipeline,
        paths::validate_pipeline,
        // ── Pipeline Signing (Android) ──
        paths::get_pipeline_android_signing,
        paths::update_pipeline_android_signing,
        // ── Pipeline Signing (iOS) ──
        paths::get_pipeline_ios_signing,
        paths::update_pipeline_ios_signing,
        paths::sync_pipeline_ios_signing,
        paths::list_pipeline_ios_devices,
        paths::register_pipeline_ios_device,
        // ── Builds ──
        paths::create_build,
        paths::list_builds,
        paths::get_build,
        paths::cancel_build,
        // ── Runners ──
        paths::register_runner,
        paths::list_runners,
        paths::update_runner,
        paths::runner_heartbeat,
        paths::claim_job,
        paths::update_job_status,
        paths::get_job_status,
        // ── Build Logs ──
        paths::append_build_logs,
        paths::get_build_logs,
        paths::stream_build_logs,
        paths::create_stream_token,
        // ── Artifacts ──
        paths::create_artifact,
        paths::list_artifacts,
        paths::generate_download_link,
        // ── Webhooks ──
        paths::github_webhook,
        paths::gitlab_webhook,
    ),
    components(schemas(
        // Setup
        oore_contract::SetupState,
        oore_contract::SetupStatus,
        oore_contract::BootstrapTokenVerifyRequest,
        oore_contract::BootstrapTokenVerifyResponse,
        oore_contract::SetupPreferencesRequest,
        oore_contract::SetupPreferencesResponse,
        oore_contract::OidcConfigureRequest,
        oore_contract::OidcConfigureResponse,
        oore_contract::SetupTrustedProxyConfigureRequest,
        oore_contract::SetupTrustedProxyConfigureResponse,
        oore_contract::SetupOidcStartRequest,
        oore_contract::SetupOidcStartResponse,
        oore_contract::SetupOidcVerifyRequest,
        oore_contract::SetupOidcVerifyResponse,
        oore_contract::SetupTrustedProxyClaimOwnerResponse,
        oore_contract::SetupLocalOwnerCreateRequest,
        oore_contract::SetupLocalOwnerCreateResponse,
        oore_contract::SetupCompleteResponse,
        oore_contract::SetupSummaryResponse,
        oore_contract::ApiError,
        // Auth
        oore_contract::OidcStartResponse,
        oore_contract::OidcCallbackResponse,
        oore_contract::LocalLoginRequest,
        oore_contract::LocalLoginResponse,
        oore_contract::AuthenticatedUser,
        oore_contract::LogoutResponse,
        // Users
        oore_contract::UserRole,
        oore_contract::UserStatus,
        oore_contract::User,
        oore_contract::InviteUserRequest,
        oore_contract::InviteUserResponse,
        oore_contract::UpdateUserRoleRequest,
        oore_contract::UpdateUserRoleResponse,
        oore_contract::ReEnableUserResponse,
        oore_contract::ListUsersResponse,
        oore_contract::UserProfileResponse,
        // Integrations
        oore_contract::ScmProvider,
        oore_contract::IntegrationAuthMode,
        oore_contract::IntegrationStatus,
        oore_contract::Integration,
        oore_contract::IntegrationInstallation,
        oore_contract::IntegrationRepository,
        oore_contract::GitHubAppStartRequest,
        oore_contract::GitHubAppStartResponse,
        oore_contract::GitHubAppCompleteRequest,
        oore_contract::GitHubAppCompleteResponse,
        oore_contract::SyncInstallationsRequest,
        oore_contract::SyncInstallationsResponse,
        oore_contract::GitLabStartRequest,
        oore_contract::GitLabCompleteResponse,
        oore_contract::GitLabAuthorizeRequest,
        oore_contract::GitLabAuthorizeResponse,
        oore_contract::LocalGitDirectoryEntry,
        oore_contract::LocalGitPathSuggestion,
        oore_contract::BrowseLocalGitDirectoriesResponse,
        oore_contract::CreateLocalGitIntegrationRequest,
        oore_contract::CreateLocalGitIntegrationResponse,
        oore_contract::ListIntegrationsResponse,
        oore_contract::IntegrationDetailResponse,
        oore_contract::ListInstallationsResponse,
        oore_contract::ListRepositoriesResponse,
        // Projects
        oore_contract::Project,
        oore_contract::CreateProjectRequest,
        oore_contract::CreateProjectResponse,
        oore_contract::UpdateProjectRequest,
        oore_contract::ProjectDetailResponse,
        oore_contract::ListProjectsResponse,
        // Pipelines
        oore_contract::BuildPlatform,
        oore_contract::PipelineCommandStages,
        oore_contract::PlatformBuildArgs,
        oore_contract::PlatformBuildCommands,
        oore_contract::PipelineEnvVar,
        oore_contract::PipelineExecutionConfig,
        oore_contract::TriggerConfig,
        oore_contract::ConcurrencyPolicy,
        oore_contract::Pipeline,
        oore_contract::CreatePipelineRequest,
        oore_contract::CreatePipelineResponse,
        oore_contract::UpdatePipelineRequest,
        oore_contract::PipelineDetailResponse,
        oore_contract::ListPipelinesResponse,
        oore_contract::ValidatePipelineRequest,
        oore_contract::ValidatePipelineResponse,
        // Android Signing
        oore_contract::AndroidSigningBuildType,
        oore_contract::AndroidSigningProfileInput,
        oore_contract::AndroidSigningProfile,
        oore_contract::UpdatePipelineAndroidSigningRequest,
        oore_contract::PipelineAndroidSigningResponse,
        // iOS Signing
        oore_contract::IosSigningMode,
        oore_contract::IosCertificateInput,
        oore_contract::IosProvisioningProfileInput,
        oore_contract::IosApiCredentialInput,
        oore_contract::UpdatePipelineIosSigningRequest,
        oore_contract::IosProvisioningProfileSummary,
        oore_contract::PipelineIosSigningResponse,
        oore_contract::RegisteredIosDevice,
        oore_contract::ListPipelineIosDevicesResponse,
        oore_contract::RegisterIosDeviceRequest,
        oore_contract::RegisterIosDeviceResponse,
        oore_contract::SyncPipelineIosSigningResponse,
        // Builds
        oore_contract::BuildStatus,
        oore_contract::TriggerType,
        oore_contract::Build,
        oore_contract::BuildEvent,
        oore_contract::CreateBuildRequest,
        oore_contract::CreateBuildResponse,
        oore_contract::BuildDetailResponse,
        oore_contract::ListBuildsResponse,
        oore_contract::CancelBuildResponse,
        oore_contract::StepResult,
        // Runners
        oore_contract::RunnerStatus,
        oore_contract::Runner,
        oore_contract::RegisterRunnerRequest,
        oore_contract::RegisterRunnerResponse,
        oore_contract::RunnerHeartbeatRequest,
        oore_contract::UpdateRunnerRequest,
        oore_contract::UpdateRunnerResponse,
        oore_contract::ClaimJobResponse,
        oore_contract::ClaimedJob,
        oore_contract::UpdateJobStatusRequest,
        oore_contract::ListRunnersResponse,
        oore_contract::JobStatusResponse,
        // Artifacts
        oore_contract::ArtifactStorageProvider,
        oore_contract::ArtifactStorageSource,
        oore_contract::ArtifactStorageSettings,
        oore_contract::Artifact,
        oore_contract::CreateArtifactRequest,
        oore_contract::CreateArtifactResponse,
        oore_contract::ListArtifactsResponse,
        oore_contract::ArtifactDownloadLinkResponse,
        oore_contract::ArtifactStorageSettingsResponse,
        oore_contract::UpdateArtifactStorageSettingsRequest,
        // Instance Settings
        oore_contract::KeyStorageMode,
        oore_contract::RuntimeMode,
        oore_contract::RemoteAuthMode,
        oore_contract::ExternalAccessNetworkSource,
        oore_contract::ExternalAccessNetworkSettings,
        oore_contract::ExternalAccessNetworkSettingsResponse,
        oore_contract::UpdateExternalAccessNetworkSettingsRequest,
        oore_contract::TrustedProxySettingsPublic,
        oore_contract::TrustedProxySettingsResponse,
        oore_contract::UpdateTrustedProxySettingsRequest,
        oore_contract::ExternalAccessPreflightCheck,
        oore_contract::ExternalAccessPreflightResponse,
        oore_contract::ConfigureExternalAccessOidcRequest,
        oore_contract::ConfigureExternalAccessOidcResponse,
        oore_contract::InstancePreferences,
        oore_contract::InstancePreferencesResponse,
        oore_contract::UpdateInstancePreferencesRequest,
        // Retention Policy
        oore_contract::RetentionCleanupTarget,
        oore_contract::RetentionPolicy,
        oore_contract::RetentionPolicyResponse,
        oore_contract::UpdateRetentionPolicyRequest,
        oore_contract::ProjectRetentionOverride,
        oore_contract::EffectiveProjectRetentionResponse,
        oore_contract::UpdateProjectRetentionOverrideRequest,
        oore_contract::RetentionCleanupSummary,
        oore_contract::RetentionCleanupSummaryResponse,
        // Build Logs
        oore_contract::BuildLogChunk,
        oore_contract::AppendBuildLogsRequest,
        oore_contract::AppendBuildLogsResponse,
        oore_contract::BuildLogsResponse,
    )),
    tags(
        (name = "Health", description = "Health check endpoint"),
        (name = "Setup", description = "Initial instance setup flow (bootstrap token → mode-aware owner creation). Auto-disabled after setup completes."),
        (name = "Auth", description = "Mode-aware authentication and session management. Enabled only after setup is complete."),
        (name = "Users", description = "User management — invite, list, update roles, disable/re-enable."),
        (name = "Instance Settings", description = "Instance-wide configuration — artifact storage, key storage preferences."),
        (name = "Retention Policy", description = "Build retention and cleanup — automatic cleanup of old builds and artifacts based on age, count, or size policies."),
        (name = "Integrations", description = "SCM integrations — local git, GitHub App, and GitLab."),
        (name = "Projects", description = "Project CRUD — each project groups one or more pipelines."),
        (name = "Pipelines", description = "Pipeline configuration — build platforms, commands, triggers, concurrency."),
        (name = "Pipeline Signing", description = "Code signing configuration — Android keystores, iOS certificates/profiles."),
        (name = "Builds", description = "Build lifecycle — queue, list, detail, cancel."),
        (name = "Runners", description = "Build runner management — register, heartbeat, job claim/status."),
        (name = "Build Logs", description = "Build log ingestion and retrieval — append, paginated fetch, SSE streaming."),
        (name = "Artifacts", description = "Build artifact management — upload, list, download via signed URLs."),
        (name = "Webhooks", description = "Incoming webhook receivers for GitHub and GitLab."),
    ),
    security(
        ("bearer_auth" = []),
    ),
)]
struct ApiDoc;

fn main() {
    let spec = ApiDoc::openapi()
        .to_pretty_json()
        .expect("failed to serialize OpenAPI spec");
    println!("{spec}");
}

// ── Path stubs ──────────────────────────────────────────────────────
// These `#[utoipa::path]` functions are documentation-only stubs.
// They map 1:1 to the real Axum handlers but exist solely so utoipa
// can generate accurate path items without touching the handler code.

#[allow(dead_code)]
mod paths {
    use oore_contract::*;

    // ── Health ──

    /// Health check
    ///
    /// Returns `{"ok": true}` when the daemon is running.
    #[utoipa::path(get, path = "/healthz", tag = "Health",
        responses((status = 200, description = "Daemon is healthy"))
    )]
    pub(super) async fn healthz() {}

    // ── Setup ──

    /// Get setup status
    ///
    /// Returns the current setup state and instance ID. This endpoint is
    /// always public and contains no sensitive information.
    #[utoipa::path(get, path = "/v1/public/setup-status", tag = "Setup",
        responses(
            (status = 200, description = "Current setup status", body = SetupStatus),
            (status = 500, description = "Internal error", body = ApiError),
        )
    )]
    pub(super) async fn get_setup_status() {}

    /// Verify bootstrap token
    ///
    /// Exchanges a one-time bootstrap token (generated by the CLI) for a
    /// setup session token. Rate-limited: 5 failed attempts per token hash
    /// triggers lockout.
    #[utoipa::path(post, path = "/v1/setup/bootstrap-token/verify", tag = "Setup",
        request_body = BootstrapTokenVerifyRequest,
        responses(
            (status = 200, description = "Token verified, session created", body = BootstrapTokenVerifyResponse),
            (status = 401, description = "Invalid token", body = ApiError),
            (status = 409, description = "Setup already complete", body = ApiError),
            (status = 410, description = "Token expired or consumed", body = ApiError),
            (status = 429, description = "Too many failed attempts", body = ApiError),
        )
    )]
    pub(super) async fn verify_bootstrap_token() {}

    /// Persist setup mode preferences
    ///
    /// Stores setup-time runtime mode and remote auth mode before owner creation.
    #[utoipa::path(post, path = "/v1/setup/preferences", tag = "Setup",
        request_body = SetupPreferencesRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Setup preferences saved", body = SetupPreferencesResponse),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 409, description = "Setup already complete or owner already created", body = ApiError),
        )
    )]
    pub(super) async fn setup_preferences() {}

    /// Configure OIDC provider
    ///
    /// Performs OIDC discovery on the provided issuer URL and stores the
    /// provider configuration. Requires a valid setup session.
    #[utoipa::path(post, path = "/v1/setup/oidc/configure", tag = "Setup",
        request_body = OidcConfigureRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "OIDC provider configured", body = OidcConfigureResponse),
            (status = 400, description = "Invalid input or discovery failed", body = ApiError),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 409, description = "Invalid state or already configured", body = ApiError),
        )
    )]
    pub(super) async fn configure_oidc() {}

    /// Configure trusted proxy auth during setup
    ///
    /// Upserts trusted proxy settings for remote trusted-proxy mode.
    #[utoipa::path(post, path = "/v1/setup/trusted-proxy/configure", tag = "Setup",
        request_body = SetupTrustedProxyConfigureRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Trusted proxy setup configured", body = SetupTrustedProxyConfigureResponse),
            (status = 400, description = "Invalid header/CIDR/secret input", body = ApiError),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 403, description = "Not in remote trusted-proxy mode", body = ApiError),
            (status = 409, description = "Setup already complete or owner already created", body = ApiError),
        )
    )]
    pub(super) async fn setup_trusted_proxy_configure() {}

    /// Start owner OIDC flow
    ///
    /// Initiates the OIDC authorization code flow to create the instance owner.
    /// Returns an authorization URL for the frontend to redirect to.
    #[utoipa::path(post, path = "/v1/setup/owner/start-oidc", tag = "Setup",
        request_body = SetupOidcStartRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Authorization URL generated", body = SetupOidcStartResponse),
            (status = 400, description = "Invalid redirect URI", body = ApiError),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 409, description = "Invalid state", body = ApiError),
            (status = 429, description = "Too many pending auth requests", body = ApiError),
        )
    )]
    pub(super) async fn setup_oidc_start() {}

    /// Verify owner OIDC callback
    ///
    /// Completes the setup OIDC flow by exchanging the authorization code
    /// for tokens and creating the owner account.
    #[utoipa::path(post, path = "/v1/setup/owner/verify-oidc", tag = "Setup",
        request_body = SetupOidcVerifyRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Owner created", body = SetupOidcVerifyResponse),
            (status = 400, description = "Invalid state or expired auth", body = ApiError),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 409, description = "Invalid state", body = ApiError),
        )
    )]
    pub(super) async fn setup_oidc_verify() {}

    /// Claim owner identity from trusted proxy headers
    ///
    /// Creates owner record from trusted proxy identity headers (email).
    #[utoipa::path(post, path = "/v1/setup/owner/claim-trusted-proxy", tag = "Setup",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Owner created from trusted proxy identity", body = SetupTrustedProxyClaimOwnerResponse),
            (status = 401, description = "Invalid setup session or missing/invalid identity header", body = ApiError),
            (status = 403, description = "Not in trusted-proxy mode or untrusted proxy peer", body = ApiError),
            (status = 409, description = "Invalid setup state", body = ApiError),
        )
    )]
    pub(super) async fn setup_owner_claim_trusted_proxy() {}

    /// Create local owner (local mode)
    ///
    /// Creates the setup owner without OIDC when runtime mode is `local`.
    #[utoipa::path(post, path = "/v1/setup/local-owner/create", tag = "Setup",
        request_body = SetupLocalOwnerCreateRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Owner created", body = SetupLocalOwnerCreateResponse),
            (status = 400, description = "Invalid owner email", body = ApiError),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 403, description = "Remote mode enabled", body = ApiError),
            (status = 409, description = "Invalid setup state", body = ApiError),
        )
    )]
    pub(super) async fn setup_local_owner_create() {}

    /// Complete setup
    ///
    /// Finalises the setup flow, transitions state to `ready`, inserts the
    /// owner into the users table, and clears the setup session.
    #[utoipa::path(post, path = "/v1/setup/complete", tag = "Setup",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Setup complete", body = SetupCompleteResponse),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 409, description = "Invalid state", body = ApiError),
        )
    )]
    pub(super) async fn complete_setup() {}

    /// Get setup summary
    ///
    /// Returns a summary of current setup configuration. Requires a valid
    /// setup session.
    #[utoipa::path(get, path = "/v1/setup/summary", tag = "Setup",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Setup summary", body = SetupSummaryResponse),
            (status = 401, description = "Invalid setup session", body = ApiError),
        )
    )]
    pub(super) async fn get_setup_summary() {}

    // ── Auth ──

    /// Start OIDC login
    ///
    /// Initiates the OIDC authorization code flow for user login.
    /// Only available after setup is complete.
    #[utoipa::path(get, path = "/v1/auth/oidc/start", tag = "Auth",
        params(
            ("redirect_uri" = String, Query, description = "Frontend callback URL"),
        ),
        responses(
            (status = 200, description = "Authorization URL generated", body = OidcStartResponse),
            (status = 400, description = "Invalid redirect URI", body = ApiError),
            (status = 409, description = "Setup not complete", body = ApiError),
        )
    )]
    pub(super) async fn oidc_start() {}

    /// OIDC callback
    ///
    /// Exchanges the authorization code for tokens and creates a session.
    #[utoipa::path(post, path = "/v1/auth/oidc/callback", tag = "Auth",
        request_body(content = inline(OidcCallbackParams), description = "Authorization code and state from OIDC provider"),
        responses(
            (status = 200, description = "Session created", body = OidcCallbackResponse),
            (status = 400, description = "Invalid state or code", body = ApiError),
            (status = 403, description = "User not authorized", body = ApiError),
        )
    )]
    pub(super) async fn oidc_callback() {}

    #[derive(serde::Deserialize, utoipa::ToSchema)]
    pub(super) struct OidcCallbackParams {
        pub code: String,
        pub state: String,
    }

    /// Local login
    ///
    /// Creates a loopback-only local session without OIDC.
    /// If setup is still pending in Local Only mode, first login auto-finalizes
    /// local owner bootstrap. When setup is already complete, loopback local
    /// login remains available even if External Access is enabled.
    #[utoipa::path(post, path = "/v1/auth/local/login", tag = "Auth",
        request_body = LocalLoginRequest,
        responses(
            (status = 200, description = "Session created", body = LocalLoginResponse),
            (status = 400, description = "Email required or invalid input", body = ApiError),
            (status = 403, description = "Blocked by mode policy or non-loopback source", body = ApiError),
        )
    )]
    pub(super) async fn local_login() {}

    /// Trusted proxy login
    ///
    /// Creates a session from trusted proxy identity headers when remote auth
    /// mode is configured to trusted proxy.
    #[utoipa::path(post, path = "/v1/auth/trusted-proxy/login", tag = "Auth",
        responses(
            (status = 200, description = "Session created", body = LocalLoginResponse),
            (status = 401, description = "Identity header missing or invalid", body = ApiError),
            (status = 403, description = "Untrusted peer, disabled user, or user missing", body = ApiError),
            (status = 409, description = "Setup incomplete", body = ApiError),
        )
    )]
    pub(super) async fn trusted_proxy_login() {}

    /// Logout
    ///
    /// Invalidates the current session.
    #[utoipa::path(post, path = "/v1/auth/logout", tag = "Auth",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Logged out", body = LogoutResponse),
        )
    )]
    pub(super) async fn logout() {}

    // ── Users ──

    /// Get current user
    ///
    /// Returns the profile of the currently authenticated user.
    #[utoipa::path(get, path = "/v1/users/me", tag = "Users",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "User profile", body = UserProfileResponse),
            (status = 401, description = "Not authenticated", body = ApiError),
        )
    )]
    pub(super) async fn get_me() {}

    /// List users
    ///
    /// Returns all users. Requires `owner` or `admin` role.
    #[utoipa::path(get, path = "/v1/users", tag = "Users",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "User list", body = ListUsersResponse),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) async fn list_users() {}

    /// Invite user
    ///
    /// Creates a new user with `invited` status. Requires `owner` or `admin` role.
    #[utoipa::path(post, path = "/v1/users/invite", tag = "Users",
        request_body = InviteUserRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "User invited", body = InviteUserResponse),
            (status = 400, description = "Invalid input", body = ApiError),
            (status = 403, description = "Forbidden", body = ApiError),
            (status = 409, description = "User already exists", body = ApiError),
        )
    )]
    pub(super) async fn invite_user() {}

    /// Update user role
    ///
    /// Changes a user's role. Requires `owner` or `admin` role.
    #[utoipa::path(patch, path = "/v1/users/{user_id}/role", tag = "Users",
        params(("user_id" = String, Path, description = "User ID")),
        request_body = UpdateUserRoleRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Role updated", body = UpdateUserRoleResponse),
            (status = 403, description = "Forbidden", body = ApiError),
            (status = 404, description = "User not found", body = ApiError),
        )
    )]
    pub(super) async fn update_user_role() {}

    /// Delete user
    ///
    /// Soft-deletes a user. Requires `owner` or `admin` role.
    #[utoipa::path(delete, path = "/v1/users/{user_id}", tag = "Users",
        params(("user_id" = String, Path, description = "User ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "User deleted"),
            (status = 403, description = "Forbidden", body = ApiError),
            (status = 404, description = "User not found", body = ApiError),
        )
    )]
    pub(super) async fn delete_user() {}

    /// Re-enable user
    ///
    /// Re-enables a previously disabled user.
    #[utoipa::path(post, path = "/v1/users/{user_id}/enable", tag = "Users",
        params(("user_id" = String, Path, description = "User ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "User re-enabled", body = ReEnableUserResponse),
            (status = 403, description = "Forbidden", body = ApiError),
            (status = 404, description = "User not found", body = ApiError),
        )
    )]
    pub(super) async fn re_enable_user() {}

    // ── Instance Settings ──

    /// Get artifact storage settings
    #[utoipa::path(get, path = "/v1/settings/artifact-storage", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Current storage settings", body = ArtifactStorageSettingsResponse),
        )
    )]
    pub(super) async fn get_artifact_storage_settings() {}

    /// Update artifact storage settings
    ///
    /// Configure the storage backend for build artifacts (local, S3, or R2).
    #[utoipa::path(put, path = "/v1/settings/artifact-storage", tag = "Instance Settings",
        request_body = UpdateArtifactStorageSettingsRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Storage settings updated", body = ArtifactStorageSettingsResponse),
            (status = 400, description = "Invalid configuration", body = ApiError),
        )
    )]
    pub(super) async fn update_artifact_storage_settings() {}

    /// Get instance preferences
    #[utoipa::path(get, path = "/v1/settings/preferences", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Current preferences", body = InstancePreferencesResponse),
        )
    )]
    pub(super) async fn get_instance_preferences() {}

    /// Update instance preferences
    ///
    /// Update instance-wide preferences (e.g. key storage mode).
    #[utoipa::path(put, path = "/v1/settings/preferences", tag = "Instance Settings",
        request_body = UpdateInstancePreferencesRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Preferences updated", body = InstancePreferencesResponse),
            (status = 400, description = "External Access preflight failed or unsupported values", body = ApiError),
            (status = 403, description = "Owner-only mode change attempted by non-owner", body = ApiError),
        )
    )]
    pub(super) async fn update_instance_preferences() {}

    /// Get External Access network settings
    ///
    /// Returns effective public URL and allowed frontend origins used by External Access checks.
    #[utoipa::path(get, path = "/v1/settings/external-access/network", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "External Access network settings", body = ExternalAccessNetworkSettingsResponse),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) async fn get_external_access_network_settings() {}

    /// Update External Access network settings
    ///
    /// Owner-only update for public URL and allowed frontend origins.
    #[utoipa::path(put, path = "/v1/settings/external-access/network", tag = "Instance Settings",
        request_body = UpdateExternalAccessNetworkSettingsRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "External Access network settings updated", body = ExternalAccessNetworkSettingsResponse),
            (status = 400, description = "Invalid configuration", body = ApiError),
            (status = 403, description = "Owner-only or loopback-only restriction violated", body = ApiError),
        )
    )]
    pub(super) async fn update_external_access_network_settings() {}

    /// Get trusted proxy runtime settings
    #[utoipa::path(get, path = "/v1/settings/external-access/trusted-proxy", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Trusted proxy settings", body = TrustedProxySettingsResponse),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) async fn get_external_access_trusted_proxy_settings() {}

    /// Update trusted proxy runtime settings
    #[utoipa::path(put, path = "/v1/settings/external-access/trusted-proxy", tag = "Instance Settings",
        request_body = UpdateTrustedProxySettingsRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Trusted proxy settings updated", body = TrustedProxySettingsResponse),
            (status = 400, description = "Invalid header/CIDR/secret input", body = ApiError),
            (status = 403, description = "Owner-only or loopback-only restriction violated", body = ApiError),
        )
    )]
    pub(super) async fn update_external_access_trusted_proxy_settings() {}

    /// Get External Access preflight readiness
    ///
    /// Returns check-by-check readiness required before enabling External Access (`runtime_mode=remote`).
    #[utoipa::path(get, path = "/v1/settings/external-access/preflight", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "External Access preflight result", body = ExternalAccessPreflightResponse),
        )
    )]
    pub(super) async fn get_external_access_preflight() {}

    /// Configure OIDC for External Access
    ///
    /// Owner-only endpoint to configure runtime OIDC after setup is complete.
    /// Performs provider discovery and stores issuer/client settings.
    #[utoipa::path(put, path = "/v1/settings/external-access/oidc", tag = "Instance Settings",
        request_body = ConfigureExternalAccessOidcRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "OIDC configured for External Access", body = ConfigureExternalAccessOidcResponse),
            (status = 400, description = "Invalid input or OIDC discovery failure", body = ApiError),
            (status = 403, description = "Owner-only operation", body = ApiError),
            (status = 409, description = "Setup state does not allow runtime OIDC configuration", body = ApiError),
        )
    )]
    pub(super) async fn configure_external_access_oidc() {}

    // ── Retention Policy ──

    /// Get global retention policy
    #[utoipa::path(get, path = "/v1/settings/retention", tag = "Retention Policy",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Current retention policy", body = RetentionPolicyResponse),
        )
    )]
    pub(super) async fn get_retention_policy() {}

    /// Update global retention policy
    #[utoipa::path(put, path = "/v1/settings/retention", tag = "Retention Policy",
        request_body = UpdateRetentionPolicyRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Policy updated", body = RetentionPolicyResponse),
        )
    )]
    pub(super) async fn update_retention_policy() {}

    /// Get last cleanup summary
    #[utoipa::path(get, path = "/v1/settings/retention/last-cleanup", tag = "Retention Policy",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Last cleanup summary", body = RetentionCleanupSummaryResponse),
        )
    )]
    pub(super) async fn get_retention_last_cleanup() {}

    /// Get project retention (effective policy merged with overrides)
    #[utoipa::path(get, path = "/v1/projects/{project_id}/retention", tag = "Retention Policy",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Effective project retention", body = EffectiveProjectRetentionResponse),
        )
    )]
    pub(super) async fn get_project_retention() {}

    /// Update project retention override
    #[utoipa::path(put, path = "/v1/projects/{project_id}/retention", tag = "Retention Policy",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = UpdateProjectRetentionOverrideRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Override updated", body = EffectiveProjectRetentionResponse),
        )
    )]
    pub(super) async fn update_project_retention() {}

    /// Delete project retention override (revert to global)
    #[utoipa::path(delete, path = "/v1/projects/{project_id}/retention", tag = "Retention Policy",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Override removed", body = EffectiveProjectRetentionResponse),
        )
    )]
    pub(super) async fn delete_project_retention() {}

    // ── Integrations ──

    /// List integrations
    ///
    /// Returns all SCM integrations, optionally filtered by provider.
    #[utoipa::path(get, path = "/v1/integrations", tag = "Integrations",
        params(
            ("provider" = Option<String>, Query, description = "Filter by SCM provider (github, gitlab)"),
            ("limit" = Option<i64>, Query, description = "Page size (default 50)"),
            ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Integration list", body = ListIntegrationsResponse),
        )
    )]
    pub(super) async fn list_integrations() {}

    /// Get integration detail
    #[utoipa::path(get, path = "/v1/integrations/{id}", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Integration detail", body = IntegrationDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn get_integration() {}

    /// Delete integration
    #[utoipa::path(delete, path = "/v1/integrations/{id}", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Integration deleted"),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn delete_integration() {}

    /// List integration repositories
    #[utoipa::path(get, path = "/v1/integrations/{id}/repositories", tag = "Integrations",
        params(
            ("id" = String, Path, description = "Integration ID"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Repository list", body = ListRepositoriesResponse),
        )
    )]
    pub(super) async fn list_repositories() {}

    /// List integration installations
    #[utoipa::path(get, path = "/v1/integrations/{id}/installations", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Installation list", body = ListInstallationsResponse),
        )
    )]
    pub(super) async fn list_installations() {}

    /// Sync integration installations
    ///
    /// - **GitHub**: Fetches GitHub App installations and syncs their repositories.
    /// - **GitLab**: Refreshes accessible projects for linked accounts and syncs them as repositories.
    #[utoipa::path(post, path = "/v1/integrations/{id}/installations", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        request_body = SyncInstallationsRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Installations synced", body = SyncInstallationsResponse),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) async fn sync_installations() {}

    /// Start GitHub App creation
    ///
    /// Returns a URL to navigate the browser to for GitHub App manifest creation.
    #[utoipa::path(post, path = "/v1/integrations/github/start", tag = "Integrations",
        request_body = GitHubAppStartRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "GitHub App creation URL", body = GitHubAppStartResponse),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) async fn github_start() {}

    /// Complete GitHub App creation
    ///
    /// Finalises the GitHub App manifest flow after the redirect back from GitHub.
    #[utoipa::path(post, path = "/v1/integrations/github/complete", tag = "Integrations",
        request_body = GitHubAppCompleteRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "GitHub App created", body = GitHubAppCompleteResponse),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) async fn github_complete() {}

    /// Start GitLab integration
    ///
    /// Creates a GitLab integration with OAuth or personal token auth.
    #[utoipa::path(post, path = "/v1/integrations/gitlab/start", tag = "Integrations",
        request_body = GitLabStartRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "GitLab integration created", body = GitLabCompleteResponse),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) async fn gitlab_start() {}

    /// Get GitLab OAuth authorization URL
    #[utoipa::path(post, path = "/v1/integrations/gitlab/authorize", tag = "Integrations",
        request_body = GitLabAuthorizeRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Authorization URL", body = GitLabAuthorizeResponse),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) async fn gitlab_authorize() {}

    /// Create local git integration
    #[utoipa::path(post, path = "/v1/integrations/local-git", tag = "Integrations",
        request_body = CreateLocalGitIntegrationRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Local git integration created", body = CreateLocalGitIntegrationResponse),
            (status = 400, description = "Invalid repository path", body = ApiError),
            (status = 403, description = "Local mode required", body = ApiError),
            (status = 409, description = "Repository already connected", body = ApiError),
        )
    )]
    pub(super) async fn create_local_git_integration() {}

    /// Browse local directories for local repository registration
    #[utoipa::path(get, path = "/v1/integrations/local-git/directories", tag = "Integrations",
        params(
            ("path" = Option<String>, Query, description = "Absolute directory path to browse. Defaults to the daemon user's home directory"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Directory listing", body = BrowseLocalGitDirectoriesResponse),
            (status = 400, description = "Invalid or inaccessible path", body = ApiError),
            (status = 403, description = "Local mode required", body = ApiError),
        )
    )]
    pub(super) async fn browse_local_git_directories() {}

    /// List local git integrations
    #[utoipa::path(get, path = "/v1/integrations/local-git", tag = "Integrations",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Local git integration list", body = ListIntegrationsResponse),
            (status = 403, description = "Local mode required", body = ApiError),
        )
    )]
    pub(super) async fn list_local_git_integrations() {}

    /// Delete local git integration
    #[utoipa::path(delete, path = "/v1/integrations/local-git/{id}", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Deleted", body = inline(serde_json::Value)),
            (status = 403, description = "Local mode required", body = ApiError),
            (status = 404, description = "Integration not found", body = ApiError),
        )
    )]
    pub(super) async fn delete_local_git_integration() {}

    // ── Projects ──

    /// Create project
    #[utoipa::path(post, path = "/v1/projects", tag = "Projects",
        request_body = CreateProjectRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Project created", body = CreateProjectResponse),
            (status = 400, description = "Invalid input", body = ApiError),
        )
    )]
    pub(super) async fn create_project() {}

    /// List projects
    #[utoipa::path(get, path = "/v1/projects", tag = "Projects",
        params(
            ("limit" = Option<i64>, Query, description = "Page size (default 50)"),
            ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
            ("search" = Option<String>, Query, description = "Filter by name (case-insensitive partial match)"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Project list", body = ListProjectsResponse),
        )
    )]
    pub(super) async fn list_projects() {}

    /// Get project detail
    #[utoipa::path(get, path = "/v1/projects/{project_id}", tag = "Projects",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Project detail", body = ProjectDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn get_project() {}

    /// Update project
    #[utoipa::path(patch, path = "/v1/projects/{project_id}", tag = "Projects",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = UpdateProjectRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Project updated"),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn update_project() {}

    /// Delete project
    #[utoipa::path(delete, path = "/v1/projects/{project_id}", tag = "Projects",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Project deleted"),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn delete_project() {}

    // ── Pipelines ──

    /// Create pipeline
    #[utoipa::path(post, path = "/v1/projects/{project_id}/pipelines", tag = "Pipelines",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = CreatePipelineRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Pipeline created", body = CreatePipelineResponse),
            (status = 400, description = "Invalid input", body = ApiError),
        )
    )]
    pub(super) async fn create_pipeline() {}

    /// List pipelines
    #[utoipa::path(get, path = "/v1/projects/{project_id}/pipelines", tag = "Pipelines",
        params(
            ("project_id" = String, Path, description = "Project ID"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Pipeline list", body = ListPipelinesResponse),
        )
    )]
    pub(super) async fn list_pipelines() {}

    /// Get pipeline detail
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}", tag = "Pipelines",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Pipeline detail", body = PipelineDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn get_pipeline() {}

    /// Update pipeline
    #[utoipa::path(patch, path = "/v1/pipelines/{pipeline_id}", tag = "Pipelines",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = UpdatePipelineRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Pipeline updated"),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn update_pipeline() {}

    /// Delete pipeline
    #[utoipa::path(delete, path = "/v1/pipelines/{pipeline_id}", tag = "Pipelines",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Pipeline deleted"),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn delete_pipeline() {}

    /// Validate pipeline config
    ///
    /// Validates a pipeline configuration without persisting it.
    #[utoipa::path(post, path = "/v1/pipelines/validate", tag = "Pipelines",
        request_body = ValidatePipelineRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Validation result", body = ValidatePipelineResponse),
        )
    )]
    pub(super) async fn validate_pipeline() {}

    // ── Pipeline Signing (Android) ──

    /// Get Android signing config
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/android-signing", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Android signing profiles", body = PipelineAndroidSigningResponse),
        )
    )]
    pub(super) async fn get_pipeline_android_signing() {}

    /// Update Android signing config
    ///
    /// Upload keystores and configure debug/release signing profiles.
    #[utoipa::path(put, path = "/v1/pipelines/{pipeline_id}/android-signing", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = UpdatePipelineAndroidSigningRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Signing config updated", body = PipelineAndroidSigningResponse),
        )
    )]
    pub(super) async fn update_pipeline_android_signing() {}

    // ── Pipeline Signing (iOS) ──

    /// Get iOS signing config
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/ios-signing", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "iOS signing configuration", body = PipelineIosSigningResponse),
        )
    )]
    pub(super) async fn get_pipeline_ios_signing() {}

    /// Update iOS signing config
    ///
    /// Upload certificates, provisioning profiles, and API credentials.
    /// Request body limit: 10 MiB.
    #[utoipa::path(put, path = "/v1/pipelines/{pipeline_id}/ios-signing", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = UpdatePipelineIosSigningRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Signing config updated", body = PipelineIosSigningResponse),
        )
    )]
    pub(super) async fn update_pipeline_ios_signing() {}

    /// Sync iOS provisioning profiles
    ///
    /// Re-downloads provisioning profiles from Apple via App Store Connect API.
    #[utoipa::path(post, path = "/v1/pipelines/{pipeline_id}/ios-signing/sync", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = UpdatePipelineIosSigningRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Profiles synced", body = SyncPipelineIosSigningResponse),
        )
    )]
    pub(super) async fn sync_pipeline_ios_signing() {}

    /// List registered iOS devices
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/ios-signing/devices", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Device list", body = ListPipelineIosDevicesResponse),
        )
    )]
    pub(super) async fn list_pipeline_ios_devices() {}

    /// Register iOS test device
    ///
    /// Registers a device UDID and optionally triggers a provisioning profile sync.
    #[utoipa::path(post, path = "/v1/pipelines/{pipeline_id}/ios-signing/devices/register", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = RegisterIosDeviceRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Device registered", body = RegisterIosDeviceResponse),
        )
    )]
    pub(super) async fn register_pipeline_ios_device() {}

    // ── Builds ──

    /// Create build
    ///
    /// Queues a new build for the specified project.
    #[utoipa::path(post, path = "/v1/projects/{project_id}/builds", tag = "Builds",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = CreateBuildRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Build queued", body = CreateBuildResponse),
            (status = 400, description = "Invalid input", body = ApiError),
            (status = 404, description = "Project or pipeline not found", body = ApiError),
            (status = 409, description = "Project source is not configured or resolvable", body = ApiError),
        )
    )]
    pub(super) async fn create_build() {}

    /// List builds
    ///
    /// Returns builds, optionally filtered by project, pipeline, or status.
    #[utoipa::path(get, path = "/v1/builds", tag = "Builds",
        params(
            ("limit" = Option<i64>, Query, description = "Page size (default 50)"),
            ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
            ("project_id" = Option<String>, Query, description = "Filter by project"),
            ("pipeline_id" = Option<String>, Query, description = "Filter by pipeline"),
            ("status" = Option<String>, Query, description = "Filter by status"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Build list", body = ListBuildsResponse),
        )
    )]
    pub(super) async fn list_builds() {}

    /// Get build detail
    #[utoipa::path(get, path = "/v1/builds/{build_id}", tag = "Builds",
        params(("build_id" = String, Path, description = "Build ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Build detail", body = BuildDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) async fn get_build() {}

    /// Cancel build
    ///
    /// Cancels a queued or running build.
    #[utoipa::path(post, path = "/v1/builds/{build_id}/cancel", tag = "Builds",
        params(("build_id" = String, Path, description = "Build ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Build cancelled", body = CancelBuildResponse),
            (status = 404, description = "Not found", body = ApiError),
            (status = 409, description = "Build not in cancellable state", body = ApiError),
        )
    )]
    pub(super) async fn cancel_build() {}

    // ── Runners ──

    /// Register runner
    ///
    /// Registers a new build runner and returns a runner authentication token.
    #[utoipa::path(post, path = "/v1/runners/register", tag = "Runners",
        request_body = RegisterRunnerRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Runner registered", body = RegisterRunnerResponse),
        )
    )]
    pub(super) async fn register_runner() {}

    /// List runners
    #[utoipa::path(get, path = "/v1/runners", tag = "Runners",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Runner list", body = ListRunnersResponse),
        )
    )]
    pub(super) async fn list_runners() {}

    /// Update runner
    #[utoipa::path(patch, path = "/v1/runners/{runner_id}", tag = "Runners",
        params(("runner_id" = String, Path, description = "Runner ID")),
        request_body = UpdateRunnerRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Runner updated", body = UpdateRunnerResponse),
        )
    )]
    pub(super) async fn update_runner() {}

    /// Runner heartbeat
    ///
    /// Sends a heartbeat from a runner to report its status. Authenticated via runner token.
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/heartbeat", tag = "Runners",
        params(("runner_id" = String, Path, description = "Runner ID")),
        request_body = RunnerHeartbeatRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Heartbeat recorded"),
        )
    )]
    pub(super) async fn runner_heartbeat() {}

    /// Claim job
    ///
    /// Runner claims the next queued build. Returns `null` job if no builds are available.
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/claim", tag = "Runners",
        params(("runner_id" = String, Path, description = "Runner ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Job claimed (or null)", body = ClaimJobResponse),
        )
    )]
    pub(super) async fn claim_job() {}

    /// Update job status
    ///
    /// Runner reports build status transitions and step results.
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/jobs/{job_id}/status", tag = "Runners",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
        ),
        request_body = UpdateJobStatusRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Status updated"),
            (status = 409, description = "Invalid state transition", body = ApiError),
        )
    )]
    pub(super) async fn update_job_status() {}

    /// Get job status
    ///
    /// Runner checks the current status of its assigned build.
    #[utoipa::path(get, path = "/v1/runners/{runner_id}/jobs/{job_id}", tag = "Runners",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Job status", body = JobStatusResponse),
        )
    )]
    pub(super) async fn get_job_status() {}

    // ── Build Logs ──

    /// Append build logs
    ///
    /// Runner appends log chunks to a build. Max 10,000 lines per build, 4KB per line.
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/jobs/{job_id}/logs", tag = "Build Logs",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
        ),
        request_body = AppendBuildLogsRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Logs appended", body = AppendBuildLogsResponse),
        )
    )]
    pub(super) async fn append_build_logs() {}

    /// Get build logs
    ///
    /// Returns paginated historical build logs.
    #[utoipa::path(get, path = "/v1/builds/{build_id}/logs", tag = "Build Logs",
        params(
            ("build_id" = String, Path, description = "Build ID"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Build logs", body = BuildLogsResponse),
        )
    )]
    pub(super) async fn get_build_logs() {}

    /// Stream build logs (SSE)
    ///
    /// Server-sent event stream of build logs. Requires a short-lived streaming
    /// token obtained from `POST /v1/builds/{build_id}/stream-token`.
    #[utoipa::path(get, path = "/v1/builds/{build_id}/logs/stream", tag = "Build Logs",
        params(
            ("build_id" = String, Path, description = "Build ID"),
            ("token" = String, Query, description = "Short-lived streaming token"),
        ),
        responses(
            (status = 200, description = "SSE log stream (text/event-stream)"),
            (status = 401, description = "Invalid or expired token", body = ApiError),
        )
    )]
    pub(super) async fn stream_build_logs() {}

    /// Create stream token
    ///
    /// Exchanges a session token for a short-lived (5 min) streaming token
    /// suitable for use in SSE query parameters.
    #[utoipa::path(post, path = "/v1/builds/{build_id}/stream-token", tag = "Build Logs",
        params(("build_id" = String, Path, description = "Build ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Streaming token created"),
        )
    )]
    pub(super) async fn create_stream_token() {}

    // ── Artifacts ──

    /// Create artifact
    ///
    /// Runner creates an artifact record and gets a signed upload URL.
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/jobs/{job_id}/artifacts", tag = "Artifacts",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
        ),
        request_body = CreateArtifactRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Artifact created with upload URL", body = CreateArtifactResponse),
        )
    )]
    pub(super) async fn create_artifact() {}

    /// List build artifacts
    #[utoipa::path(get, path = "/v1/builds/{build_id}/artifacts", tag = "Artifacts",
        params(("build_id" = String, Path, description = "Build ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Artifact list", body = ListArtifactsResponse),
        )
    )]
    pub(super) async fn list_artifacts() {}

    /// Generate download link
    ///
    /// Returns a signed download URL for an artifact. URLs expire after 15 minutes.
    #[utoipa::path(post, path = "/v1/artifacts/{artifact_id}/download-link", tag = "Artifacts",
        params(("artifact_id" = String, Path, description = "Artifact ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Signed download URL", body = ArtifactDownloadLinkResponse),
            (status = 404, description = "Artifact not found", body = ApiError),
        )
    )]
    pub(super) async fn generate_download_link() {}

    // ── Webhooks ──

    /// GitHub webhook receiver
    ///
    /// Receives push/PR events from GitHub and triggers matching pipelines.
    /// Authenticated via webhook signature verification.
    #[utoipa::path(post, path = "/v1/webhooks/github", tag = "Webhooks",
        responses(
            (status = 200, description = "Webhook processed"),
            (status = 401, description = "Invalid signature"),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) async fn github_webhook() {}

    /// GitLab webhook receiver
    ///
    /// Receives push/MR events from GitLab and triggers matching pipelines.
    /// Authenticated via webhook secret token.
    #[utoipa::path(post, path = "/v1/webhooks/gitlab", tag = "Webhooks",
        responses(
            (status = 200, description = "Webhook processed"),
            (status = 401, description = "Invalid token"),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) async fn gitlab_webhook() {}
}
