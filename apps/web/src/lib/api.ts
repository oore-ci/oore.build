import type {
  ApiError,
  ArtifactDownloadLinkResponse,
  ArtifactStorageSettingsResponse,
  BootstrapTokenVerifyResponse,
  BrowseLocalGitDirectoriesResponse,
  BuildDetailResponse,
  BuildLogsResponse,
  CancelBuildResponse,
  ConfigureExternalAccessOidcRequest,
  ConfigureExternalAccessOidcResponse,
  CreateBuildRequest,
  CreateBuildResponse,
  CreateLocalGitIntegrationRequest,
  CreateLocalGitIntegrationResponse,
  CreateNotificationChannelRequest,
  CreatePipelineRequest,
  CreatePipelineResponse,
  CreateProjectRequest,
  CreateProjectResponse,
  DeleteNotificationChannelResponse,
  EffectiveProjectRetentionResponse,
  ExternalAccessNetworkSettingsResponse,
  ExternalAccessPreflightResponse,
  GetExternalAccessOidcResponse,
  GitHubAppCompleteRequest,
  GitHubAppCompleteResponse,
  GitHubAppStartRequest,
  GitHubAppStartResponse,
  GitLabAuthorizeRequest,
  GitLabAuthorizeResponse,
  GitLabCompleteResponse,
  GitLabStartRequest,
  InstancePreferencesResponse,
  IntegrationDetailResponse,
  InviteUserRequest,
  InviteUserResponse,
  ListArtifactsResponse,
  ListAuditLogsResponse,
  ListBuildsResponse,
  ListInstallationsResponse,
  ListIntegrationsResponse,
  ListNotificationChannelsResponse,
  ListNotificationDeliveriesResponse,
  ListPipelineIosDevicesResponse,
  ListPipelinesResponse,
  ListProjectsResponse,
  ListRepositoriesResponse,
  ListRunnersResponse,
  ListUsersResponse,
  LocalLoginRequest,
  LocalLoginResponse,
  LogoutResponse,
  NotificationChannelResponse,
  OidcConfigureRequest,
  OidcConfigureResponse,
  PipelineAndroidSigningResponse,
  PipelineDetailResponse,
  PipelineIosSigningResponse,
  ProjectDetailResponse,
  ReEnableUserResponse,
  RegisterIosDeviceRequest,
  RegisterIosDeviceResponse,
  RetentionCleanupSummaryResponse,
  RetentionPolicyResponse,
  SetupCompleteResponse,
  SetupLocalOwnerCreateResponse,
  SetupOidcStartResponse,
  SetupOidcVerifyResponse,
  SetupPreferencesRequest,
  SetupPreferencesResponse,
  SetupStatus,
  SetupSummaryResponse,
  SetupTrustedProxyClaimOwnerResponse,
  SetupTrustedProxyConfigureRequest,
  SetupTrustedProxyConfigureResponse,
  SyncInstallationsResponse,
  SyncPipelineIosSigningResponse,
  TestNotificationChannelResponse,
  TestOidcConnectionRequest,
  TestOidcConnectionResponse,
  TrustedProxySettingsResponse,
  UpdateArtifactStorageSettingsRequest,
  UpdateExternalAccessNetworkSettingsRequest,
  UpdateInstancePreferencesRequest,
  UpdateNotificationChannelRequest,
  UpdatePipelineAndroidSigningRequest,
  UpdatePipelineIosSigningRequest,
  UpdatePipelineRequest,
  UpdateProjectRequest,
  UpdateProjectRetentionOverrideRequest,
  UpdateRetentionPolicyRequest,
  UpdateRunnerRequest,
  UpdateRunnerResponse,
  UpdateTrustedProxySettingsRequest,
  UpdateUserRoleRequest,
  UpdateUserRoleResponse,
  UserProfileResponse,
  ValidatePipelineRequest,
  ValidatePipelineResponse,
} from '@/lib/types'

// ── Error class ─────────────────────────────────────────────────

export class ApiClientError extends Error {
  readonly status: number
  readonly code: string
  readonly details: string | undefined

  constructor(status: number, body: ApiError) {
    super(body.error)
    this.name = 'ApiClientError'
    this.status = status
    this.code = body.code
    this.details = body.details
  }
}

// ── Fetch wrapper ───────────────────────────────────────────────

