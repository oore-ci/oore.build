// ── Instance registry ───────────────────────────────────────────

export interface Instance {
  id: string
  label: string
  url: string
  icon?: string
  addedAt: number
}

// ── Setup state machine ─────────────────────────────────────────

export type SetupState =
  | 'uninitialized'
  | 'bootstrap_pending'
  | 'idp_configured'
  | 'owner_created'
  | 'ready'

// ── Public setup status (non-sensitive) ─────────────────────────

export interface SetupStatus {
  instance_id: string
  state: SetupState
  runtime_mode: RuntimeMode
  remote_auth_mode: RemoteAuthMode
  setup_mode: boolean
  is_configured: boolean
}

// ── API request/response types ──────────────────────────────────

export interface BootstrapTokenVerifyResponse {
  session_token: string
  expires_at: number
}

export interface OidcConfigureRequest {
  issuer_url: string
  client_id: string
  client_secret?: string
}

export interface OidcConfigureResponse {
  state: SetupState
  discovered_issuer: string
  session_expires_at?: number
}

export interface SetupOidcStartRequest {
  redirect_uri: string
}

export interface SetupOidcStartResponse {
  authorization_url: string
  state: string
}

export interface SetupOidcVerifyRequest {
  code: string
  state: string
}

export interface SetupOidcVerifyResponse {
  state: SetupState
  owner_email: string
  oidc_subject: string
  session_expires_at?: number
}

export interface SetupLocalOwnerCreateRequest {
  email: string
}

export interface SetupLocalOwnerCreateResponse {
  state: SetupState
  owner_email: string
  session_expires_at?: number
}

export interface SetupPreferencesRequest {
  runtime_mode: RuntimeMode
  remote_auth_mode?: RemoteAuthMode
}

export interface SetupPreferencesResponse {
  runtime_mode: RuntimeMode
  remote_auth_mode: RemoteAuthMode
  session_expires_at?: number
}

export interface SetupTrustedProxyConfigureRequest {
  user_email_header?: string
  setup_owner_email?: string
  trusted_proxy_cidrs: Array<string>
  shared_secret?: string
}

export interface SetupTrustedProxyConfigureResponse {
  state: SetupState
  setup_owner_email?: string
  has_shared_secret: boolean
  configured_at: number
  session_expires_at?: number
}

export interface SetupTrustedProxyClaimOwnerResponse {
  state: SetupState
  owner_email: string
  session_expires_at?: number
}

export interface SetupCompleteResponse {
  state: SetupState
  instance_id: string
}

export interface SetupSummaryResponse {
  instance_id: string
  state: SetupState
  issuer_url?: string
  owner_email?: string
}

// ── Auth response types ─────────────────────────────────────────

export interface AuthenticatedUser {
  email: string
  oidc_subject: string
  user_id?: string
  role?: UserRole
  avatar_url?: string
}

export interface OidcCallbackResponse {
  session_token: string
  expires_at: number
  user: AuthenticatedUser
}

export interface LocalLoginRequest {
  email?: string
}

export interface LocalLoginResponse {
  session_token: string
  expires_at: number
  user: AuthenticatedUser
}

// ── User management types ───────────────────────────────────────

export type UserRole = 'owner' | 'admin' | 'developer' | 'qa_viewer'

export type UserStatus = 'active' | 'disabled' | 'invited'

export interface User {
  id: string
  email: string
  display_name?: string
  role: UserRole
  status: UserStatus
  avatar_url?: string
  created_at: number
  updated_at: number
}

export interface InviteUserRequest {
  email: string
  role: UserRole
}

export interface InviteUserResponse {
  user: User
}

export interface UpdateUserRoleRequest {
  role: UserRole
}

export interface UpdateUserRoleResponse {
  user: User
}

export interface ReEnableUserResponse {
  user: User
}

export interface ListUsersResponse {
  users: Array<User>
}

export interface UserProfileResponse {
  user: User
}

export interface LogoutResponse {
  ok: boolean
}

export interface RuntimeUpdateStatus {
  phase: 'idle' | 'updating' | 'restarting' | 'failed'
  error?: string | null
  managed_service: boolean
}

export interface RuntimeReleaseStatus extends RuntimeUpdateStatus {
  version: string
  latest_version: string
  channel: string
  github_repo: string
  update_available: boolean
  release_name: string
  release_notes: string
  release_url: string
  changelog_url: string
}

// ── Structured API error ────────────────────────────────────────

export interface ApiError {
  error: string
  code: string
  details?: string
}

