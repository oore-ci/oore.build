//! Standalone binary that prints the Oore CI OpenAPI 3.1 specification to stdout.
//!
//! Usage:
//!   cargo run -p oore-contract --bin openapi-export --locked > apps/docs/public/openapi.json
//!
//! This is used in CI (`make gen-openapi`) to generate a static spec file that
//! the Fumadocs site bundles and serves.

use utoipa::{
    Modify, OpenApi,
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
};

/// Root OpenAPI document — registers every path and schema.
///
/// Schemas are pulled from `oore_contract` types via `ToSchema`.
/// Paths are declared inline with the `paths(…)` list.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Oore CI API",
        version = "1.0.0",
        description = "REST API for Oore CI — a self-hosted, Flutter-first mobile CI and internal app distribution platform.\n\nThe backend daemon (`oored`) exposes this API on the configured listen address. All endpoints under `/v1/` use JSON request/response bodies unless noted otherwise.\n\n## Authentication\n\n- **Setup endpoints** (`/v1/setup/*`) are token-gated by a bootstrap session token and auto-disabled after setup completes.\n- **Auth endpoints** (`/v1/auth/*`) support Local Only login, single-use local recovery in Ready External Access mode, and configured OIDC or trusted-proxy flows.\n- **Protected API endpoints** declare their session, API, or runner bearer-token requirement individually.\n- **Callbacks, webhooks, metrics, readiness, and signed-token delivery routes** use their route-specific public, state, signature, or token contract instead of a global bearer requirement.\n\n## Base URL\n\nSince Oore CI is self-hosted, the base URL is your daemon's listen address (e.g. `http://localhost:8787`).",
        license(name = "MIT", url = "https://github.com/oore-ci/oore.build/blob/master/LICENSE"),
        contact(name = "Oore CI", url = "https://oore.build"),
    ),
    servers(
        (url = "http://localhost:8787", description = "Local development (default)"),
        (url = "https://ci.example.com", description = "Self-hosted instance (replace with your URL)"),
    ),
    paths(
        // ── Health ──
        operations::healthz,
        operations::readyz,
        // ── System ──
        operations::metrics,
        // ── Setup ──
        operations::get_setup_status,
        operations::frontend_pair,
        operations::verify_bootstrap_token,
        operations::setup_preferences,
        operations::configure_oidc,
        operations::setup_trusted_proxy_configure,
        operations::setup_oidc_start,
        operations::setup_oidc_verify,
        operations::setup_owner_claim_trusted_proxy,
        operations::setup_local_owner_create,
        operations::complete_setup,
        operations::get_setup_summary,
        // ── Auth ──
        operations::oidc_start,
        operations::oidc_callback,
        operations::local_login,
        operations::trusted_proxy_login,
        operations::logout,
        // ── Runtime updates ──
        operations::get_runtime_update_status,
        operations::start_runtime_update,
        // ── Users ──
        operations::get_me,
        operations::list_users,
        operations::invite_user,
        operations::update_user_role,
        operations::delete_user,
        operations::re_enable_user,
        // ── Instance Settings ──
        operations::get_artifact_storage_settings,
        operations::update_artifact_storage_settings,
        operations::get_instance_preferences,
        operations::update_instance_preferences,
        operations::get_external_access_network_settings,
        operations::update_external_access_network_settings,
        operations::get_external_access_trusted_proxy_settings,
        operations::update_external_access_trusted_proxy_settings,
        operations::get_external_access_preflight,
        operations::get_external_access_oidc,
        operations::configure_external_access_oidc,
        operations::test_oidc_connection,
        // ── Retention Policy ──
        operations::get_retention_policy,
        operations::update_retention_policy,
        operations::get_retention_last_cleanup,
        operations::get_project_retention,
        operations::update_project_retention,
        operations::delete_project_retention,
        // ── Integrations ──
        operations::list_integrations,
        operations::get_integration,
        operations::delete_integration,
        operations::list_repositories,
        operations::list_source_repositories,
        operations::repository_avatar,
        operations::list_installations,
        operations::sync_installations,
        operations::github_start,
        operations::github_complete,
        operations::github_create_page,
        operations::github_callback,
        operations::github_installed,
        operations::gitlab_start,
        operations::gitlab_authorize,
        operations::gitlab_callback,
        operations::check_gitlab_personal_token,
        operations::replace_gitlab_personal_token,
        operations::rotate_gitlab_repository_webhook_secret,
        operations::browse_local_git_directories,
        operations::create_local_git_integration,
        operations::list_local_git_integrations,
        operations::delete_local_git_integration,
        operations::list_operator_incidents,
        operations::mark_operator_incident_read,
        // ── Projects ──
        operations::create_project,
        operations::list_projects,
        operations::get_project,
        operations::update_project,
        operations::delete_project,
        operations::discover_repository_workflows,
        // ── Project Members ──
        operations::list_project_members,
        operations::list_project_member_candidates,
        operations::add_project_member,
        operations::update_project_member,
        operations::remove_project_member,
        // ── Pipelines ──
        operations::create_pipeline,
        operations::list_pipelines,
        operations::get_pipeline,
        operations::update_pipeline,
        operations::delete_pipeline,
        operations::validate_pipeline,
        // ── Pipeline Signing (Android) ──
        operations::get_pipeline_android_signing,
        operations::update_pipeline_android_signing,
        // ── Pipeline Signing (iOS) ──
        operations::get_pipeline_ios_signing,
        operations::update_pipeline_ios_signing,
        operations::sync_pipeline_ios_signing,
        operations::list_pipeline_ios_devices,
        operations::register_pipeline_ios_device,
        // ── Pipeline Publishing ──
        operations::get_pipeline_google_play_publish,
        operations::update_pipeline_google_play_publish,
        // ── Secrets ──
        operations::list_instance_secrets,
        operations::create_instance_secret,
        operations::rotate_instance_secret,
        operations::delete_instance_secret,
        operations::list_project_secrets,
        operations::create_project_secret,
        operations::rotate_project_secret,
        operations::delete_project_secret,
        operations::list_pipeline_secrets,
        operations::create_pipeline_secret,
        operations::rotate_pipeline_secret,
        operations::delete_pipeline_secret,
        // ── Builds ──
        operations::create_build,
        operations::preview_build_changelog,
        operations::list_builds,
        operations::get_build,
        operations::cancel_build,
        operations::rerun_build,
        // ── Audit Logs ──
        operations::list_audit_logs,
        // ── API Tokens ──
        operations::list_api_tokens,
        operations::create_api_token,
        operations::revoke_api_token,
        // ── Runners ──
        operations::register_runner,
        operations::list_runners,
        operations::get_runner,
        operations::update_runner,
        operations::delete_runner,
        operations::runner_heartbeat,
        operations::claim_job,
        operations::gitlab_checkout_discovery,
        operations::gitlab_checkout_upload_pack,
        operations::update_job_status,
        operations::get_job_status,
        operations::get_job_android_signing,
        operations::get_job_ios_signing,
        operations::get_job_secrets,
        operations::get_job_publish_plan,
        operations::report_job_publish_result,
        // ── Build Logs ──
        operations::append_build_logs,
        operations::get_build_logs,
        operations::stream_build_logs,
        operations::create_stream_token,
        // ── Artifacts ──
        operations::create_artifact,
        operations::complete_artifact,
        operations::abort_artifact,
        operations::list_artifacts,
        operations::list_project_artifacts,
        operations::list_build_artifacts,
        operations::generate_download_link,
        operations::create_artifact_install_link,
        operations::get_ios_install_manifest,
        operations::upload_local_artifact,
        operations::download_local_artifact,
        operations::download_local_artifact_legacy,
        operations::get_ios_install_manifest_v1,
        operations::download_local_artifact_install,
        // ── Scoped Download Tokens (OOR-140) ──
        operations::create_scoped_download_token,
        operations::list_scoped_download_tokens,
        operations::revoke_scoped_download_token,
        operations::download_via_scoped_token,
        operations::download_via_scoped_token_v1,
        // ── Notification Channels ──
        operations::list_notification_channels,
        operations::create_notification_channel,
        operations::get_notification_channel,
        operations::update_notification_channel,
        operations::delete_notification_channel,
        operations::test_notification_channel,
        operations::list_notification_deliveries,
        // ── Webhooks ──
        operations::github_webhook,
        operations::gitlab_webhook,
    ),
    components(schemas(
        // Setup
        oore_contract::SetupState,
        oore_contract::SetupStatus,
        oore_contract::FrontendPairRequest,
        oore_contract::FrontendPairResponse,
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
        oore_contract::OkResponse,
        oore_contract::HealthResponse,
        oore_contract::StreamTokenResponse,
        oore_contract::WebhookResponse,
        oore_contract::BinaryPayload,
        // Auth
        oore_contract::OidcStartResponse,
        oore_contract::OidcCallbackResponse,
        oore_contract::LocalLoginRequest,
        oore_contract::LocalLoginResponse,
        oore_contract::AuthenticatedUser,
        oore_contract::LogoutResponse,
        // Runtime updates
        oore_contract::RuntimeUpdatePhase,
        oore_contract::RuntimeUpdateStatus,
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
        oore_contract::GitLabCredentialStatus,
        oore_contract::GitLabCredentialStatusResponse,
        oore_contract::ReplaceGitLabTokenRequest,
        oore_contract::GitLabRepositoryWebhookSecretResponse,
        oore_contract::LocalGitDirectoryEntry,
        oore_contract::LocalGitPathSuggestion,
        oore_contract::BrowseLocalGitDirectoriesResponse,
        oore_contract::CreateLocalGitIntegrationRequest,
        oore_contract::CreateLocalGitIntegrationResponse,
        oore_contract::ListIntegrationsResponse,
        oore_contract::IntegrationDetailResponse,
        oore_contract::OperatorIncidentStatus,
        oore_contract::OperatorIncidentSeverity,
        oore_contract::OperatorIncident,
        oore_contract::ListOperatorIncidentsResponse,
        oore_contract::ListInstallationsResponse,
        oore_contract::ListRepositoriesResponse,
        oore_contract::SourceRepository,
        oore_contract::ListSourceRepositoriesResponse,
        // Projects
        oore_contract::Project,
        oore_contract::CreateProjectRequest,
        oore_contract::CreateProjectResponse,
        oore_contract::UpdateProjectRequest,
        oore_contract::ProjectDetailResponse,
        oore_contract::ListProjectsResponse,
        oore_contract::RepositoryWorkflowExecutionPreview,
        oore_contract::RepositoryWorkflowPreview,
        oore_contract::DiscoverRepositoryWorkflowsResponse,
        // Project Members
        oore_contract::ProjectRole,
        oore_contract::ProjectMember,
        oore_contract::ProjectMemberCandidate,
        oore_contract::AddProjectMemberRequest,
        oore_contract::AddProjectMemberResponse,
        oore_contract::UpdateProjectMemberRequest,
        oore_contract::UpdateProjectMemberResponse,
        oore_contract::RemoveProjectMemberResponse,
        oore_contract::ListProjectMembersResponse,
        oore_contract::ListProjectMemberCandidatesResponse,
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
        // Publishing
        oore_contract::GooglePlayReleaseNotesSource,
        oore_contract::GooglePlayPublishConfig,
        oore_contract::PipelineGooglePlayPublishResponse,
        oore_contract::RunnerPublishPlan,
        oore_contract::PublishStore,
        oore_contract::RunnerPublishResult,
        // Secrets
        oore_contract::SecretScope,
        oore_contract::SecretSummary,
        oore_contract::ListSecretsResponse,
        oore_contract::CreateSecretRequest,
        oore_contract::RotateSecretRequest,
        oore_contract::SecretResponse,
        oore_contract::RunnerJobSecret,
        oore_contract::RunnerJobSecretsResponse,
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
        oore_contract::RunnerPolicyBlockReason,
        oore_contract::TriggerType,
        oore_contract::Build,
        oore_contract::BuildEvent,
        oore_contract::CreateBuildRequest,
        oore_contract::CreateBuildResponse,
        oore_contract::BuildChangelogPreviewResponse,
        oore_contract::BuildDetailResponse,
        oore_contract::ListBuildsResponse,
        oore_contract::CancelBuildResponse,
        oore_contract::RerunBuildResponse,
        oore_contract::StepResult,
        // Runners
        oore_contract::RunnerStatus,
        oore_contract::Runner,
        oore_contract::RegisterRunnerRequest,
        oore_contract::RegisterRunnerResponse,
        oore_contract::RunnerHeartbeatRequest,
        oore_contract::UpdateRunnerRequest,
        oore_contract::UpdateRunnerResponse,
        oore_contract::ClaimJobRequest,
        oore_contract::ClaimJobResponse,
        oore_contract::ClaimedJob,
        oore_contract::UpdateJobStatusRequest,
        oore_contract::ListRunnersResponse,
        oore_contract::JobStatusResponse,
        oore_contract::RunnerAndroidSigningProfile,
        oore_contract::RunnerAndroidSigningResponse,
        oore_contract::RunnerIosProvisioningProfile,
        oore_contract::RunnerIosSigningBundle,
        oore_contract::RunnerIosSigningResponse,
        // Artifacts
        oore_contract::ArtifactStorageProvider,
        oore_contract::ArtifactStorageSource,
        oore_contract::ArtifactStorageSettings,
        oore_contract::Artifact,
        oore_contract::ArtifactType,
        oore_contract::CreateArtifactRequest,
        oore_contract::CreateArtifactResponse,
        oore_contract::CompleteArtifactRequest,
        oore_contract::CompleteArtifactResponse,
        oore_contract::ListBuildArtifactsRequest,
        oore_contract::ListArtifactsResponse,
        oore_contract::ArtifactDownloadLinkResponse,
        oore_contract::ArtifactInstallPlatform,
        oore_contract::ArtifactInstallLinkResponse,
        oore_contract::CreateScopedDownloadTokenRequest,
        oore_contract::CreateScopedDownloadTokenResponse,
        oore_contract::ArtifactDownloadTokenSummary,
        oore_contract::ListArtifactDownloadTokensResponse,
        oore_contract::RevokeArtifactDownloadTokenResponse,
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
        oore_contract::GetExternalAccessOidcResponse,
        oore_contract::TestOidcConnectionRequest,
        oore_contract::TestOidcConnectionResponse,
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
        oore_contract::BuildLogStream,
        oore_contract::AppendBuildLogsRequest,
        oore_contract::AppendBuildLogsResponse,
        oore_contract::BuildLogsResponse,
        // Notification Channels
        oore_contract::NotificationChannelType,
        oore_contract::SmtpTlsMode,
        oore_contract::SmtpConfig,
        oore_contract::UpdateSmtpConfig,
        oore_contract::NotificationDeliveryStatus,
        oore_contract::NotificationChannel,
        oore_contract::CreateNotificationChannelRequest,
        oore_contract::UpdateNotificationChannelRequest,
        oore_contract::NotificationChannelResponse,
        oore_contract::ListNotificationChannelsResponse,
        oore_contract::DeleteNotificationChannelResponse,
        oore_contract::TestNotificationChannelResponse,
        oore_contract::NotificationDelivery,
        oore_contract::ListNotificationDeliveriesResponse,
        // Audit Logs
        oore_contract::AuditLogEntry,
        oore_contract::ListAuditLogsResponse,
        // API Tokens
        oore_contract::CreateApiTokenRequest,
        oore_contract::CreateApiTokenResponse,
        oore_contract::ApiTokenSummary,
        oore_contract::ListApiTokensResponse,
        oore_contract::RevokeApiTokenResponse,
        operations::ReadinessResponse,
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
        (name = "Project Members", description = "Per-project role assignments for granular RBAC."),
        (name = "Pipelines", description = "Pipeline configuration — build platforms, commands, triggers, concurrency."),
        (name = "Pipeline Publishing", description = "Store publishing settings. The runner uploads from its own host after a successful build."),
        (name = "Secrets", description = "Write-only secrets scoped to the instance, a project or a pipeline. Values are encrypted at rest, referenced as ${{ secrets.NAME }} and never returned."),
        (name = "Pipeline Signing", description = "Code signing configuration — Android keystores, iOS certificates/profiles."),
        (name = "Builds", description = "Build lifecycle — queue, list, detail, cancel."),
        (name = "Runners", description = "Build runner management — register, heartbeat, job claim/status."),
        (name = "Build Logs", description = "Build log ingestion and retrieval — append, paginated fetch, SSE streaming."),
        (name = "Artifacts", description = "Build artifact management — upload, list, download via signed URLs."),
        (name = "Notification Channels", description = "Outbound notification channel configuration — webhook, Mattermost."),
        (name = "Audit Logs", description = "Read-only audit trail of all user and system actions."),
        (name = "API Tokens", description = "API token management — create, list, and revoke tokens for programmatic access."),
        (name = "Webhooks", description = "Incoming webhook receivers for GitHub and GitLab."),
        (name = "System", description = "Daemon runtime and metrics."),
        (name = "Scoped Download Tokens", description = "Time-limited share tokens and token-authorized artifact delivery."),
    ),
    modifiers(&BearerAuth),
)]
struct ApiDoc;

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi
            .components
            .get_or_insert_with(Default::default)
            .add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
    }
}

