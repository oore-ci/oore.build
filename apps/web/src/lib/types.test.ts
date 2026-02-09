import { describe, expect, it } from 'vitest'
import type {
  ApiError,
  ArtifactStorageProvider,
  ArtifactStorageSettings,
  ArtifactStorageSettingsResponse,
  BuildPlatform,
  BootstrapTokenVerifyResponse,
  CreatePipelineRequest,
  Pipeline,
  PipelineExecutionConfig,
  InstancePreferences,
  InstancePreferencesResponse,
  ListRunnersResponse,
  OidcConfigureRequest,
  OidcConfigureResponse,
  Runner,
  SetupCompleteResponse,
  SetupOidcStartResponse,
  SetupOidcVerifyResponse,
  SetupState,
  SetupStatus,
  UpdateRunnerRequest,
  UpdateRunnerResponse,
} from '@/lib/types'

describe('types', () => {
  it('SetupState accepts valid states', () => {
    const states: Array<SetupState> = [
      'uninitialized',
      'bootstrap_pending',
      'idp_configured',
      'owner_created',
      'ready',
    ]
    expect(states).toHaveLength(5)
  })

  it('SetupStatus can be constructed', () => {
    const status: SetupStatus = {
      instance_id: 'test',
      state: 'uninitialized',
      setup_mode: true,
      is_configured: false,
    }
    expect(status.instance_id).toBe('test')
  })

  it('BootstrapTokenVerifyResponse can be constructed', () => {
    const resp: BootstrapTokenVerifyResponse = {
      session_token: 'tok',
      expires_at: 123,
    }
    expect(resp.session_token).toBe('tok')
  })

  it('OidcConfigureRequest can be constructed', () => {
    const req: OidcConfigureRequest = {
      issuer_url: 'https://example.com',
      client_id: 'cid',
    }
    expect(req.issuer_url).toBe('https://example.com')
  })

  it('OidcConfigureResponse can be constructed', () => {
    const resp: OidcConfigureResponse = {
      state: 'idp_configured',
      discovered_issuer: 'https://example.com',
    }
    expect(resp.state).toBe('idp_configured')
  })

  it('SetupOidcStartResponse can be constructed', () => {
    const resp: SetupOidcStartResponse = {
      authorization_url: 'https://auth.example.com',
      state: 'random-state',
    }
    expect(resp.authorization_url).toBe('https://auth.example.com')
  })

  it('SetupOidcVerifyResponse can be constructed', () => {
    const resp: SetupOidcVerifyResponse = {
      state: 'owner_created',
      owner_email: 'user@example.com',
      oidc_subject: 'sub-123',
    }
    expect(resp.state).toBe('owner_created')
  })

  it('SetupCompleteResponse can be constructed', () => {
    const resp: SetupCompleteResponse = {
      state: 'ready',
      instance_id: 'inst-1',
    }
    expect(resp.state).toBe('ready')
  })

  it('ApiError can be constructed', () => {
    const err: ApiError = {
      error: 'Something went wrong',
      code: 'internal_error',
      details: 'stack trace here',
    }
    expect(err.code).toBe('internal_error')
  })

  it('Runner and ListRunnersResponse can be constructed', () => {
    const runner: Runner = {
      id: 'runner-1',
      name: 'local-runner',
      status: 'online',
      capabilities: { os: 'macos', arch: 'arm64' },
      created_at: 10,
      updated_at: 20,
    }
    const response: ListRunnersResponse = { runners: [runner] }
    expect(response.runners[0].name).toBe('local-runner')
  })

  it('UpdateRunnerRequest and UpdateRunnerResponse can be constructed', () => {
    const req: UpdateRunnerRequest = { name: 'renamed-runner' }
    const resp: UpdateRunnerResponse = {
      runner: {
        id: 'runner-2',
        name: 'renamed-runner',
        status: 'offline',
        capabilities: {},
        created_at: 1,
        updated_at: 2,
      },
    }
    expect(req.name).toBe('renamed-runner')
    expect(resp.runner.name).toBe('renamed-runner')
  })

  it('Pipeline execution config types can be constructed', () => {
    const platforms: BuildPlatform[] = ['android', 'ios']
    const execution: PipelineExecutionConfig = {
      platforms,
      flutter_version: '3.24.0',
      commands: {
        pre_build: ['flutter pub get'],
        build: ['flutter build apk --release'],
        post_build: ['echo done'],
      },
      platform_build_args: {
        android: ['--build-number=$PROJECT_BUILD_NUMBER'],
        ios: [],
        macos: [],
      },
      platform_commands: {
        android: 'flutter build appbundle --release',
      },
      env: [{ key: 'PROJECT_BUILD_NUMBER', value: '42' }],
      artifact_patterns: ['*.apk', '*.ipa'],
    }

    const pipeline: Pipeline = {
      id: 'pipe-1',
      project_id: 'proj-1',
      name: 'Mobile',
      config_path: '.oore.yaml',
      config_path_explicit: false,
      execution_config: execution,
      trigger_config: { events: [], branches: [] },
      concurrency: { cancel_previous: false },
      enabled: true,
      created_at: 1,
      updated_at: 2,
    }

    const req: CreatePipelineRequest = {
      name: 'Mobile',
      config_path_explicit: false,
      execution_config: execution,
      trigger_config: { events: [], branches: [] },
      concurrency: { cancel_previous: false },
    }

    expect(pipeline.execution_config.platforms).toEqual(['android', 'ios'])
    expect(pipeline.execution_config.flutter_version).toBe('3.24.0')
    expect(req.execution_config?.artifact_patterns).toContain('*.apk')
  })

  it('Artifact storage settings types can be constructed', () => {
    const provider: ArtifactStorageProvider = 'r2'
    const settings: ArtifactStorageSettings = {
      provider,
      s3_bucket: 'oore-artifacts',
      s3_region: 'auto',
      s3_endpoint: 'https://example.r2.cloudflarestorage.com',
      has_access_key_id: true,
      has_secret_access_key: true,
      source: 'database',
      updated_at: 123,
    }
    const response: ArtifactStorageSettingsResponse = { settings }
    expect(response.settings.provider).toBe('r2')
    expect(response.settings.has_secret_access_key).toBe(true)
  })

  it('Instance preferences types can be constructed', () => {
    const prefs: InstancePreferences = {
      key_storage_mode: 'keychain',
      restart_required: true,
      updated_at: 123,
    }
    const response: InstancePreferencesResponse = { preferences: prefs }
    expect(response.preferences.key_storage_mode).toBe('keychain')
    expect(response.preferences.restart_required).toBe(true)
  })
})