// ── SCM Integration types ──────────────────────────────────────

export type ScmProvider = 'github' | 'gitlab' | 'local_git'

export type IntegrationAuthMode =
  'github_app' | 'oauth_app' | 'personal_token' | 'local_path'

export type IntegrationStatus = 'active' | 'inactive' | 'error'

export interface Integration {
  id: string
  provider: ScmProvider
  host_url: string
  auth_mode: IntegrationAuthMode
  status: IntegrationStatus
  display_name?: string
  app_id?: string
  app_slug?: string
  created_by: string
  created_at: number
  updated_at: number
}

export interface IntegrationInstallation {
  id: string
  integration_id: string
  external_id: string
  account_name: string
  account_type?: string
  created_at: number
}

export interface IntegrationRepository {
  id: string
  installation_id: string
  external_id: string
  full_name: string
  default_branch?: string
  is_private: boolean
  created_at: number
  updated_at: number
}

export interface GitHubAppStartRequest {
  webhook_url: string
  redirect_url: string
}

export interface GitHubAppStartResponse {
  create_url: string
}

export interface GitHubAppCompleteRequest {
  code: string
}

export interface GitHubAppCompleteResponse {
  integration: Integration
}

export interface SyncInstallationsResponse {
  installations: Array<IntegrationInstallation>
}

export interface GitLabStartRequest {
  host_url: string
  auth_mode: string
  webhook_secret: string
  client_id?: string
  client_secret?: string
  access_token?: string
}

export interface GitLabCompleteResponse {
  integration: Integration
}

export interface GitLabAuthorizeRequest {
  integration_id: string
  redirect_url: string
}

export interface GitLabAuthorizeResponse {
  authorize_url: string
}

export interface CreateLocalGitIntegrationRequest {
  repository_path: string
  display_name?: string
}

export interface CreateLocalGitIntegrationResponse {
  integration: Integration
  repository: IntegrationRepository
}

export interface LocalGitDirectoryEntry {
  name: string
  path: string
  is_git_repository: boolean
}

export interface LocalGitPathSuggestion {
  label: string
  path: string
}

export interface BrowseLocalGitDirectoriesResponse {
  current_path: string
  current_is_git_repository: boolean
  parent_path?: string
  directories: Array<LocalGitDirectoryEntry>
  suggestions: Array<LocalGitPathSuggestion>
}

export interface ListIntegrationsResponse {
  integrations: Array<Integration>
  total: number
}

export interface IntegrationDetailResponse {
  integration: Integration
  installation_count: number
  repository_count: number
  last_webhook_at?: number
}

export interface ListInstallationsResponse {
  installations: Array<IntegrationInstallation>
}

export interface ListRepositoriesResponse {
  repositories: Array<IntegrationRepository>
}

// ── Runner domain types ────────────────────────────────────────

export type RunnerStatus = 'online' | 'offline' | 'busy' | 'draining'

export interface Runner {
  id: string
  name: string
  status: RunnerStatus | string
  capabilities: Record<string, unknown>
  last_heartbeat_at?: number
  registered_by?: string
  created_at: number
  updated_at: number
}

export interface ListRunnersResponse {
  runners: Array<Runner>
}

export interface UpdateRunnerRequest {
  name?: string
}

export interface UpdateRunnerResponse {
  runner: Runner
}

// ── Build domain types ─────────────────────────────────────────

export type BuildStatus =
  | 'queued'
  | 'scheduled'
  | 'assigned'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'canceled'
  | 'timed_out'
  | 'expired'

export type TriggerType = 'manual' | 'api' | 'webhook' | 'schedule'

export interface StepResult {
  name: string
  status: string
  exit_code?: number
  started_at: number
  finished_at: number
  duration_ms: number
}

export interface Build {
  id: string
  project_id: string
  pipeline_id: string
  build_number: number
  status: BuildStatus
  trigger_type: TriggerType
  trigger_actor?: string
  trigger_event?: string
  trigger_ref?: string
  commit_sha?: string
  branch?: string
  source_build_id?: string
  config_snapshot: Record<string, unknown>
  runner_id?: string
  /** Optional display context supplied by newer backend responses. */
  context?: BuildContext
  step_results?: Array<StepResult>
  exit_code?: number
  queued_at: number
  started_at?: number
  finished_at?: number
  created_at: number
  updated_at: number
}

export interface BuildContext {
  project_name?: string
  pipeline_name?: string
  runner_name?: string
}