fn main() {
    let spec = ApiDoc::openapi()
        .to_pretty_json()
        .expect("failed to serialize OpenAPI spec");
    println!("{spec}");
}

// Utoipa attaches operation metadata to functions. These functions describe
// the public contract only; the runtime handlers remain in `oored`.

#[allow(dead_code)]
mod operations {
    use oore_contract::*;

    #[derive(utoipa::ToSchema)]
    pub(super) struct ReadinessResponse {
        pub ok: bool,
        pub database: bool,
        pub migrations: bool,
        pub encryption: bool,
        /// Installed release version from the install root's `VERSION` file.
        pub version: String,
        /// Version of the running `oored` binary.
        pub package_version: String,
    }

    // ── Health ──

    /// Health check
    ///
    /// Returns daemon build metadata when the daemon is running.
    #[utoipa::path(get, path = "/healthz", tag = "Health",
        responses((status = 200, description = "Daemon is healthy", body = HealthResponse))
    )]
    pub(super) fn healthz() {}

    /// Readiness check
    ///
    /// Reports whether the database, migrations, and runtime encryption key are
    /// ready to serve requests.
    #[utoipa::path(get, path = "/readyz", tag = "Health",
        responses(
            (status = 200, description = "Daemon dependencies are ready", body = ReadinessResponse),
            (status = 503, description = "One or more daemon dependencies are unavailable", body = ReadinessResponse),
        )
    )]
    pub(super) fn readyz() {}

    // ── System ──

    /// Get Prometheus metrics
    ///
    /// Returns the daemon's Prometheus exposition payload without session
    /// authentication.
    #[utoipa::path(get, path = "/metrics", tag = "System",
        responses(
            (status = 200, description = "Prometheus metrics", body = String, content_type = "text/plain"),
        )
    )]
    pub(super) fn metrics() {}

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
    pub(super) fn get_setup_status() {}

    /// Exchange a frontend pairing code
    ///
    /// Exchanges a short-lived, single-use code created by `oore frontend invite`
    /// for the backend trusted-proxy proof and configured identity header. This
    /// endpoint accepts requests only from configured trusted-proxy peer CIDRs.
    #[utoipa::path(post, path = "/v1/frontend/pair", tag = "Setup",
        request_body = FrontendPairRequest,
        responses(
            (status = 200, description = "Frontend paired with backend", body = FrontendPairResponse),
            (status = 401, description = "Invalid, expired, or consumed pairing code", body = ApiError),
            (status = 403, description = "Request peer is not allowlisted", body = ApiError),
            (status = 409, description = "Backend setup or trusted proxy configuration is incomplete", body = ApiError),
            (status = 500, description = "Internal error", body = ApiError),
        )
    )]
    pub(super) fn frontend_pair() {}

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
    pub(super) fn verify_bootstrap_token() {}

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
    pub(super) fn setup_preferences() {}

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
    pub(super) fn configure_oidc() {}

    /// Configure trusted proxy auth during setup
    ///
    /// Upserts trusted proxy settings for remote trusted-proxy mode, including
    /// the expected initial owner email.
    #[utoipa::path(post, path = "/v1/setup/trusted-proxy/configure", tag = "Setup",
        request_body = SetupTrustedProxyConfigureRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Trusted proxy setup configured", body = SetupTrustedProxyConfigureResponse),
            (status = 400, description = "Invalid owner/header/CIDR/secret input", body = ApiError),
            (status = 401, description = "Invalid setup session", body = ApiError),
            (status = 403, description = "Not in remote trusted-proxy mode", body = ApiError),
            (status = 409, description = "Setup already complete or owner already created", body = ApiError),
        )
    )]
    pub(super) fn setup_trusted_proxy_configure() {}

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
    pub(super) fn setup_oidc_start() {}

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
    pub(super) fn setup_oidc_verify() {}

    /// Claim owner identity from trusted proxy headers
    ///
    /// Creates the owner record from trusted proxy identity headers after the
    /// proxy-authenticated email matches the configured setup owner email.
    #[utoipa::path(post, path = "/v1/setup/owner/claim-trusted-proxy", tag = "Setup",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Owner created from trusted proxy identity", body = SetupTrustedProxyClaimOwnerResponse),
            (status = 401, description = "Invalid setup session or missing/invalid identity header", body = ApiError),
            (status = 403, description = "Not in trusted-proxy mode, untrusted proxy peer, or owner email mismatch", body = ApiError),
            (status = 409, description = "Invalid setup state", body = ApiError),
        )
    )]
    pub(super) fn setup_owner_claim_trusted_proxy() {}

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
    pub(super) fn setup_local_owner_create() {}

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
    pub(super) fn complete_setup() {}

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
    pub(super) fn get_setup_summary() {}

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
    pub(super) fn oidc_start() {}

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
    pub(super) fn oidc_callback() {}

    #[derive(serde::Deserialize, utoipa::ToSchema)]
    pub(super) struct OidcCallbackParams {
        pub code: String,
        pub state: String,
    }

    /// Local login and recovery
    ///
    /// In Local Only mode, creates a loopback local session and may auto-finalize
    /// first-run owner bootstrap. In Ready External Access mode, every request
    /// requires a short-lived, single-use recovery capability minted by the
    /// local `oore recovery` command over the daemon's Unix management socket.
    /// TCP loopback and forwarding headers do not grant recovery authority.
    #[utoipa::path(post, path = "/v1/auth/local/login", tag = "Auth",
        request_body = LocalLoginRequest,
        responses(
            (status = 200, description = "Session created", body = LocalLoginResponse),
            (status = 400, description = "Email required or invalid input", body = ApiError),
            (status = 403, description = "Blocked by mode policy, source policy, or missing/invalid recovery capability", body = ApiError),
        )
    )]
    pub(super) fn local_login() {}

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
    pub(super) fn trusted_proxy_login() {}

    /// Logout
    ///
    /// Invalidates the current session.
    #[utoipa::path(post, path = "/v1/auth/logout", tag = "Auth",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Logged out", body = LogoutResponse),
        )
    )]
    pub(super) fn logout() {}

    // ── Runtime updates ──

    /// Get backend update state
    ///
    /// Reports whether the backend is managed by the supported macOS launchd
    /// service and whether an update is currently running. Requires owner role.
    #[utoipa::path(get, path = "/v1/system/update", tag = "System",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Backend update state", body = RuntimeUpdateStatus),
            (status = 401, description = "Not authenticated", body = ApiError),
            (status = 403, description = "Owner role required", body = ApiError),
        )
    )]
    pub(super) fn get_runtime_update_status() {}

    /// Start a backend update
    ///
    /// Runs the installed updater and hands restart control to the managed
    /// macOS launchd service. Requires owner role.
    #[utoipa::path(post, path = "/v1/system/update", tag = "System",
        security(("bearer_auth" = [])),
        responses(
            (status = 202, description = "Backend update started", body = RuntimeUpdateStatus),
            (status = 401, description = "Not authenticated", body = ApiError),
            (status = 403, description = "Owner role required", body = ApiError),
            (status = 409, description = "Managed updater unavailable or already running", body = ApiError),
        )
    )]
    pub(super) fn start_runtime_update() {}

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
    pub(super) fn get_me() {}

    /// List users
    ///
    /// Returns all users. Requires `owner` or `admin` role.
    #[utoipa::path(get, path = "/v1/users", tag = "Users",
        params(
            ("q" = Option<String>, Query, description = "Search users"),
            ("sort" = Option<String>, Query, description = "Sort field"),
            ("direction" = Option<String>, Query, description = "Sort direction"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "User list", body = ListUsersResponse),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) fn list_users() {}

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
    pub(super) fn invite_user() {}

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
    pub(super) fn update_user_role() {}

    /// Delete user
    ///
    /// Soft-deletes a user. Requires `owner` or `admin` role.
    #[utoipa::path(delete, path = "/v1/users/{user_id}", tag = "Users",
        params(("user_id" = String, Path, description = "User ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "User deleted", body = OkResponse),
            (status = 400, description = "Cannot delete the current user", body = ApiError),
            (status = 403, description = "Forbidden", body = ApiError),
            (status = 404, description = "User not found", body = ApiError),
        )
    )]
    pub(super) fn delete_user() {}

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
    pub(super) fn re_enable_user() {}

    // ── Instance Settings ──

    /// Get artifact storage settings
    #[utoipa::path(get, path = "/v1/settings/artifact-storage", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Current storage settings", body = ArtifactStorageSettingsResponse),
        )
    )]
    pub(super) fn get_artifact_storage_settings() {}

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
    pub(super) fn update_artifact_storage_settings() {}

    /// Get instance preferences
    #[utoipa::path(get, path = "/v1/settings/preferences", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Current preferences", body = InstancePreferencesResponse),
        )
    )]
    pub(super) fn get_instance_preferences() {}

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
    pub(super) fn update_instance_preferences() {}

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
    pub(super) fn get_external_access_network_settings() {}

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
    pub(super) fn update_external_access_network_settings() {}

    /// Get trusted proxy runtime settings
    #[utoipa::path(get, path = "/v1/settings/external-access/trusted-proxy", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Trusted proxy settings", body = TrustedProxySettingsResponse),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) fn get_external_access_trusted_proxy_settings() {}

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
    pub(super) fn update_external_access_trusted_proxy_settings() {}

    /// Get External Access preflight readiness
    ///
    /// Returns check-by-check readiness required before enabling External Access (`runtime_mode=remote`).
    #[utoipa::path(get, path = "/v1/settings/external-access/preflight", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "External Access preflight result", body = ExternalAccessPreflightResponse),
        )
    )]
    pub(super) fn get_external_access_preflight() {}

    /// Get current OIDC configuration for External Access
    ///
    /// Returns the current runtime OIDC provider configuration (issuer, client ID,
    /// discovered endpoints). Never exposes the client secret.
    #[utoipa::path(get, path = "/v1/settings/external-access/oidc", tag = "Instance Settings",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Current OIDC configuration", body = GetExternalAccessOidcResponse),
            (status = 404, description = "OIDC not configured", body = ApiError),
            (status = 409, description = "Setup not ready", body = ApiError),
        )
    )]
    pub(super) fn get_external_access_oidc() {}

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
    pub(super) fn configure_external_access_oidc() {}

    /// Test OIDC provider connection
    ///
    /// Owner-only endpoint to test OIDC provider discovery without committing changes.
    /// Validates that the issuer URL is reachable and returns discovered endpoints.
    #[utoipa::path(post, path = "/v1/settings/external-access/oidc/test-connection", tag = "Instance Settings",
        request_body = TestOidcConnectionRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Connection test succeeded", body = TestOidcConnectionResponse),
            (status = 400, description = "Invalid input or OIDC discovery failure", body = ApiError),
            (status = 403, description = "Owner-only operation", body = ApiError),
        )
    )]
    pub(super) fn test_oidc_connection() {}

    // ── Retention Policy ──

    /// Get global retention policy
    #[utoipa::path(get, path = "/v1/settings/retention", tag = "Retention Policy",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Current retention policy", body = RetentionPolicyResponse),
        )
    )]
    pub(super) fn get_retention_policy() {}

    /// Update global retention policy
    #[utoipa::path(put, path = "/v1/settings/retention", tag = "Retention Policy",
        request_body = UpdateRetentionPolicyRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Policy updated", body = RetentionPolicyResponse),
        )
    )]
    pub(super) fn update_retention_policy() {}

    /// Get last cleanup summary
    #[utoipa::path(get, path = "/v1/settings/retention/last-cleanup", tag = "Retention Policy",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Last cleanup summary", body = RetentionCleanupSummaryResponse),
        )
    )]
    pub(super) fn get_retention_last_cleanup() {}

    /// Get project retention (effective policy merged with overrides)
    #[utoipa::path(get, path = "/v1/projects/{project_id}/retention", tag = "Retention Policy",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Effective project retention", body = EffectiveProjectRetentionResponse),
        )
    )]
    pub(super) fn get_project_retention() {}

    /// Update project retention override
    #[utoipa::path(put, path = "/v1/projects/{project_id}/retention", tag = "Retention Policy",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = UpdateProjectRetentionOverrideRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Override updated", body = EffectiveProjectRetentionResponse),
        )
    )]
    pub(super) fn update_project_retention() {}

    /// Delete project retention override (revert to global)
    #[utoipa::path(delete, path = "/v1/projects/{project_id}/retention", tag = "Retention Policy",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Override removed", body = EffectiveProjectRetentionResponse),
        )
    )]
    pub(super) fn delete_project_retention() {}

    // ── Integrations ──

    /// List integrations
    ///
    /// Returns all SCM integrations, optionally filtered by provider.
    #[utoipa::path(get, path = "/v1/integrations", tag = "Integrations",
        params(
            ("provider" = Option<String>, Query, description = "Filter by SCM provider (github, gitlab)"),
            ("q" = Option<String>, Query, description = "Search integrations"),
            ("sort" = Option<String>, Query, description = "Sort field"),
            ("direction" = Option<String>, Query, description = "Sort direction"),
            ("limit" = Option<i64>, Query, description = "Page size (default 20)"),
            ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Integration list", body = ListIntegrationsResponse),
        )
    )]
    pub(super) fn list_integrations() {}

    /// Get integration detail
    #[utoipa::path(get, path = "/v1/integrations/{id}", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Integration detail", body = IntegrationDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn get_integration() {}

    /// Delete integration
    #[utoipa::path(delete, path = "/v1/integrations/{id}", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Integration deleted", body = OkResponse),
            (status = 404, description = "Not found", body = ApiError),
            (status = 409, description = "Integration has active builds", body = ApiError),
        )
    )]
    pub(super) fn delete_integration() {}

    /// List durable operator incidents for the current source manager
    #[utoipa::path(get, path = "/v1/operator-incidents", tag = "Integrations",
        params(
            ("status" = Option<String>, Query, description = "Filter by open or resolved status"),
            ("resource_id" = Option<String>, Query, description = "Filter by affected resource ID"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Operator incident list", body = ListOperatorIncidentsResponse),
            (status = 403, description = "Manage Sources permission required", body = ApiError),
        )
    )]
    pub(super) fn list_operator_incidents() {}

    /// Mark one targeted operator incident notification as read
    #[utoipa::path(post, path = "/v1/operator-incidents/{id}/read", tag = "Integrations",
        params(("id" = String, Path, description = "Incident ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Updated operator incident", body = OperatorIncident),
            (status = 404, description = "Notification not found", body = ApiError),
        )
    )]
    pub(super) fn mark_operator_incident_read() {}

    /// List integration repositories
    #[utoipa::path(get, path = "/v1/integrations/{id}/repositories", tag = "Integrations",
        params(
            ("id" = String, Path, description = "Integration ID"),
            ("q" = Option<String>, Query, description = "Search repositories"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Repository list", body = ListRepositoriesResponse),
        )
    )]
    pub(super) fn list_repositories() {}

    /// Search repositories across integrations
    #[utoipa::path(get, path = "/v1/integration-repositories", tag = "Integrations",
        params(
            ("q" = Option<String>, Query, description = "Search repositories"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Source repository list", body = ListSourceRepositoriesResponse),
        )
    )]
    pub(super) fn list_source_repositories() {}

    /// Fetch a private GitLab repository avatar through Oore
    #[utoipa::path(get, path = "/v1/integration-repositories/{id}/avatar", tag = "Integrations",
        params(("id" = String, Path, description = "Integration repository ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Repository avatar image", body = BinaryPayload, content_type = "image/*"),
            (status = 404, description = "Avatar not found", body = ApiError),
            (status = 502, description = "GitLab avatar unavailable", body = ApiError),
        )
    )]
    pub(super) fn repository_avatar() {}

    /// List integration installations
    #[utoipa::path(get, path = "/v1/integrations/{id}/installations", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Installation list", body = ListInstallationsResponse),
        )
    )]
    pub(super) fn list_installations() {}

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
    pub(super) fn sync_installations() {}

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
    pub(super) fn github_start() {}

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
    pub(super) fn github_complete() {}

    /// Open the GitHub App manifest flow
    ///
    /// Serves the browser form that creates a GitHub App. The sealed `state`
    /// query value authenticates the flow; no session bearer token is used.
    #[utoipa::path(get, path = "/v1/integrations/github/create", tag = "Integrations",
        params(
            ("state" = String, Query, description = "Sealed GitHub manifest-flow state"),
        ),
        responses(
            (status = 200, description = "GitHub App creation page", body = String, content_type = "text/html"),
            (status = 400, description = "Missing or invalid query parameters"),
        )
    )]
    pub(super) fn github_create_page() {}

    /// Complete the GitHub App manifest callback
    ///
    /// Exchanges GitHub's manifest code, stores the integration, and redirects
    /// the browser. Callback errors are rendered as HTML.
    #[utoipa::path(get, path = "/v1/integrations/github/callback", tag = "Integrations",
        params(
            ("code" = Option<String>, Query, description = "GitHub manifest conversion code"),
            ("state" = Option<String>, Query, description = "Sealed GitHub manifest-flow state"),
        ),
        responses(
            (status = 200, description = "Callback error page", body = String, content_type = "text/html"),
            (status = 303, description = "Redirect to GitHub installation or the configured frontend"),
        )
    )]
    pub(super) fn github_callback() {}

    /// Complete the GitHub App installation callback
    ///
    /// Consumes the optional sealed install-state cookie, synchronizes the
    /// installation when authorized, and serves a browser redirect page.
    #[utoipa::path(get, path = "/v1/integrations/github/installed", tag = "Integrations",
        params(
            ("installation_id" = Option<i64>, Query, description = "GitHub installation ID"),
            ("setup_action" = Option<String>, Query, description = "GitHub installation action"),
            ("Cookie" = Option<String>, Header, description = "Optional sealed oore_gh_install_state cookie"),
        ),
        responses(
            (status = 200, description = "GitHub installation completion page", body = String, content_type = "text/html"),
            (status = 400, description = "Invalid query parameters"),
        )
    )]
    pub(super) fn github_installed() {}

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
    pub(super) fn gitlab_start() {}

    /// Get GitLab OAuth authorization URL
    #[utoipa::path(post, path = "/v1/integrations/gitlab/authorize", tag = "Integrations",
        request_body = GitLabAuthorizeRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Authorization URL", body = GitLabAuthorizeResponse),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) fn gitlab_authorize() {}

    /// Complete the GitLab OAuth callback
    ///
    /// Exchanges GitLab's authorization code and redirects the browser.
    /// Callback errors are rendered as HTML.
    #[utoipa::path(get, path = "/v1/integrations/gitlab/callback", tag = "Integrations",
        params(
            ("code" = Option<String>, Query, description = "GitLab authorization code"),
            ("state" = Option<String>, Query, description = "Sealed GitLab OAuth state"),
            ("error" = Option<String>, Query, description = "GitLab OAuth error code"),
            ("error_description" = Option<String>, Query, description = "GitLab OAuth error description"),
        ),
        responses(
            (status = 200, description = "Callback error page", body = String, content_type = "text/html"),
            (status = 303, description = "Redirect to the configured frontend"),
        )
    )]
    pub(super) fn gitlab_callback() {}

    /// Check a saved GitLab personal access token
    #[utoipa::path(post, path = "/v1/integrations/{id}/gitlab-token/check", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Token status", body = GitLabCredentialStatusResponse),
            (status = 400, description = "Source does not use a personal access token", body = ApiError),
            (status = 404, description = "GitLab source not found", body = ApiError),
        )
    )]
    pub(super) fn check_gitlab_personal_token() {}

    /// Replace a saved GitLab personal access token
    #[utoipa::path(put, path = "/v1/integrations/{id}/gitlab-token", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        request_body = ReplaceGitLabTokenRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Replacement token status", body = GitLabCredentialStatusResponse),
            (status = 400, description = "Token rejected or required scopes are missing", body = ApiError),
            (status = 404, description = "GitLab source not found", body = ApiError),
            (status = 409, description = "Token belongs to another account or saved credentials are missing", body = ApiError),
            (status = 502, description = "GitLab could not validate the token", body = ApiError),
        )
    )]
    pub(super) fn replace_gitlab_personal_token() {}

    /// Generate or rotate a repository-scoped GitLab webhook token
    #[utoipa::path(post, path = "/v1/integration-repositories/{id}/gitlab-webhook-secret", tag = "Integrations",
        params(("id" = String, Path, description = "Integration repository ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "One-time webhook token", body = GitLabRepositoryWebhookSecretResponse),
            (status = 403, description = "Integration write permission or Remote mode required", body = ApiError),
            (status = 404, description = "Active GitLab repository not found", body = ApiError),
        )
    )]
    pub(super) fn rotate_gitlab_repository_webhook_secret() {}

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
    pub(super) fn create_local_git_integration() {}

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
    pub(super) fn browse_local_git_directories() {}

    /// List local git integrations
    #[utoipa::path(get, path = "/v1/integrations/local-git", tag = "Integrations",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Local git integration list", body = ListIntegrationsResponse),
            (status = 403, description = "Local mode required", body = ApiError),
        )
    )]
    pub(super) fn list_local_git_integrations() {}

    /// Delete local git integration
    #[utoipa::path(delete, path = "/v1/integrations/local-git/{id}", tag = "Integrations",
        params(("id" = String, Path, description = "Integration ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Deleted", body = OkResponse),
            (status = 403, description = "Local mode required", body = ApiError),
            (status = 404, description = "Integration not found", body = ApiError),
        )
    )]
    pub(super) fn delete_local_git_integration() {}

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
    pub(super) fn create_project() {}

    /// List projects
    #[utoipa::path(get, path = "/v1/projects", tag = "Projects",
        params(
            ("limit" = Option<i64>, Query, description = "Page size (default 50)"),
            ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
            ("search" = Option<String>, Query, description = "Filter by name (case-insensitive partial match)"),
            ("integration_id" = Option<String>, Query, description = "Filter by linked integration"),
            ("sort" = Option<String>, Query, description = "Sort by created_at, updated_at, or name"),
            ("direction" = Option<String>, Query, description = "Sort direction: asc or desc"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Project list", body = ListProjectsResponse),
            (status = 400, description = "Invalid input", body = ApiError),
        )
    )]
    pub(super) fn list_projects() {}

    /// Get project detail
    #[utoipa::path(get, path = "/v1/projects/{project_id}", tag = "Projects",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Project detail", body = ProjectDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn get_project() {}

    /// Update project
    #[utoipa::path(patch, path = "/v1/projects/{project_id}", tag = "Projects",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = UpdateProjectRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Project updated", body = CreateProjectResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn update_project() {}

    /// Delete project
    #[utoipa::path(delete, path = "/v1/projects/{project_id}", tag = "Projects",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Project deleted", body = OkResponse),
            (status = 404, description = "Not found", body = ApiError),
            (status = 409, description = "Project has active builds", body = ApiError),
        )
    )]
    pub(super) fn delete_project() {}

    // ── Project Members ──

    /// List project members
    #[utoipa::path(get, path = "/v1/projects/{project_id}/members", tag = "Project Members",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Member list", body = ListProjectMembersResponse),
            (status = 404, description = "Project not found", body = ApiError),
        )
    )]
    pub(super) fn list_project_members() {}

    /// List eligible project member candidates
    #[utoipa::path(get, path = "/v1/projects/{project_id}/members/candidates", tag = "Project Members",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Eligible member candidates", body = ListProjectMemberCandidatesResponse),
            (status = 403, description = "Manage Members permission required", body = ApiError),
            (status = 404, description = "Project not found", body = ApiError),
        )
    )]
    pub(super) fn list_project_member_candidates() {}

    /// Add a member to a project
    #[utoipa::path(post, path = "/v1/projects/{project_id}/members", tag = "Project Members",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = AddProjectMemberRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Member added", body = AddProjectMemberResponse),
            (status = 400, description = "Invalid user or user already has implicit access", body = ApiError),
            (status = 404, description = "Project not found", body = ApiError),
            (status = 409, description = "User is already a member", body = ApiError),
        )
    )]
    pub(super) fn add_project_member() {}

    /// Update a project member's role
    #[utoipa::path(patch, path = "/v1/projects/{project_id}/members/{user_id}", tag = "Project Members",
        params(
            ("project_id" = String, Path, description = "Project ID"),
            ("user_id" = String, Path, description = "User ID"),
        ),
        request_body = UpdateProjectMemberRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Member role updated", body = UpdateProjectMemberResponse),
            (status = 404, description = "Project or member not found", body = ApiError),
        )
    )]
    pub(super) fn update_project_member() {}

    /// Remove a member from a project
    #[utoipa::path(delete, path = "/v1/projects/{project_id}/members/{user_id}", tag = "Project Members",
        params(
            ("project_id" = String, Path, description = "Project ID"),
            ("user_id" = String, Path, description = "User ID"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Member removed", body = RemoveProjectMemberResponse),
            (status = 404, description = "Project or member not found", body = ApiError),
        )
    )]
    pub(super) fn remove_project_member() {}

    /// Discover repository-owned workflows
    ///
    /// Reads supported Oore workflow files from the linked repository without
    /// modifying it. The response contains validated behavior but never raw YAML,
    /// environment values, or source credentials.
    #[utoipa::path(get, path = "/v1/projects/{project_id}/repository-workflows", tag = "Pipelines",
        params(
            ("project_id" = String, Path, description = "Project ID"),
            ("ref" = Option<String>, Query, description = "Branch, tag, or commit; defaults to the project branch"),
            ("path" = Option<String>, Query, description = "Additional explicit repository-relative config path"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret-free repository workflow previews", body = DiscoverRepositoryWorkflowsResponse),
            (status = 400, description = "Invalid ref or config path", body = ApiError),
            (status = 403, description = "Manage Pipelines permission required", body = ApiError),
            (status = 404, description = "Project source not found", body = ApiError),
            (status = 502, description = "Source provider request failed", body = ApiError),
        )
    )]
    pub(super) fn discover_repository_workflows() {}

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
    pub(super) fn create_pipeline() {}

    /// List pipelines
    #[utoipa::path(get, path = "/v1/projects/{project_id}/pipelines", tag = "Pipelines",
        params(
            ("project_id" = String, Path, description = "Project ID"),
            ("search" = Option<String>, Query, description = "Case-insensitive pipeline name search"),
            ("sort" = Option<String>, Query, description = "Sort by created_at or name"),
            ("direction" = Option<String>, Query, description = "Sort direction: asc or desc"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Pipeline list", body = ListPipelinesResponse),
            (status = 400, description = "Invalid sort or direction", body = ApiError),
        )
    )]
    pub(super) fn list_pipelines() {}

    /// Get pipeline detail
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}", tag = "Pipelines",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Pipeline detail", body = PipelineDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn get_pipeline() {}

    /// Update pipeline
    #[utoipa::path(patch, path = "/v1/pipelines/{pipeline_id}", tag = "Pipelines",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = UpdatePipelineRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Pipeline updated", body = CreatePipelineResponse),
            (status = 400, description = "Invalid input", body = ApiError),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn update_pipeline() {}

    /// Delete pipeline
    #[utoipa::path(delete, path = "/v1/pipelines/{pipeline_id}", tag = "Pipelines",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Pipeline deleted", body = OkResponse),
            (status = 404, description = "Not found", body = ApiError),
            (status = 409, description = "Pipeline has active builds", body = ApiError),
        )
    )]
    pub(super) fn delete_pipeline() {}

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
    pub(super) fn validate_pipeline() {}

    // ── Pipeline Publishing ──

    /// Get Google Play publish settings
    ///
    /// Requires project read access. Also reports whether the referenced
    /// service account secret exists in a visible scope.
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/publish/google-play", tag = "Pipeline Publishing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Google Play publish settings", body = PipelineGooglePlayPublishResponse),
            (status = 404, description = "Pipeline not found", body = ApiError),
        )
    )]
    pub(super) fn get_pipeline_google_play_publish() {}

    /// Update Google Play publish settings
    ///
    /// Requires the manage pipelines project permission. The service account
    /// JSON is stored separately as a secret and referenced by name.
    #[utoipa::path(put, path = "/v1/pipelines/{pipeline_id}/publish/google-play", tag = "Pipeline Publishing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = GooglePlayPublishConfig,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Saved settings", body = PipelineGooglePlayPublishResponse),
            (status = 400, description = "Invalid settings", body = ApiError),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 404, description = "Pipeline not found", body = ApiError),
        )
    )]
    pub(super) fn update_pipeline_google_play_publish() {}

    // ── Secrets ──

    /// List instance secrets
    ///
    /// Returns secret names and timestamps. Values are never returned. Requires instance settings read access.
    #[utoipa::path(get, path = "/v1/secrets", tag = "Secrets",
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret names", body = ListSecretsResponse),
            (status = 403, description = "Permission denied", body = ApiError),
        )
    )]
    pub(super) fn list_instance_secrets() {}

    /// Create instance secret
    ///
    /// Stores an encrypted value. The response never contains it. Requires instance settings write access.
    #[utoipa::path(post, path = "/v1/secrets", tag = "Secrets",
        request_body = CreateSecretRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Secret created", body = SecretResponse),
            (status = 400, description = "Invalid name or value", body = ApiError),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 409, description = "A secret with this name already exists in this scope", body = ApiError),
        )
    )]
    pub(super) fn create_instance_secret() {}

    /// Rotate instance secret
    ///
    /// Replaces the stored value. Requires instance settings write access.
    #[utoipa::path(put, path = "/v1/secrets/{name}", tag = "Secrets",
        params(("name" = String, Path, description = "Secret name")),
        request_body = RotateSecretRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret rotated", body = SecretResponse),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 404, description = "Secret not found", body = ApiError),
        )
    )]
    pub(super) fn rotate_instance_secret() {}

    /// Delete instance secret
    #[utoipa::path(delete, path = "/v1/secrets/{name}", tag = "Secrets",
        params(("name" = String, Path, description = "Secret name")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret deleted", body = OkResponse),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 404, description = "Secret not found", body = ApiError),
        )
    )]
    pub(super) fn delete_instance_secret() {}

    /// List project secrets
    ///
    /// Returns secret names and timestamps. Values are never returned. Requires project read access.
    #[utoipa::path(get, path = "/v1/projects/{project_id}/secrets", tag = "Secrets",
        params(("project_id" = String, Path, description = "Project ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret names", body = ListSecretsResponse),
            (status = 403, description = "Permission denied", body = ApiError),
        )
    )]
    pub(super) fn list_project_secrets() {}

    /// Create project secret
    ///
    /// Stores an encrypted value. The response never contains it. Requires the manage pipelines project permission.
    #[utoipa::path(post, path = "/v1/projects/{project_id}/secrets", tag = "Secrets",
        params(("project_id" = String, Path, description = "Project ID")),
        request_body = CreateSecretRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Secret created", body = SecretResponse),
            (status = 400, description = "Invalid name or value", body = ApiError),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 409, description = "A secret with this name already exists in this scope", body = ApiError),
        )
    )]
    pub(super) fn create_project_secret() {}

    /// Rotate project secret
    ///
    /// Replaces the stored value. Requires the manage pipelines project permission.
    #[utoipa::path(put, path = "/v1/projects/{project_id}/secrets/{name}", tag = "Secrets",
        params(("project_id" = String, Path, description = "Project ID"), ("name" = String, Path, description = "Secret name")),
        request_body = RotateSecretRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret rotated", body = SecretResponse),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 404, description = "Secret not found", body = ApiError),
        )
    )]
    pub(super) fn rotate_project_secret() {}

    /// Delete project secret
    #[utoipa::path(delete, path = "/v1/projects/{project_id}/secrets/{name}", tag = "Secrets",
        params(("project_id" = String, Path, description = "Project ID"), ("name" = String, Path, description = "Secret name")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret deleted", body = OkResponse),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 404, description = "Secret not found", body = ApiError),
        )
    )]
    pub(super) fn delete_project_secret() {}

    /// List pipeline secrets
    ///
    /// Returns secret names and timestamps. Values are never returned. Requires project read access.
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/secrets", tag = "Secrets",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret names", body = ListSecretsResponse),
            (status = 403, description = "Permission denied", body = ApiError),
        )
    )]
    pub(super) fn list_pipeline_secrets() {}

    /// Create pipeline secret
    ///
    /// Stores an encrypted value. The response never contains it. Requires the manage pipelines project permission.
    #[utoipa::path(post, path = "/v1/pipelines/{pipeline_id}/secrets", tag = "Secrets",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        request_body = CreateSecretRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Secret created", body = SecretResponse),
            (status = 400, description = "Invalid name or value", body = ApiError),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 409, description = "A secret with this name already exists in this scope", body = ApiError),
        )
    )]
    pub(super) fn create_pipeline_secret() {}

    /// Rotate pipeline secret
    ///
    /// Replaces the stored value. Requires the manage pipelines project permission.
    #[utoipa::path(put, path = "/v1/pipelines/{pipeline_id}/secrets/{name}", tag = "Secrets",
        params(("pipeline_id" = String, Path, description = "Pipeline ID"), ("name" = String, Path, description = "Secret name")),
        request_body = RotateSecretRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret rotated", body = SecretResponse),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 404, description = "Secret not found", body = ApiError),
        )
    )]
    pub(super) fn rotate_pipeline_secret() {}

    /// Delete pipeline secret
    #[utoipa::path(delete, path = "/v1/pipelines/{pipeline_id}/secrets/{name}", tag = "Secrets",
        params(("pipeline_id" = String, Path, description = "Pipeline ID"), ("name" = String, Path, description = "Secret name")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Secret deleted", body = OkResponse),
            (status = 403, description = "Permission denied", body = ApiError),
            (status = 404, description = "Secret not found", body = ApiError),
        )
    )]
    pub(super) fn delete_pipeline_secret() {}

    // ── Pipeline Signing (Android) ──

    /// Get Android signing config
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/android-signing", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Android signing profiles", body = PipelineAndroidSigningResponse),
        )
    )]
    pub(super) fn get_pipeline_android_signing() {}

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
    pub(super) fn update_pipeline_android_signing() {}

    // ── Pipeline Signing (iOS) ──

    /// Get iOS signing config
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/ios-signing", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "iOS signing configuration", body = PipelineIosSigningResponse),
        )
    )]
    pub(super) fn get_pipeline_ios_signing() {}

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
    pub(super) fn update_pipeline_ios_signing() {}

    /// Sync iOS provisioning profiles
    ///
    /// Re-downloads provisioning profiles from Apple via App Store Connect API.
    #[utoipa::path(post, path = "/v1/pipelines/{pipeline_id}/ios-signing/sync", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Profiles synced", body = SyncPipelineIosSigningResponse),
        )
    )]
    pub(super) fn sync_pipeline_ios_signing() {}

    /// List registered iOS devices
    #[utoipa::path(get, path = "/v1/pipelines/{pipeline_id}/ios-signing/devices", tag = "Pipeline Signing",
        params(("pipeline_id" = String, Path, description = "Pipeline ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Device list", body = ListPipelineIosDevicesResponse),
        )
    )]
    pub(super) fn list_pipeline_ios_devices() {}

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
    pub(super) fn register_pipeline_ios_device() {}

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
    pub(super) fn create_build() {}

    /// Preview a Markdown changelog for a manual build.
    #[utoipa::path(get, path = "/v1/projects/{project_id}/builds/changelog-preview", tag = "Builds",
        params(
            ("project_id" = String, Path, description = "Project ID"),
            ("pipeline_id" = String, Query, description = "Pipeline ID"),
            ("branch" = Option<String>, Query, description = "Target branch"),
            ("commit_sha" = Option<String>, Query, description = "Target commit"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Generated changelog draft", body = BuildChangelogPreviewResponse),
            (status = 400, description = "Invalid revision", body = ApiError),
            (status = 404, description = "Project or pipeline not found", body = ApiError),
        )
    )]
    pub(super) fn preview_build_changelog() {}

    /// List builds
    ///
    /// Returns builds, optionally filtered by project, pipeline, or status.
    #[utoipa::path(get, path = "/v1/builds", tag = "Builds",
        params(
            ("limit" = Option<i64>, Query, description = "Page size (default 50)"),
            ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
            ("project_id" = Option<String>, Query, description = "Filter by project"),
            ("pipeline_id" = Option<String>, Query, description = "Filter by pipeline"),
            ("status" = Option<String>, Query, description = "Filter by one status or up to 9 comma-separated statuses"),
            ("sort" = Option<String>, Query, description = "Sort by created_at, status, project_name, pipeline_name, or branch"),
            ("direction" = Option<String>, Query, description = "Sort direction: asc or desc"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Build list", body = ListBuildsResponse),
            (status = 400, description = "Invalid input", body = ApiError),
        )
    )]
    pub(super) fn list_builds() {}

    /// Get build detail
    #[utoipa::path(get, path = "/v1/builds/{build_id}", tag = "Builds",
        params(("build_id" = String, Path, description = "Build ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Build detail", body = BuildDetailResponse),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn get_build() {}

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
    pub(super) fn cancel_build() {}

    /// Re-run build
    ///
    /// Creates a new build cloning the original build's config_snapshot, branch, and commit_sha.
    #[utoipa::path(post, path = "/v1/builds/{build_id}/rerun", tag = "Builds",
        params(("build_id" = String, Path, description = "Source build ID to re-run")),
        security(("bearer_auth" = [])),
        responses(
            (status = 201, description = "Re-run build queued", body = RerunBuildResponse),
            (status = 404, description = "Source build not found", body = ApiError),
            (status = 409, description = "Source build is not in a terminal state", body = ApiError),
        )
    )]
    pub(super) fn rerun_build() {}

    // ── Runners ──

    /// Register runner
    ///
    /// Registers a new build runner and returns a runner authentication token.
    #[utoipa::path(post, path = "/v1/runners/register", tag = "Runners",
        request_body = RegisterRunnerRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Runner registered", body = RegisterRunnerResponse),
        )
    )]
    pub(super) fn register_runner() {}

    /// List runners
    #[utoipa::path(get, path = "/v1/runners", tag = "Runners",
        params(
            ("q" = Option<String>, Query, description = "Search runners"),
            ("sort" = Option<String>, Query, description = "Sort field"),
            ("direction" = Option<String>, Query, description = "Sort direction"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Runner list", body = ListRunnersResponse),
        )
    )]
    pub(super) fn list_runners() {}

    /// Get runner
    ///
    /// Retrieves a single runner by ID, including its current status and health information.
    #[utoipa::path(get, path = "/v1/runners/{runner_id}", tag = "Runners",
        params(("runner_id" = String, Path, description = "Runner ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Runner details", body = UpdateRunnerResponse),
            (status = 404, description = "Runner not found", body = ApiError),
        )
    )]
    pub(super) fn get_runner() {}

    /// Update runner
    #[utoipa::path(patch, path = "/v1/runners/{runner_id}", tag = "Runners",
        params(("runner_id" = String, Path, description = "Runner ID")),
        request_body = UpdateRunnerRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Runner updated", body = UpdateRunnerResponse),
        )
    )]
    pub(super) fn update_runner() {}

    /// Delete runner
    ///
    /// Deletes a user-registered runner that has never claimed a build.
    #[utoipa::path(delete, path = "/v1/runners/{runner_id}", tag = "Runners",
        params(("runner_id" = String, Path, description = "Runner ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 204, description = "Runner deleted"),
            (status = 403, description = "Insufficient permissions", body = ApiError),
            (status = 404, description = "Runner not found", body = ApiError),
            (status = 409, description = "Managed runner or runner with builds", body = ApiError),
        )
    )]
    pub(super) fn delete_runner() {}

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
    pub(super) fn runner_heartbeat() {}

    /// Claim job
    ///
    /// Runner claims the next queued build. Returns `null` job if no builds are available.
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/claim", tag = "Runners",
        params(("runner_id" = String, Path, description = "Runner ID")),
        request_body = ClaimJobRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Job claimed (or null)", body = ClaimJobResponse),
        )
    )]
    pub(super) fn claim_job() {}

    /// Discover a private GitLab repository for checkout
    ///
    /// Internal Git smart-HTTP endpoint. The runner token is accepted only for
    /// the repository assigned to this job; GitLab credentials remain server-side.
    #[utoipa::path(get, path = "/v1/runners/{runner_id}/jobs/{job_id}/gitlab/{git_path}", tag = "Runners",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
            ("git_path" = String, Path, description = "Multi-segment assigned repository info/refs path; preserve slash separators"),
            ("service" = String, Query, description = "Must be git-upload-pack"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Git upload-pack discovery response", body = BinaryPayload, content_type = "application/x-git-upload-pack-advertisement"),
            (status = 403, description = "Repository path does not match the assigned job", body = ApiError),
        )
    )]
    pub(super) fn gitlab_checkout_discovery() {}

    /// Stream a private GitLab upload-pack request
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/jobs/{job_id}/gitlab/{git_path}", tag = "Runners",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
            ("git_path" = String, Path, description = "Multi-segment assigned repository git-upload-pack path; preserve slash separators"),
        ),
        request_body(content = BinaryPayload, description = "Git upload-pack request", content_type = "application/x-git-upload-pack-request"),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Git upload-pack response", body = BinaryPayload, content_type = "application/x-git-upload-pack-result"),
            (status = 403, description = "Repository path does not match the assigned job", body = ApiError),
        )
    )]
    pub(super) fn gitlab_checkout_upload_pack() {}

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
            (status = 200, description = "Status updated", body = BuildDetailResponse),
            (status = 409, description = "Invalid state transition", body = ApiError),
        )
    )]
    pub(super) fn update_job_status() {}

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
    pub(super) fn get_job_status() {}

    /// Get job-scoped Android signing material
    ///
    /// Returns Android signing profiles only to the assigned runner while the
    /// job is active. Both the runner bearer token and job-scoped signing grant
    /// are required.
    #[utoipa::path(get, path = "/v1/runners/{runner_id}/jobs/{job_id}/android-signing", tag = "Pipeline Signing",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
            ("x-oore-signing-token" = String, Header, description = "Job-scoped signing grant"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Android signing material", body = RunnerAndroidSigningResponse),
            (status = 401, description = "Runner authentication or signing grant is missing or invalid", body = ApiError),
            (status = 403, description = "Runner does not match the active assignment", body = ApiError),
            (status = 404, description = "Build not found", body = ApiError),
            (status = 409, description = "Job is not active", body = ApiError),
            (status = 500, description = "Signing material could not be loaded", body = ApiError),
        )
    )]
    pub(super) fn get_job_android_signing() {}

    /// Get job-scoped secrets
    ///
    /// Returns resolved instance, project and pipeline secrets (narrowest scope
    /// wins) only to the assigned runner while the job is active. Both the runner
    /// bearer token and job-scoped signing grant are required.
    #[utoipa::path(get, path = "/v1/runners/{runner_id}/jobs/{job_id}/secrets", tag = "Secrets",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
            ("x-oore-signing-token" = String, Header, description = "Job-scoped signing grant"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Resolved secrets", body = RunnerJobSecretsResponse),
            (status = 401, description = "Runner authentication or signing grant is missing or invalid", body = ApiError),
            (status = 403, description = "Runner does not match the active assignment", body = ApiError),
            (status = 404, description = "Build not found", body = ApiError),
            (status = 409, description = "Job is not active", body = ApiError),
            (status = 500, description = "Secrets could not be loaded", body = ApiError),
        )
    )]
    pub(super) fn get_job_secrets() {}

    /// Get job publish plan
    ///
    /// Returns enabled store publish settings and the build changelog to the
    /// assigned runner while the job is active.
    #[utoipa::path(get, path = "/v1/runners/{runner_id}/jobs/{job_id}/publish", tag = "Pipeline Publishing",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
            ("x-oore-signing-token" = String, Header, description = "Job-scoped signing grant"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Publish plan", body = RunnerPublishPlan),
            (status = 401, description = "Runner authentication or signing grant is missing or invalid", body = ApiError),
            (status = 403, description = "Runner does not match the active assignment", body = ApiError),
            (status = 409, description = "Job is not active", body = ApiError),
        )
    )]
    pub(super) fn get_job_publish_plan() {}

    /// Report a store upload
    ///
    /// Records a completed store upload for an active job.
    #[utoipa::path(post, path = "/v1/runners/{runner_id}/jobs/{job_id}/publish-results", tag = "Pipeline Publishing",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
            ("x-oore-signing-token" = String, Header, description = "Job-scoped signing grant"),
        ),
        request_body = RunnerPublishResult,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Recorded", body = OkResponse),
            (status = 401, description = "Runner authentication or signing grant is missing or invalid", body = ApiError),
            (status = 403, description = "Runner does not match the active assignment", body = ApiError),
            (status = 409, description = "Job is not active", body = ApiError),
        )
    )]
    pub(super) fn report_job_publish_result() {}

    /// Get job-scoped iOS signing material
    ///
    /// Returns the iOS signing bundle only to the assigned runner while the job
    /// is active. Both the runner bearer token and job-scoped signing grant are
    /// required.
    #[utoipa::path(get, path = "/v1/runners/{runner_id}/jobs/{job_id}/ios-signing", tag = "Pipeline Signing",
        params(
            ("runner_id" = String, Path, description = "Runner ID"),
            ("job_id" = String, Path, description = "Build/Job ID"),
            ("x-oore-signing-token" = String, Header, description = "Job-scoped signing grant"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "iOS signing material", body = RunnerIosSigningResponse),
            (status = 401, description = "Runner authentication or signing grant is missing or invalid", body = ApiError),
            (status = 403, description = "Runner does not match the active assignment", body = ApiError),
            (status = 404, description = "Build not found", body = ApiError),
            (status = 409, description = "Job is not active", body = ApiError),
            (status = 500, description = "Signing material could not be loaded", body = ApiError),
        )
    )]
    pub(super) fn get_job_ios_signing() {}

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
    pub(super) fn append_build_logs() {}

    /// Get build logs
    ///
    /// Returns paginated historical build logs.
    #[utoipa::path(get, path = "/v1/builds/{build_id}/logs", tag = "Build Logs",
        params(
            ("build_id" = String, Path, description = "Build ID"),
            ("after_sequence" = Option<i64>, Query, description = "Return logs after this sequence"),
            ("limit" = Option<i64>, Query, description = "Page size"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Build logs", body = BuildLogsResponse),
        )
    )]
    pub(super) fn get_build_logs() {}

    /// Stream build logs (SSE)
    ///
    /// Server-sent event stream of build logs. Requires a short-lived streaming
    /// token obtained from `POST /v1/builds/{build_id}/stream-token`.
    #[utoipa::path(get, path = "/v1/builds/{build_id}/logs/stream", tag = "Build Logs",
        params(
            ("build_id" = String, Path, description = "Build ID"),
            ("token" = Option<String>, Query, description = "Short-lived streaming token; omit when bearer authentication is used"),
        ),
        security(("bearer_auth" = []), ()),
        responses(
            (status = 200, description = "SSE log stream", body = String, content_type = "text/event-stream"),
            (status = 401, description = "Invalid or expired token", body = ApiError),
        )
    )]
    pub(super) fn stream_build_logs() {}

    /// Create stream token
    ///
    /// Exchanges a session token for a short-lived (5 min) streaming token
    /// suitable for use in SSE query parameters.
    #[utoipa::path(post, path = "/v1/builds/{build_id}/stream-token", tag = "Build Logs",
        params(("build_id" = String, Path, description = "Build ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Streaming token created", body = StreamTokenResponse),
        )
    )]
    pub(super) fn create_stream_token() {}

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
            (status = 200, description = "Artifact created with upload URL", body = CreateArtifactResponse),
        )
    )]
    pub(super) fn create_artifact() {}

    #[utoipa::path(post, path = "/v1/runners/{runner_id}/jobs/{job_id}/artifacts/{artifact_id}/complete", tag = "Artifacts",
        params(
            ("runner_id" = String, Path),
            ("job_id" = String, Path),
            ("artifact_id" = String, Path),
        ),
        request_body = CompleteArtifactRequest,
        security(("bearer_auth" = [])),
        responses((status = 200, body = CompleteArtifactResponse))
    )]
    pub(super) fn complete_artifact() {}

    #[utoipa::path(post, path = "/v1/runners/{runner_id}/jobs/{job_id}/artifacts/{artifact_id}/abort", tag = "Artifacts",
        params(
            ("runner_id" = String, Path),
            ("job_id" = String, Path),
            ("artifact_id" = String, Path),
        ),
        request_body = CompleteArtifactRequest,
        security(("bearer_auth" = [])),
        responses((status = 200, body = CompleteArtifactResponse))
    )]
    pub(super) fn abort_artifact() {}

    /// List build artifacts
    #[utoipa::path(get, path = "/v1/builds/{build_id}/artifacts", tag = "Artifacts",
        params(("build_id" = String, Path, description = "Build ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Artifact list", body = ListArtifactsResponse),
        )
    )]
    pub(super) fn list_artifacts() {}

    /// List available artifacts across a project
    #[utoipa::path(get, path = "/v1/projects/{project_id}/artifacts", tag = "Artifacts",
        params(
            ("project_id" = String, Path, description = "Project ID"),
            ("limit" = Option<i64>, Query, description = "Maximum artifacts to return (defaults to 200, max 200)"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Project artifact list", body = ListArtifactsResponse),
            (status = 400, description = "Invalid input", body = ApiError),
        )
    )]
    pub(super) fn list_project_artifacts() {}

    /// List available artifacts for a bounded set of builds
    #[utoipa::path(post, path = "/v1/artifacts/query", tag = "Artifacts",
        request_body = ListBuildArtifactsRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Artifact list", body = ListArtifactsResponse),
            (status = 400, description = "Too many build IDs", body = ApiError),
        )
    )]
    pub(super) fn list_build_artifacts() {}

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
    pub(super) fn generate_download_link() {}

    /// Create device install link
    ///
    /// Creates a one-hour install session for an APK or install-ready signed IPA.
    /// QA viewers may use this endpoint through their artifact read permission.
    #[utoipa::path(post, path = "/v1/artifacts/{artifact_id}/install-link", tag = "Artifacts",
        params(("artifact_id" = String, Path, description = "Artifact ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Device install session", body = ArtifactInstallLinkResponse),
            (status = 404, description = "Artifact not found", body = ApiError),
            (status = 410, description = "Artifact expired", body = ApiError),
            (status = 412, description = "External HTTPS access is not ready", body = ApiError),
            (status = 422, description = "Artifact is not install-ready", body = ApiError),
        )
    )]
    pub(super) fn create_artifact_install_link() {}

    /// Upload an artifact to local storage
    ///
    /// Stores a bounded binary artifact using the signed path token. The token
    /// is the authorization; no session bearer token is used.
    #[utoipa::path(put, path = "/v1/artifacts/local-upload/{token}", tag = "Artifacts",
        params(
            ("token" = String, Path, description = "Signed local upload token"),
        ),
        request_body(content = BinaryPayload, description = "Raw artifact bytes", content_type = "application/octet-stream"),
        responses(
            (status = 200, description = "Artifact stored"),
            (status = 401, description = "Upload token is invalid or expired", body = ApiError),
            (status = 413, description = "Artifact exceeds the configured upload limit", body = ApiError),
            (status = 500, description = "Artifact storage failed", body = ApiError),
        )
    )]
    pub(super) fn upload_local_artifact() {}

    /// Download an artifact from local storage
    ///
    /// Returns the artifact bytes using the signed path token.
    #[utoipa::path(get, path = "/v1/artifacts/download/{token}", tag = "Artifacts",
        params(
            ("token" = String, Path, description = "Signed local download token"),
        ),
        responses(
            (status = 200, description = "Artifact bytes", body = BinaryPayload, content_type = "application/octet-stream"),
            (status = 404, description = "Artifact not found or token expired", body = ApiError),
            (status = 500, description = "Artifact storage failed", body = ApiError),
        )
    )]
    pub(super) fn download_local_artifact() {}

    /// Download an artifact from the legacy local-storage route
    ///
    /// Backward-compatible alias for signed local artifact downloads.
    #[utoipa::path(get, path = "/v1/artifacts/local-download/{token}", tag = "Artifacts",
        params(
            ("token" = String, Path, description = "Signed local download token"),
        ),
        responses(
            (status = 200, description = "Artifact bytes", body = BinaryPayload, content_type = "application/octet-stream"),
            (status = 404, description = "Artifact not found or token expired", body = ApiError),
            (status = 500, description = "Artifact storage failed", body = ApiError),
        )
    )]
    pub(super) fn download_local_artifact_legacy() {}

    /// Get iOS OTA install manifest
    ///
    /// Returns an Apple property-list manifest authorized by the scoped token in the URL.
    #[utoipa::path(get, path = "/install/ios/{token}/manifest.plist", tag = "Artifacts",
        params(("token" = String, Path, description = "Artifact install token")),
        responses(
            (status = 200, description = "Apple OTA installation manifest", body = String, content_type = "application/xml"),
            (status = 401, description = "Invalid or expired token", body = ApiError),
            (status = 404, description = "Artifact not found", body = ApiError),
            (status = 410, description = "Artifact expired", body = ApiError),
            (status = 412, description = "External HTTPS access is not ready", body = ApiError),
            (status = 422, description = "IPA is not install-ready", body = ApiError),
            (status = 500, description = "Install manifest could not be generated", body = ApiError),
        )
    )]
    pub(super) fn get_ios_install_manifest() {}

    /// Get an iOS OTA install manifest from the versioned route
    ///
    /// Returns an Apple property-list manifest authorized by the install token
    /// in the URL.
    #[utoipa::path(get, path = "/v1/artifacts/install/ios/{token}/manifest.plist", tag = "Artifacts",
        params(("token" = String, Path, description = "Artifact install token")),
        responses(
            (status = 200, description = "Apple OTA installation manifest", body = String, content_type = "application/xml"),
            (status = 401, description = "Invalid or expired token", body = ApiError),
            (status = 404, description = "Artifact not found", body = ApiError),
            (status = 410, description = "Artifact expired", body = ApiError),
            (status = 412, description = "External HTTPS access is not ready", body = ApiError),
            (status = 422, description = "IPA is not install-ready", body = ApiError),
            (status = 500, description = "Install manifest could not be generated", body = ApiError),
        )
    )]
    pub(super) fn get_ios_install_manifest_v1() {}

    /// Download an artifact from the install delivery route
    ///
    /// Returns local artifact bytes using the signed path token.
    #[utoipa::path(get, path = "/install/download/{token}", tag = "Artifacts",
        params(
            ("token" = String, Path, description = "Signed local download token"),
        ),
        responses(
            (status = 200, description = "Artifact bytes", body = BinaryPayload, content_type = "application/octet-stream"),
            (status = 404, description = "Artifact not found or token expired", body = ApiError),
            (status = 500, description = "Artifact storage failed", body = ApiError),
        )
    )]
    pub(super) fn download_local_artifact_install() {}

    // ── Scoped Download Tokens (OOR-140) ──

    /// Create scoped download token
    ///
    /// Generates a scoped, time-limited download token for a specific artifact.
    /// The token can be shared with external users who don't have an account.
    #[utoipa::path(post, path = "/v1/artifacts/{artifact_id}/scoped-token", tag = "Scoped Download Tokens",
        params(("artifact_id" = String, Path, description = "Artifact ID")),
        request_body = CreateScopedDownloadTokenRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Scoped download token created", body = CreateScopedDownloadTokenResponse),
            (status = 404, description = "Artifact not found", body = ApiError),
            (status = 410, description = "Artifact expired", body = ApiError),
        )
    )]
    pub(super) fn create_scoped_download_token() {}

    /// List scoped download tokens
    ///
    /// Lists all scoped download tokens for a specific artifact, including expired and revoked ones.
    #[utoipa::path(get, path = "/v1/artifacts/{artifact_id}/scoped-tokens", tag = "Scoped Download Tokens",
        params(("artifact_id" = String, Path, description = "Artifact ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "List of scoped download tokens", body = ListArtifactDownloadTokensResponse),
        )
    )]
    pub(super) fn list_scoped_download_tokens() {}

    /// Revoke scoped download token
    ///
    /// Revokes a scoped download token so it can no longer be used.
    #[utoipa::path(delete, path = "/v1/artifact-tokens/{token_id}", tag = "Scoped Download Tokens",
        params(("token_id" = String, Path, description = "Download token ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Token revoked", body = RevokeArtifactDownloadTokenResponse),
            (status = 404, description = "Token not found", body = ApiError),
        )
    )]
    pub(super) fn revoke_scoped_download_token() {}

    /// Download via scoped token
    ///
    /// Downloads an artifact using a scoped download token. No session auth required —
    /// the token itself is the authorization. For S3/R2 storage, redirects to a presigned URL.
    #[utoipa::path(get, path = "/install/artifact/{token}", tag = "Scoped Download Tokens",
        params(("token" = String, Path, description = "Scoped download token")),
        responses(
            (status = 307, description = "Temporary redirect to presigned download URL"),
            (status = 401, description = "Invalid or expired token", body = ApiError),
            (status = 410, description = "Artifact expired", body = ApiError),
            (status = 500, description = "Artifact delivery failed", body = ApiError),
            (status = 503, description = "Artifact storage is not configured", body = ApiError),
        )
    )]
    pub(super) fn download_via_scoped_token() {}

    /// Download via scoped token from the versioned route
    ///
    /// Resolves a time-limited path token and redirects to the backing storage
    /// URL. The token is the authorization; no session bearer token is used.
    #[utoipa::path(get, path = "/v1/artifacts/dl/{token}", tag = "Scoped Download Tokens",
        params(("token" = String, Path, description = "Scoped download token")),
        responses(
            (status = 307, description = "Temporary redirect to the artifact download URL"),
            (status = 401, description = "Invalid, expired, or consumed token", body = ApiError),
            (status = 410, description = "Artifact expired", body = ApiError),
            (status = 500, description = "Artifact delivery failed", body = ApiError),
            (status = 503, description = "Artifact storage is not configured", body = ApiError),
        )
    )]
    pub(super) fn download_via_scoped_token_v1() {}

    // ── Webhooks ──

    /// GitHub webhook receiver
    ///
    /// Receives push/PR events from GitHub and triggers matching pipelines.
    /// Authenticated via webhook signature verification.
    #[utoipa::path(post, path = "/v1/webhooks/github", tag = "Webhooks",
        params(
            ("X-Hub-Signature-256" = String, Header, description = "GitHub HMAC signature"),
            ("X-GitHub-Delivery" = Option<String>, Header, description = "GitHub delivery identifier"),
            ("X-GitHub-Event" = Option<String>, Header, description = "GitHub event type"),
        ),
        request_body(content = serde_json::Value, description = "GitHub webhook payload", content_type = "application/json"),
        responses(
            (status = 200, description = "Webhook processed", body = WebhookResponse),
            (status = 400, description = "Invalid JSON payload", body = ApiError),
            (status = 401, description = "Invalid signature", body = ApiError),
            (status = 413, description = "Payload exceeds the webhook limit", body = ApiError),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) fn github_webhook() {}

    /// GitLab webhook receiver
    ///
    /// Receives push/MR events from GitLab and triggers matching pipelines.
    /// Authenticated via webhook secret token.
    #[utoipa::path(post, path = "/v1/webhooks/gitlab", tag = "Webhooks",
        params(
            ("X-Gitlab-Token" = String, Header, description = "GitLab webhook token"),
            ("X-Gitlab-Event-UUID" = Option<String>, Header, description = "GitLab event identifier"),
            ("X-Gitlab-Event" = Option<String>, Header, description = "GitLab event type"),
        ),
        request_body(content = serde_json::Value, description = "GitLab webhook payload", content_type = "application/json"),
        responses(
            (status = 200, description = "Webhook processed", body = WebhookResponse),
            (status = 400, description = "Invalid JSON payload", body = ApiError),
            (status = 401, description = "Invalid token", body = ApiError),
            (status = 413, description = "Payload exceeds the webhook limit", body = ApiError),
            (status = 403, description = "Remote mode required", body = ApiError),
        )
    )]
    pub(super) fn gitlab_webhook() {}

    // ── Audit Logs ──

    /// List audit log entries
    ///
    /// Returns a paginated, filterable list of audit log entries recording
    /// user and system actions. Owner and admin roles only.
    #[utoipa::path(get, path = "/v1/audit-logs", tag = "Audit Logs",
        params(
            ("limit" = Option<i64>, Query, description = "Page size (default 50, max 200)"),
            ("offset" = Option<i64>, Query, description = "Page offset (default 0)"),
            ("actor_id" = Option<String>, Query, description = "Filter by actor user ID"),
            ("action" = Option<String>, Query, description = "Filter by action string"),
            ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
            ("from_ts" = Option<i64>, Query, description = "Start of time range (unix timestamp)"),
            ("to_ts" = Option<i64>, Query, description = "End of time range (unix timestamp)"),
            ("sort" = Option<String>, Query, description = "Sort by created_at, actor_email, action, or resource_type"),
            ("direction" = Option<String>, Query, description = "Sort direction: asc or desc"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Paginated audit log entries", body = ListAuditLogsResponse),
            (status = 400, description = "Invalid input", body = ApiError),
            (status = 403, description = "Insufficient permissions", body = ApiError),
        )
    )]
    pub(super) fn list_audit_logs() {}

    // ── API Tokens ──

    /// List API tokens
    ///
    /// Returns all API tokens visible to the caller. Admins and owners see all
    /// tokens; other roles see only their own.
    #[utoipa::path(get, path = "/v1/api-tokens", tag = "API Tokens",
        params(
            ("q" = Option<String>, Query, description = "Search API tokens"),
            ("sort" = Option<String>, Query, description = "Sort field"),
            ("direction" = Option<String>, Query, description = "Sort direction"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "List of API tokens", body = ListApiTokensResponse),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) fn list_api_tokens() {}

    /// Create API token
    ///
    /// Creates a new API token. The plaintext token is returned only in this
    /// response and cannot be retrieved again.
    #[utoipa::path(post, path = "/v1/api-tokens", tag = "API Tokens",
        request_body = CreateApiTokenRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Token created", body = CreateApiTokenResponse),
            (status = 400, description = "Invalid request", body = ApiError),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 403, description = "Forbidden or role escalation", body = ApiError),
        )
    )]
    pub(super) fn create_api_token() {}

    /// Revoke API token
    ///
    /// Revokes an API token by ID. Non-admin users can only revoke their own
    /// tokens.
    #[utoipa::path(delete, path = "/v1/api-tokens/{token_id}", tag = "API Tokens",
        params(("token_id" = String, Path, description = "API token ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Token revoked", body = RevokeApiTokenResponse),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 403, description = "Forbidden", body = ApiError),
            (status = 404, description = "Token not found", body = ApiError),
        )
    )]
    pub(super) fn revoke_api_token() {}

    // ── Notification Channels ──

    /// List notification channels
    ///
    /// Returns all configured notification channels.
    #[utoipa::path(get, path = "/v1/settings/notification-channels", tag = "Notification Channels",
        params(
            ("q" = Option<String>, Query, description = "Search notification channels"),
            ("sort" = Option<String>, Query, description = "Sort field"),
            ("direction" = Option<String>, Query, description = "Sort direction"),
            ("limit" = Option<i64>, Query, description = "Page size"),
            ("offset" = Option<i64>, Query, description = "Page offset"),
        ),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "List of notification channels", body = ListNotificationChannelsResponse),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) fn list_notification_channels() {}

    /// Create notification channel
    ///
    /// Creates a new webhook or Mattermost notification channel.
    #[utoipa::path(post, path = "/v1/settings/notification-channels", tag = "Notification Channels",
        request_body = CreateNotificationChannelRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Channel created", body = NotificationChannelResponse),
            (status = 400, description = "Invalid input", body = ApiError),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 403, description = "Forbidden", body = ApiError),
        )
    )]
    pub(super) fn create_notification_channel() {}

    /// Get notification channel
    ///
    /// Returns details for a single notification channel.
    #[utoipa::path(get, path = "/v1/settings/notification-channels/{id}", tag = "Notification Channels",
        params(("id" = String, Path, description = "Channel ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Channel details", body = NotificationChannelResponse),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn get_notification_channel() {}

    /// Update notification channel
    ///
    /// Updates an existing notification channel. Omitted fields are preserved.
    #[utoipa::path(put, path = "/v1/settings/notification-channels/{id}", tag = "Notification Channels",
        params(("id" = String, Path, description = "Channel ID")),
        request_body = UpdateNotificationChannelRequest,
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Channel updated", body = NotificationChannelResponse),
            (status = 400, description = "Invalid input", body = ApiError),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn update_notification_channel() {}

    /// Delete notification channel
    ///
    /// Permanently deletes a notification channel and its delivery history.
    #[utoipa::path(delete, path = "/v1/settings/notification-channels/{id}", tag = "Notification Channels",
        params(("id" = String, Path, description = "Channel ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Channel deleted", body = DeleteNotificationChannelResponse),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn delete_notification_channel() {}

    /// Test notification channel
    ///
    /// Sends a test notification to verify the channel is configured correctly.
    #[utoipa::path(post, path = "/v1/settings/notification-channels/{id}/test", tag = "Notification Channels",
        params(("id" = String, Path, description = "Channel ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Test result", body = TestNotificationChannelResponse),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn test_notification_channel() {}

    /// List notification deliveries
    ///
    /// Returns delivery history for a notification channel (most recent 100).
    #[utoipa::path(get, path = "/v1/settings/notification-channels/{id}/deliveries", tag = "Notification Channels",
        params(("id" = String, Path, description = "Channel ID")),
        security(("bearer_auth" = [])),
        responses(
            (status = 200, description = "Delivery history", body = ListNotificationDeliveriesResponse),
            (status = 401, description = "Unauthorized", body = ApiError),
            (status = 404, description = "Not found", body = ApiError),
        )
    )]
    pub(super) fn list_notification_deliveries() {}
}
