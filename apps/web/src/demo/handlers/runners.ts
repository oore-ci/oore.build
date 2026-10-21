import { demoApi } from './api'
import { HttpResponse, delay, http } from 'msw'
import * as z from 'zod'
import { ago } from '../seed'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

export const runnerHandlers = [
  demoApi.listRunners(async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const query = url.searchParams.get('q')?.toLowerCase()
    const sort = url.searchParams.get('sort') ?? 'created_at'
    const direction = url.searchParams.get('direction') === 'asc' ? 1 : -1
    const limit = Math.min(Number(url.searchParams.get('limit')) || 20, 100)
    const offset = Math.max(Number(url.searchParams.get('offset')) || 0, 0)
    const runners = demoState.runners
      .filter((runner) =>
        query
          ? [runner.id, runner.name, runner.status, runner.registered_by]
              .filter(Boolean)
              .some((value) => value?.toLowerCase().includes(query))
          : true,
      )
      .slice()
      .sort((left, right) => {
        const leftValue =
          sort === 'name'
            ? left.name
            : sort === 'status'
              ? left.status
              : sort === 'last_heartbeat_at'
                ? left.last_heartbeat_at
                : left.created_at
        const rightValue =
          sort === 'name'
            ? right.name
            : sort === 'status'
              ? right.status
              : sort === 'last_heartbeat_at'
                ? right.last_heartbeat_at
                : right.created_at
        return (
          direction *
          String(leftValue ?? '').localeCompare(String(rightValue ?? ''))
        )
      })
    return HttpResponse.json({
      runners: runners.slice(offset, offset + limit),
      total: runners.length,
      online_total: demoState.runners.filter(
        (runner) => runner.status === 'online' || runner.status === 'busy',
      ).length,
    })
  }),

  demoApi.updateRunner(async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(request, 'runners:write')
    if (forbidden) return forbidden
    const body = z
      .object({ name: z.string().optional() })
      .parse(await request.json())
    const runner = demoState.runners.find((r) => r.id === params.runner_id)
    if (!runner) {
      return HttpResponse.json(
        { error: 'Runner not found', code: 'not_found' },
        { status: 404 },
      )
    }
    Object.assign(runner, body, { updated_at: ago(0) })
    return HttpResponse.json({ runner })
  }),

  http.post(
    '*/v1/runners/:runner_id/caches/purge',
    async ({ params, request }) => {
      await delay(200)
      const forbidden = requireDemoInstancePermission(request, 'runners:write')
      if (forbidden) return forbidden
      const runner = demoState.runners.find((r) => r.id === params.runner_id)
      if (!runner) {
        return HttpResponse.json(
          { error: 'Runner not found', code: 'not_found' },
          { status: 404 },
        )
      }
      Object.assign(runner, {
        cache_purge_requested_at: ago(0),
        updated_at: ago(0),
      })
      return HttpResponse.json({ runner })
    },
  ),
]
