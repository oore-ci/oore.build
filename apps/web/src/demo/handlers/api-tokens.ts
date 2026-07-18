import { HttpResponse, delay, http } from 'msw'
import { getDemoPersonaFromRequest } from '../personas'
import { requireDemoInstancePermission } from '../authorization'
import { ago } from '../seed'
import { demoState } from '../state'

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
        ? demoState.apiTokens
        : demoState.apiTokens.filter(
            (token) => token.created_by === persona.userId,
          )
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
    demoState.apiTokens.unshift({
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
    const token = demoState.apiTokens.find(
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
