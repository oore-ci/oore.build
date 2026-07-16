import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  CreateNotificationChannelRequest,
  UpdateNotificationChannelRequest,
} from '@/lib/types'
import {
  createNotificationChannel,
  deleteNotificationChannel,
  listNotificationChannels,
  listNotificationDeliveries,
  testNotificationChannel,
  updateNotificationChannel,
} from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'

function useAuthToken(): string | null {
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  if (!token || expiresAt == null) return null
  if (expiresAt <= Math.floor(Date.now() / 1000)) return null
  return token
}

function useBaseUrl(): string | null {
  const instance = useActiveInstance()
  return resolveInstanceApiBaseUrl(instance)
}

export function useNotificationChannels() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'notification-channels'],
    queryFn: ({ signal }) =>
      listNotificationChannels(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useCreateNotificationChannel() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: CreateNotificationChannelRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createNotificationChannel(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'notification-channels'],
      })
    },
  })
}

export function useUpdateNotificationChannel() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
      return updateNotificationChannel(baseUrl, token, id, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'notification-channels'],
      })
    },
  })
}

export function useDeleteNotificationChannel() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (id: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deleteNotificationChannel(baseUrl, token, id)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'notification-channels'],
      })
    },
  })
}

export function useTestNotificationChannel() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return useMutation({
    mutationFn: (id: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return testNotificationChannel(baseUrl, token, id)
    },
  })
}

export function useNotificationDeliveries(channelId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'notification-deliveries',
      channelId,
    ],
    queryFn: ({ signal }) =>
      listNotificationDeliveries(baseUrl!, token!, channelId, { signal }),
    enabled: !!baseUrl && !!token && !!channelId,
  })
}
