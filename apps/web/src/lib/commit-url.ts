import type { BuildContext } from '@oore/client/models'

const DEFAULT_HOSTS = {
  github: 'https://github.com',
  gitlab: 'https://gitlab.com',
} as const

/**
 * The provider's web page for a commit, or undefined when the repository has
 * no browsable host (local Git, or a build without repository context).
 */
export function commitUrl(
  context: BuildContext | null | undefined,
  sha: string,
): string | undefined {
  const provider = context?.repository_provider
  const fullName = context?.repository_full_name
  if (!fullName || (provider !== 'github' && provider !== 'gitlab')) {
    return undefined
  }
  const host = (context.repository_host_url ?? DEFAULT_HOSTS[provider]).replace(
    /\/+$/,
    '',
  )
  const separator = provider === 'gitlab' ? '/-/commit/' : '/commit/'
  return `${host}/${fullName}${separator}${encodeURIComponent(sha)}`
}
