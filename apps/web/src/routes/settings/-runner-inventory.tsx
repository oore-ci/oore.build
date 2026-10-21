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
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { RelativeTime } from '@/components/relative-time'
import RunnerStatusDot from '@/components/runner-status-dot'
import { StatusBadge } from '@/components/status-badge'
import {
  runnerCachePurgeRequestedAt,
  runnerCacheStats,
} from '@/hooks/use-runners'
import { formatFileSize } from '@/lib/format-utils'

export type RunnerSort = 'created_at' | 'name' | 'status' | 'last_heartbeat_at'

const RUNNER_SORTS = [
  'name',
  'status',
  'last_heartbeat_at',
] satisfies ReadonlyArray<RunnerSort>

function runnerVersion(runner: Runner) {
  return z.string().safeParse(runner.capabilities.version).data
}

function capabilityNames(value: Runner['capabilities']) {
  return Object.entries(value).flatMap(([name, enabled]) =>
    enabled === true ? [name] : [],
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

/**
 * Version and capabilities are reported by newer runners only; the columns
 * exist once any runner on the page reports them, so a fresh instance does
 * not show a table of "Unknown" and "None reported".
 */
function getRunnerColumns({
  canWrite,
  onPurgeCaches,
  onRename,
  showCapabilities,
  showVersion,
}: {
  canWrite: boolean
  onPurgeCaches: (runner: Runner) => void
  onRename: (runner: Runner) => void
  showCapabilities: boolean
  showVersion: boolean
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
          <StatusBadge kind="runner" value={row.original.status} />
        </div>
      ),
    },
    {
      accessorKey: 'last_heartbeat_at',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Last heartbeat" />
      ),
      cell: ({ row }) =>
        row.original.last_heartbeat_at ? (
          <RelativeTime at={row.original.last_heartbeat_at} />
        ) : (
          <span className="text-muted-foreground">Never</span>
        ),
    },
  ]

  if (showVersion) {
    columns.push({
      id: 'version',
      accessorFn: runnerVersion,
      header: 'Version',
      cell: ({ row }) => (
        <span className="font-mono text-xs">
          {runnerVersion(row.original) ?? 'Unknown'}
        </span>
      ),
      enableSorting: false,
    })
  }

  if (showCapabilities) {
    columns.push({
      id: 'capabilities',
      accessorFn: (runner) => capabilityNames(runner.capabilities).join(', '),
      header: 'Capabilities',
      cell: ({ row }) =>
        capabilityNames(row.original.capabilities).join(', ') || (
          <span className="text-muted-foreground">None reported</span>
        ),
      enableSorting: false,
    })
  }

  columns.push(
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
      cell: ({ row }) =>
        row.original.registered_by ?? (
          <span className="text-muted-foreground">This Mac</span>
        ),
      enableSorting: false,
    },
  )

  if (canWrite) {
    columns.push({
      id: 'actions',
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) => (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={<Button variant="ghost" size="icon-xs" />}
          >
            <span className="sr-only">Open menu</span>
            <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-40">
            <DropdownMenuGroup>
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
  const showVersion = runners.some((runner) => !!runnerVersion(runner))
  const showCapabilities = runners.some(
    (runner) => capabilityNames(runner.capabilities).length > 0,
  )
  const columns = useMemo(
    () =>
      getRunnerColumns({
        canWrite,
        onPurgeCaches,
        onRename,
        showCapabilities,
        showVersion,
      }),
    [canWrite, onPurgeCaches, onRename, showCapabilities, showVersion],
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
