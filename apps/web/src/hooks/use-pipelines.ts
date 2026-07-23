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
  discoverRepositoryWorkflows,
  getPipeline,
  getPipelineAndroidSigning,
  getPipelineIosSigning,
  listAllPipelines,
  listPipelineIosDevices,
  listPipelines,
  registerPipelineIosDevice,
  syncPipelineIosSigning,
  updatePipeline,
  updatePipelineAndroidSigning,
  updatePipelineIosSigning,
  validatePipeline,
} from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

export function usePipelines(
  projectId: string,
  params?: {
    search?: string
    sort?: 'created_at' | 'name'
    direction?: 'asc' | 'desc'
    limit?: number
    offset?: number
  },
  options?: { enabled?: boolean },
) {
  const { baseUrl, instance, token } = useApiContext()
  const enabled = options?.enabled ?? true

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'pipelines',
      projectId,
      params ?? {},
    ],
    queryFn: ({ signal }) =>
      listPipelines(baseUrl!, token!, projectId, params, { signal }),
    enabled: enabled && !!baseUrl && !!token && !!projectId,
  })
}

export function useAllPipelines(
  projectId: string,
  params?: {
    search?: string
    sort?: 'created_at' | 'name'
    direction?: 'asc' | 'desc'
  },
  options?: { enabled?: boolean },
) {
  const { baseUrl, instance, token } = useApiContext()
  const enabled = options?.enabled ?? true

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'all-pipelines',
      projectId,
      params ?? {},
    ],
    queryFn: ({ signal }) =>
      listAllPipelines(baseUrl!, token!, projectId, params, { signal }),
    enabled: enabled && !!baseUrl && !!token && !!projectId,
  })
}

export function useRepositoryWorkflows(
  projectId: string,
  params?: { reference?: string; path?: string },
  options?: { enabled?: boolean },
) {
  const { baseUrl, instance, token } = useApiContext()
  const enabled = options?.enabled ?? true

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'repository-workflows',
      projectId,
      params ?? {},
    ],
    queryFn: ({ signal }) =>
      discoverRepositoryWorkflows(baseUrl!, token!, projectId, params, {
        signal,
      }),
    enabled: enabled && !!baseUrl && !!token && !!projectId,
    staleTime: 30_000,
  })
}

export function usePipeline(pipelineId: string) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'pipeline', pipelineId],
    queryFn: ({ signal }) =>
      getPipeline(baseUrl!, token!, pipelineId, { signal }),
    enabled: !!baseUrl && !!token && !!pipelineId,
  })
}

export function useCreatePipeline() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'all-pipelines'],
      })
    },
  })
}

export function useUpdatePipeline() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
        queryKey: [instance?.id ?? '__none__', 'all-pipelines'],
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
  const { baseUrl, instance, token } = useApiContext()

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
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'all-pipelines'],
      })
    },
  })
}

export function useValidatePipeline() {
  const { baseUrl, token } = useApiContext()

  return useMutation({
    mutationFn: (data: ValidatePipelineRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return validatePipeline(baseUrl, token, data)
    },
  })
}

export function usePipelineAndroidSigning(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'pipeline-android-signing',
      pipelineId,
    ],
    queryFn: ({ signal }) =>
      getPipelineAndroidSigning(baseUrl!, token!, pipelineId, { signal }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!pipelineId,
  })
}

export function useUpdatePipelineAndroidSigning() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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

export function usePipelineIosSigning(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'pipeline-ios-signing', pipelineId],
    queryFn: ({ signal }) =>
      getPipelineIosSigning(baseUrl!, token!, pipelineId, { signal }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!pipelineId,
  })
}

export function useUpdatePipelineIosSigning() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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

export function usePipelineIosDevices(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'pipeline-ios-signing-devices',
      pipelineId,
    ],
    queryFn: ({ signal }) =>
      listPipelineIosDevices(baseUrl!, token!, pipelineId, { signal }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!pipelineId,
  })
}

export function useRegisterPipelineIosDevice() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
  const { baseUrl, instance, token } = useApiContext()

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
