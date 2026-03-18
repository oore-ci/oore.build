import { HttpResponse, delay, http } from 'msw'
import { demoAuditLogs } from '../data/audit-logs'

export const auditLogHandlers = [
  http.get('/v1/audit-logs', async ({ request }) => {
    await delay(200)
    const url = new URL(request.url)

    const limit = Number(url.searchParams.get('limit')) || 25
    const offset = Number(url.searchParams.get('offset')) || 0
    const actorId = url.searchParams.get('actor_id')
    const action = url.searchParams.get('action')
    const resourceType = url.searchParams.get('resource_type')
    const fromTs = url.searchParams.get('from_ts')
    const toTs = url.searchParams.get('to_ts')

    let filtered = [...demoAuditLogs]

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

    const total = filtered.length
    const entries = filtered.slice(offset, offset + limit)

    return HttpResponse.json({ entries, total })
  }),
]
