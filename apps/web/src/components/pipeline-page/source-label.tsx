import { createContext, useContext, type ReactNode } from 'react'

import type { SourcedValue, ValueSource } from '@/lib/pipeline-sources'
import { Mono } from '@/components/layout/mono'
import { cn } from '@/lib/utils'

/**
 * The web page of a repository file, by path, or null when the repository
 * has none (local Git). Provided by the pipeline page.
 */
export const RepositoryFileUrl = createContext<(path: string) => string | null>(
  () => null,
)

/** A file path, linked to the repository when it has a web page. */
export function RepositoryFileLink({ path }: { path: string }) {
  const href = useContext(RepositoryFileUrl)(path)
  if (!href) return <Mono>{path}</Mono>
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="font-mono underline underline-offset-4"
    >
      {path}
    </a>
  )
}

/**
 * "from .oore.yaml", "set here" or "overridden for workflow release",
 * with the workflows it covers when that is not every build.
 */
export function SourceLabel({
  className,
  source,
  workflows = [],
}: {
  className?: string
  source: ValueSource
  workflows?: Array<string>
}) {
  const scope =
    source.kind !== 'workflow' && workflows.length > 0 ? (
      <>
        {' '}
        for <Mono>{workflows.join(', ')}</Mono>
      </>
    ) : null
  return (
    <span
      className={cn('text-[13px] text-muted-foreground', className)}
      data-source={source.kind}
    >
      {source.kind === 'file' ? (
        <>
          from <RepositoryFileLink path={source.path} />
          {scope}
        </>
      ) : source.kind === 'workflow' ? (
        <>
          overridden for workflow <Mono>{source.workflow}</Mono>
        </>
      ) : (
        <>set here{scope}</>
      )}
    </span>
  )
}

/** One line of a values-in-force list. */
export interface SourcedRow {
  key: string
  label: string
  value: ReactNode
  source: ValueSource
  workflows?: Array<string>
}

export function sourcedRows(
  label: string,
  entries: Array<SourcedValue>,
): Array<SourcedRow> {
  return entries.map((entry, index) => ({
    key: `${label}-${index}`,
    label: index === 0 ? label : '',
    value: entry.value,
    source: entry.source,
    workflows: entry.workflows,
  }))
}

/**
 * Read-only values with where each comes from: a label, the value, and
 * its source, stacked on narrow screens.
 */
export function SourcedList({ rows }: { rows: Array<SourcedRow> }) {
  if (rows.length === 0) return null
  return (
    <dl className="grid grid-cols-1 gap-x-6 gap-y-2 text-sm sm:grid-cols-[10rem_minmax(0,1fr)_auto]">
      {rows.map((row) => (
        <div
          key={row.key}
          className="grid grid-cols-subgrid gap-y-0.5 max-sm:pt-1 sm:col-span-3"
        >
          <dt className="text-muted-foreground">{row.label}</dt>
          <dd className="min-w-0 wrap-break-word">{row.value}</dd>
          <dd className="sm:text-right">
            <SourceLabel source={row.source} workflows={row.workflows} />
          </dd>
        </div>
      ))}
    </dl>
  )
}
