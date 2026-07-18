import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import {
  DEMO_PERSONAS,
  getDemoPersonaFromRequest,
  getDemoSession,
} from '../personas'
import { READ_ONLY_REASON, isDemoMutationAllowed } from '@/lib/demo-mode'

const UNAUTHENTICATED_PATHS = new Set([
  '/v1/auth/oidc/start',
  '/v1/auth/oidc/callback',
  '/v1/auth/local/login',
  '/v1/auth/trusted-proxy/login',
])

export const authGuardHandlers = [
  http.all(/\/v1\/.*/, ({ request }) => {
    const path = new URL(request.url).pathname
    if (
      path.startsWith('/v1/public/') ||
      path.startsWith('/v1/setup/') ||
      UNAUTHENTICATED_PATHS.has(path)
    ) {
      return
    }
    const authorization = request.headers.get('Authorization')
    const token = authorization?.replace(/^Bearer\s+/i, '')
    if (DEMO_PERSONAS.some((persona) => persona.token === token)) return
    return HttpResponse.json(
      { error: 'Authentication required.', code: 'unauthorized' },
      { status: 401 },
    )
  }),
]

export const demoReadOnlyGuardHandlers = [
  http.all(/\/(?:v1|__oore_).*/, ({ request }) => {
    const url = new URL(request.url)
    if (
      isDemoMutationAllowed(
        request.method.toUpperCase(),
        url.pathname,
        url.hostname,
      )
    ) {
      return
    }
    return HttpResponse.json(
      { error: READ_ONLY_REASON, code: 'demo_read_only' },
      { status: 403 },
    )
  }),
]

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

  http.post('/v1/auth/oidc/callback', async () => {
    await delay(300)
    return HttpResponse.json(getDemoSession(DEMO_PERSONAS[0]))
  }),

  http.post('/v1/auth/local/login', async ({ request }) => {
    await delay(150)
    const body = (await request.json().catch(() => ({}))) as { email?: string }
    const persona = body.email
      ? DEMO_PERSONAS.find(
          (candidate) => candidate.email === body.email?.trim().toLowerCase(),
        )
      : DEMO_PERSONAS[0]
    return persona
      ? HttpResponse.json(getDemoSession(persona))
      : HttpResponse.json(
          { error: 'Invalid demo account.', code: 'unauthorized' },
          { status: 401 },
        )
  }),

  http.post('/v1/auth/trusted-proxy/login', async () => {
    await delay(150)
    return HttpResponse.json(getDemoSession(DEMO_PERSONAS[0]))
  }),
]
