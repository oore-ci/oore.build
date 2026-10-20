import type {
  CreateNotificationChannelRequest as ClientCreateNotificationChannelRequest,
  NotificationChannel as ClientNotificationChannel,
  NotificationChannelType as ClientNotificationChannelType,
  UpdateNotificationChannelRequest as ClientUpdateNotificationChannelRequest,
} from '@oore/client/models'

// The daemon contract has Slack channels and project scope, but the published
// @oore/client models predate them. Drop these once the client is regenerated.
type Flatten<T> = { [K in keyof T]: T[K] }

export type NotificationChannelType = ClientNotificationChannelType | 'slack'

export type NotificationChannel = Flatten<
  Omit<ClientNotificationChannel, 'channel_type'> & {
    channel_type: NotificationChannelType
    project_id?: string | null
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
