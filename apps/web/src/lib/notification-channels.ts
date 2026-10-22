import type {
  CreateNotificationChannelRequest as ClientCreateNotificationChannelRequest,
  NotificationChannel as ClientNotificationChannel,
  NotificationChannelType as ClientNotificationChannelType,
  SmtpConfig,
  UpdateNotificationChannelRequest as ClientUpdateNotificationChannelRequest,
} from '@oore/client/models'

// The daemon contract has Slack channels, project scope and readable channel
// settings, but the published @oore/client models predate them. Drop these
// once the client is regenerated.
type Flatten<T> = { [K in keyof T]: T[K] }

export type NotificationChannelType = ClientNotificationChannelType | 'slack'

export type NotificationChannel = Flatten<
  Omit<ClientNotificationChannel, 'channel_type'> & {
    channel_type: NotificationChannelType
    project_id?: string | null
    url_host?: string | null
    smtp_summary?: Omit<SmtpConfig, 'password'> | null
  }
>

export type CreateNotificationChannelRequest = Flatten<
  Omit<ClientCreateNotificationChannelRequest, 'channel_type'> & {
    channel_type: NotificationChannelType
    project_id?: string | null
  }
>

export type UpdateNotificationChannelRequest = Flatten<
  ClientUpdateNotificationChannelRequest & {
    project_id?: string | null
  }
>

export function toClientCreateRequest(
  request: CreateNotificationChannelRequest,
): ClientCreateNotificationChannelRequest {
  // SAFETY: the daemon accepts every field here; only the client's generated
  // types lag behind, and operations do not validate request bodies.
  return request as ClientCreateNotificationChannelRequest
}

export function fromClientChannel(
  channel: ClientNotificationChannel,
): NotificationChannel {
  // SAFETY: the daemon returns every field here; only the client's generated
  // types lag behind.
  return channel as NotificationChannel
}
