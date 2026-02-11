import { HttpResponse, delay, http } from 'msw'
import { DEMO_INSTANCE_ID } from '../seed'

export const setupHandlers = [
  http.get('/v1/public/setup-status', async () => {
    await delay(100)
    return HttpResponse.json({
      instance_id: DEMO_INSTANCE_ID,
      state: 'ready',
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

  http.post('/v1/setup/oidc/configure', async () => {
    await delay(200)
    return HttpResponse.json({
      state: 'idp_configured',
      discovered_issuer: 'https://accounts.google.com',
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

  http.post('/v1/setup/complete', async () => {
    await delay(200)
    return HttpResponse.json({
      state: 'ready',
      instance_id: DEMO_INSTANCE_ID,
    })
  }),
]
