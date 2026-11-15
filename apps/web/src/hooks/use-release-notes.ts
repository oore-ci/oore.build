import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { isOoreApiError } from '@oore/client/client'
import type {
  ReleaseNotesDraftResponse,
  UpdateVersionReleaseNotesRequest,
  VersionReleaseNotes,
} from '@oore/client/models'
import {
  draftReleaseNotes,
  getReleaseNotes,
  updateReleaseNotes,
} from '@oore/client/operations'
import { getReleaseNotesQueryKey } from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import { scopeOoreQueryKey } from '@/lib/api-client/client'

/** Query keys that show a version's release notes. */
const NOTES_QUERIES = ['"getBuild"', '"getReleaseNotes"']

/**
 * Save the release notes of one app version. Every build of the version,
 * its tester page and its next store upload or promotion use them.
 */
export function useUpdateReleaseNotes(projectId: string, versionName: string) {
  const queryClient = useQueryClient()
  const { client } = useApiContext()
  return useMutation({
    mutationFn: async (
      body: UpdateVersionReleaseNotesRequest,
    ): Promise<VersionReleaseNotes> =>
      updateReleaseNotes({
        client,
        path: { project_id: projectId, version_name: versionName },
        body,
      }),
    onSuccess: () =>
      queryClient.invalidateQueries({
        predicate: (query) => {
          const key = JSON.stringify(query.queryKey)
          return NOTES_QUERIES.some((name) => key.includes(name))
        },
      }),
  })
}

/** Notes drafted from the commits since the last released version. */
export function useDraftReleaseNotes(projectId: string, versionName: string) {
  const { client } = useApiContext()
  return useMutation({
    mutationFn: async (): Promise<ReleaseNotesDraftResponse> =>
      draftReleaseNotes({
        client,
        path: { project_id: projectId, version_name: versionName },
      }),
  })
}

/**
 * The release notes of one app version, or null when nobody has written
 * or drafted any yet.
 */
export function useReleaseNotes(
  projectId: string,
  versionName: string | null | undefined,
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const path = { project_id: projectId, version_name: versionName ?? '' }
  return useQuery({
    queryKey: scopeOoreQueryKey(
      instanceId,
      getReleaseNotesQueryKey({ client, path }),
    ),
    queryFn: async (): Promise<VersionReleaseNotes | null> => {
      try {
        return await getReleaseNotes({ client, path })
      } catch (error) {
        if (isOoreApiError(error) && error.status === 404) return null
        throw error
      }
    },
    enabled: !!baseUrl && !!token && !!versionName,
  })
}
