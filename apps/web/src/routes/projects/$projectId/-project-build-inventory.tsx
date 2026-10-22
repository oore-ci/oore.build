import { useMemo, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'

import type { Build } from '@oore/client/models'
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
import { BuildSourceValue, EmptyValue } from '@/components/table-values'
import { formatDuration, relativeTime } from '@/lib/format-utils'
import { TRIGGER_TYPE_LABELS } from '@/lib/status-variants'
import type { ProjectBuildSort } from './-project-build-sort'

const PROJECT_BUILD_SORTS = [
  'pipeline_name',
  'status',
  'branch',
  'created_at',
] satisfies ReadonlyArray<ProjectBuildSort>

function buildDuration(build: Build): string | undefined {
  if (!build.started_at || !build.finished_at) return undefined
  return formatDuration(build.finished_at - build.started_at)
}

const projectBuildColumns: Array<DataTableColumnDef<Build>> = [
  {
    accessorKey: 'build_number',
    header: 'Build',
    cell: ({ row }) => (
      <Link
        to="/builds/$buildId"
        params={{ buildId: row.original.id }}
        className="font-mono font-medium underline-offset-4 hover:underline"
      >
        #{row.original.build_number}
      </Link>
    ),
    enableHiding: false,
    enableSorting: false,
  },
  {
    id: 'pipeline_name',
    accessorFn: (build) => build.context?.pipeline_name,
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Pipeline" />
    ),
    cell: ({ row }) =>
      row.original.context?.pipeline_name ?? 'Unknown pipeline',
  },
  {
    accessorKey: 'status',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Status" />
    ),
    cell: ({ row }) => <StatusBadge kind="build" value={row.original.status} />,
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
    cell: ({ row }) => (
      <BuildSourceValue
        branch={row.original.branch}
        commitSha={row.original.commit_sha}
        context={row.original.context}
      />
    ),
  },
  {
    id: 'duration',
    header: 'Duration',
    cell: ({ row }) => {
      const duration = buildDuration(row.original)
      return duration ? (
        <span className="font-mono text-xs tabular-nums">{duration}</span>
      ) : (
        <EmptyValue />
      )
    },
    enableSorting: false,
  },
  {
    accessorKey: 'created_at',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Created" />
    ),
    cell: ({ row }) => relativeTime(row.original.created_at),
  },
]

export function ProjectBuildInventory({
  activeFilterCount,
  builds,
  direction,
  filters,
  isLoading,
  onClearFilters,
  onPageChange,
  onSearch,
  onSortChange,
  page,
  pageSize,
  query,
  sort,
  total,
}: {
  activeFilterCount: number
  builds: Array<Build>
  direction: SortDirection
  filters: ReactNode
  isLoading: boolean
  onClearFilters?: () => void
  onPageChange: (page: number) => void
  onSearch: (query: string) => void
  onSortChange: (sort: ProjectBuildSort, direction: SortDirection) => void
  page: number
  pageSize: number
  query: string
  sort: ProjectBuildSort
  total: number
}) {
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns: projectBuildColumns,
    data: builds,
    getRowId: (build) => build.id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(
        updater,
        sorting,
        PROJECT_BUILD_SORTS,
      )
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <DataTable
      table={table}
      filters={filters}
      activeFilterCount={activeFilterCount}
      onClearFilters={onClearFilters}
      search={{
        value: query,
        onChange: onSearch,
        placeholder: 'Search by branch',
      }}
      pagination={{ onPageChange, page, pageSize, total }}
      isLoading={isLoading}
      emptyMessage={
        isLoading
          ? 'Loading builds…'
          : query
            ? `No builds on branch “${query}”.`
            : 'No builds match these filters.'
      }
    />
  )
}
