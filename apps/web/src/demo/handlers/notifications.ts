import { HttpResponse, delay, http } from 'msw'
import type { NotificationChannel } from '@/lib/types'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

function now(): number {
  return Math.floor(Date.now() / 1000)
}

export const notificationHandlers = [
  http.get('/v1/settings/notification-channels', async () => {
    await delay(150)
    return HttpResponse.json({
      channels: demoState.notificationChannels,
      total: demoState.notificationChannels.length,
    })
  }),

  http.post('/v1/settings/notification-channels', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as Record<string, unknown>

    const channel: NotificationChannel = {
      id: `notif-demo-${crypto.randomUUID().slice(0, 8)}`,
      name: body.name as string,
      channel_type: body.channel_type as NotificationChannel['channel_type'],
      enabled: (body.enabled as boolean | undefined) ?? true,
      events: (body.events as Array<string> | undefined) ?? [],
      has_url: !!(body.url as string),
      has_secret: !!(body.secret as string),
      has_smtp_config: !!(body.smtp_config as
        Record<string, unknown> | undefined),
      created_by: 'usr-demo-owner-001',
      created_at: now(),
      updated_at: now(),
    }

    demoState.notificationChannels.push(channel)
    return HttpResponse.json({ channel }, { status: 201 })
  }),

  http.get('/v1/settings/notification-channels/:id', async ({ params }) => {
    await delay(150)
    const channel = demoState.notificationChannels.find(
      (c) => c.id === params.id,
    )
    if (!channel) {
      return HttpResponse.json(
        { error: 'not_found', message: 'Channel not found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({ channel })
  }),

  http.put(
    '/v1/settings/notification-channels/:id',
    async ({ params, request }) => {
      await delay(300)
      const forbidden = requireDemoInstancePermission(
        request,
        'instance_settings:write',
      )
      if (forbidden) return forbidden
      const idx = demoState.notificationChannels.findIndex(
        (c) => c.id === params.id,
      )
      if (idx === -1) {
        return HttpResponse.json(
          { error: 'not_found', message: 'Channel not found' },
          { status: 404 },
        )
      }
      const body = (await request.json()) as Record<string, unknown>
      const existing = demoState.notificationChannels[idx]

      const updated: NotificationChannel = {
        ...existing,
        name: (body.name as string | undefined) ?? existing.name,
        enabled: (body.enabled as boolean | undefined) ?? existing.enabled,
        events: (body.events as Array<string> | undefined) ?? existing.events,
        has_url: body.url ? true : existing.has_url,
        has_secret: body.secret ? true : existing.has_secret,
        has_smtp_config: body.smtp_config ? true : existing.has_smtp_config,
        updated_at: now(),
      }

      demoState.notificationChannels[idx] = updated
      return HttpResponse.json({ channel: updated })
    },
  ),

  http.delete(
    '/v1/settings/notification-channels/:id',
    async ({ params, request }) => {
      await delay(300)
      const forbidden = requireDemoInstancePermission(
        request,
        'instance_settings:write',
      )
      if (forbidden) return forbidden
      const idx = demoState.notificationChannels.findIndex(
        (c) => c.id === params.id,
      )
      if (idx === -1) {
        return HttpResponse.json(
          { error: 'not_found', message: 'Channel not found' },
          { status: 404 },
        )
      }
      demoState.notificationChannels.splice(idx, 1)
      demoState.notificationDeliveries =
        demoState.notificationDeliveries.filter(
          (delivery) => delivery.channel_id !== params.id,
        )
      return HttpResponse.json({ deleted: true })
    },
  ),

  http.post(
    '/v1/settings/notification-channels/:id/test',
    async ({ params, request }) => {
      await delay(500)
      const forbidden = requireDemoInstancePermission(
        request,
        'instance_settings:write',
      )
      if (forbidden) return forbidden
      const channel = demoState.notificationChannels.find(
        (c) => c.id === params.id,
      )
      if (!channel) {
        return HttpResponse.json(
          { error: 'not_found', message: 'Channel not found' },
          { status: 404 },
        )
      }
      return HttpResponse.json({ success: true })
    },
  ),

  http.get(
    '/v1/settings/notification-channels/:id/deliveries',
    async ({ params }) => {
      await delay(150)
      const channelDeliveries = demoState.notificationDeliveries.filter(
        (d) => d.channel_id === params.id,
      )
      return HttpResponse.json({
        deliveries: channelDeliveries,
        total: channelDeliveries.length,
      })
    },
  ),
]
