import type { BuildContext } from '@oore/client/models'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

/** A cell with nothing to show: a quiet dash that reads as "None". */
export function EmptyValue() {
  return (
    <span className="text-muted-foreground">
      <span aria-hidden>—</span>
      <span className="sr-only">None</span>
    </span>
  )
}

function commitUrl(context: BuildContext | null | undefined, sha: string) {
  const fullName = context?.repository_full_name
  if (!fullName) return null
  const provider = context.repository_provider
  if (provider === 'github') {
    const host = context.repository_host_url?.replace(/\/$/, '')
    return `${host || 'https://github.com'}/${fullName}/commit/${sha}`
  }
  if (provider === 'gitlab') {
    const host = context.repository_host_url?.replace(/\/$/, '')
    return `${host || 'https://gitlab.com'}/${fullName}/-/commit/${sha}`
  }
  return null
}

/** A short monospace commit hash, linked to the provider when it has one. */
export function CommitValue({
  context,
  sha,
}: {
  context?: BuildContext | null
  sha: string | null | undefined
}) {
  if (!sha) return <EmptyValue />
  const short = sha.slice(0, 7)
  const url = commitUrl(context, sha)
  return url ? (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      title={sha}
      className={cn(
        textVariants({ variant: 'mono' }),
        'hover:text-foreground hover:underline',
      )}
    >
      {short}
    </a>
  ) : (
    <span title={sha} className={textVariants({ variant: 'mono' })}>
      {short}
    </span>
  )
}

/**
 * A cell's primary value over its secondary detail, side by side in a
 * compact table (`data-density="compact"`).
 */
export const CELL_STACK =
  'flex flex-col in-data-[density=compact]:flex-row in-data-[density=compact]:items-baseline in-data-[density=compact]:gap-x-2'

/** A build's branch with its commit underneath, one column for its source. */
export function BuildSourceValue({
  branch,
  commitSha,
  context,
}: {
  branch: string | null | undefined
  commitSha: string | null | undefined
  context?: BuildContext | null
}) {
  if (!branch && !commitSha) return <EmptyValue />
  return (
    <div className={CELL_STACK}>
      {branch ? <span>{branch}</span> : <EmptyValue />}
      {commitSha ? <CommitValue context={context} sha={commitSha} /> : null}
    </div>
  )
}
