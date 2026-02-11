import { HttpResponse, delay, http } from 'msw'
import { demoUsers } from '../data/users'
import { ago } from '../seed'

export const userHandlers = [
  http.get('/v1/users', async () => {
    await delay(150)
    return HttpResponse.json({ users: demoUsers })
  }),

  http.post('/v1/users/invite', async ({ request }) => {
    await delay(300)
    const body = (await request.json()) as { email: string; role: string }
    return HttpResponse.json({
      user: {
        id: `usr-demo-new-${Date.now()}`,
        email: body.email,
        role: body.role,
        status: 'invited',
        created_at: ago(0),
        updated_at: ago(0),
      },
    })
  }),

  http.patch('/v1/users/:userId/role', async ({ params, request }) => {
    await delay(200)
    const body = (await request.json()) as { role: string }
    const user = demoUsers.find((u) => u.id === params.userId)
    return HttpResponse.json({
      user: user
        ? { ...user, role: body.role, updated_at: ago(0) }
        : { id: params.userId, role: body.role, updated_at: ago(0) },
    })
  }),

  http.post('/v1/users/:userId/enable', async ({ params }) => {
    await delay(200)
    const user = demoUsers.find((u) => u.id === params.userId)
    return HttpResponse.json({
      user: user
        ? { ...user, status: 'active', updated_at: ago(0) }
        : { id: params.userId, status: 'active', updated_at: ago(0) },
    })
  }),

  http.delete('/v1/users/:userId', async () => {
    await delay(200)
    return HttpResponse.json({ ok: true })
  }),
]
