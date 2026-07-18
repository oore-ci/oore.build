import { HttpResponse, delay, http } from 'msw'
import { DEMO_INSTANCE_ID, DEMO_USER_EMAIL } from '../seed'
import { READ_ONLY_REASON } from '@/lib/demo-mode'
import { demoState } from '../state'

const demoRelease = {
  phase: 'idle',
  managed_service: true,
  version: '0.1.0-alpha.24',
  latest_version: '0.1.0-alpha.24',
  channel: 'alpha',
  github_repo: 'oore-ci/oore.build',
  update_available: false,
  release_name: 'Oore CI 0.1.0 Alpha 24',
  release_notes: 'Role-aware demo data and mobile release workflows.',
  release_url: 'https://github.com/oore-ci/oore.build/releases',
  changelog_url: 'https://github.com/oore-ci/oore.build/releases',
} as const

export const setupHandlers = [
  http.get('/healthz', () =>
    HttpResponse.json({
      ok: true,
      status: 'ok',
      package_version: demoRelease.version,
      version: demoRelease.version,
      channel: demoRelease.channel,
      github_repo: demoRelease.github_repo,
    }),
  ),
  http.get('/__oore_web_healthz', () =>
    HttpResponse.json({
      ok: true,
      package_version: demoRelease.version,
      version: demoRelease.version,
      channel: demoRelease.channel,
      github_repo: demoRelease.github_repo,
    }),
  ),
  http.get('/__oore_web_update', () => HttpResponse.json(demoRelease)),
  http.post('/__oore_web_update', () =>
    HttpResponse.json(
      { error: READ_ONLY_REASON, code: 'demo_read_only' },
      { status: 403 },
    ),
  ),
  http.get('/v1/system/update', () =>
    HttpResponse.json(
      demoState.scenario === 'degraded'
        ? {
            phase: 'failed',
            managed_service: true,
            error: 'The last update health check failed.',
          }
        : { phase: 'idle', managed_service: true },
    ),
  ),
  http.get('/v1/public/setup-status', async () => {
    await delay(100)
    return HttpResponse.json(demoState.setupStatus)
  }),

  // Setup flow endpoints — return plausible responses in case someone navigates there
  http.post('/v1/setup/bootstrap-token/verify', async () => {
    await delay(200)
    return HttpResponse.json({
      session_token: 'demo-setup-token',
      expires_at: 4102444800,
    })
  }),

  http.get('/v1/setup/summary', async () => {
    await delay(150)
    return HttpResponse.json({
      instance_id: DEMO_INSTANCE_ID,
      state: demoState.setupStatus.state,
      issuer_url: demoState.oidc.issuer,
      owner_email: DEMO_USER_EMAIL,
    })
  }),

  http.post('/v1/setup/preferences', async ({ request }) => {
    await delay(200)
    const body = (await request.json()) as {
      runtime_mode?: 'local' | 'remote'
      remote_auth_mode?: 'oidc' | 'trusted_proxy'
    }
    demoState.setupStatus.runtime_mode = body.runtime_mode ?? 'local'
    demoState.setupStatus.remote_auth_mode = body.remote_auth_mode ?? 'oidc'
    return HttpResponse.json({
      runtime_mode: demoState.setupStatus.runtime_mode,
      remote_auth_mode: demoState.setupStatus.remote_auth_mode,
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/oidc/configure', async () => {
    await delay(200)
    demoState.setupStatus.state = 'idp_configured'
    demoState.oidc.configured = true
    return HttpResponse.json({
      state: 'idp_configured',
      discovered_issuer: 'https://accounts.google.com',
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/trusted-proxy/configure', async ({ request }) => {
    await delay(200)
    const body = (await request.json()) as {
      setup_owner_email?: string
      shared_secret?: string
    }
    demoState.setupStatus.state = 'idp_configured'
    demoState.trustedProxy.has_shared_secret = !!body.shared_secret
    return HttpResponse.json({
      state: 'idp_configured',
      setup_owner_email: body.setup_owner_email,
      has_shared_secret: !!body.shared_secret,
      configured_at: Math.floor(Date.now() / 1000),
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/owner/start-oidc', async () => {
    await delay(200)
    return HttpResponse.json({
      authorization_url: '#demo-setup-oidc',
      state: 'demo-state',
    })
  }),

  http.post('/v1/setup/owner/verify-oidc', async () => {
    await delay(200)
    demoState.setupStatus.state = 'owner_created'
    return HttpResponse.json({
      state: 'owner_created',
      owner_email: 'alex@oore.build',
      oidc_subject: 'demo-oidc-subject-001',
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/owner/claim-trusted-proxy', async () => {
    await delay(200)
    demoState.setupStatus.state = 'owner_created'
    return HttpResponse.json({
      state: 'owner_created',
      owner_email: DEMO_USER_EMAIL,
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/local-owner/create', async ({ request }) => {
    await delay(200)
    const body = (await request.json()) as { email?: string }
    demoState.setupStatus.state = 'owner_created'
    return HttpResponse.json({
      state: 'owner_created',
      owner_email: body.email ?? 'owner@local',
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/complete', async () => {
    await delay(200)
    demoState.setupStatus.state = 'ready'
    demoState.setupStatus.setup_mode = false
    demoState.setupStatus.is_configured = true
    return HttpResponse.json({
      state: 'ready',
      instance_id: DEMO_INSTANCE_ID,
    })
  }),
]
