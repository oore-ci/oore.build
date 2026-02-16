import { HttpResponse, delay, http } from 'msw'
import {
  DEMO_AUTH_EXPIRES_AT,
  DEMO_AUTH_TOKEN,
  DEMO_OIDC_SUBJECT,
  DEMO_USER_EMAIL,
  DEMO_USER_ID,
  DEMO_USER_ROLE,
  ago,
} from '../seed'

export const authHandlers = [
  http.get('/v1/users/me', async () => {
    await delay(100)
    return HttpResponse.json({
      user: {
        id: DEMO_USER_ID,
        email: DEMO_USER_EMAIL,
        display_name: 'Alex Chen',
        role: DEMO_USER_ROLE,
        status: 'active',
        created_at: ago(86400 * 90),
        updated_at: ago(3600),
      },
    })
  }),

  http.post('/v1/auth/logout', async () => {
    await delay(100)
    return HttpResponse.json({ ok: true })
  }),

  // OIDC login flow — returns a callback URL pointing back to our own origin
  http.get('/v1/auth/oidc/start', async ({ request }) => {
    await delay(200)
    const url = new URL(request.url)
    const redirectUri = url.searchParams.get('redirect_uri') ?? '/auth/callback'
    return HttpResponse.json({
      authorization_url: `${redirectUri}?code=demo-code&state=demo-state`,
      state: 'demo-state',
    })
  }),

  http.post('/v1/auth/oidc/callback', async () => {
    await delay(300)
    return HttpResponse.json({
      session_token: DEMO_AUTH_TOKEN,
      expires_at: DEMO_AUTH_EXPIRES_AT,
      user: {
        email: DEMO_USER_EMAIL,
        oidc_subject: DEMO_OIDC_SUBJECT,
        user_id: DEMO_USER_ID,
        role: DEMO_USER_ROLE,
      },
    })
  }),

  http.post('/v1/auth/local/login', async () => {
    await delay(150)
    return HttpResponse.json({
      session_token: DEMO_AUTH_TOKEN,
      expires_at: DEMO_AUTH_EXPIRES_AT,
      user: {
        email: DEMO_USER_EMAIL,
        oidc_subject: DEMO_OIDC_SUBJECT,
        user_id: DEMO_USER_ID,
        role: DEMO_USER_ROLE,
      },
    })
  }),

  http.post('/v1/auth/trusted-proxy/login', async () => {
    await delay(150)
    return HttpResponse.json({
      session_token: DEMO_AUTH_TOKEN,
      expires_at: DEMO_AUTH_EXPIRES_AT,
      user: {
        email: DEMO_USER_EMAIL,
        oidc_subject: `warpgate::${DEMO_USER_EMAIL}`,
        user_id: DEMO_USER_ID,
        role: DEMO_USER_ROLE,
      },
    })
  }),
]
