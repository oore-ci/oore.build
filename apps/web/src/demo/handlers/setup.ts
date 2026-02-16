import { HttpResponse, delay, http } from 'msw'
import { DEMO_INSTANCE_ID, DEMO_USER_EMAIL } from '../seed'

export const setupHandlers = [
  http.get('/v1/public/setup-status', async () => {
    await delay(100)
    return HttpResponse.json({
      instance_id: DEMO_INSTANCE_ID,
      state: 'ready',
      runtime_mode: 'local',
      remote_auth_mode: 'oidc',
      setup_mode: false,
      is_configured: true,
    })
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
      state: 'ready',
      issuer_url: 'https://accounts.google.com',
      owner_email: DEMO_USER_EMAIL,
    })
  }),

  http.post('/v1/setup/preferences', async ({ request }) => {
    await delay(200)
    const body = (await request.json()) as {
      runtime_mode?: 'local' | 'remote'
      remote_auth_mode?: 'oidc' | 'trusted_proxy'
    }
    return HttpResponse.json({
      runtime_mode: body.runtime_mode ?? 'local',
      remote_auth_mode: body.remote_auth_mode ?? 'oidc',
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/oidc/configure', async () => {
    await delay(200)
    return HttpResponse.json({
      state: 'idp_configured',
      discovered_issuer: 'https://accounts.google.com',
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/trusted-proxy/configure', async ({ request }) => {
    await delay(200)
    const body = (await request.json()) as {
      shared_secret?: string
    }
    return HttpResponse.json({
      state: 'idp_configured',
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
    return HttpResponse.json({
      state: 'owner_created',
      owner_email: 'alex@oore.build',
      oidc_subject: 'demo-oidc-subject-001',
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/owner/claim-trusted-proxy', async () => {
    await delay(200)
    return HttpResponse.json({
      state: 'owner_created',
      owner_email: DEMO_USER_EMAIL,
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/local-owner/create', async ({ request }) => {
    await delay(200)
    const body = (await request.json()) as { email?: string }
    return HttpResponse.json({
      state: 'owner_created',
      owner_email: body.email ?? 'owner@local',
      session_expires_at: 4102444800,
    })
  }),

  http.post('/v1/setup/complete', async () => {
    await delay(200)
    return HttpResponse.json({
      state: 'ready',
      instance_id: DEMO_INSTANCE_ID,
    })
  }),
]
