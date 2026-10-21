import { useMemo, type ReactNode } from 'react'

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

function auditActionLabel(action: string) {
  return formatEnumLabel(action.replace(/[.-]+/g, '_'))
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
    cell: ({ row }) => row.original.resource_id?.slice(0, 8) ?? 'Not available',
    enableSorting: false,
  },
  {
    accessorKey: 'details',
    header: 'Details',
    cell: ({ row }) => row.original.details ?? 'Not available',
    enableSorting: false,
  },
]

export function AuditLogCollection({
  direction,
  emptyState,
  entries,
  error,
  filters,
  isFiltered,
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
  filters: ReactNode
  isFiltered: boolean
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
          search={{
            value: query,
            onChange: onSearch,
            placeholder: 'Search actions',
          }}
          pagination={{ onPageChange, page, pageSize, total }}
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
