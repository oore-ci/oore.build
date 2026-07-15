import { HttpResponse, delay, http } from 'msw'
import type { ApiTokenSummary } from '@/lib/types'
import { getDemoPersonaFromRequest } from '../personas'
import { requireDemoInstancePermission } from '../authorization'
import { USER_IDS, ago } from '../seed'

const demoApiTokens: Array<ApiTokenSummary> = [
  {
    id: 'token-demo-001',
    name: 'Production deploys',
    prefix: 'oore_prod',
    role: 'developer',
    created_by: USER_IDS.owner,
    created_by_email: 'demo+owner@oore.build',
    created_at: ago(86400 * 48),
    expires_at: null,
    last_used_at: ago(60 * 8),
    is_expired: false,
    is_revoked: false,
  },
  {
    id: 'token-demo-002',
    name: 'Release automation',
    prefix: 'oore_rel',
    role: 'admin',
    created_by: USER_IDS.admin,
    created_by_email: 'demo+admin@oore.build',
    created_at: ago(86400 * 21),
    expires_at: ago(-86400 * 30),
    last_used_at: ago(3600 * 3),
    is_expired: false,
    is_revoked: false,
  },
  {
    id: 'token-demo-003',
    name: 'Local CLI',
    prefix: 'oore_cli',
    role: 'developer',
    created_by: USER_IDS.developer,
    created_by_email: 'demo+developer@oore.build',
    created_at: ago(86400 * 7),
    expires_at: null,
    last_used_at: ago(3600 * 18),
    is_expired: false,
    is_revoked: false,
  },
  {
    id: 'token-demo-004',
    name: 'Old build agent',
    prefix: 'oore_old',
    role: 'developer',
    created_by: USER_IDS.developer,
    created_by_email: 'demo+developer@oore.build',
    created_at: ago(86400 * 120),
    expires_at: ago(86400 * 30),
    last_used_at: ago(86400 * 31),
    is_expired: true,
    is_revoked: false,
  },
  {
    id: 'token-demo-005',
    name: 'Retired integration',
    prefix: 'oore_ret',
    role: 'qa_viewer',
    created_by: USER_IDS.owner,
    created_by_email: 'demo+owner@oore.build',
    created_at: ago(86400 * 180),
    expires_at: null,
    last_used_at: ago(86400 * 90),
    is_expired: false,
    is_revoked: true,
  },
]

export const apiTokenHandlers = [
  http.get('/v1/api-tokens', async ({ request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    if (persona.role === 'qa_viewer') {
      return HttpResponse.json(
        {
          error: 'You do not have permission to access this resource.',
          code: 'forbidden',
        },
        { status: 403 },
      )
    }

    const tokens =
      persona.role === 'owner' || persona.role === 'admin'
        ? demoApiTokens
        : demoApiTokens.filter((token) => token.created_by === persona.userId)
    return HttpResponse.json({ tokens })
  }),

  http.post('/v1/api-tokens', async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'api_tokens:write')
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const body = (await request.json()) as {
      name: string
      role: string
      expires_at?: number
    }
    if (
      persona.role === 'developer' &&
      body.role !== 'developer' &&
      body.role !== 'qa_viewer'
    ) {
      return HttpResponse.json(
        {
          error: 'You cannot create a token with a higher role.',
          code: 'forbidden',
        },
        { status: 403 },
      )
    }
    const id = `token-demo-new-${Date.now()}`
    const createdAt = ago(0)
    demoApiTokens.unshift({
      id,
      name: body.name,
      prefix: `oore_${id.slice(-6)}`,
      role: body.role,
      created_by: persona.userId,
      created_by_email: persona.email,
      created_at: createdAt,
      expires_at: body.expires_at ?? null,
      last_used_at: null,
      is_expired: false,
      is_revoked: false,
    })
    return HttpResponse.json({
      id,
      name: body.name,
      prefix: `oore_${id.slice(-6)}`,
      role: body.role,
      created_at: createdAt,
      expires_at: body.expires_at ?? null,
      token: `oore_demo_${crypto.randomUUID().replaceAll('-', '')}`,
    })
  }),

  http.delete('/v1/api-tokens/:tokenId', async ({ params, request }) => {
    await delay(150)
    const forbidden = requireDemoInstancePermission(
      request,
      'api_tokens:delete',
    )
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const token = demoApiTokens.find(
      (candidate) => candidate.id === params.tokenId,
    )
    if (!token) {
      return HttpResponse.json(
        { error: 'Token not found', code: 'not_found' },
        { status: 404 },
      )
    }
    if (persona.role === 'developer' && token.created_by !== persona.userId) {
      return HttpResponse.json(
        {
          error: 'You do not have permission to revoke this token.',
          code: 'forbidden',
        },
        { status: 403 },
      )
    }
    token.is_revoked = true
    return HttpResponse.json({ revoked: true })
  }),
]
