import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '@/lib/api'
import {
  ApiClientError,
  completeSetup,
  configureExternalAccessOidc,
  configureOidc,
  addProjectMember,
  createArtifactInstallLink,
  createScopedDownloadToken,
  createPipeline,
  discoverRepositoryWorkflows,
  getApiErrorMessage,
  getArtifactDownloadLink,
  getArtifactStorageSettings,
  getExternalAccessOidc,
  getInstancePreferences,
  getPipeline,
  getRepositoryAvatar,
  getSetupStatus,
  listAllIntegrations,
  listAllPipelines,
  listAllProjects,
  listIntegrationRepos,
  listBuildArtifacts,
  listBuilds,
  listAuditLogs,
  listProjectArtifacts,
  listProjectMemberCandidates,
  listProjectMembers,
  listProjects,
  listPipelines,
  listRunners,
  removeProjectMember,
  testOidcConnection,
  updateArtifactStorageSettings,
  updateInstancePreferences,
  updateProjectMember,
  updateRepositoryRunnerPolicy,
  updatePipeline,
  updateRunner,
  validatePipeline,
  verifyBootstrapToken,
} from '@/lib/api'

// ── Mock global fetch ──────────────────────────────────────────

const mockFetch = vi.fn()
global.fetch = mockFetch

beforeEach(() => {
  mockFetch.mockReset()
})

// ── ApiClientError ─────────────────────────────────────────────

describe('ApiClientError', () => {
  it('stores status, code, and details', () => {
    const err = new ApiClientError(422, {
      error: 'Validation failed',
      code: 'validation_error',
      details: 'issuer_url is required',
    })

    expect(err).toBeInstanceOf(Error)
    expect(err.name).toBe('ApiClientError')
    expect(err.message).toBe('Validation failed')
    expect(err.status).toBe(422)
    expect(err.code).toBe('validation_error')
    expect(err.details).toBe('issuer_url is required')
  })

  it('handles missing details', () => {
    const err = new ApiClientError(401, {
      error: 'Unauthorized',
      code: 'unauthorized',
    })

    expect(err.details).toBeUndefined()
  })
})

// ── getApiErrorMessage ─────────────────────────────────────────

describe('getApiErrorMessage', () => {
  it('returns mapped message when code is in codeMap', () => {
    const err = new ApiClientError(422, {
      error: 'Server message',
      code: 'invalid_token',
    })
    const result = getApiErrorMessage(err, {
      invalid_token: 'Your token is invalid.',
    })
    expect(result).toBe('Your token is invalid.')
  })

  it('falls back to error.message when code is not in map', () => {
    const err = new ApiClientError(500, {
      error: 'Internal failure',
      code: 'server_error',
    })
    const result = getApiErrorMessage(err, {})
    expect(result).toBe('Internal failure')
  })

  it('returns message for non-ApiClientError Error instances', () => {
    const err = new Error('Network timeout')
    const result = getApiErrorMessage(err, {})
    expect(result).toBe('Network timeout')
  })

  it('returns generic fallback for unknown error types', () => {
    const result = getApiErrorMessage('something weird', {})
    expect(result).toBe('An unexpected error occurred. Please try again.')
  })
})

// ── API functions ──────────────────────────────────────────────

function mockJsonResponse(status: number, body: unknown) {
  return Promise.resolve({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  })
}