export interface BuildEvent {
  id: string
  build_id: string
  from_status?: string
  to_status: string
  actor?: string
  reason?: string
  created_at: number
}

export interface CreateBuildRequest {
  pipeline_id: string
  branch?: string
  commit_sha?: string
  trigger_ref?: string
}

export interface CreateBuildResponse {
  build: Build
}

export interface BuildDetailResponse {
  build: Build
  events: Array<BuildEvent>
}

export interface ListBuildsResponse {
  builds: Array<Build>
  total: number
}

export interface CancelBuildResponse {
  build: Build
}

export interface RerunBuildResponse {
  build: Build
}

// ── Build Log types ─────────────────────────────────────────
export interface BuildLogChunk {
  sequence: number
  content: string
  stream: 'stdout' | 'stderr'
}

export interface AppendBuildLogsRequest {
  chunks: Array<BuildLogChunk>
}

export interface AppendBuildLogsResponse {
  appended: number
}

export interface BuildLogsResponse {
  logs: Array<BuildLogChunk>
  total: number
}

// ── Artifact types ──────────────────────────────────────────
export interface Artifact {
  id: string
  build_id: string
  name: string
  artifact_type: 'apk' | 'ipa' | 'app' | 'generic'
  file_path: string
  file_size?: number
  checksum?: string
  metadata: Record<string, unknown>
  created_at: number
  expires_at?: number
}

export interface ListArtifactsResponse {
  artifacts: Array<Artifact>
}

export interface ArtifactDownloadLinkResponse {
  download_url: string
  expires_at: number
}

// ── Scoped download token types ────────────────────────────

export interface CreateScopedDownloadTokenRequest {
  ttl_secs?: number
  single_use?: boolean
}

export interface CreateScopedDownloadTokenResponse {
  id: string
  download_url: string
  token: string
  prefix: string
  expires_at: number
  single_use: boolean
}

export interface ArtifactDownloadTokenSummary {
  id: string
  artifact_id: string
  prefix: string
  created_by: string
  created_by_email: string
  expires_at: number
  single_use: boolean
  used_at?: number
  revoked_at?: number
  is_expired: boolean
  is_used: boolean
  is_revoked: boolean
  created_at: number
}

export interface ListArtifactDownloadTokensResponse {
  tokens: Array<ArtifactDownloadTokenSummary>
}

export type ArtifactStorageProvider = 'disabled' | 'local' | 's3' | 'r2'
export type ArtifactStorageSource = 'database' | 'environment' | 'default'

export interface ArtifactStorageSettings {
  provider: ArtifactStorageProvider
  local_base_dir?: string
  s3_bucket?: string
  s3_region?: string
  s3_endpoint?: string
  has_access_key_id: boolean
  has_secret_access_key: boolean
  source: ArtifactStorageSource
  updated_at?: number
}

export interface ArtifactStorageSettingsResponse {
  settings: ArtifactStorageSettings
}

export interface UpdateArtifactStorageSettingsRequest {
  provider: ArtifactStorageProvider
  local_base_dir?: string
  s3_bucket?: string
  s3_region?: string
  s3_endpoint?: string
  access_key_id?: string
  secret_access_key?: string
}

export type KeyStorageMode = 'keychain' | 'file'
export type RuntimeMode = 'local' | 'remote'
export type RemoteAuthMode = 'oidc' | 'trusted_proxy'

export interface ExternalAccessPreflightCheck {
  id: string
  label: string
  ok: boolean
  message: string
  failure_code?: string
}

export interface ExternalAccessPreflightResponse {
  ready: boolean
  checks: Array<ExternalAccessPreflightCheck>
}

export type ExternalAccessNetworkSource = 'database' | 'environment' | 'default'

export interface ExternalAccessNetworkSettings {
  public_url?: string
  allowed_origins: Array<string>
  source: ExternalAccessNetworkSource
  updated_at?: number
}

export interface ExternalAccessNetworkSettingsResponse {
  settings: ExternalAccessNetworkSettings
}

export interface UpdateExternalAccessNetworkSettingsRequest {
  public_url?: string
  allowed_origins: Array<string>
}

export interface ConfigureExternalAccessOidcRequest {
  issuer_url: string
  client_id: string
  client_secret?: string
}

export interface ConfigureExternalAccessOidcResponse {
  discovered_issuer: string
  has_client_secret: boolean
  configured_at: number
}

export interface GetExternalAccessOidcResponse {
  issuer_url: string
  client_id: string
  has_client_secret: boolean
  authorization_endpoint: string
  token_endpoint: string
  userinfo_endpoint?: string
  jwks_uri: string
  configured_at: number
}

