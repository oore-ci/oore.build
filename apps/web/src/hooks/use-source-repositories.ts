import { useQuery } from '@tanstack/react-query'

import type {
  Integration,
  IntegrationRepository,
  ListRepositoriesResponse,
  ScmProvider,
} from '@/lib/types'
import { listAllIntegrations, listIntegrationRepos } from '@/lib/api'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

export type SourceRepository = IntegrationRepository & {
  integration_id: string
  provider: ScmProvider
  host_url: string
}

export async function discoverSourceRepositories(
  integrations: Array<Integration>,
  listRepositories: (
    integration: Integration,
  ) => Promise<ListRepositoriesResponse>,
  signal?: AbortSignal,
): Promise<Array<SourceRepository>> {
  signal?.throwIfAborted()
  const results = await Promise.allSettled(
    integrations.map(async (integration) => {
      signal?.throwIfAborted()
      const response = await listRepositories(integration)
      signal?.throwIfAborted()
      return response.repositories.map((repository) => ({
        ...repository,
        integration_id: integration.id,
        provider: integration.provider,
        host_url: integration.host_url,
      }))
    }),
  )
  signal?.throwIfAborted()

  return results.flatMap((result) =>
    result.status === 'fulfilled' ? result.value : [],
  )
}

export async function discoverSourceRepositoriesStrict(
  integrations: Array<Integration>,
  listRepositories: (
    integration: Integration,
  ) => Promise<ListRepositoriesResponse>,
  signal?: AbortSignal,
): Promise<Array<SourceRepository>> {
  signal?.throwIfAborted()
  const results = await Promise.all(
    integrations.map(async (integration) => {
      signal?.throwIfAborted()
      const response = await listRepositories(integration)
      signal?.throwIfAborted()
      return response.repositories.map((repository) => ({
        ...repository,
        integration_id: integration.id,
        provider: integration.provider,
        host_url: integration.host_url,
      }))
    }),
  )
  signal?.throwIfAborted()
  return results.flat()
}

export function useSourceRepositories(enabled: boolean) {
  const instance = useActiveInstance()
  const token = useAuthStore((state) => state.token)
  const baseUrl = resolveInstanceApiBaseUrl(instance)

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'all-repos-for-project'],
    queryFn: async ({ signal }) => {
      signal.throwIfAborted()
      if (!baseUrl || !token) return []
      const { integrations } = await listAllIntegrations(
        baseUrl,
        token,
        undefined,
        {
          signal,
        },
      )
      return discoverSourceRepositories(
        integrations,
        (integration) =>
          listIntegrationRepos(baseUrl, token, integration.id, { signal }),
        signal,
      )
    },
    enabled: enabled && !!baseUrl && !!token,
  })
}

export function useRunnerPolicyRepositories(enabled: boolean) {
  const instance = useActiveInstance()
  const token = useAuthStore((state) => state.token)
  const baseUrl = resolveInstanceApiBaseUrl(instance)

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'all-repos-for-runner-policy'],
    queryFn: async ({ signal }) => {
      signal.throwIfAborted()
      if (!baseUrl || !token) return []
      const { integrations } = await listAllIntegrations(
        baseUrl,
        token,
        undefined,
        { signal },
      )
      return discoverSourceRepositoriesStrict(
        integrations,
        (integration) =>
          listIntegrationRepos(baseUrl, token, integration.id, { signal }),
        signal,
      )
    },
    enabled: enabled && !!baseUrl && !!token,
  })
}
