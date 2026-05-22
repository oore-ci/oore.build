import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  CreatePipelineRequest,
  RegisterIosDeviceRequest,
  UpdatePipelineAndroidSigningRequest,
  UpdatePipelineIosSigningRequest,
  UpdatePipelineRequest,
  ValidatePipelineRequest,
} from '@/lib/types'
import {
  createPipeline,
  deletePipeline,
  getPipeline,
  getPipelineAndroidSigning,
  getPipelineIosSigning,
  listPipelineIosDevices,
  listPipelines,
  registerPipelineIosDevice,
  syncPipelineIosSigning,
  updatePipeline,
  updatePipelineAndroidSigning,
  updatePipelineIosSigning,
  validatePipeline,
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

export function usePipelines(
  projectId: string,
  params?: { limit?: number; offset?: number },
  options?: { enabled?: boolean },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()
  const enabled = options?.enabled ?? true

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'pipelines',
      projectId,
      params ?? {},
    ],
    queryFn: () => listPipelines(baseUrl!, token!, projectId, params),
    enabled: enabled && !!baseUrl && !!token && !!projectId,
  })
}

export function usePipeline(pipelineId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'pipeline', pipelineId],
    queryFn: () => getPipeline(baseUrl!, token!, pipelineId),
    enabled: !!baseUrl && !!token && !!pipelineId,
  })
}

export function useCreatePipeline() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      projectId,
      data,
    }: {
      projectId: string
      data: CreatePipelineRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createPipeline(baseUrl, token, projectId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'pipelines'],
      })
    },
  })
}

export function useUpdatePipeline() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      pipelineId,
      data,
    }: {
      pipelineId: string
      data: UpdatePipelineRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updatePipeline(baseUrl, token, pipelineId, data)
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'pipelines'],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline',
          variables.pipelineId,
        ],
      })
    },
  })
}

export function useDeletePipeline() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (pipelineId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deletePipeline(baseUrl, token, pipelineId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'pipelines'],
      })
    },
  })
}

export function useValidatePipeline() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return useMutation({
    mutationFn: (data: ValidatePipelineRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return validatePipeline(baseUrl, token, data)
    },
  })
}

export function usePipelineAndroidSigning(pipelineId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'pipeline-android-signing',
      pipelineId,
    ],
    queryFn: () => getPipelineAndroidSigning(baseUrl!, token!, pipelineId),
    enabled: !!baseUrl && !!token && !!pipelineId,
  })
}

export function useUpdatePipelineAndroidSigning() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      pipelineId,
      data,
    }: {
      pipelineId: string
      data: UpdatePipelineAndroidSigningRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updatePipelineAndroidSigning(baseUrl, token, pipelineId, data)
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline-android-signing',
          variables.pipelineId,
        ],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline',
          variables.pipelineId,
        ],
      })
    },
  })
}

export function usePipelineIosSigning(pipelineId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'pipeline-ios-signing', pipelineId],
    queryFn: () => getPipelineIosSigning(baseUrl!, token!, pipelineId),
    enabled: !!baseUrl && !!token && !!pipelineId,
  })
}

export function useUpdatePipelineIosSigning() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      pipelineId,
      data,
    }: {
      pipelineId: string
      data: UpdatePipelineIosSigningRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updatePipelineIosSigning(baseUrl, token, pipelineId, data)
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline-ios-signing',
          variables.pipelineId,
        ],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline-ios-signing-devices',
          variables.pipelineId,
        ],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline',
          variables.pipelineId,
        ],
      })
    },
  })
}

export function usePipelineIosDevices(pipelineId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'pipeline-ios-signing-devices',
      pipelineId,
    ],
    queryFn: () => listPipelineIosDevices(baseUrl!, token!, pipelineId),
    enabled: !!baseUrl && !!token && !!pipelineId,
  })
}

export function useRegisterPipelineIosDevice() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      pipelineId,
      data,
    }: {
      pipelineId: string
      data: RegisterIosDeviceRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return registerPipelineIosDevice(baseUrl, token, pipelineId, data)
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline-ios-signing-devices',
          variables.pipelineId,
        ],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline-ios-signing',
          variables.pipelineId,
        ],
      })
    },
  })
}

export function useSyncPipelineIosSigning() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (pipelineId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return syncPipelineIosSigning(baseUrl, token, pipelineId)
    },
    onSuccess: (_data, pipelineId) => {
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline-ios-signing',
          pipelineId,
        ],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'pipeline-ios-signing-devices',
          pipelineId,
        ],
      })
    },
  })
}