export interface TestOidcConnectionRequest {
  issuer_url: string
}

export interface TestOidcConnectionResponse {
  success: boolean
  discovered_issuer: string
  authorization_endpoint: string
  token_endpoint: string
  userinfo_endpoint?: string
  jwks_uri: string
  scopes_supported: Array<string>
}

export interface TrustedProxySettingsPublic {
  user_email_header: string
  trusted_proxy_cidrs: Array<string>
  has_shared_secret: boolean
  updated_at?: number
}

export interface TrustedProxySettingsResponse {
  settings: TrustedProxySettingsPublic
}

export interface UpdateTrustedProxySettingsRequest {
  user_email_header?: string
  trusted_proxy_cidrs: Array<string>
  shared_secret?: string
}

export interface InstancePreferences {
  key_storage_mode: KeyStorageMode
  runtime_mode: RuntimeMode
  remote_auth_mode: RemoteAuthMode
  restart_required: boolean
  updated_at?: number
}

export interface InstancePreferencesResponse {
  preferences: InstancePreferences
}

export interface UpdateInstancePreferencesRequest {
  key_storage_mode: KeyStorageMode
  runtime_mode?: RuntimeMode
  remote_auth_mode?: RemoteAuthMode
}

// ── Project domain types ────────────────────────────────────────

export interface Project {
  id: string
  name: string
  description?: string
  repository_id?: string
  settings: Record<string, unknown>
  default_branch?: string
  created_by: string
  created_at: number
  updated_at: number
}

export interface CreateProjectRequest {
  name: string
  description?: string
  repository_id?: string
  local_repository_path?: string
  default_branch?: string
}

export interface UpdateProjectRequest {
  name?: string
  description?: string
  repository_id?: string
  default_branch?: string
}

export interface CreateProjectResponse {
  project: Project
}

export interface ProjectDetailResponse {
  project: Project
  pipeline_count: number
  build_count: number
}

export interface ListProjectsResponse {
  projects: Array<Project>
  total: number
}

// ── Pipeline domain types ───────────────────────────────────────

export interface TriggerConfig {
  events: Array<string>
  branches: Array<string>
}

export interface ConcurrencyPolicy {
  cancel_previous: boolean
  max_concurrent?: number
}

export type BuildPlatform = 'android' | 'ios' | 'macos'

export interface PipelineCommandStages {
  pre_build: Array<string>
  build: Array<string>
  post_build: Array<string>
}

export interface PlatformBuildArgs {
  android: Array<string>
  ios: Array<string>
  macos: Array<string>
}

export interface PlatformBuildCommands {
  android?: string
  ios?: string
  macos?: string
}

export interface PipelineEnvVar {
  key: string
  value: string
}

export interface PipelineExecutionConfig {
  platforms: Array<BuildPlatform>
  flutter_version?: string
  commands: PipelineCommandStages
  platform_build_args?: PlatformBuildArgs
  platform_commands?: PlatformBuildCommands
  env?: Array<PipelineEnvVar>
  artifact_patterns: Array<string>
}

export interface RepositoryWorkflowExecutionPreview {
  platforms: Array<BuildPlatform>
  flutter_version?: string
  commands: PipelineCommandStages
  platform_build_args: PlatformBuildArgs
  platform_commands: PlatformBuildCommands
  env_keys: Array<string>
  artifact_patterns: Array<string>
}

export interface RepositoryWorkflowPreview {
  path: string
  valid: boolean
  errors: Array<string>
  execution?: RepositoryWorkflowExecutionPreview
}

export interface DiscoverRepositoryWorkflowsResponse {
  project_id: string
  provider: 'github' | 'gitlab' | 'local_git'
  reference: string
  workflows: Array<RepositoryWorkflowPreview>
  truncated: boolean
}

export interface Pipeline {
  id: string
  project_id: string
  name: string
  config_path: string
  config_path_explicit: boolean
  execution_config: PipelineExecutionConfig
  trigger_config: TriggerConfig
  concurrency: ConcurrencyPolicy
  enabled: boolean
  created_at: number
  updated_at: number
}

export interface CreatePipelineRequest {
  name: string
  config_path?: string
  config_path_explicit?: boolean
  execution_config?: PipelineExecutionConfig
  trigger_config: TriggerConfig
  concurrency: ConcurrencyPolicy
}

