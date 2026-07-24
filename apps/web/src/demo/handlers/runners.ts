import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

export const runnerHandlers = [
  http.get('/v1/runners', async () => {
    await delay(150)
    return HttpResponse.json({ runners: demoState.runners })
  }),

  http.patch('/v1/runners/:runnerId', async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'runners:write')
    if (forbidden) return forbidden
    const body = (await request.json()) as { name?: string }
    const runner = demoState.runners.find((r) => r.id === params.runnerId)
    if (!runner) {
      return HttpResponse.json(
        { error: 'Runner not found', code: 'not_found' },
        { status: 404 },
      )
    }
    Object.assign(runner, body, { updated_at: ago(0) })
    return HttpResponse.json({ runner })
  }),
]
