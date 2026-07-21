import type { IntegrationRepository } from '@/lib/types'

export function filterIntegrationRepositories(
  repositories: Array<IntegrationRepository>,
  query: string | undefined,
): Array<IntegrationRepository> {
  const normalizedQuery = query?.trim().toLocaleLowerCase()
  return repositories
    .filter((repository) => {
      if (!normalizedQuery) return true
      return [
        repository.full_name,
        repository.default_branch,
        repository.is_private ? 'private' : 'public',
      ]
        .filter(Boolean)
        .join(' ')
        .toLocaleLowerCase()
        .includes(normalizedQuery)
    })
    .sort((left, right) => left.full_name.localeCompare(right.full_name))
}

export function paginateIntegrationRepositories(
  repositories: Array<IntegrationRepository>,
  page: number,
  pageSize: number,
): Array<IntegrationRepository> {
  return repositories.slice((page - 1) * pageSize, page * pageSize)
}
