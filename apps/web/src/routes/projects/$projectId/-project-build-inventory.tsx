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
import { relativeTime } from '@/lib/format-utils'
import { TRIGGER_TYPE_LABELS } from '@/lib/status-variants'
import type { ProjectBuildSort } from './-project-build-sort'

const PROJECT_BUILD_SORTS = [
  'pipeline_name',
  'status',
  'branch',
  'created_at',
] satisfies ReadonlyArray<ProjectBuildSort>

const projectBuildColumns: Array<DataTableColumnDef<Build>> = [
  {
    accessorKey: 'build_number',
    header: 'Build',
    cell: ({ row }) => (
      <Link to="/builds/$buildId" params={{ buildId: row.original.id }}>
        #{row.original.build_number}
      </Link>
    ),
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
]

export function ProjectBuildInventory({
  builds,
  direction,
  filters,
  isLoading,
  onPageChange,
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
  filters: ReactNode
  isLoading: boolean
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
      search={{
        value: query,
        onChange: onSearch,
        placeholder: 'Search by branch',
      }}
      pagination={{ onPageChange, page, pageSize, total }}
      emptyMessage={isLoading ? 'Loading builds…' : 'No matching builds.'}
    />
  )
}
