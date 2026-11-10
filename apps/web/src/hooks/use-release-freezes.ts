import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  CreateReleaseRequest,
  SetReleaseFreezeRequest,
} from '@oore/client/models'
import {
  createRelease,
  deleteReleaseFreeze,
  setReleaseFreeze,
} from '@oore/client/operations'
import {
  listReleaseFreezesOptions,
  listReleaseFreezesQueryKey,
  listReleasesQueryKey,
} from '@oore/client/react-query'

import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

export function useReleaseFreezes() {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(instanceId, listReleaseFreezesOptions({ client })),
    enabled: !!baseUrl && !!token,
    staleTime: 30_000,
  })
}

/** The freeze that holds an app: its own, else the instance's. */
export function useActiveFreeze(projectId: string) {
  const freezes = useReleaseFreezes().data?.freezes ?? []
  return (
    freezes.find((freeze) => freeze.project_id === projectId) ??
    freezes.find((freeze) => !freeze.project_id) ??
    null
  )
}

function useInvalidateFreezes() {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return () =>
    void queryClient.invalidateQueries({
      queryKey: scopeOoreQueryKey(
        instanceId,
        listReleaseFreezesQueryKey({ client }),
      ),
    })
}

export function useSetReleaseFreeze() {
  const { client } = useApiContext()
  const invalidate = useInvalidateFreezes()
  return useMutation({
    mutationFn: (body: SetReleaseFreezeRequest) =>
      setReleaseFreeze({ body, client }),
    onSuccess: invalidate,
  })
}

export function useLiftReleaseFreeze() {
  const { client } = useApiContext()
  const invalidate = useInvalidateFreezes()
  return useMutation({
    mutationFn: (freezeId: string) =>
      deleteReleaseFreeze({ client, path: { freeze_id: freezeId } }),
    onSuccess: invalidate,
  })
}

/** Records a release made by hand in a store console, without a store call. */
export function useRecordRelease() {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (body: CreateReleaseRequest) => createRelease({ body, client }),
    onSuccess: () =>
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listReleasesQueryKey({ client }),
        ),
      }),
  })
}
