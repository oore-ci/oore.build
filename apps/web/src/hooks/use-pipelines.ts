import {
  keepPreviousData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import type {
  CreatePipelineRequest,
  ExportPipelineYamlRequest,
  ImportPipelineYamlRequest,
  PlanPipelineRequest,
  RegisterIosDeviceRequest,
  UpdatePipelineAndroidSigningRequest,
  UpdatePipelineIosSigningRequest,
  UpdatePipelineRequest,
  ValidatePipelineRequest,
} from '@oore/client/models'
import {
  createPipeline,
  deletePipeline,
  exportPipelineYaml,
  importPipelineYaml,
  planPipeline,
  updatePipeline,
  validatePipeline,
  registerPipelineIosDevice,
  syncPipelineIosSigning,
  updatePipelineAndroidSigning,
  updatePipelineIosSigning,
} from '@oore/client/operations'
import {
  discoverRepositoryWorkflowsOptions,
  getPipelineAndroidSigningOptions,
  getPipelineAndroidSigningQueryKey,
  getPipelineIosSigningOptions,
  getPipelineIosSigningQueryKey,
  getPipelineOptions,
  getProjectQueryKey,
  getPipelineQueryKey,
  listPipelineIosDevicesOptions,
  listPipelineIosDevicesQueryKey,
  listPipelinesInfiniteOptions,
  listPipelinesOptions,
  listPipelinesQueryKey,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreInfiniteQueryOptions,
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

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
  const { baseUrl, client, instanceId, token } = useApiContext()
  const enabled = options?.enabled ?? true
  const query = scopeOoreQueryOptions(
    instanceId,
    listPipelinesOptions({
      client,
      path: { project_id: projectId },
      query: params,
    }),
  )

  return useQuery({
    ...query,
    enabled: enabled && !!baseUrl && !!token && !!projectId,
  })
}

export function useInfinitePipelines(
  projectId: string,
  params?: {
    search?: string
    sort?: 'created_at' | 'name'
    direction?: 'asc' | 'desc'
    limit?: number
  },
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const enabled = options?.enabled ?? true
  const query = scopeOoreInfiniteQueryOptions(
    instanceId,
    listPipelinesInfiniteOptions({
      client,
      path: { project_id: projectId },
      query: { ...params, limit: params?.limit ?? 100 },
    }),
  )

  return useInfiniteQuery({
    ...query,
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) => {
      const loaded = allPages.reduce(
        (count, page) => count + page.pipelines.length,
        0,
      )
      return loaded < lastPage.total ? loaded : undefined
    },
    enabled: enabled && !!baseUrl && !!token && !!projectId,
  })
}

export function useRepositoryWorkflows(
  projectId: string,
  params?: { ref?: string; path?: string },
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const enabled = options?.enabled ?? true
  const query = scopeOoreQueryOptions(
    instanceId,
    discoverRepositoryWorkflowsOptions({
      client,
      path: { project_id: projectId },
      query: params,
    }),
  )

  return useQuery({
    ...query,
    enabled: enabled && !!baseUrl && !!token && !!projectId,
    staleTime: 30_000,
  })
}

export function usePipeline(pipelineId: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getPipelineOptions({ client, path: { pipeline_id: pipelineId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!pipelineId,
  })
}

export function useCreatePipeline() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

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
      return createPipeline({
        body: data,
        client,
        path: { project_id: projectId },
      })
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getProjectQueryKey({
            client,
            path: { project_id: variables.projectId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listPipelinesQueryKey({
            client,
            path: { project_id: variables.projectId },
          }),
        ),
      })
    },
  })
}

export function useUpdatePipeline() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

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
      return updatePipeline({
        body: data,
        client,
        path: { pipeline_id: pipelineId },
      })
    },
    onSuccess: (data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listPipelinesQueryKey({
            client,
            path: { project_id: data.pipeline.project_id },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getPipelineQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
    },
  })
}

export function useDeletePipeline() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: ({ pipelineId }: { pipelineId: string; projectId: string }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deletePipeline({ client, path: { pipeline_id: pipelineId } })
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getProjectQueryKey({
            client,
            path: { project_id: variables.projectId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listPipelinesQueryKey({
            client,
            path: { project_id: variables.projectId },
          }),
        ),
      })
    },
  })
}