async function request<T>(
  baseUrl: string,
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const method = (options.method ?? 'GET').toUpperCase()
  // Only set Content-Type on requests with a body. GET/HEAD without it
  // avoids triggering CORS preflight (important for tunneled backends).
  const headers: Record<string, string> = {
    ...(method !== 'GET' && method !== 'HEAD'
      ? { 'Content-Type': 'application/json' }
      : {}),
    ...(options.headers as Record<string, string>),
  }

  const res = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers,
  })

  if (!res.ok) {
    let body: ApiError
    try {
      body = (await res.json()) as ApiError
    } catch {
      body = {
        error: `Request failed with status ${res.status}`,
        code: 'unknown_error',
      }
    }
    throw new ApiClientError(res.status, body)
  }

  return (await res.json()) as T
}

function authHeaders(token: string): Record<string, string> {
  return { Authorization: `Bearer ${token}` }
}

// ── Error helpers ───────────────────────────────────────────────

export function getApiErrorMessage(
  error: unknown,
  codeMap: Record<string, string>,
): string {
  if (error instanceof ApiClientError) {
    return codeMap[error.code] ?? error.message
  }
  if (error instanceof Error) {
    return error.message
  }
  return 'An unexpected error occurred. Please try again.'
}

// ── API functions ───────────────────────────────────────────────

export function getSetupStatus(baseUrl: string): Promise<SetupStatus> {
  return request<SetupStatus>(baseUrl, '/v1/public/setup-status')
}

export function verifyBootstrapToken(
  baseUrl: string,
  token: string,
): Promise<BootstrapTokenVerifyResponse> {
  return request<BootstrapTokenVerifyResponse>(
    baseUrl,
    '/v1/setup/bootstrap-token/verify',
    {
      method: 'POST',
      body: JSON.stringify({ token }),
    },
  )
}

export function configureOidc(
  baseUrl: string,
  sessionToken: string,
  data: OidcConfigureRequest,
): Promise<OidcConfigureResponse> {
  return request<OidcConfigureResponse>(baseUrl, '/v1/setup/oidc/configure', {
    method: 'POST',
    headers: authHeaders(sessionToken),
    body: JSON.stringify(data),
  })
}

export function setupOidcStart(
  baseUrl: string,
  sessionToken: string,
  redirectUri: string,
): Promise<SetupOidcStartResponse> {
  return request<SetupOidcStartResponse>(
    baseUrl,
    '/v1/setup/owner/start-oidc',
    {
      method: 'POST',
      headers: authHeaders(sessionToken),
      body: JSON.stringify({ redirect_uri: redirectUri }),
    },
  )
}

export function setupOidcVerify(
  baseUrl: string,
  sessionToken: string,
  code: string,
  state: string,
): Promise<SetupOidcVerifyResponse> {
  return request<SetupOidcVerifyResponse>(
    baseUrl,
    '/v1/setup/owner/verify-oidc',
    {
      method: 'POST',
      headers: authHeaders(sessionToken),
      body: JSON.stringify({ code, state }),
    },
  )
}

export function setupLocalOwnerCreate(
  baseUrl: string,
  sessionToken: string,
  email: string,
): Promise<SetupLocalOwnerCreateResponse> {
  return request<SetupLocalOwnerCreateResponse>(
    baseUrl,
    '/v1/setup/local-owner/create',
    {
      method: 'POST',
      headers: authHeaders(sessionToken),
      body: JSON.stringify({ email }),
    },
  )
}

export function setupPreferences(
  baseUrl: string,
  sessionToken: string,
  data: SetupPreferencesRequest,
): Promise<SetupPreferencesResponse> {
  return request<SetupPreferencesResponse>(baseUrl, '/v1/setup/preferences', {
    method: 'POST',
    headers: authHeaders(sessionToken),
    body: JSON.stringify(data),
  })
}

export function setupTrustedProxyConfigure(
  baseUrl: string,
  sessionToken: string,
  data: SetupTrustedProxyConfigureRequest,
): Promise<SetupTrustedProxyConfigureResponse> {
  return request<SetupTrustedProxyConfigureResponse>(
    baseUrl,
    '/v1/setup/trusted-proxy/configure',
    {
      method: 'POST',
      headers: authHeaders(sessionToken),
      body: JSON.stringify(data),
    },
  )
}

export function setupTrustedProxyClaimOwner(
  baseUrl: string,
  sessionToken: string,
): Promise<SetupTrustedProxyClaimOwnerResponse> {
  return request<SetupTrustedProxyClaimOwnerResponse>(
    baseUrl,
    '/v1/setup/owner/claim-trusted-proxy',
    {
      method: 'POST',
      headers: authHeaders(sessionToken),
    },
  )
}