export interface UpdatePipelineRequest {
  name?: string
  config_path?: string
  config_path_explicit?: boolean
  execution_config?: PipelineExecutionConfig
  trigger_config?: TriggerConfig
  concurrency?: ConcurrencyPolicy
  enabled?: boolean
}

export interface CreatePipelineResponse {
  pipeline: Pipeline
}

export interface PipelineDetailResponse {
  pipeline: Pipeline
  build_count: number
}

export interface ListPipelinesResponse {
  pipelines: Array<Pipeline>
  total: number
}

export interface ValidatePipelineRequest {
  config_path?: string
  config_path_explicit?: boolean
  execution_config?: PipelineExecutionConfig
  trigger_config?: TriggerConfig
  concurrency?: ConcurrencyPolicy
}

export interface ValidatePipelineResponse {
  valid: boolean
  errors?: Array<string>
}

export type AndroidSigningBuildType = 'debug' | 'release'

export interface AndroidSigningProfileInput {
  enabled: boolean
  keystore_filename?: string
  keystore_base64?: string
  store_password?: string
  key_alias?: string
  key_password?: string
}

export interface UpdatePipelineAndroidSigningRequest {
  debug?: AndroidSigningProfileInput
  release?: AndroidSigningProfileInput
}

export interface AndroidSigningProfile {
  build_type: AndroidSigningBuildType
  enabled: boolean
  has_keystore: boolean
  keystore_filename?: string
  keystore_checksum?: string
  key_alias?: string
  has_store_password: boolean
  has_key_password: boolean
  updated_at?: number
}

export interface PipelineAndroidSigningResponse {
  pipeline_id: string
  debug: AndroidSigningProfile
  release: AndroidSigningProfile
}

export type IosSigningMode = 'manual' | 'api' | 'hybrid'

export interface IosCertificateInput {
  p12_filename?: string
  p12_base64?: string
  p12_password?: string
}

export interface IosProvisioningProfileInput {
  bundle_id: string
  profile_filename?: string
  profile_base64?: string
}

export interface IosApiCredentialInput {
  key_id?: string
  issuer_id?: string
  private_key_base64?: string
}

export interface UpdatePipelineIosSigningRequest {
  enabled: boolean
  mode: IosSigningMode
  team_id?: string
  bundle_ids: Array<string>
  certificate?: IosCertificateInput
  provisioning_profiles?: Array<IosProvisioningProfileInput>
  api_credentials?: IosApiCredentialInput
}

export interface IosProvisioningProfileSummary {
  bundle_id: string
  has_profile: boolean
  profile_filename?: string
  profile_uuid?: string
  profile_name?: string
  team_id?: string
  expires_at?: number
  checksum?: string
}

export interface PipelineIosSigningResponse {
  pipeline_id: string
  enabled: boolean
  mode: IosSigningMode
  team_id?: string
  export_method: string
  bundle_ids: Array<string>
  has_p12: boolean
  p12_filename?: string
  p12_fingerprint?: string
  p12_expires_at?: number
  has_p12_password: boolean
  has_api_key: boolean
  api_key_id?: string
  api_issuer_id?: string
  provisioning_profiles: Array<IosProvisioningProfileSummary>
  updated_at?: number
}

export interface RegisteredIosDevice {
  id: string
  device_id?: string
  udid: string
  name: string
  platform: string
  status: string
  added_at: number
  last_synced_at?: number
}

export interface ListPipelineIosDevicesResponse {
  pipeline_id: string
  devices: Array<RegisteredIosDevice>
}

export interface RegisterIosDeviceRequest {
  udid: string
  name: string
  platform?: string
}

export interface RegisterIosDeviceResponse {
  pipeline_id: string
  device: RegisteredIosDevice
  profile_sync_triggered: boolean
}

export interface SyncPipelineIosSigningResponse {
  pipeline_id: string
  ok: boolean
  updated_profiles: number
  synced_bundle_ids: Array<string>
  warnings: Array<string>
}

// ── Notification channels ───────────────────────────────────────

export type NotificationChannelType = 'webhook' | 'mattermost' | 'email'
export type NotificationDeliveryStatus = 'pending' | 'delivered' | 'failed'
export type SmtpTlsMode = 'none' | 'start_tls' | 'tls'

export interface SmtpConfig {
  host: string
  port: number
  username: string
  password: string
  tls_mode: SmtpTlsMode
  from_address: string
  recipients: Array<string>
}

export interface UpdateSmtpConfig {
  host?: string
  port?: number
  username?: string
  password?: string
  tls_mode?: SmtpTlsMode
  from_address?: string
  recipients?: Array<string>
}

