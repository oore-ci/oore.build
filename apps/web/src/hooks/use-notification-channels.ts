import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  createNotificationChannel,
  deleteNotificationChannel,
  testNotificationChannel,
  updateNotificationChannel,
} from '@oore/client/operations'
import {
  getNotificationChannelOptions,
  listNotificationChannelsOptions,
  listNotificationChannelsQueryKey,
  listNotificationDeliveriesOptions,
} from '@oore/client/react-query'
import type { ListNotificationChannelsData } from '@oore/client/models'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'
import type {
  CreateNotificationChannelRequest,
  UpdateNotificationChannelRequest,
} from '@/lib/notification-channels'

type ListNotificationChannelsParams = NonNullable<
  ListNotificationChannelsData['query']
>

export function useNotificationChannels(
  params?: ListNotificationChannelsParams,
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listNotificationChannelsOptions({ client, query: params }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token,
  })
}

export function useNotificationChannel(channelId: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getNotificationChannelOptions({ client, path: { id: channelId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!channelId,
  })
}

export function useCreateNotificationChannel() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (data: CreateNotificationChannelRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createNotificationChannel({
        body: data,
        client,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listNotificationChannelsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useUpdateNotificationChannel() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: ({
      id,
      data,
    }: {
      id: string
      data: UpdateNotificationChannelRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updateNotificationChannel({ body: data, client, path: { id } })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listNotificationChannelsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useDeleteNotificationChannel() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (id: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deleteNotificationChannel({ client, path: { id } })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listNotificationChannelsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useTestNotificationChannel() {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: (id: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return testNotificationChannel({ client, path: { id } })
    },
  })
}

export function useNotificationDeliveries(channelId: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listNotificationDeliveriesOptions({ client, path: { id: channelId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!channelId,
  })
}