export function completeSetup(
  baseUrl: string,
  sessionToken: string,
): Promise<SetupCompleteResponse> {
  return request<SetupCompleteResponse>(baseUrl, '/v1/setup/complete', {
    method: 'POST',
    headers: authHeaders(sessionToken),
  })
}

export function getSetupSummary(
  baseUrl: string,
  sessionToken: string,
): Promise<SetupSummaryResponse> {
  return request<SetupSummaryResponse>(baseUrl, '/v1/setup/summary', {
    headers: authHeaders(sessionToken),
  })
}

// ── User management API ─────────────────────────────────────────

export function getMe(
  baseUrl: string,
  token: string,
): Promise<UserProfileResponse> {
  return request<UserProfileResponse>(baseUrl, '/v1/users/me', {
    headers: authHeaders(token),
  })
}

export function listUsers(
  baseUrl: string,
  token: string,
): Promise<ListUsersResponse> {
  return request<ListUsersResponse>(baseUrl, '/v1/users', {
    headers: authHeaders(token),
  })
}

export function inviteUser(
  baseUrl: string,
  token: string,
  data: InviteUserRequest,
): Promise<InviteUserResponse> {
  return request<InviteUserResponse>(baseUrl, '/v1/users/invite', {
    method: 'POST',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export function updateUserRole(
  baseUrl: string,
  token: string,
  userId: string,
  data: UpdateUserRoleRequest,
): Promise<UpdateUserRoleResponse> {
  return request<UpdateUserRoleResponse>(baseUrl, `/v1/users/${userId}/role`, {
    method: 'PATCH',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export function reEnableUser(
  baseUrl: string,
  token: string,
  userId: string,
): Promise<ReEnableUserResponse> {
  return request<ReEnableUserResponse>(baseUrl, `/v1/users/${userId}/enable`, {
    method: 'POST',
    headers: authHeaders(token),
  })
}

export function deleteUser(
  baseUrl: string,
  token: string,
  userId: string,
): Promise<{ ok: boolean }> {
  return request<{ ok: boolean }>(baseUrl, `/v1/users/${userId}`, {
    method: 'DELETE',
    headers: authHeaders(token),
  })
}

export function logout(
  baseUrl: string,
  token: string,
): Promise<LogoutResponse> {
  return request<LogoutResponse>(baseUrl, '/v1/auth/logout', {
    method: 'POST',
    headers: authHeaders(token),
  })
}

export function localLogin(
  baseUrl: string,
  data: LocalLoginRequest,
): Promise<LocalLoginResponse> {
  return request<LocalLoginResponse>(baseUrl, '/v1/auth/local/login', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}

export function trustedProxyLogin(
  baseUrl: string,
): Promise<LocalLoginResponse> {
  return request<LocalLoginResponse>(baseUrl, '/v1/auth/trusted-proxy/login', {
    method: 'POST',
  })
}

// ── Integration API ─────────────────────────────────────────────

export function listIntegrations(
  baseUrl: string,
  token: string,
  params?: { provider?: string; limit?: number; offset?: number },
): Promise<ListIntegrationsResponse> {
  const query = new URLSearchParams()
  if (params?.provider) query.set('provider', params.provider)
  if (params?.limit) query.set('limit', String(params.limit))
  if (params?.offset) query.set('offset', String(params.offset))
  const qs = query.toString()
  return request<ListIntegrationsResponse>(
    baseUrl,
    `/v1/integrations${qs ? `?${qs}` : ''}`,
    { headers: authHeaders(token) },
  )
}

export function getIntegration(
  baseUrl: string,
  token: string,
  id: string,
): Promise<IntegrationDetailResponse> {
  return request<IntegrationDetailResponse>(baseUrl, `/v1/integrations/${id}`, {
    headers: authHeaders(token),
  })
}

export function deleteIntegration(
  baseUrl: string,
  token: string,
  id: string,
): Promise<{ ok: boolean }> {
  return request<{ ok: boolean }>(baseUrl, `/v1/integrations/${id}`, {
    method: 'DELETE',
    headers: authHeaders(token),
  })
}

export function listIntegrationRepos(
  baseUrl: string,
  token: string,
  integrationId: string,
): Promise<ListRepositoriesResponse> {
  return request<ListRepositoriesResponse>(
    baseUrl,
    `/v1/integrations/${integrationId}/repositories`,
    { headers: authHeaders(token) },
  )
}

export function githubAppStart(
  baseUrl: string,
  token: string,
  data: GitHubAppStartRequest,
): Promise<GitHubAppStartResponse> {
  return request<GitHubAppStartResponse>(
    baseUrl,
    '/v1/integrations/github/start',
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function githubAppComplete(
  baseUrl: string,
  token: string,
  data: GitHubAppCompleteRequest,
): Promise<GitHubAppCompleteResponse> {
  return request<GitHubAppCompleteResponse>(
    baseUrl,
    '/v1/integrations/github/complete',
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function syncInstallations(
  baseUrl: string,
  token: string,
  integrationId: string,
): Promise<SyncInstallationsResponse> {
  return request<SyncInstallationsResponse>(
    baseUrl,
    `/v1/integrations/${integrationId}/installations`,
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify({}),
    },
  )
}

export function listInstallations(
  baseUrl: string,
  token: string,
  integrationId: string,
): Promise<ListInstallationsResponse> {
  return request<ListInstallationsResponse>(
    baseUrl,
    `/v1/integrations/${integrationId}/installations`,
    { headers: authHeaders(token) },
  )
}

export function gitlabStart(
  baseUrl: string,
  token: string,
  data: GitLabStartRequest,
): Promise<GitLabCompleteResponse> {
  return request<GitLabCompleteResponse>(
    baseUrl,
    '/v1/integrations/gitlab/start',
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function gitlabAuthorize(
  baseUrl: string,
  token: string,
  data: GitLabAuthorizeRequest,
): Promise<GitLabAuthorizeResponse> {
  return request<GitLabAuthorizeResponse>(
    baseUrl,
    '/v1/integrations/gitlab/authorize',
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function createLocalGitIntegration(
  baseUrl: string,
  token: string,
  data: CreateLocalGitIntegrationRequest,
): Promise<CreateLocalGitIntegrationResponse> {
  return request<CreateLocalGitIntegrationResponse>(
    baseUrl,
    '/v1/integrations/local-git',
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function browseLocalGitDirectories(
  baseUrl: string,
  token: string,
  path?: string,
): Promise<BrowseLocalGitDirectoriesResponse> {
  const params = new URLSearchParams()
  if (path?.trim()) {
    params.set('path', path.trim())
  }
  const query = params.toString()
  const endpoint = query
    ? `/v1/integrations/local-git/directories?${query}`
    : '/v1/integrations/local-git/directories'

  return request<BrowseLocalGitDirectoriesResponse>(baseUrl, endpoint, {
    headers: authHeaders(token),
  })
}

export function listLocalGitIntegrations(
  baseUrl: string,
  token: string,
): Promise<ListIntegrationsResponse> {
  return request<ListIntegrationsResponse>(
    baseUrl,
    '/v1/integrations/local-git',
    {
      headers: authHeaders(token),
    },
  )
}

export function deleteLocalGitIntegration(
  baseUrl: string,
  token: string,
  id: string,
): Promise<{ ok: boolean }> {
  return request<{ ok: boolean }>(baseUrl, `/v1/integrations/local-git/${id}`, {
    method: 'DELETE',
    headers: authHeaders(token),
  })
}

// ── Runner API ─────────────────────────────────────────────────

export function listRunners(
  baseUrl: string,
  token: string,
): Promise<ListRunnersResponse> {
  return request<ListRunnersResponse>(baseUrl, '/v1/runners', {
    headers: authHeaders(token),
  })
}

export function getRunner(
  baseUrl: string,
  token: string,
  runnerId: string,
): Promise<UpdateRunnerResponse> {
  return request<UpdateRunnerResponse>(baseUrl, `/v1/runners/${runnerId}`, {
    headers: authHeaders(token),
  })
}

export function updateRunner(
  baseUrl: string,
  token: string,
  runnerId: string,
  data: UpdateRunnerRequest,
): Promise<UpdateRunnerResponse> {
  return request<UpdateRunnerResponse>(baseUrl, `/v1/runners/${runnerId}`, {
    method: 'PATCH',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

// ── Instance Settings API ──────────────────────────────────────

export function getArtifactStorageSettings(
  baseUrl: string,
  token: string,
): Promise<ArtifactStorageSettingsResponse> {
  return request<ArtifactStorageSettingsResponse>(
    baseUrl,
    '/v1/settings/artifact-storage',
    {
      headers: authHeaders(token),
    },
  )
}

export function updateArtifactStorageSettings(
  baseUrl: string,
  token: string,
  data: UpdateArtifactStorageSettingsRequest,
): Promise<ArtifactStorageSettingsResponse> {
  return request<ArtifactStorageSettingsResponse>(
    baseUrl,
    '/v1/settings/artifact-storage',
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function getInstancePreferences(
  baseUrl: string,
  token: string,
): Promise<InstancePreferencesResponse> {
  return request<InstancePreferencesResponse>(
    baseUrl,
    '/v1/settings/preferences',
    {
      headers: authHeaders(token),
    },
  )
}

export function getExternalAccessPreflight(
  baseUrl: string,
  token: string,
): Promise<ExternalAccessPreflightResponse> {
  return request<ExternalAccessPreflightResponse>(
    baseUrl,
    '/v1/settings/external-access/preflight',
    {
      headers: authHeaders(token),
    },
  )
}

export function getExternalAccessNetworkSettings(
  baseUrl: string,
  token: string,
): Promise<ExternalAccessNetworkSettingsResponse> {
  return request<ExternalAccessNetworkSettingsResponse>(
    baseUrl,
    '/v1/settings/external-access/network',
    {
      headers: authHeaders(token),
    },
  )
}

export function updateExternalAccessNetworkSettings(
  baseUrl: string,
  token: string,
  data: UpdateExternalAccessNetworkSettingsRequest,
): Promise<ExternalAccessNetworkSettingsResponse> {
  return request<ExternalAccessNetworkSettingsResponse>(
    baseUrl,
    '/v1/settings/external-access/network',
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function getExternalAccessTrustedProxySettings(
  baseUrl: string,
  token: string,
): Promise<TrustedProxySettingsResponse> {
  return request<TrustedProxySettingsResponse>(
    baseUrl,
    '/v1/settings/external-access/trusted-proxy',
    {
      headers: authHeaders(token),
    },
  )
}

export function updateExternalAccessTrustedProxySettings(
  baseUrl: string,
  token: string,
  data: UpdateTrustedProxySettingsRequest,
): Promise<TrustedProxySettingsResponse> {
  return request<TrustedProxySettingsResponse>(
    baseUrl,
    '/v1/settings/external-access/trusted-proxy',
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function getExternalAccessOidc(
  baseUrl: string,
  token: string,
): Promise<GetExternalAccessOidcResponse> {
  return request<GetExternalAccessOidcResponse>(
    baseUrl,
    '/v1/settings/external-access/oidc',
    { headers: authHeaders(token) },
  )
}

export function configureExternalAccessOidc(
  baseUrl: string,
  token: string,
  data: ConfigureExternalAccessOidcRequest,
): Promise<ConfigureExternalAccessOidcResponse> {
  return request<ConfigureExternalAccessOidcResponse>(
    baseUrl,
    '/v1/settings/external-access/oidc',
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function testOidcConnection(
  baseUrl: string,
  token: string,
  data: TestOidcConnectionRequest,
): Promise<TestOidcConnectionResponse> {
  return request<TestOidcConnectionResponse>(
    baseUrl,
    '/v1/settings/external-access/oidc/test-connection',
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function updateInstancePreferences(
  baseUrl: string,
  token: string,
  data: UpdateInstancePreferencesRequest,
): Promise<InstancePreferencesResponse> {
  return request<InstancePreferencesResponse>(
    baseUrl,
    '/v1/settings/preferences',
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

// ── Build API ──────────────────────────────────────────────────

export function createBuild(
  baseUrl: string,
  token: string,
  projectId: string,
  data: CreateBuildRequest,
): Promise<CreateBuildResponse> {
  return request<CreateBuildResponse>(
    baseUrl,
    `/v1/projects/${projectId}/builds`,
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function listBuilds(
  baseUrl: string,
  token: string,
  params?: {
    project_id?: string
    pipeline_id?: string
    status?: string
    branch?: string
    limit?: number
    offset?: number
  },
): Promise<ListBuildsResponse> {
  const query = new URLSearchParams()
  if (params?.project_id) query.set('project_id', params.project_id)
  if (params?.pipeline_id) query.set('pipeline_id', params.pipeline_id)
  if (params?.status) query.set('status', params.status)
  if (params?.branch) query.set('branch', params.branch)
  if (params?.limit) query.set('limit', String(params.limit))
  if (params?.offset) query.set('offset', String(params.offset))
  const qs = query.toString()
  return request<ListBuildsResponse>(
    baseUrl,
    `/v1/builds${qs ? `?${qs}` : ''}`,
    { headers: authHeaders(token) },
  )
}

export function getBuild(
  baseUrl: string,
  token: string,
  buildId: string,
): Promise<BuildDetailResponse> {
  return request<BuildDetailResponse>(baseUrl, `/v1/builds/${buildId}`, {
    headers: authHeaders(token),
  })
}

export function cancelBuild(
  baseUrl: string,
  token: string,
  buildId: string,
): Promise<CancelBuildResponse> {
  return request<CancelBuildResponse>(baseUrl, `/v1/builds/${buildId}/cancel`, {
    method: 'POST',
    headers: authHeaders(token),
  })
}

// ── Stream Token API ────────────────────────────────────────

export function createStreamToken(
  baseUrl: string,
  token: string,
  buildId: string,
): Promise<{ token: string; expires_at: number }> {
  return request<{ token: string; expires_at: number }>(
    baseUrl,
    `/v1/builds/${buildId}/stream-token`,
    { method: 'POST', headers: authHeaders(token) },
  )
}

// ── Build Logs API ──────────────────────────────────────────

export function getBuildLogs(
  baseUrl: string,
  token: string,
  buildId: string,
  params?: { after_sequence?: number; limit?: number },
): Promise<BuildLogsResponse> {
  const query = new URLSearchParams()
  if (params?.after_sequence != null)
    query.set('after_sequence', String(params.after_sequence))
  if (params?.limit) query.set('limit', String(params.limit))
  const qs = query.toString()
  return request<BuildLogsResponse>(
    baseUrl,
    `/v1/builds/${buildId}/logs${qs ? `?${qs}` : ''}`,
    { headers: authHeaders(token) },
  )
}

// ── Artifact API ────────────────────────────────────────────

export function listArtifacts(
  baseUrl: string,
  token: string,
  buildId: string,
): Promise<ListArtifactsResponse> {
  return request<ListArtifactsResponse>(
    baseUrl,
    `/v1/builds/${buildId}/artifacts`,
    { headers: authHeaders(token) },
  )
}

export function getArtifactDownloadLink(
  baseUrl: string,
  token: string,
  artifactId: string,
): Promise<ArtifactDownloadLinkResponse> {
  return request<ArtifactDownloadLinkResponse>(
    baseUrl,
    `/v1/artifacts/${artifactId}/download-link`,
    { method: 'POST', headers: authHeaders(token) },
  )
}

// ── Project API ─────────────────────────────────────────────────

export function listProjects(
  baseUrl: string,
  token: string,
  params?: { search?: string; limit?: number; offset?: number },
): Promise<ListProjectsResponse> {
  const query = new URLSearchParams()
  if (params?.search) query.set('search', params.search)
  if (params?.limit) query.set('limit', String(params.limit))
  if (params?.offset) query.set('offset', String(params.offset))
  const qs = query.toString()
  return request<ListProjectsResponse>(
    baseUrl,
    `/v1/projects${qs ? `?${qs}` : ''}`,
    { headers: authHeaders(token) },
  )
}

export function getProject(
  baseUrl: string,
  token: string,
  projectId: string,
): Promise<ProjectDetailResponse> {
  return request<ProjectDetailResponse>(baseUrl, `/v1/projects/${projectId}`, {
    headers: authHeaders(token),
  })
}

export function createProject(
  baseUrl: string,
  token: string,
  data: CreateProjectRequest,
): Promise<CreateProjectResponse> {
  return request<CreateProjectResponse>(baseUrl, '/v1/projects', {
    method: 'POST',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export function updateProject(
  baseUrl: string,
  token: string,
  projectId: string,
  data: UpdateProjectRequest,
): Promise<CreateProjectResponse> {
  return request<CreateProjectResponse>(baseUrl, `/v1/projects/${projectId}`, {
    method: 'PATCH',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export async function deleteProject(
  baseUrl: string,
  token: string,
  projectId: string,
): Promise<void> {
  const res = await fetch(`${baseUrl}/v1/projects/${projectId}`, {
    method: 'DELETE',
    headers: {
      'Content-Type': 'application/json',
      ...authHeaders(token),
    },
  })
  if (!res.ok) {
    let body: ApiError
    try {
      body = (await res.json()) as ApiError
    } catch {
      body = {
        error: `Request failed with status ${res.status}`,
        code: 'unknown_error',
      }
    }
    throw new ApiClientError(res.status, body)
  }
}

// ── Pipeline API ────────────────────────────────────────────────

export function listPipelines(
  baseUrl: string,
  token: string,
  projectId: string,
  params?: { limit?: number; offset?: number },
): Promise<ListPipelinesResponse> {
  const query = new URLSearchParams()
  if (params?.limit) query.set('limit', String(params.limit))
  if (params?.offset) query.set('offset', String(params.offset))
  const qs = query.toString()
  return request<ListPipelinesResponse>(
    baseUrl,
    `/v1/projects/${projectId}/pipelines${qs ? `?${qs}` : ''}`,
    { headers: authHeaders(token) },
  )
}

export function getPipeline(
  baseUrl: string,
  token: string,
  pipelineId: string,
): Promise<PipelineDetailResponse> {
  return request<PipelineDetailResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}`,
    { headers: authHeaders(token) },
  )
}

export function createPipeline(
  baseUrl: string,
  token: string,
  projectId: string,
  data: CreatePipelineRequest,
): Promise<CreatePipelineResponse> {
  return request<CreatePipelineResponse>(
    baseUrl,
    `/v1/projects/${projectId}/pipelines`,
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function updatePipeline(
  baseUrl: string,
  token: string,
  pipelineId: string,
  data: UpdatePipelineRequest,
): Promise<CreatePipelineResponse> {
  return request<CreatePipelineResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}`,
    {
      method: 'PATCH',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export async function deletePipeline(
  baseUrl: string,
  token: string,
  pipelineId: string,
): Promise<void> {
  const res = await fetch(`${baseUrl}/v1/pipelines/${pipelineId}`, {
    method: 'DELETE',
    headers: {
      'Content-Type': 'application/json',
      ...authHeaders(token),
    },
  })
  if (!res.ok) {
    let body: ApiError
    try {
      body = (await res.json()) as ApiError
    } catch {
      body = {
        error: `Request failed with status ${res.status}`,
        code: 'unknown_error',
      }
    }
    throw new ApiClientError(res.status, body)
  }
}

export function validatePipeline(
  baseUrl: string,
  token: string,
  data: ValidatePipelineRequest,
): Promise<ValidatePipelineResponse> {
  return request<ValidatePipelineResponse>(baseUrl, '/v1/pipelines/validate', {
    method: 'POST',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export function getPipelineAndroidSigning(
  baseUrl: string,
  token: string,
  pipelineId: string,
): Promise<PipelineAndroidSigningResponse> {
  return request<PipelineAndroidSigningResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}/android-signing`,
    { headers: authHeaders(token) },
  )
}

export function updatePipelineAndroidSigning(
  baseUrl: string,
  token: string,
  pipelineId: string,
  data: UpdatePipelineAndroidSigningRequest,
): Promise<PipelineAndroidSigningResponse> {
  return request<PipelineAndroidSigningResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}/android-signing`,
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function getPipelineIosSigning(
  baseUrl: string,
  token: string,
  pipelineId: string,
): Promise<PipelineIosSigningResponse> {
  return request<PipelineIosSigningResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}/ios-signing`,
    { headers: authHeaders(token) },
  )
}

export function updatePipelineIosSigning(
  baseUrl: string,
  token: string,
  pipelineId: string,
  data: UpdatePipelineIosSigningRequest,
): Promise<PipelineIosSigningResponse> {
  return request<PipelineIosSigningResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}/ios-signing`,
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function syncPipelineIosSigning(
  baseUrl: string,
  token: string,
  pipelineId: string,
): Promise<SyncPipelineIosSigningResponse> {
  return request<SyncPipelineIosSigningResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}/ios-signing/sync`,
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify({}),
    },
  )
}

export function listPipelineIosDevices(
  baseUrl: string,
  token: string,
  pipelineId: string,
): Promise<ListPipelineIosDevicesResponse> {
  return request<ListPipelineIosDevicesResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}/ios-signing/devices`,
    { headers: authHeaders(token) },
  )
}

export function registerPipelineIosDevice(
  baseUrl: string,
  token: string,
  pipelineId: string,
  data: RegisterIosDeviceRequest,
): Promise<RegisterIosDeviceResponse> {
  return request<RegisterIosDeviceResponse>(
    baseUrl,
    `/v1/pipelines/${pipelineId}/ios-signing/devices/register`,
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

// ── Notification channels ───────────────────────────────────────

export function listNotificationChannels(
  baseUrl: string,
  token: string,
): Promise<ListNotificationChannelsResponse> {
  return request<ListNotificationChannelsResponse>(
    baseUrl,
    '/v1/settings/notification-channels',
    { headers: authHeaders(token) },
  )
}

export function createNotificationChannel(
  baseUrl: string,
  token: string,
  data: CreateNotificationChannelRequest,
): Promise<NotificationChannelResponse> {
  return request<NotificationChannelResponse>(
    baseUrl,
    '/v1/settings/notification-channels',
    {
      method: 'POST',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function getNotificationChannel(
  baseUrl: string,
  token: string,
  id: string,
): Promise<NotificationChannelResponse> {
  return request<NotificationChannelResponse>(
    baseUrl,
    `/v1/settings/notification-channels/${id}`,
    { headers: authHeaders(token) },
  )
}

export function updateNotificationChannel(
  baseUrl: string,
  token: string,
  id: string,
  data: UpdateNotificationChannelRequest,
): Promise<NotificationChannelResponse> {
  return request<NotificationChannelResponse>(
    baseUrl,
    `/v1/settings/notification-channels/${id}`,
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export function deleteNotificationChannel(
  baseUrl: string,
  token: string,
  id: string,
): Promise<DeleteNotificationChannelResponse> {
  return request<DeleteNotificationChannelResponse>(
    baseUrl,
    `/v1/settings/notification-channels/${id}`,
    {
      method: 'DELETE',
      headers: authHeaders(token),
    },
  )
}

export function testNotificationChannel(
  baseUrl: string,
  token: string,
  id: string,
): Promise<TestNotificationChannelResponse> {
  return request<TestNotificationChannelResponse>(
    baseUrl,
    `/v1/settings/notification-channels/${id}/test`,
    {
      method: 'POST',
      headers: authHeaders(token),
    },
  )
}

export function listNotificationDeliveries(
  baseUrl: string,
  token: string,
  channelId: string,
): Promise<ListNotificationDeliveriesResponse> {
  return request<ListNotificationDeliveriesResponse>(
    baseUrl,
    `/v1/settings/notification-channels/${channelId}/deliveries`,
    { headers: authHeaders(token) },
  )
}

// ── Retention Policy API ────────────────────────────────────────

export function getRetentionPolicy(
  baseUrl: string,
  token: string,
): Promise<RetentionPolicyResponse> {
  return request<RetentionPolicyResponse>(baseUrl, '/v1/settings/retention', {
    headers: authHeaders(token),
  })
}

export function updateRetentionPolicy(
  baseUrl: string,
  token: string,
  data: UpdateRetentionPolicyRequest,
): Promise<RetentionPolicyResponse> {
  return request<RetentionPolicyResponse>(baseUrl, '/v1/settings/retention', {
    method: 'PUT',
    headers: authHeaders(token),
    body: JSON.stringify(data),
  })
}

export function getRetentionLastCleanup(
  baseUrl: string,
  token: string,
): Promise<RetentionCleanupSummaryResponse> {
  return request<RetentionCleanupSummaryResponse>(
    baseUrl,
    '/v1/settings/retention/last-cleanup',
    { headers: authHeaders(token) },
  )
}

export function getProjectRetention(
  baseUrl: string,
  token: string,
  projectId: string,
): Promise<EffectiveProjectRetentionResponse> {
  return request<EffectiveProjectRetentionResponse>(
    baseUrl,
    `/v1/projects/${projectId}/retention`,
    { headers: authHeaders(token) },
  )
}

export function updateProjectRetention(
  baseUrl: string,
  token: string,
  projectId: string,
  data: UpdateProjectRetentionOverrideRequest,
): Promise<EffectiveProjectRetentionResponse> {
  return request<EffectiveProjectRetentionResponse>(
    baseUrl,
    `/v1/projects/${projectId}/retention`,
    {
      method: 'PUT',
      headers: authHeaders(token),
      body: JSON.stringify(data),
    },
  )
}

export async function deleteProjectRetention(
  baseUrl: string,
  token: string,
  projectId: string,
): Promise<EffectiveProjectRetentionResponse> {
  return request<EffectiveProjectRetentionResponse>(
    baseUrl,
    `/v1/projects/${projectId}/retention`,
    {
      method: 'DELETE',
      headers: authHeaders(token),
    },
  )
}

// ── Audit Logs ──────────────────────────────────────────────────

export function listAuditLogs(
  baseUrl: string,
  token: string,
  params?: {
    limit?: number
    offset?: number
    actor_id?: string
    action?: string
    resource_type?: string
    from_ts?: number
    to_ts?: number
  },
): Promise<ListAuditLogsResponse> {
  const query = new URLSearchParams()
  if (params?.limit) query.set('limit', String(params.limit))
  if (params?.offset) query.set('offset', String(params.offset))
  if (params?.actor_id) query.set('actor_id', params.actor_id)
  if (params?.action) query.set('action', params.action)
  if (params?.resource_type) query.set('resource_type', params.resource_type)
  if (params?.from_ts) query.set('from_ts', String(params.from_ts))
  if (params?.to_ts) query.set('to_ts', String(params.to_ts))
  const qs = query.toString()
  return request<ListAuditLogsResponse>(
    baseUrl,
    `/v1/audit-logs${qs ? `?${qs}` : ''}`,
    { headers: authHeaders(token) },
  )
}
