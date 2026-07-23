import { useQuery } from '@tanstack/react-query'

import type {
  Integration,
  IntegrationRepository,
  ListRepositoriesResponse,
  ScmProvider,
} from '@/lib/types'
import { listAllIntegrations, listIntegrationRepos } from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

export type SourceRepository = IntegrationRepository & {
  integration_id: string
  provider: ScmProvider
  host_url: string
}

export interface SourceRepositoryFailure {
  integration_id: string
  provider: ScmProvider
  host_url: string
  display_name?: string
  message: string
}

export interface SourceRepositoryDiscovery {
  repositories: Array<SourceRepository>
  failures: Array<SourceRepositoryFailure>
}

function sourceFailure(
  integration: Integration,
  error: unknown,
): SourceRepositoryFailure {
  return {
    integration_id: integration.id,
    provider: integration.provider,
    host_url: integration.host_url,
    display_name: integration.display_name,
    message:
      error instanceof Error
        ? error.message
        : 'Repositories could not be loaded from this source.',
  }
}

export async function discoverSourceRepositories(
  integrations: Array<Integration>,
  listRepositories: (
    integration: Integration,
  ) => Promise<ListRepositoriesResponse>,
  signal?: AbortSignal,
): Promise<SourceRepositoryDiscovery> {
  signal?.throwIfAborted()
  const results = await Promise.all(
    integrations.map(async (integration) => {
      try {
        signal?.throwIfAborted()
        const response = await listRepositories(integration)
        signal?.throwIfAborted()
        return {
          repositories: response.repositories.map((repository) => ({
            ...repository,
            integration_id: integration.id,
            provider: integration.provider,
            host_url: integration.host_url,
          })),
          failure: undefined,
        }
      } catch (error) {
        signal?.throwIfAborted()
        return {
          repositories: [],
          failure: sourceFailure(integration, error),
        }
      }
    }),
  )
  signal?.throwIfAborted()
  return {
    repositories: results.flatMap((result) => result.repositories),
    failures: results.flatMap((result) =>
      result.failure ? [result.failure] : [],
    ),
  }
}

export function useSourceRepositories(enabled: boolean) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'all-repos-for-project'],
    queryFn: async ({ signal }) => {
      signal.throwIfAborted()
      if (!baseUrl || !token) {
        return { repositories: [], failures: [] }
      }
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
