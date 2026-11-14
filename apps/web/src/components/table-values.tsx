import type { ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import type { Build, BuildContext } from '@oore/client/models'
import { buildVersionLabel } from '@/lib/build-identity'
import { LIST_NAV_ITEM } from '@/lib/keyboard-shortcuts'
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

/**
 * A build's source: the commit title when the runner reported one, over its
 * branch, commit and author. Without a title, the branch over the commit.
 */
export function BuildSourceValue({
  branch,
  commitSha,
  context,
  title,
  author,
}: {
  branch: string | null | undefined
  commitSha: string | null | undefined
  context?: BuildContext | null
  title?: string | null
  author?: string | null
}) {
  if (!branch && !commitSha) return <EmptyValue />
  if (title) {
    return (
      <div className={cn(CELL_STACK, 'min-w-0')}>
        <span className="max-w-72 truncate" title={title}>
          {title}
        </span>
        <span className={cn(textVariants({ variant: 'caption' }), 'truncate')}>
          {branch ? `${branch} · ` : ''}
          {commitSha ? <CommitValue context={context} sha={commitSha} /> : null}
          {author ? ` · ${author}` : ''}
        </span>
      </div>
    )
  }
  return (
    <div className={CELL_STACK}>
      {branch ? <span>{branch}</span> : <EmptyValue />}
      {commitSha ? <CommitValue context={context} sha={commitSha} /> : null}
    </div>
  )
}

/** A build's number, linked, with the app version it made underneath. */
export function BuildNumberValue({
  build,
  navItem = false,
  note,
}: {
  build: Pick<Build, 'id' | 'build_number' | 'version_name' | 'version_code'>
  /** Make the link a keyboard list stop. */
  navItem?: boolean
  /** One more short line, such as which push the build belongs to. */
  note?: ReactNode
}) {
  const version = buildVersionLabel(build)
  return (
    <div className={CELL_STACK}>
      <Link
        {...(navItem ? LIST_NAV_ITEM : {})}
        to="/builds/$buildId"
        params={{ buildId: build.id }}
        className="font-mono font-medium underline-offset-4 hover:underline"
      >
        #{build.build_number}
      </Link>
      {version ? (
        <span className={cn(textVariants({ variant: 'caption' }), 'truncate')}>
          {version}
        </span>
      ) : null}
      {note ? (
        <span className={cn(textVariants({ variant: 'caption' }), 'truncate')}>
          {note}
        </span>
      ) : null}
    </div>
  )
}
