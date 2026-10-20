import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import type {
  CreateNotificationChannelRequest,
  NotificationChannel,
  UpdateNotificationChannelRequest,
} from '@/lib/notification-channels'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

const channelTypeSchema = z.enum(['webhook', 'mattermost', 'email', 'slack'])
const smtpTlsModeSchema = z.enum(['none', 'start_tls', 'tls'])
const smtpConfigSchema = z.object({
  from_address: z.string(),
  host: z.string(),
  password: z.string(),
  port: z.number(),
  recipients: z.array(z.string()),
  tls_mode: smtpTlsModeSchema,
  username: z.string(),
})
const updateSmtpConfigSchema = z.object({
  from_address: z.string().nullish(),
  host: z.string().nullish(),
  password: z.string().nullish(),
  port: z.number().nullish(),
  recipients: z.array(z.string()).nullish(),
  tls_mode: smtpTlsModeSchema.nullish(),
  username: z.string().nullish(),
})
const createChannelSchema = z.toZod<CreateNotificationChannelRequest>()(
  z.object({
    name: z.string(),
    channel_type: channelTypeSchema,
    enabled: z.boolean().optional(),
    events: z.array(z.string()).optional(),
    project_id: z.string().nullish(),
    url: z.string().nullish(),
    secret: z.string().nullish(),
    smtp_config: smtpConfigSchema.nullish(),
  }),
)
const updateChannelSchema = z.toZod<UpdateNotificationChannelRequest>()(
  z.object({
    name: z.string().nullish(),
    enabled: z.boolean().nullish(),
    events: z.array(z.string()).nullish(),
    project_id: z.string().nullish(),
    url: z.string().nullish(),
    secret: z.string().nullish(),
    smtp_config: updateSmtpConfigSchema.nullish(),
  }),
)

function now(): number {
  return Math.floor(Date.now() / 1000)
}

export const notificationHandlers = [
  demoApi.listNotificationChannels(async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const query = url.searchParams.get('q')?.toLowerCase()
    const sort = url.searchParams.get('sort') ?? 'name'
    const direction = url.searchParams.get('direction') === 'desc' ? -1 : 1
    const limit = Math.min(Number(url.searchParams.get('limit')) || 20, 100)
    const offset = Math.max(Number(url.searchParams.get('offset')) || 0, 0)
    const channels = demoState.notificationChannels
      .filter((channel) =>
        query
          ? [channel.name, channel.channel_type, ...channel.events].some(
              (value) => value.toLowerCase().includes(query),
            )
          : true,
      )
      .slice()
      .sort((left, right) => {
        const leftValue =
          sort === 'type'
            ? left.channel_type
            : sort === 'status'
              ? left.enabled
              : sort === 'updated_at'
                ? left.updated_at
                : left.name
        const rightValue =
          sort === 'type'
            ? right.channel_type
            : sort === 'status'
              ? right.enabled
              : sort === 'updated_at'
                ? right.updated_at
                : right.name
        return direction * String(leftValue).localeCompare(String(rightValue))
      })
    return HttpResponse.json({
      channels: channels.slice(offset, offset + limit),
      total: channels.length,
    })
  }),

  demoApi.createNotificationChannel(async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const body = createChannelSchema.parse(await request.json())

    const channel: NotificationChannel = {
      id: `notif-demo-${crypto.randomUUID().slice(0, 8)}`,
      name: body.name,
      channel_type: body.channel_type,
      enabled: body.enabled ?? true,
      events: body.events ?? [],
      project_id: body.project_id || undefined,
      has_url: !!body.url,
      has_secret: !!body.secret,
      has_smtp_config: !!body.smtp_config,
      created_by: 'usr-demo-owner-001',
      created_at: now(),
      updated_at: now(),
    }

    demoState.notificationChannels.push(channel)
    return HttpResponse.json({ channel }, { status: 201 })
  }),

  demoApi.getNotificationChannel(async ({ params }) => {
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

  demoApi.updateNotificationChannel(async ({ params, request }) => {
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
    const body = updateChannelSchema.parse(await request.json())
    const existing = demoState.notificationChannels[idx]

    const updated: NotificationChannel = {
      ...existing,
      name: body.name ?? existing.name,
      enabled: body.enabled ?? existing.enabled,
      events: body.events ?? existing.events,
      project_id:
        body.project_id === undefined
          ? existing.project_id
          : body.project_id || undefined,
      has_url: body.url ? true : existing.has_url,
      has_secret: body.secret ? true : existing.has_secret,
      has_smtp_config: body.smtp_config ? true : existing.has_smtp_config,
      updated_at: now(),
    }

    demoState.notificationChannels[idx] = updated
    return HttpResponse.json({ channel: updated })
  }),

  demoApi.deleteNotificationChannel(async ({ params, request }) => {
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
    demoState.notificationDeliveries = demoState.notificationDeliveries.filter(
      (delivery) => delivery.channel_id !== params.id,
    )
    return HttpResponse.json({ deleted: true })
  }),

  demoApi.testNotificationChannel(async ({ params, request }) => {
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
  }),

  demoApi.listNotificationDeliveries(async ({ params }) => {
    await delay(150)
    const channelDeliveries = demoState.notificationDeliveries.filter(
      (d) => d.channel_id === params.id,
    )
    return HttpResponse.json({
      deliveries: channelDeliveries,
      total: channelDeliveries.length,
    })
  }),
]
