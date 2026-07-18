import { HttpResponse, delay, http } from 'msw'
import { demoState } from '../state'

export const auditLogHandlers = [
  http.get('/v1/audit-logs', async ({ request }) => {
    await delay(200)
    const url = new URL(request.url)

    const limit = Math.min(Number(url.searchParams.get('limit')) || 50, 200)
    const offset = Number(url.searchParams.get('offset')) || 0
    const actorId = url.searchParams.get('actor_id')
    const action = url.searchParams.get('action')
    const resourceType = url.searchParams.get('resource_type')
    const fromTs = url.searchParams.get('from_ts')
    const toTs = url.searchParams.get('to_ts')

    let filtered = [...demoState.auditLogs]

    if (actorId) {
      filtered = filtered.filter((e) => e.actor_id === actorId)
    }
    if (action) {
      filtered = filtered.filter((e) =>
        e.action.toLowerCase().includes(action.toLowerCase()),
      )
    }
    if (resourceType) {
      filtered = filtered.filter((e) => e.resource_type === resourceType)
    }
    if (fromTs) {
      const from = Number(fromTs)
      filtered = filtered.filter((e) => e.created_at >= from)
    }
    if (toTs) {
      const to = Number(toTs)
      filtered = filtered.filter((e) => e.created_at <= to)
    }

    const sort = url.searchParams.get('sort')
    const direction = url.searchParams.get('direction') === 'asc' ? 1 : -1
    filtered.sort((left, right) => {
      const value = (entry: (typeof demoState.auditLogs)[number]) => {
        if (sort === 'actor_email')
          return entry.actor_email?.toLowerCase() ?? ''
        if (sort === 'action') return entry.action.toLowerCase()
        if (sort === 'resource_type') return entry.resource_type.toLowerCase()
        return entry.created_at
      }
      const leftValue = value(left)
      const rightValue = value(right)
      const compared =
        typeof leftValue === 'string'
          ? leftValue.localeCompare(String(rightValue))
          : leftValue - Number(rightValue)
      return (compared || left.id - right.id) * direction
    })

    const total = filtered.length
    const entries = filtered.slice(offset, offset + limit)

    return HttpResponse.json({ entries, total })
  }),
]
