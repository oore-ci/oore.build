import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import type {
  CreatePipelineRequest,
  UpdatePipelineRequest,
  ValidatePipelineRequest,
} from '@/lib/types'
import {
  createPipeline,
  deletePipeline,
  getPipeline,
  listPipelines,
  updatePipeline,
  validatePipeline,
} from '@/lib/api'
import {
  useAuthToken,
  useBaseUrl,
  useInstanceQueryPrefix,
} from '@/hooks/query-context'

export function usePipelines(
  projectId: string,
  params?: { limit?: number; offset?: number },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'pipelines', projectId, params ?? {}],
    queryFn: () => listPipelines(baseUrl()!, token()!, projectId, params),
    enabled: !!baseUrl() && !!token() && !!projectId,
  }))
}

export function usePipeline(pipelineId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'pipeline', pipelineId],
    queryFn: () => getPipeline(baseUrl()!, token()!, pipelineId),
    enabled: !!baseUrl() && !!token() && !!pipelineId,
  }))
}

export function useCreatePipeline(projectId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: CreatePipelineRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return createPipeline(baseUrl()!, token()!, projectId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'pipelines'] })
    },
  }))
}

export function useUpdatePipeline() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async ({
      pipelineId,
      data,
    }: {
      pipelineId: string
      data: UpdatePipelineRequest
    }) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return updatePipeline(baseUrl()!, token()!, pipelineId, data)
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'pipelines'] })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'pipeline', variables.pipelineId],
      })
    },
  }))
}

export function useDeletePipeline() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (pipelineId: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return deletePipeline(baseUrl()!, token()!, pipelineId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'pipelines'] })
    },
  }))
}

export function useValidatePipeline() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return createMutation(() => ({
    mutationFn: async (data: ValidatePipelineRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return validatePipeline(baseUrl()!, token()!, data)
    },
  }))
}
