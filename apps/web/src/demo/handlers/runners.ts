import { HttpResponse, delay, http } from 'msw'
import { demoRunners } from '../data/runners'
import { ago } from '../seed'

export const runnerHandlers = [
  http.get('/v1/runners', async () => {
    await delay(150)
    return HttpResponse.json({ runners: demoRunners })
  }),

  http.patch('/v1/runners/:runnerId', async ({ params, request }) => {
    await delay(200)
    const body = (await request.json()) as { name?: string }
    const runner = demoRunners.find((r) => r.id === params.runnerId)
    return HttpResponse.json({
      runner: runner
        ? { ...runner, ...body, updated_at: ago(0) }
        : { id: params.runnerId, ...body, updated_at: ago(0) },
    })
  }),
]
