import type { SourceRepository } from '@/hooks/use-source-repositories'

export function repositoryProjectDefaults(repository: SourceRepository) {
  return {
    name: repository.full_name.split('/').filter(Boolean).at(-1) ?? '',
    defaultBranch: repository.default_branch ?? '',
  }
}
