import * as z from 'zod'
import { useMemo } from 'react'
import type { Runner } from '@oore/client/models'
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
import { HugeiconsIcon } from '@hugeicons/react'
import { MoreHorizontalCircle01Icon } from '@hugeicons/core-free-icons'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Badge } from '@/components/ui/badge'
import RunnerStatusDot from '@/components/runner-status-dot'
import {
  runnerCachePurgeRequestedAt,
  runnerCacheStats,
} from '@/hooks/use-runners'
import { formatFileSize } from '@/lib/format-utils'
import { getRunnerStatusVariant } from '@/lib/status-variants'

export type RunnerSort = 'created_at' | 'name' | 'status' | 'last_heartbeat_at'

const RUNNER_SORTS = [
  'name',
  'status',
  'last_heartbeat_at',
] satisfies ReadonlyArray<RunnerSort>

function relative(epoch?: number | null) {
  if (!epoch) return 'Never'
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - epoch)
  if (seconds < 60) return `${seconds}s ago`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  return `${Math.floor(seconds / 3600)}h ago`
}

function capabilities(value: Runner['capabilities']) {
  return (
    Object.entries(value)
      .flatMap(([name, enabled]) => (enabled === true ? [name] : []))
      .join(', ') || 'None reported'
  )
}

function RunnerCacheCell({ runner }: { runner: Runner }) {
  const stats = runnerCacheStats(runner)
  const purgePending = runnerCachePurgeRequestedAt(runner) !== undefined
  return (
    <>
      <p>{stats ? formatFileSize(stats.total_bytes) : 'Not reported'}</p>
      {purgePending ? (
        <p className="text-xs text-muted-foreground">Purge pending</p>
      ) : stats ? (
        <p className="text-xs text-muted-foreground">
          of {formatFileSize(stats.max_bytes)}
        </p>
      ) : null}
    </>
  )
}

function getRunnerColumns({
  canWrite,
  onPurgeCaches,
  onRename,
}: {
  canWrite: boolean
  onPurgeCaches: (runner: Runner) => void
  onRename: (runner: Runner) => void
}): Array<DataTableColumnDef<Runner>> {
  const columns: Array<DataTableColumnDef<Runner>> = [
    {
      accessorKey: 'name',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Name" />
      ),
      cell: ({ row }) => (
        <>
          <p className="font-medium">{row.original.name}</p>
          <p className="font-mono text-xs text-muted-foreground">
            {row.original.id.slice(0, 8)}
          </p>
        </>
      ),
    },
    {
      accessorKey: 'status',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Status" />
      ),
      cell: ({ row }) => (
        <div className="flex items-center">
          <RunnerStatusDot status={row.original.status} />
          <Badge variant={getRunnerStatusVariant(row.original.status)}>
            {row.original.status}
          </Badge>
        </div>
      ),
    },
    {
      accessorKey: 'last_heartbeat_at',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Last heartbeat" />
      ),
      cell: ({ row }) => relative(row.original.last_heartbeat_at),
    },
    {
      id: 'version',
      accessorFn: (runner) =>
        z.string().safeParse(runner.capabilities.version).data,
      header: 'Version',
      cell: ({ row }) =>
        z.string().safeParse(row.original.capabilities.version).data ??
        'Unknown',
      enableSorting: false,
    },
    {
      id: 'capabilities',
      accessorFn: (runner) => capabilities(runner.capabilities),
      header: 'Capabilities',
      enableSorting: false,
    },
    {
      id: 'cache',
      accessorFn: (runner) => runnerCacheStats(runner)?.total_bytes,
      header: 'Cache',
      cell: ({ row }) => <RunnerCacheCell runner={row.original} />,
      enableSorting: false,
    },
    {
      accessorKey: 'registered_by',
      header: 'Registered by',
      cell: ({ row }) => row.original.registered_by ?? 'embedded',
      enableSorting: false,
    },
  ]

  if (canWrite) {
    columns.push({
      id: 'actions',
      header: 'Action',
      cell: ({ row }) => (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={<Button variant="ghost" size="icon-xs" />}
          >
            <span className="sr-only">Open menu</span>
            <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-44">
            <DropdownMenuGroup>
              <DropdownMenuLabel>Actions</DropdownMenuLabel>
              {row.original.registered_by ? (
                <DropdownMenuItem onClick={() => onRename(row.original)}>
                  Rename
                </DropdownMenuItem>
              ) : null}
              <DropdownMenuItem
                disabled={
                  runnerCachePurgeRequestedAt(row.original) !== undefined
                }
                onClick={() => onPurgeCaches(row.original)}
              >
                Purge cache
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      ),
      enableHiding: false,
      enableSorting: false,
    })
  }

  return columns
}

export function RunnerInventory({
  canWrite,
  direction,
  isLoading,
  onPageChange,
  onPurgeCaches,
  onRename,
  onSearch,
  onSortChange,
  page,
  pageSize,
  query,
  runners,
  sort,
  total,
}: {
  canWrite: boolean
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onPurgeCaches: (runner: Runner) => void
  onRename: (runner: Runner) => void
  onSearch: (query: string) => void
  onSortChange: (sort: RunnerSort, direction: SortDirection) => void
  page: number
  pageSize: number
  query: string
  runners: Array<Runner>
  sort: RunnerSort
  total: number
}) {
  const columns = useMemo(
    () => getRunnerColumns({ canWrite, onPurgeCaches, onRename }),
    [canWrite, onPurgeCaches, onRename],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: runners,
    getRowId: (runner) => runner.id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, RUNNER_SORTS)
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <section
      aria-label="Runner inventory"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <DataTable
        table={table}
        search={{
          value: query,
          onChange: onSearch,
          placeholder: 'Search runners',
        }}
        pagination={{ onPageChange, page, pageSize, total }}
        emptyMessage={isLoading ? 'Loading runners…' : undefined}
      />
    </section>
  )
}