export function useValidatePipeline() {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: (data: ValidatePipelineRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return validatePipeline({ body: data, client })
    },
  })
}

/**
 * What a build would run for a `.oore.yaml` or the form's settings. Pass
 * `null` to pause. The previous plan stays on screen while a new one loads.
 */
export function usePipelinePlan(body: PlanPipelineRequest | null) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    queryKey: scopeOoreQueryKey(instanceId, ['planPipeline', body]),
    queryFn: ({ signal }) =>
      // SAFETY: the query is disabled while `body` is null.
      planPipeline({ body: body as PlanPipelineRequest, client, signal }),
    enabled: !!baseUrl && !!token && body !== null,
    placeholderData: keepPreviousData,
    staleTime: Infinity,
  })
}

/**
 * The form's settings as a `version: 2` `.oore.yaml`, from the daemon. Pass
 * `null` to pause. The previous file stays on screen while a new one loads.
 */
export function usePipelineYamlExport(body: ExportPipelineYamlRequest | null) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    queryKey: scopeOoreQueryKey(instanceId, ['exportPipelineYaml', body]),
    queryFn: ({ signal }) =>
      // SAFETY: the query is disabled while `body` is null.
      exportPipelineYaml({
        body: body as ExportPipelineYamlRequest,
        client,
        signal,
      }),
    enabled: !!baseUrl && !!token && body !== null,
    placeholderData: keepPreviousData,
    staleTime: Infinity,
  })
}

/**
 * An edited `.oore.yaml` read back into form settings, from the daemon.
 * Pass `null` to pause. While a new answer loads the previous one stays as
 * placeholder data, which callers check before applying it.
 */
export function usePipelineYamlImport(body: ImportPipelineYamlRequest | null) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    queryKey: scopeOoreQueryKey(instanceId, ['importPipelineYaml', body]),
    queryFn: ({ signal }) =>
      // SAFETY: the query is disabled while `body` is null.
      importPipelineYaml({
        body: body as ImportPipelineYamlRequest,
        client,
        signal,
      }),
    enabled: !!baseUrl && !!token && body !== null,
    placeholderData: keepPreviousData,
    staleTime: Infinity,
    // Keys carry the whole file, so old answers are dropped soon.
    gcTime: 30_000,
  })
}

export function usePipelineAndroidSigning(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getPipelineAndroidSigningOptions({
      client,
      path: { pipeline_id: pipelineId },
    }),
  )

  return useQuery({
    ...query,
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!pipelineId,
  })
}

export function useUpdatePipelineAndroidSigning() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

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
      return updatePipelineAndroidSigning({
        body: data,
        client,
        path: { pipeline_id: pipelineId },
      })
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getPipelineAndroidSigningQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getPipelineQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
    },
  })
}

export function usePipelineIosSigning(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getPipelineIosSigningOptions({
      client,
      path: { pipeline_id: pipelineId },
    }),
  )

  return useQuery({
    ...query,
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!pipelineId,
  })
}

export function useUpdatePipelineIosSigning() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

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
      return updatePipelineIosSigning({
        body: data,
        client,
        path: { pipeline_id: pipelineId },
      })
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getPipelineIosSigningQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listPipelineIosDevicesQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getPipelineQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
    },
  })
}

export function usePipelineIosDevices(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listPipelineIosDevicesOptions({
      client,
      path: { pipeline_id: pipelineId },
    }),
  )

  return useQuery({
    ...query,
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!pipelineId,
  })
}

export function useRegisterPipelineIosDevice() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

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
      return registerPipelineIosDevice({
        body: data,
        client,
        path: { pipeline_id: pipelineId },
      })
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listPipelineIosDevicesQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getPipelineIosSigningQueryKey({
            client,
            path: { pipeline_id: variables.pipelineId },
          }),
        ),
      })
    },
  })
}

export function useSyncPipelineIosSigning() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (pipelineId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return syncPipelineIosSigning({
        client,
        path: { pipeline_id: pipelineId },
      })
    },
    onSuccess: (_data, pipelineId) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getPipelineIosSigningQueryKey({
            client,
            path: { pipeline_id: pipelineId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listPipelineIosDevicesQueryKey({
            client,
            path: { pipeline_id: pipelineId },
          }),
        ),
      })
    },
  })
}