describe('getSetupStatus', () => {
  it('calls GET /v1/public/setup-status with baseUrl', async () => {
    const payload = {
      instance_id: 'test-id',
      state: 'uninitialized',
      runtime_mode: 'local',
      setup_mode: true,
      is_configured: false,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await getSetupStatus('')

    expect(mockFetch).toHaveBeenCalledWith('/v1/public/setup-status', {
      headers: {},
    })
    expect(result).toEqual(payload)
  })

  it('prepends baseUrl to path', async () => {
    const payload = {
      instance_id: 'test-id',
      state: 'uninitialized',
      runtime_mode: 'local',
      setup_mode: true,
      is_configured: false,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    await getSetupStatus('https://ci.example.com')

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/public/setup-status',
      { headers: {} },
    )
  })
})

describe('query cancellation', () => {
  it('passes the TanStack signal through build requests', async () => {
    const controller = new AbortController()
    mockFetch.mockReturnValue(mockJsonResponse(200, { builds: [], total: 0 }))

    await listBuilds('https://ci.example.com', 'token', undefined, {
      signal: controller.signal,
    })

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/builds',
      expect.objectContaining({
        headers: { Authorization: 'Bearer token' },
        signal: controller.signal,
      }),
    )
  })

  it('queries an artifact batch with bearer auth and cancellation', async () => {
    const controller = new AbortController()
    mockFetch.mockReturnValue(mockJsonResponse(200, { artifacts: [] }))

    await listBuildArtifacts(
      'https://ci.example.com',
      'token',
      { build_ids: ['build-1', 'build-2'] },
      { signal: controller.signal },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/artifacts/query',
      {
        method: 'POST',
        headers: {
          Authorization: 'Bearer token',
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ build_ids: ['build-1', 'build-2'] }),
        signal: controller.signal,
      },
    )
  })

  it('encodes collection sorting and forwards cancellation to GET requests', async () => {
    const controller = new AbortController()
    mockFetch.mockReturnValue(mockJsonResponse(200, {}))

    await listProjects(
      'https://ci.example.com',
      'token',
      { search: 'shop', sort: 'name', direction: 'asc', limit: 20, offset: 20 },
      { signal: controller.signal },
    )
    await listBuilds(
      'https://ci.example.com',
      'token',
      {
        status: ['failed', 'timed_out'],
        sort: 'project_name',
        direction: 'desc',
      },
      { signal: controller.signal },
    )
    await listAuditLogs(
      'https://ci.example.com',
      'token',
      { sort: 'action', direction: 'asc' },
      { signal: controller.signal },
    )

    const request = {
      headers: { Authorization: 'Bearer token' },
      signal: controller.signal,
    }
    expect(mockFetch).toHaveBeenNthCalledWith(
      1,
      'https://ci.example.com/v1/projects?search=shop&sort=name&direction=asc&limit=20&offset=20',
      request,
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      'https://ci.example.com/v1/builds?status=failed%2Ctimed_out&sort=project_name&direction=desc',
      request,
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      3,
      'https://ci.example.com/v1/audit-logs?sort=action&direction=asc',
      request,
    )
  })

  it('requests bounded project artifact history with cancellation', async () => {
    const controller = new AbortController()
    mockFetch.mockReturnValue(mockJsonResponse(200, { artifacts: [] }))

    await listProjectArtifacts(
      'https://ci.example.com',
      'token',
      'project-1',
      { limit: 50 },
      { signal: controller.signal },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/projects/project-1/artifacts?limit=50',
      {
        headers: { Authorization: 'Bearer token' },
        signal: controller.signal,
      },
    )
  })

  it('loads every integration page for client-side collection controls', async () => {
    const controller = new AbortController()
    const firstPage = Array.from({ length: 200 }, (_, index) => ({
      id: `integration-${index}`,
    }))
    mockFetch
      .mockReturnValueOnce(
        mockJsonResponse(200, { integrations: firstPage, total: 202 }),
      )
      .mockReturnValueOnce(
        mockJsonResponse(200, {
          integrations: [{ id: 'integration-200' }, { id: 'integration-201' }],
          total: 202,
        }),
      )

    const result = await listAllIntegrations(
      'https://ci.example.com',
      'token',
      undefined,
      { signal: controller.signal },
    )

    expect(result.integrations).toHaveLength(202)
    expect(result.total).toBe(202)
    expect(mockFetch).toHaveBeenNthCalledWith(
      1,
      'https://ci.example.com/v1/integrations?limit=200',
      expect.objectContaining({ signal: controller.signal }),
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      'https://ci.example.com/v1/integrations?limit=200&offset=200',
      expect.objectContaining({ signal: controller.signal }),
    )
  })

  it('loads every project page for full collection selectors', async () => {
    const firstPage = Array.from({ length: 200 }, (_, index) => ({
      id: `project-${index}`,
    }))
    mockFetch
      .mockReturnValueOnce(
        mockJsonResponse(200, { projects: firstPage, total: 201 }),
      )
      .mockReturnValueOnce(
        mockJsonResponse(200, {
          projects: [{ id: 'project-200' }],
          total: 201,
        }),
      )

    const result = await listAllProjects('https://ci.example.com', 'token', {
      sort: 'name',
      direction: 'asc',
    })

    expect(result.projects).toHaveLength(201)
    expect(mockFetch).toHaveBeenNthCalledWith(
      1,
      'https://ci.example.com/v1/projects?sort=name&direction=asc&limit=200',
      expect.any(Object),
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      'https://ci.example.com/v1/projects?sort=name&direction=asc&limit=200&offset=200',
      expect.any(Object),
    )
  })

  it('loads every repository page so each source can be approved', async () => {
    const controller = new AbortController()
    const firstPage = Array.from({ length: 500 }, (_, index) => ({
      id: `repository-${index}`,
    }))
    mockFetch
      .mockReturnValueOnce(mockJsonResponse(200, { repositories: firstPage }))
      .mockReturnValueOnce(
        mockJsonResponse(200, { repositories: [{ id: 'repository-500' }] }),
      )

    const result = await listIntegrationRepos(
      'https://ci.example.com',
      'token',
      'integration-1',
      { signal: controller.signal },
    )

    expect(result.repositories).toHaveLength(501)
    expect(mockFetch).toHaveBeenNthCalledWith(
      1,
      'https://ci.example.com/v1/integrations/integration-1/repositories?limit=500&offset=0',
      expect.objectContaining({ signal: controller.signal }),
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      'https://ci.example.com/v1/integrations/integration-1/repositories?limit=500&offset=500',
      expect.objectContaining({ signal: controller.signal }),
    )
  })
})

