import {
  useMutation,
  useQuery,
  useQueryClient,
  type Query,
} from '@tanstack/react-query'
import * as z from 'zod'

import type {
  AppStoreConnectKeySummary,
  CreateAppStoreConnectKeyRequest,
  UpdateAppStoreConnectKeyRequest,
} from '@oore/client/models'
import {
  createAppStoreConnectKey,
  deleteAppStoreConnectKey,
  updateAppStoreConnectKey,
} from '@oore/client/operations'
import { listAppStoreConnectKeysOptions } from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import { scopeOoreQueryOptions } from '@/lib/api-client/client'

export type { AppStoreConnectKeySummary } from '@oore/client/models'

// Pipeline responses embed the key they use, so a key change refreshes them.
const KEY_DEPENDENT_OPERATIONS = new Set([
  'listAppStoreConnectKeys',
  'getPipelineIosSigning',
  'getPipelineAppStorePublish',
])

const generatedQueryKeySchema = z.object({ _id: z.string() })

function isKeyDependentQuery(query: Query) {
  return query.queryKey.some((part) => {
    const operation = generatedQueryKeySchema.safeParse(part).data?._id
    return !!operation && KEY_DEPENDENT_OPERATIONS.has(operation)
  })
}

function selectKeys(response: { keys: Array<AppStoreConnectKeySummary> }) {
  return response.keys
}

export function useAppStoreConnectKeys(options?: { enabled?: boolean }) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      listAppStoreConnectKeysOptions({ client }),
    ),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
    select: selectKeys,
  })
}

type OoreClient = ReturnType<typeof useApiContext>['client']

function useKeyMutation<TVariables, TResult>(
  run: (client: OoreClient, variables: TVariables) => Promise<TResult>,
) {
  const queryClient = useQueryClient()
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: async (variables: TVariables) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return run(client, variables)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ predicate: isKeyDependentQuery })
    },
  })
}

export function useCreateAppStoreConnectKey() {
  return useKeyMutation((client, body: CreateAppStoreConnectKeyRequest) =>
    createAppStoreConnectKey({ body, client }),
  )
}

export function useUpdateAppStoreConnectKey() {
  return useKeyMutation(
    (
      client,
      { id, ...body }: UpdateAppStoreConnectKeyRequest & { id: string },
    ) => updateAppStoreConnectKey({ body, client, path: { id } }),
  )
}

export function useDeleteAppStoreConnectKey() {
  return useKeyMutation((client, id: string) =>
    deleteAppStoreConnectKey({ client, path: { id } }),
  )
}
