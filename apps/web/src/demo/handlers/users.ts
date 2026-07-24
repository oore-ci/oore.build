import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import { getDemoPersonaFromRequest } from '../personas'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

function requireAdmin(request: Request): Response | null {
  const { role } = getDemoPersonaFromRequest(request)
  if (role === 'owner' || role === 'admin') return null
  return HttpResponse.json(
    {
      error: 'You do not have permission to access this resource.',
      code: 'forbidden',
    },
    { status: 403 },
  )
}

export const userHandlers = [
  http.get('/v1/users', async ({ request }) => {
    await delay(150)
    const forbidden = requireAdmin(request)
    if (forbidden) return forbidden
    return HttpResponse.json({ users: demoState.users })
  }),

  http.post('/v1/users/invite', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(request, 'users:invite')
    if (forbidden) return forbidden
    const body = (await request.json()) as { email: string; role: string }
    const user = {
      id: `usr-demo-new-${crypto.randomUUID().slice(0, 8)}`,
      email: body.email,
      role: body.role as 'owner' | 'admin' | 'developer' | 'qa_viewer',
      status: 'invited' as const,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.users.push(user)
    return HttpResponse.json({ user })
  }),

  http.patch('/v1/users/:userId/role', async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'users:write')
    if (forbidden) return forbidden
    const body = (await request.json()) as { role: string }
    const user = demoState.users.find(
      (candidate) => candidate.id === params.userId,
    )
    if (!user) {
      return HttpResponse.json(
        { error: 'User not found', code: 'not_found' },
        { status: 404 },
      )
    }
    user.role = body.role as typeof user.role
    user.updated_at = ago(0)
    return HttpResponse.json({ user })
  }),

  http.post('/v1/users/:userId/enable', async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'users:enable')
    if (forbidden) return forbidden
    const user = demoState.users.find(
      (candidate) => candidate.id === params.userId,
    )
    if (!user) {
      return HttpResponse.json(
        { error: 'User not found', code: 'not_found' },
        { status: 404 },
      )
    }
    user.status = 'active'
    user.updated_at = ago(0)
    return HttpResponse.json({ user })
  }),

  http.delete('/v1/users/:userId', async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'users:delete')
    if (forbidden) return forbidden
    const index = demoState.users.findIndex(
      (candidate) => candidate.id === params.userId,
    )
    if (index < 0) {
      return HttpResponse.json(
        { error: 'User not found', code: 'not_found' },
        { status: 404 },
      )
    }
    demoState.users.splice(index, 1)
    for (const roles of Object.values(demoState.projectRoles)) {
      if (roles) delete roles[String(params.userId)]
    }
    return HttpResponse.json({ ok: true })
  }),
]