describe('removed API contracts', () => {
  it('does not expose a QA preview client', () => {
    expect(api).not.toHaveProperty('previewQaUser')
  })
})

describe('QA project access', () => {
  it('lists project-scoped member candidates with cancellation', async () => {
    const controller = new AbortController()
    mockFetch.mockReturnValueOnce(mockJsonResponse(200, { candidates: [] }))

    await listProjectMemberCandidates(
      'https://ci.example.com',
      'maintainer-token',
      'project-1',
      { signal: controller.signal },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/projects/project-1/members/candidates',
      {
        headers: { Authorization: 'Bearer maintainer-token' },
        signal: controller.signal,
      },
    )
  })

  it('calls the project membership endpoints with bearer auth', async () => {
    mockFetch
      .mockReturnValueOnce(mockJsonResponse(200, { members: [] }))
      .mockReturnValueOnce(mockJsonResponse(200, { member: { id: 'm1' } }))
      .mockReturnValueOnce(mockJsonResponse(200, { member: { id: 'm1' } }))
      .mockReturnValueOnce(mockJsonResponse(200, { ok: true }))

    await listProjectMembers(
      'https://ci.example.com',
      'owner-token',
      'project-1',
    )
    await addProjectMember(
      'https://ci.example.com',
      'owner-token',
      'project-1',
      { user_id: 'qa-1', role: 'viewer' },
    )
    await updateProjectMember(
      'https://ci.example.com',
      'owner-token',
      'project-1',
      'qa-1',
      { role: 'viewer' },
    )
    await removeProjectMember(
      'https://ci.example.com',
      'owner-token',
      'project-1',
      'qa-1',
    )

    const auth = { Authorization: 'Bearer owner-token' }
    expect(mockFetch).toHaveBeenNthCalledWith(
      1,
      'https://ci.example.com/v1/projects/project-1/members',
      { headers: auth },
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      'https://ci.example.com/v1/projects/project-1/members',
      expect.objectContaining({
        method: 'POST',
        headers: { ...auth, 'Content-Type': 'application/json' },
      }),
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      3,
      'https://ci.example.com/v1/projects/project-1/members/qa-1',
      expect.objectContaining({
        method: 'PATCH',
        headers: { ...auth, 'Content-Type': 'application/json' },
      }),
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      4,
      'https://ci.example.com/v1/projects/project-1/members/qa-1',
      expect.objectContaining({
        method: 'DELETE',
        headers: { ...auth, 'Content-Type': 'application/json' },
      }),
    )
  })
})

describe('repository avatars', () => {
  it('fetches the image through Oore with the session token', async () => {
    const controller = new AbortController()
    const avatar = new Blob(['avatar'], { type: 'image/png' })
    mockFetch.mockResolvedValue({
      ok: true,
      status: 200,
      blob: () => Promise.resolve(avatar),
    })

    const result = await getRepositoryAvatar(
      'https://oore.example.com',
      'session-token',
      'repo-1',
      { signal: controller.signal },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://oore.example.com/v1/integration-repositories/repo-1/avatar',
      {
        headers: { Authorization: 'Bearer session-token' },
        signal: controller.signal,
      },
    )
    expect(result).toBe(avatar)
  })
})

