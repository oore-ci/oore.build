import { HttpResponse, delay, http } from 'msw'
import {
  demoNotificationChannels,
  demoNotificationDeliveries,
} from '../data/notification-channels'
import type {
  NotificationChannel,
  NotificationDelivery,
} from '@/lib/types'

function now(): number {
  return Math.floor(Date.now() / 1000)
}

let channels: Array<NotificationChannel> = demoNotificationChannels.map(
  (c) => ({ ...c }),
)
let deliveries: Array<NotificationDelivery> = demoNotificationDeliveries.map(
  (d) => ({ ...d }),
)

export const notificationHandlers = [
  http.get('/v1/settings/notification-channels', async () => {
    await delay(150)
    return HttpResponse.json({ channels, total: channels.length })
  }),

  http.post('/v1/settings/notification-channels', async ({ request }) => {
    await delay(300)
    const body = (await request.json()) as Record<string, unknown>

    const channel: NotificationChannel = {
      id: `notif-demo-${crypto.randomUUID().slice(0, 8)}`,
      name: body.name as string,
      channel_type: body.channel_type as NotificationChannel['channel_type'],
      enabled: (body.enabled as boolean | undefined) ?? true,
      events: (body.events as Array<string> | undefined) ?? [],
      has_url: !!(body.url as string),
      has_secret: !!(body.secret as string),
      created_by: 'usr-demo-owner-001',
      created_at: now(),
      updated_at: now(),
    }

    channels = [...channels, channel]
    return HttpResponse.json({ channel }, { status: 201 })
  }),

  http.get('/v1/settings/notification-channels/:id', async ({ params }) => {
    await delay(150)
    const channel = channels.find((c) => c.id === params.id)
    if (!channel) {
      return HttpResponse.json(
        { error: 'not_found', message: 'Channel not found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({ channel })
  }),

  http.put('/v1/settings/notification-channels/:id', async ({ params, request }) => {
    await delay(300)
    const idx = channels.findIndex((c) => c.id === params.id)
    if (idx === -1) {
      return HttpResponse.json(
        { error: 'not_found', message: 'Channel not found' },
        { status: 404 },
      )
    }
    const body = (await request.json()) as Record<string, unknown>
    const existing = channels[idx]

    const updated: NotificationChannel = {
      ...existing,
      name: (body.name as string | undefined) ?? existing.name,
      enabled: (body.enabled as boolean | undefined) ?? existing.enabled,
      events: (body.events as Array<string> | undefined) ?? existing.events,
      has_url: body.url ? true : existing.has_url,
      has_secret: body.secret ? true : existing.has_secret,
      updated_at: now(),
    }

    channels = channels.map((c, i) => (i === idx ? updated : c))
    return HttpResponse.json({ channel: updated })
  }),

  http.delete('/v1/settings/notification-channels/:id', async ({ params }) => {
    await delay(300)
    const idx = channels.findIndex((c) => c.id === params.id)
    if (idx === -1) {
      return HttpResponse.json(
        { error: 'not_found', message: 'Channel not found' },
        { status: 404 },
      )
    }
    channels = channels.filter((_, i) => i !== idx)
    deliveries = deliveries.filter((d) => d.channel_id !== params.id)
    return HttpResponse.json({ deleted: true })
  }),

  http.post('/v1/settings/notification-channels/:id/test', async ({ params }) => {
    await delay(500)
    const channel = channels.find((c) => c.id === params.id)
    if (!channel) {
      return HttpResponse.json(
        { error: 'not_found', message: 'Channel not found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({ success: true })
  }),

  http.get(
    '/v1/settings/notification-channels/:id/deliveries',
    async ({ params }) => {
      await delay(150)
      const channelDeliveries = deliveries.filter(
        (d) => d.channel_id === params.id,
      )
      return HttpResponse.json({
        deliveries: channelDeliveries,
        total: channelDeliveries.length,
      })
    },
  ),
]
