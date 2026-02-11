import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  ApiClientError,
  completeSetup,
  configureOidc,
  createPipeline,
  getApiErrorMessage,
  getArtifactStorageSettings,
  getInstancePreferences,
  getPipeline,
  getSetupStatus,
  listPipelines,
  listRunners,
  updateArtifactStorageSettings,
  updateInstancePreferences,
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
      setup_mode: true,
      is_configured: false,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await getSetupStatus('')

    expect(mockFetch).toHaveBeenCalledWith('/v1/public/setup-status', {
      headers: { 'Content-Type': 'application/json' },
    })
    expect(result).toEqual(payload)
  })

  it('prepends baseUrl to path', async () => {
    const payload = {
      instance_id: 'test-id',
      state: 'uninitialized',
      setup_mode: true,
      is_configured: false,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    await getSetupStatus('https://ci.example.com')

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/public/setup-status',
      { headers: { 'Content-Type': 'application/json' } },
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
          'Content-Type': 'application/json',
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
          'Content-Type': 'application/json',
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
        key_storage_mode: 'keychain',
        restart_required: true,
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
          'Content-Type': 'application/json',
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
        restart_required: true,
      },
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await updateInstancePreferences(
      'https://ci.example.com',
      'session-token',
      { key_storage_mode: 'file' },
    )

    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/settings/preferences',
      {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
        body: JSON.stringify({ key_storage_mode: 'file' }),
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('pipeline api', () => {
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
      limit: 10,
      offset: 0,
    })
    expect(mockFetch).toHaveBeenCalledWith(
      'https://ci.example.com/v1/projects/proj-1/pipelines?limit=10',
      {
        headers: {
          'Content-Type': 'application/json',
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
          'Content-Type': 'application/json',
          Authorization: 'Bearer session-token',
        },
      },
    )
  })
})
