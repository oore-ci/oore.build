import { useMemo, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'

import { CollectionError, CollectionFrame } from '@/components/collection'
import {
  DataTable,
  DataTableColumnHeader,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import {
  dataTableSortingState,
  resolveDataTableSorting,
} from '@/components/data-table-features'
import type { SortDirection } from '@/components/data-table-features'
import { Badge } from '@/components/ui/badge'
import { formatEnumLabel } from '@/lib/format-utils'
import { relativeTime } from '@/lib/format-utils'
import type { AuditLogEntry } from '@oore/client/models'
import { ROLE_LABELS } from './-user-role-labels'

export type AuditSort =
  | 'created_at'
  | 'actor_email'
  | 'action'
  | 'resource_type'

const AUDIT_SORTS = [
  'created_at',
  'actor_email',
  'action',
  'resource_type',
] satisfies ReadonlyArray<AuditSort>

const PROPER_WORDS = new Map([
  ['api', 'API'],
  ['github', 'GitHub'],
  ['gitlab', 'GitLab'],
  ['oidc', 'OIDC'],
  ['qa', 'QA'],
  ['smtp', 'SMTP'],
  ['url', 'URL'],
])

function auditActionLabel(action: string) {
  return formatEnumLabel(action.replace(/[.-]+/g, '_'))
    .split(' ')
    .map((word) => PROPER_WORDS.get(word.toLowerCase()) ?? word)
    .join(' ')
}

// Details are free text from the daemon and can carry raw role values.
function auditDetails(details: string) {
  return details.replace(
    /\b(owner|admin|developer|qa_viewer)\b/g,
    (role) => ROLE_LABELS[role] ?? role,
  )
}

function NotRecorded() {
  return (
    <span className="text-muted-foreground">
      <span aria-hidden>—</span>
      <span className="sr-only">Not recorded</span>
    </span>
  )
}

function AuditTime({ entry }: { entry: AuditLogEntry }) {
  return (
    <time
      dateTime={new Date(entry.created_at * 1000).toISOString()}
      title={new Date(entry.created_at * 1000).toLocaleString()}
      className="text-xs whitespace-nowrap text-muted-foreground"
    >
      {relativeTime(entry.created_at)}
    </time>
  )
}

// The full id, linked to the resource when it has a page of its own.
function AuditResourceId({
  id,
  type,
}: {
  id: string | null | undefined
  type: string
}) {
  if (!id) return <NotRecorded />
  const className =
    'block max-w-48 truncate font-mono text-xs text-muted-foreground'
  const linkClassName = `${className} hover:text-foreground hover:underline`
  if (type === 'build')
    return (
      <Link
        to="/builds/$buildId"
        params={{ buildId: id }}
        title={id}
        className={linkClassName}
      >
        {id}
      </Link>
    )
  if (type === 'project')
    return (
      <Link
        to="/projects/$projectId"
        params={{ projectId: id }}
        title={id}
        className={linkClassName}
      >
        {id}
      </Link>
    )
  if (type === 'integration')
    return (
      <Link
        to="/settings/integrations/$integrationId"
        params={{ integrationId: id }}
        title={id}
        className={linkClassName}
      >
        {id}
      </Link>
    )
  return (
    <span title={id} className={className}>
      {id}
    </span>
  )
}

const auditColumns: Array<DataTableColumnDef<AuditLogEntry>> = [
  {
    accessorKey: 'created_at',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Time" />
    ),
    cell: ({ row }) => <AuditTime entry={row.original} />,
  },
  {
    accessorKey: 'actor_email',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Actor" />
    ),
    cell: ({ row }) =>
      row.original.actor_email ?? (
        <span className="text-muted-foreground">System</span>
      ),
  },
  {
    accessorKey: 'action',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Action" />
    ),
    cell: ({ row }) => (
      <Badge variant="outline" className="max-w-full truncate">
        {auditActionLabel(row.original.action)}
      </Badge>
    ),
  },
  {
    accessorKey: 'resource_type',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Resource" />
    ),
    cell: ({ row }) => (
      <Badge variant="secondary">
        {formatEnumLabel(row.original.resource_type)}
      </Badge>
    ),
  },
  {
    accessorKey: 'resource_id',
    header: 'Resource ID',
    cell: ({ row }) => (
      <AuditResourceId
        id={row.original.resource_id}
        type={row.original.resource_type}
      />
    ),
    enableSorting: false,
  },
  {
    accessorKey: 'details',
    header: 'Details',
    cell: ({ row }) =>
      row.original.details ? (
        <span className="block max-w-md min-w-48 break-words whitespace-normal">
          {auditDetails(row.original.details)}
        </span>
      ) : (
        <NotRecorded />
      ),
    enableSorting: false,
  },
]

export function AuditLogCollection({
  direction,
  emptyState,
  entries,
  error,
  activeFilterCount,
  filters,
  isFiltered,
  onClearFilters,
  isLoading,
  isRefreshing,
  onPageChange,
  onRetry,
  onSearch,
  onSortChange,
  page,
  pageSize,
  query,
  sort,
  total,
}: {
  direction: SortDirection
  emptyState: ReactNode
  entries: Array<AuditLogEntry>
  error: Error | null
  activeFilterCount: number
  filters: ReactNode
  isFiltered: boolean
  onClearFilters: () => void
  isLoading: boolean
  isRefreshing: boolean
  onPageChange: (page: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  onSortChange: (sort: AuditSort, direction: SortDirection) => void
  page: number
  pageSize: number
  query: string
  sort: AuditSort
  total: number
}) {
  const hasResults = total > 0
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns: auditColumns,
    data: entries,
    getRowId: (entry) => String(entry.id),
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, AUDIT_SORTS)
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <CollectionFrame
      ariaLabel="Audit activity"
      isBusy={isLoading || isRefreshing}
    >
      {error ? (
        <CollectionError
          title="Audit log could not be loaded"
          description={error.message}
          onRetry={onRetry}
        />
      ) : null}

      {!isLoading && !hasResults && !error && !isFiltered ? (
        emptyState
      ) : (
        <DataTable
          table={table}
          filters={filters}
          activeFilterCount={activeFilterCount}
          onClearFilters={isFiltered ? onClearFilters : undefined}
          search={{
            value: query,
            onChange: onSearch,
            placeholder: 'Search actions',
          }}
          pagination={{ onPageChange, page, pageSize, total }}
          isLoading={isLoading}
          emptyMessage={
            isLoading
              ? 'Loading audit entries…'
              : isFiltered
                ? 'No matching activity.'
                : undefined
          }
        />
      )}
    </CollectionFrame>
  )
}
