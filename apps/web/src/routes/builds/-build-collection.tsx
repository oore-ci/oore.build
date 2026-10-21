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
import { StatusBadge } from '@/components/status-badge'
import { relativeTime } from '@/lib/format-utils'
import {
  getRunnerPolicyBlockLabel,
  TRIGGER_TYPE_LABELS,
} from '@/lib/status-variants'
import type { Build } from '@oore/client/models'
import BuildActionsMenu from './-build-actions-menu'
import type { BuildSort } from './-build-sort'

const BUILD_TABLE_SORTS = [
  'project_name',
  'status',
  'branch',
  'created_at',
] satisfies ReadonlyArray<BuildSort>

function projectName(build: Build) {
  return build.context?.project_name ?? 'Unknown project'
}

const columns: Array<DataTableColumnDef<Build>> = [
  {
    accessorKey: 'build_number',
    header: 'Build',
    cell: ({ row }) => (
      <Link
        to="/builds/$buildId"
        params={{ buildId: row.original.id }}
        className="font-mono"
      >
        #{row.original.build_number}
      </Link>
    ),
    enableSorting: false,
  },
  {
    id: 'project_name',
    accessorFn: projectName,
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Project" />
    ),
    cell: ({ row }) => (
      <div>
        <div>{projectName(row.original)}</div>
        {row.original.context?.pipeline_name ? (
          <div className="text-muted-foreground">
            {row.original.context.pipeline_name}
          </div>
        ) : null}
      </div>
    ),
  },
  {
    accessorKey: 'status',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Status" />
    ),
    cell: ({ row }) => (
      <div>
        <StatusBadge kind="build" value={row.original.status} />
        {row.original.runner_policy_block_reason ? (
          <div className="text-warning">
            {getRunnerPolicyBlockLabel(row.original.runner_policy_block_reason)}
          </div>
        ) : null}
      </div>
    ),
  },
  {
    accessorKey: 'trigger_type',
    header: 'Trigger',
    cell: ({ row }) => (
      <Badge variant="outline">
        {TRIGGER_TYPE_LABELS[row.original.trigger_type]}
      </Badge>
    ),
    enableSorting: false,
  },
  {
    accessorKey: 'branch',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Branch" />
    ),
    cell: ({ row }) => row.original.branch ?? 'n/a',
  },
  {
    accessorKey: 'commit_sha',
    header: 'Commit',
    cell: ({ row }) => row.original.commit_sha?.slice(0, 10) ?? 'n/a',
    enableSorting: false,
  },
  {
    accessorKey: 'created_at',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Created" />
    ),
    cell: ({ row }) => relativeTime(row.original.created_at),
  },
  {
    id: 'actions',
    cell: ({ row }) => <BuildActionsMenu build={row.original} />,
    enableHiding: false,
    enableSorting: false,
  },
]

export function BuildCollection({
  builds,
  direction,
  emptyState,
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
  builds: Array<Build>
  direction: SortDirection
  emptyState: ReactNode
  error: Error | null
  filters: ReactNode
  isFiltered: boolean
  isLoading: boolean
  isRefreshing: boolean
  onPageChange: (page: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  onSortChange: (sort: BuildSort, direction: SortDirection) => void
  page: number
  pageSize: number
  query: string
  sort: BuildSort
  total: number
}) {
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: builds,
    getRowId: (build) => build.id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, BUILD_TABLE_SORTS)
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <CollectionFrame
      ariaLabel="Build queue and history"
      isBusy={isLoading || isRefreshing}
    >
      {error ? (
        <CollectionError
          title="Builds could not be loaded"
          description={error.message}
          onRetry={onRetry}
        />
      ) : null}

      {!isLoading && total === 0 && !error && !isFiltered ? (
        emptyState
      ) : (
        <DataTable
          table={table}
          filters={filters}
          search={{
            value: query,
            onChange: onSearch,
            placeholder: 'Search by branch',
          }}
          pagination={{ onPageChange, page, pageSize, total }}
          emptyMessage={
            isLoading
              ? 'Loading builds…'
              : isFiltered
                ? 'No matching builds.'
                : undefined
          }
        />
      )}
    </CollectionFrame>
  )
}
