import type { BuildContext } from '@oore/client/models'

const DEFAULT_HOSTS = {
  github: 'https://github.com',
  gitlab: 'https://gitlab.com',
} as const

/** Web URL of a commit on its source host, or null for local repositories. */
export function commitUrl(
  context: BuildContext | null | undefined,
  sha: string,
): string | null {
  const provider = context?.repository_provider
  const repository = context?.repository_full_name
  if (!repository || (provider !== 'github' && provider !== 'gitlab'))
    return null
  const host = (context.repository_host_url || DEFAULT_HOSTS[provider])
    .replace(/\/+$/, '')
    .replace('://api.github.com', '://github.com')
  return provider === 'gitlab'
    ? `${host}/${repository}/-/commit/${sha}`
    : `${host}/${repository}/commit/${sha}`
}
