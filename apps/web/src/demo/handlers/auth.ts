import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoSession } from '../personas'

function safeDemoRedirectUri(requestUrl: string): string {
  const request = new URL(requestUrl)
  const fallback = new URL('/auth/callback', request.origin)
  const requestedValue = safeRedirectParam(
    request.searchParams.get('redirect_uri'),
  )
  if (!requestedValue) return fallback.toString()

  try {
    const requested = new URL(requestedValue, request.origin)
    return requested.origin === request.origin &&
      requested.pathname === '/auth/callback'
      ? requested.toString()
      : fallback.toString()
  } catch {
    return fallback.toString()
  }
}

function safeRedirectParam(value: string | null): string | null {
  return value?.trim() || null
}

export const authHandlers = [
  http.get('/v1/users/me', async ({ request }) => {
    await delay(100)
    const persona = getDemoPersonaFromRequest(request)
    return HttpResponse.json({
      user: {
        id: persona.userId,
        email: persona.email,
        display_name: persona.displayName,
        role: persona.role,
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
    const redirectUri = safeDemoRedirectUri(request.url)
    return HttpResponse.json({
      authorization_url: `${redirectUri}?code=demo-code&state=demo-state`,
      state: 'demo-state',
    })
  }),

  http.post('/v1/auth/oidc/callback', async ({ request }) => {
    await delay(300)
    return HttpResponse.json(getDemoSession(getDemoPersonaFromRequest(request)))
  }),

  http.post('/v1/auth/local/login', async ({ request }) => {
    await delay(150)
    return HttpResponse.json(getDemoSession(getDemoPersonaFromRequest(request)))
  }),

  http.post('/v1/auth/trusted-proxy/login', async ({ request }) => {
    await delay(150)
    return HttpResponse.json(getDemoSession(getDemoPersonaFromRequest(request)))
  }),
]
