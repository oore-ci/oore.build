import type { UpdateNotificationChannelRequest } from '@/lib/types'

export function buildWebhookChannelUpdate(values: {
  enabled: boolean
  events: Array<string>
  name: string
  removeSecret: boolean
  secret?: string
  url?: string
}): UpdateNotificationChannelRequest {
  return {
    name: values.name,
    enabled: values.enabled,
    events: values.events,
    url: values.url || undefined,
    secret: values.removeSecret ? '' : values.secret || undefined,
  }
}