describe('repository runner policy', () => {
  it('updates one repository through the narrow policy endpoint', async () => {
    const payload = {
      repository: {
        id: 'repo-1',
        allow_direct_macos_runner: true,
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await updateRepositoryRunnerPolicy(
      'https://oore.example.com',
      'session-token',
      'repo-1',
      { allow_direct_macos_runner: true },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://oore.example.com/v1/integration-repositories/repo-1/runner-policy',
      {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({ allow_direct_macos_runner: true }),
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('artifact download links', () => {
  it('replaces the daemon loopback fallback with the reachable instance origin', async () => {
    mockFetch
      .mockReturnValueOnce(
        mockJsonResponse(200, {
          download_url: 'http://127.0.0.1:8787/v1/artifacts/download/direct',
          expires_at: 1,
        }),
      )
      .mockReturnValueOnce(
        mockJsonResponse(200, {
          id: 'share-1',
          download_url: 'http://127.0.0.1:8787/install/artifact/scoped',
          token: 'scoped',
          prefix: 'scoped',
          expires_at: 1,
          single_use: false,
        }),
      )

    const direct = await getArtifactDownloadLink(
      'https://oore.example.com',
      'token',
      'artifact-1',
    )
    const scoped = await createScopedDownloadToken(
      'https://oore.example.com',
      'token',
      'artifact-1',
      {},
    )

    expect(direct.download_url).toBe(
      'https://oore.example.com/v1/artifacts/download/direct',
    )
    expect(scoped.download_url).toBe(
      'https://oore.example.com/install/artifact/scoped',
    )
  })

  it('keeps custom-protocol install URLs while normalizing HTTPS artifact URLs', async () => {
    mockFetch.mockReturnValue(
      mockJsonResponse(200, {
        platform: 'ios',
        install_url:
          'itms-services://?action=download-manifest&url=https%3A%2F%2Fci.example.com%2Fmanifest.plist',
        download_url: 'http://127.0.0.1:8787/install/artifact/install',
        manifest_url:
          'http://127.0.0.1:8787/install/ios/install/manifest.plist',
        expires_at: 1,
      }),
    )

    const result = await createArtifactInstallLink(
      'https://oore.example.com',
      'token',
      'artifact-1',
    )

    expect(result.install_url).toMatch(/^itms-services:/)
    expect(result.download_url).toBe(
      'https://oore.example.com/install/artifact/install',
    )
    expect(result.manifest_url).toBe(
      'https://oore.example.com/install/ios/install/manifest.plist',
    )
  })
})

describe('verifyBootstrapToken', () => {
  it('calls POST /v1/setup/bootstrap-token/verify with baseUrl and token', async () => {
    const payload = { session_token: 'sess-abc', expires_at: 9999999 }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await verifyBootstrapToken('', 'my-token')

    expect(mockFetch).toHaveBeenCalledWith('/v1/setup/bootstrap-token/verify', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token: 'my-token' }),
    })
    expect(result).toEqual(payload)
  })
})

describe('configureOidc', () => {
  it('calls POST /v1/setup/oidc/configure with baseUrl and auth header', async () => {
    const payload = {
      state: 'idp_configured',
      discovered_issuer: 'https://issuer.example.com',
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await configureOidc('', 'sess-token', {
      issuer_url: 'https://issuer.example.com',
      client_id: 'cid',
    })

    expect(mockFetch).toHaveBeenCalledWith('/v1/setup/oidc/configure', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: 'Bearer sess-token',
      },
      body: JSON.stringify({
        issuer_url: 'https://issuer.example.com',
        client_id: 'cid',
      }),
    })
    expect(result).toEqual(payload)
  })
})

describe('completeSetup', () => {
  it('calls POST /v1/setup/complete with baseUrl and auth header', async () => {
    const payload = { state: 'ready', instance_id: 'inst-1' }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await completeSetup('', 'sess-token')

    expect(mockFetch).toHaveBeenCalledWith('/v1/setup/complete', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: 'Bearer sess-token',
      },
    })
    expect(result).toEqual(payload)
  })

  it('throws ApiClientError on non-ok response', async () => {
    mockFetch.mockReturnValue(
      mockJsonResponse(401, {
        error: 'Invalid session',
        code: 'unauthorized',
      }),
    )

    await expect(completeSetup('', 'bad-token')).rejects.toThrow(ApiClientError)
  })
})

describe('listRunners', () => {
  it('calls GET /v1/runners with auth header', async () => {
    const payload = {
      runners: [
        {
          id: 'runner-1',
          name: 'mac-mini',
          status: 'online',
          capabilities: { os: 'macos' },
          created_at: 100,
          updated_at: 200,
        },
      ],
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await listRunners('https://ci.example.com', 'session-token')

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/runners',
      {
        headers: {
          Authorization: 'Bearer session-token',
        },
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('updateRunner', () => {
  it('calls PATCH /v1/runners/{runner_id} with JSON body', async () => {
    const payload = {
      runner: {
        id: 'runner-1',
        name: 'renamed-runner',
        status: 'offline',
        capabilities: {},
        created_at: 10,
        updated_at: 20,
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await updateRunner(
      'https://ci.example.com',
      'session-token',
      'runner-1',
      { name: 'renamed-runner' },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/runners/runner-1',
      {
        method: 'PATCH',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({ name: 'renamed-runner' }),
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('artifact storage settings api', () => {
  it('calls GET /v1/settings/artifact-storage with auth header', async () => {
    const payload = {
      settings: {
        provider: 'local',
        local_base_dir: '/tmp/oore-artifacts',
        has_access_key_id: false,
        has_secret_access_key: false,
        source: 'database',
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await getArtifactStorageSettings(
      'https://ci.example.com',
      'session-token',
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/artifact-storage',
      {
        headers: {
          Authorization: 'Bearer session-token',
        },
      },
    )
    expect(result).toEqual(payload)
  })

  it('calls PUT /v1/settings/artifact-storage with payload', async () => {
    const payload = {
      settings: {
        provider: 'r2',
        s3_bucket: 'build-artifacts',
        s3_region: 'auto',
        s3_endpoint: 'https://example.r2.cloudflarestorage.com',
        has_access_key_id: true,
        has_secret_access_key: true,
        source: 'database',
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await updateArtifactStorageSettings(
      'https://ci.example.com',
      'session-token',
      {
        provider: 'r2',
        s3_bucket: 'build-artifacts',
        s3_region: 'auto',
        s3_endpoint: 'https://example.r2.cloudflarestorage.com',
        access_key_id: 'AKIA...',
        secret_access_key: 'secret',
      },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/artifact-storage',
      {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          provider: 'r2',
          s3_bucket: 'build-artifacts',
          s3_region: 'auto',
          s3_endpoint: 'https://example.r2.cloudflarestorage.com',
          access_key_id: 'AKIA...',
          secret_access_key: 'secret',
        }),
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('instance preferences api', () => {
  it('calls GET /v1/settings/preferences with auth header', async () => {
    const payload = {
      preferences: {
        key_storage_mode: 'file',
        runtime_mode: 'local',
        direct_macos_runner_enabled: false,
        restart_required: false,
        updated_at: 123,
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await getInstancePreferences(
      'https://ci.example.com',
      'session-token',
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/preferences',
      {
        headers: {
          Authorization: 'Bearer session-token',
        },
      },
    )
    expect(result).toEqual(payload)
  })

  it('calls PUT /v1/settings/preferences with payload', async () => {
    const payload = {
      preferences: {
        key_storage_mode: 'file',
        runtime_mode: 'remote',
        direct_macos_runner_enabled: true,
        restart_required: false,
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await updateInstancePreferences(
      'https://ci.example.com',
      'session-token',
      {
        key_storage_mode: 'file',
        runtime_mode: 'remote',
        direct_macos_runner_enabled: true,
      },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/preferences',
      {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          key_storage_mode: 'file',
          runtime_mode: 'remote',
          direct_macos_runner_enabled: true,
        }),
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('external access oidc api', () => {
  it('calls GET /v1/settings/external-access/oidc with auth header', async () => {
    const payload = {
      issuer_url: 'https://accounts.google.com',
      client_id: 'my-client-id',
      has_client_secret: true,
      authorization_endpoint: 'https://accounts.google.com/o/oauth2/v2/auth',
      token_endpoint: 'https://oauth2.googleapis.com/token',
      userinfo_endpoint: 'https://openidconnect.googleapis.com/v1/userinfo',
      jwks_uri: 'https://www.googleapis.com/oauth2/v3/certs',
      configured_at: 1700000000,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await getExternalAccessOidc(
      'https://ci.example.com',
      'session-token',
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/external-access/oidc',
      {
        headers: {
          Authorization: 'Bearer session-token',
        },
      },
    )
    expect(result).toEqual(payload)
  })

  it('calls PUT /v1/settings/external-access/oidc with payload', async () => {
    const payload = {
      discovered_issuer: 'https://accounts.google.com',
      has_client_secret: true,
      configured_at: 1700000000,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await configureExternalAccessOidc(
      'https://ci.example.com',
      'session-token',
      {
        issuer_url: 'https://accounts.google.com',
        client_id: 'my-client-id',
        client_secret: 'my-secret',
      },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/external-access/oidc',
      {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          issuer_url: 'https://accounts.google.com',
          client_id: 'my-client-id',
          client_secret: 'my-secret',
        }),
      },
    )
    expect(result).toEqual(payload)
  })

  it('calls PUT without client_secret when omitted (preserve existing)', async () => {
    const payload = {
      discovered_issuer: 'https://accounts.google.com',
      has_client_secret: true,
      configured_at: 1700000000,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    await configureExternalAccessOidc(
      'https://ci.example.com',
      'session-token',
      {
        issuer_url: 'https://accounts.google.com',
        client_id: 'my-client-id',
      },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/external-access/oidc',
      {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          issuer_url: 'https://accounts.google.com',
          client_id: 'my-client-id',
        }),
      },
    )
  })

  it('calls POST /v1/settings/external-access/oidc/test-connection', async () => {
    const payload = {
      success: true,
      discovered_issuer: 'https://accounts.google.com',
      authorization_endpoint: 'https://accounts.google.com/o/oauth2/v2/auth',
      token_endpoint: 'https://oauth2.googleapis.com/token',
      userinfo_endpoint: 'https://openidconnect.googleapis.com/v1/userinfo',
      jwks_uri: 'https://www.googleapis.com/oauth2/v3/certs',
      scopes_supported: ['openid', 'email', 'profile'],
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await testOidcConnection(
      'https://ci.example.com',
      'session-token',
      { issuer_url: 'https://accounts.google.com' },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/external-access/oidc/test-connection',
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          issuer_url: 'https://accounts.google.com',
        }),
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('pipeline api', () => {
  it('discovers repository workflows at an encoded ref and path', async () => {
    mockFetch.mockReturnValue(
      mockJsonResponse(200, {
        project_id: 'proj-1',
        provider: 'gitlab',
        reference: 'feature/mobile',
        workflows: [],
        truncated: false,
      }),
    )

    await discoverRepositoryWorkflows(
      'https://ci.example.com',
      'session-token',
      'proj-1',
      { reference: 'feature/mobile', path: '.oore/android release.yaml' },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/projects/proj-1/repository-workflows?ref=feature%2Fmobile&path=.oore%2Fandroid+release.yaml',
      {
        headers: {
          Authorization: 'Bearer session-token',
        },
      },
    )
  })

  it('calls POST /v1/projects/{project_id}/pipelines with execution config fields', async () => {
    const payload = {
      pipeline: {
        id: 'pipe-1',
        project_id: 'proj-1',
        name: 'Mobile',
        config_path: '.oore.yaml',
        config_path_explicit: false,
        execution_config: {
          platforms: ['android'],
          commands: { pre_build: [], build: [], post_build: [] },
          platform_build_args: { android: [], ios: [], macos: [] },
          platform_commands: {},
          env: [],
          artifact_patterns: ['*.apk'],
        },
        trigger_config: { events: [], branches: [] },
        concurrency: { cancel_previous: false },
        enabled: true,
        created_at: 1,
        updated_at: 1,
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    await createPipeline('https://ci.example.com', 'session-token', 'proj-1', {
      name: 'Mobile',
      config_path: '.oore.yaml',
      config_path_explicit: false,
      execution_config: {
        platforms: ['android'],
        commands: { pre_build: [], build: [], post_build: [] },
        platform_build_args: { android: [], ios: [], macos: [] },
        platform_commands: {},
        env: [],
        artifact_patterns: ['*.apk'],
      },
      trigger_config: { events: [], branches: [] },
      concurrency: { cancel_previous: false },
    })

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/projects/proj-1/pipelines',
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          name: 'Mobile',
          config_path: '.oore.yaml',
          config_path_explicit: false,
          execution_config: {
            platforms: ['android'],
            commands: { pre_build: [], build: [], post_build: [] },
            platform_build_args: { android: [], ios: [], macos: [] },
            platform_commands: {},
            env: [],
            artifact_patterns: ['*.apk'],
          },
          trigger_config: { events: [], branches: [] },
          concurrency: { cancel_previous: false },
        }),
      },
    )
  })

  it('calls PATCH /v1/pipelines/{pipeline_id} with explicit mode', async () => {
    mockFetch.mockReturnValue(
      mockJsonResponse(200, { pipeline: { id: 'pipe-1' } }),
    )

    await updatePipeline('https://ci.example.com', 'session-token', 'pipe-1', {
      config_path: 'ci/mobile.yaml',
      config_path_explicit: true,
    })

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/pipelines/pipe-1',
      {
        method: 'PATCH',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          config_path: 'ci/mobile.yaml',
          config_path_explicit: true,
        }),
      },
    )
  })

  it('calls validate pipeline endpoint with execution config payload', async () => {
    mockFetch.mockReturnValue(
      mockJsonResponse(200, { valid: true, errors: [] }),
    )

    await validatePipeline('https://ci.example.com', 'session-token', {
      config_path_explicit: false,
      execution_config: {
        platforms: ['android', 'ios'],
        commands: { pre_build: [], build: ['echo custom'], post_build: [] },
        platform_build_args: {
          android: ['--build-number=$PROJECT_BUILD_NUMBER'],
          ios: [],
          macos: [],
        },
        platform_commands: {},
        env: [{ key: 'APP_FLAVOR', value: 'dev' }],
        artifact_patterns: ['*.apk', '*.ipa'],
      },
    })

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/pipelines/validate',
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({
          config_path_explicit: false,
          execution_config: {
            platforms: ['android', 'ios'],
            commands: { pre_build: [], build: ['echo custom'], post_build: [] },
            platform_build_args: {
              android: ['--build-number=$PROJECT_BUILD_NUMBER'],
              ios: [],
              macos: [],
            },
            platform_commands: {},
            env: [{ key: 'APP_FLAVOR', value: 'dev' }],
            artifact_patterns: ['*.apk', '*.ipa'],
          },
        }),
      },
    )
  })

  it('lists and fetches pipeline endpoints', async () => {
    mockFetch.mockReturnValueOnce(
      mockJsonResponse(200, { pipelines: [], total: 0 }),
    )
    await listPipelines('https://ci.example.com', 'session-token', 'proj-1', {
      search: 'release',
      sort: 'name',
      direction: 'asc',
      limit: 10,
      offset: 0,
    })
    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/projects/proj-1/pipelines?search=release&sort=name&direction=asc&limit=10',
      {
        headers: {
          Authorization: 'Bearer session-token',
        },
      },
    )

    mockFetch.mockReturnValueOnce(
      mockJsonResponse(200, { pipeline: { id: 'pipe-1' }, build_count: 0 }),
    )
    await getPipeline('https://ci.example.com', 'session-token', 'pipe-1')
    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/pipelines/pipe-1',
      {
        headers: {
          Authorization: 'Bearer session-token',
        },
      },
    )
  })

  it('loads every pipeline page for full selectors', async () => {
    const firstPage = Array.from({ length: 200 }, (_, index) => ({
      id: `pipeline-${index}`,
    }))
    mockFetch
      .mockReturnValueOnce(
        mockJsonResponse(200, { pipelines: firstPage, total: 201 }),
      )
      .mockReturnValueOnce(
        mockJsonResponse(200, {
          pipelines: [{ id: 'pipeline-200' }],
          total: 201,
        }),
      )

    const result = await listAllPipelines(
      'https://ci.example.com',
      'session-token',
      'proj-1',
      { sort: 'name', direction: 'asc' },
    )

    expect(result.pipelines).toHaveLength(201)
    expect(mockFetch).toHaveBeenNthCalledWith(
      1,
      'https://ci.example.com/v1/projects/proj-1/pipelines?sort=name&direction=asc&limit=200',
      expect.any(Object),
    )
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      'https://ci.example.com/v1/projects/proj-1/pipelines?sort=name&direction=asc&limit=200&offset=200',
      expect.any(Object),
    )
  })
})
