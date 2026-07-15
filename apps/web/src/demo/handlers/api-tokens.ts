import { HttpResponse, delay, http } from 'msw'
import type { ApiTokenSummary } from '@/lib/types'
import { getDemoPersonaFromRequest } from '../personas'
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
]