export interface NotificationChannel {
  id: string
  name: string
  channel_type: NotificationChannelType
  enabled: boolean
  events: Array<string>
  has_url: boolean
  has_secret: boolean
  has_smtp_config: boolean
  created_by?: string
  created_at: number
  updated_at: number
}

export interface CreateNotificationChannelRequest {
  name: string
  channel_type: NotificationChannelType
  enabled?: boolean
  events?: Array<string>
  url?: string
  secret?: string
  smtp_config?: SmtpConfig
}

export interface UpdateNotificationChannelRequest {
  name?: string
  enabled?: boolean
  events?: Array<string>
  url?: string
  secret?: string
  smtp_config?: UpdateSmtpConfig
}

export interface NotificationChannelResponse {
  channel: NotificationChannel
}

export interface ListNotificationChannelsResponse {
  channels: Array<NotificationChannel>
  total: number
}

export interface DeleteNotificationChannelResponse {
  deleted: boolean
}

export interface TestNotificationChannelResponse {
  success: boolean
  error?: string
}

export interface NotificationDelivery {
  id: string
  channel_id: string
  build_id?: string
  event_type: string
  status: NotificationDeliveryStatus
  attempt_count: number
  last_error?: string
  created_at: number
  delivered_at?: number
}

export interface ListNotificationDeliveriesResponse {
  deliveries: Array<NotificationDelivery>
  total: number
}

// ── Retention policy types ──────────────────────────────────────

export type RetentionCleanupTarget = 'artifacts_only' | 'full'

export interface RetentionPolicy {
  enabled: boolean
  max_age_days?: number
  max_builds_per_project?: number
  max_artifact_size_bytes?: number
  cleanup_target: RetentionCleanupTarget
  keep_statuses: Array<string>
  dry_run: boolean
  cleanup_interval_secs: number
  artifact_ttl_days?: number
  updated_at?: number
}

export interface RetentionPolicyResponse {
  policy: RetentionPolicy
}

export interface UpdateRetentionPolicyRequest {
  enabled: boolean
  max_age_days?: number
  max_builds_per_project?: number
  max_artifact_size_bytes?: number
  cleanup_target: RetentionCleanupTarget
  keep_statuses: Array<string>
  dry_run: boolean
  cleanup_interval_secs: number
  artifact_ttl_days?: number
}

export interface ProjectRetentionOverride {
  project_id: string
  enabled?: boolean
  max_age_days?: number
  max_builds_per_project?: number
  max_artifact_size_bytes?: number
  cleanup_target?: RetentionCleanupTarget
  keep_statuses?: Array<string>
  artifact_ttl_days?: number
  updated_at?: number
}

export interface EffectiveProjectRetentionResponse {
  effective: RetentionPolicy
  has_override: boolean
  override_fields?: ProjectRetentionOverride
}

export interface UpdateProjectRetentionOverrideRequest {
  enabled?: boolean
  max_age_days?: number
  max_builds_per_project?: number
  max_artifact_size_bytes?: number
  cleanup_target?: RetentionCleanupTarget
  keep_statuses?: Array<string>
  artifact_ttl_days?: number
}

export interface RetentionCleanupSummary {
  builds_expired: number
  artifacts_deleted: number
  bytes_reclaimed: number
  dry_run: boolean
  ran_at: number
}

export interface RetentionCleanupSummaryResponse {
  last_cleanup?: RetentionCleanupSummary
}

// ── Audit Logs ──────────────────────────────────────────────────

export interface AuditLogEntry {
  id: number
  actor_id: string | null
  actor_email: string | null
  action: string
  resource_type: string
  resource_id: string | null
  details: string | null
  created_at: number
}

export interface ListAuditLogsResponse {
  entries: Array<AuditLogEntry>
  total: number
}

// ── API Token types ─────────────────────────────────────────────

export interface CreateApiTokenRequest {
  name: string
  role: string
  expires_at?: number
}

export interface CreateApiTokenResponse {
  id: string
  name: string
  prefix: string
  role: string
  created_at: number
  expires_at: number | null
  token: string
}

export interface ApiTokenSummary {
  id: string
  name: string
  prefix: string
  role: string
  created_by: string
  created_by_email: string
  created_at: number
  expires_at: number | null
  last_used_at: number | null
  is_expired: boolean
  is_revoked: boolean
}

export interface ListApiTokensResponse {
  tokens: Array<ApiTokenSummary>
}

export interface RevokeApiTokenResponse {
  revoked: boolean
}
