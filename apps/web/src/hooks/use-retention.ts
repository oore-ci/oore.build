import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type {
  UpdateProjectRetentionOverrideRequest,
  UpdateRetentionPolicyRequest,
} from '@/lib/types'
import {
  deleteProjectRetention,
  getProjectRetention,
  getRetentionLastCleanup,
  getRetentionPolicy,
  updateProjectRetention,
  updateRetentionPolicy,
} from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
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
  return instance?.url ?? null
}

export function useRetentionPolicy() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'retention-policy'],
    queryFn: () => getRetentionPolicy(baseUrl!, token!),
    enabled: baseUrl !== null && !!token,
  })
}

export function useUpdateRetentionPolicy() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: UpdateRetentionPolicyRequest) => {
      if (baseUrl === null || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateRetentionPolicy(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'retention-policy'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'retention-last-cleanup'],
      })
    },
  })
}

export function useRetentionLastCleanup() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'retention-last-cleanup'],
    queryFn: () => getRetentionLastCleanup(baseUrl!, token!),
    enabled: baseUrl !== null && !!token,
  })
}

export function useProjectRetention(projectId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'project-retention', projectId],
    queryFn: () => getProjectRetention(baseUrl!, token!, projectId),
    enabled: baseUrl !== null && !!token && !!projectId,
  })
}

export function useUpdateProjectRetention(projectId: string) {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: UpdateProjectRetentionOverrideRequest) => {
      if (baseUrl === null || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateProjectRetention(baseUrl, token, projectId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'project-retention', projectId],
      })
    },
  })
}

export function useDeleteProjectRetention(projectId: string) {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: () => {
      if (baseUrl === null || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return deleteProjectRetention(baseUrl, token, projectId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'project-retention', projectId],
      })
    },
  })
}
