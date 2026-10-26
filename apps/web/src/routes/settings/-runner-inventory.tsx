import * as z from 'zod'
import { useMemo } from 'react'
import type { Runner } from '@oore/client/models'
import {
  DataTable,
  DataTableColumnHeader,
  useDataTable,
  type DataTableColumnDef,
  DataTableRowActionsTrigger,
} from '@/components/data-table'
import {
  dataTableSortingState,
  resolveDataTableSorting,
} from '@/components/data-table-features'
import type { SortDirection } from '@/components/data-table-features'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
} from '@/components/ui/dropdown-menu'
import { Badge } from '@/components/ui/badge'
import { RelativeTime } from '@/components/relative-time'
import { StatusBadge } from '@/components/status-badge'
import {
  runnerCachePurgeRequestedAt,
  runnerCacheStats,
} from '@/hooks/use-runners'
import { formatFileSize } from '@/lib/format-utils'
import { Text } from '@/components/layout/text'
import { Stack } from '@/components/layout/stack'

export type RunnerSort = 'created_at' | 'name' | 'status' | 'last_heartbeat_at'

const RUNNER_SORTS = [
  'name',
  'status',
  'last_heartbeat_at',
] satisfies ReadonlyArray<RunnerSort>

function runnerVersion(runner: Runner) {
  return z.string().safeParse(runner.capabilities.version).data
}

function RunnerLabels({ runner }: { runner: Runner }) {
  const labels = runner.labels ?? []
  if (labels.length === 0) {
    return <span className="text-muted-foreground">None reported</span>
  }
  const custom = new Set(runner.custom_labels ?? [])
  return (
    <div className="flex max-w-56 min-w-40 flex-wrap gap-1">
      {labels.map((label) => (
        <Badge
          key={label}
          variant={custom.has(label) ? 'secondary' : 'outline'}
          className="font-mono"
        >
          {label}
        </Badge>
      ))}
    </div>
  )
}

function percent(used: number, total: number) {
  return total > 0 ? Math.round((used / total) * 100) : 0
}

function RunnerUtilisation({ runner }: { runner: Runner }) {
  const metrics = runner.metrics
  if (!metrics || runner.status === 'offline') {
    return <span className="text-muted-foreground">No recent data</span>
  }
  return (
    <div className="space-y-0.5 text-xs whitespace-nowrap">
      <p>
        CPU {Math.round(metrics.cpu_percent)}% · Memory{' '}
        {percent(metrics.memory_used_bytes, metrics.memory_total_bytes)}%
      </p>
      <p
        className={
          runner.disk_pressure ? 'font-medium text-destructive' : undefined
        }
      >
        {formatFileSize(metrics.disk_free_bytes)} free of{' '}
        {formatFileSize(metrics.disk_total_bytes)}
      </p>
    </div>
  )
}

function RunnerCacheCell({ runner }: { runner: Runner }) {
  const stats = runnerCacheStats(runner)
  const purgePending = runnerCachePurgeRequestedAt(runner) !== undefined
  return (
    <>
      <p>{stats ? formatFileSize(stats.total_bytes) : 'Not reported'}</p>
      {purgePending ? (
        <Text variant="caption">Purge pending</Text>
      ) : stats ? (
        <Text variant="caption">of {formatFileSize(stats.max_bytes)}</Text>
      ) : null}
    </>
  )
}

/**
 * Version, labels and utilisation are reported by newer runners only; the
 * columns exist once any runner on the page reports them, so a fresh instance
 * does not show a table of "Unknown" and "None reported".
 */
function getRunnerColumns({
  canWrite,
  onPurgeCaches,
  onEdit,
  showLabels,
  showUtilisation,
  showVersion,
}: {
  canWrite: boolean
  onPurgeCaches: (runner: Runner) => void
  onEdit: (runner: Runner) => void
  showLabels: boolean
  showUtilisation: boolean
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
          {(row.original.max_concurrent_builds ?? 1) > 1 ? (
            <Text variant="caption">
              {row.original.max_concurrent_builds} builds at once
            </Text>
          ) : null}
        </>
      ),
    },
    {
      accessorKey: 'status',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Status" />
      ),
      cell: ({ row }) => (
        <StatusBadge kind="runner" value={row.original.status} />
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

  if (showLabels) {
    columns.push({
      id: 'labels',
      accessorFn: (runner) => (runner.labels ?? []).join(' '),
      header: 'Labels',
      cell: ({ row }) => <RunnerLabels runner={row.original} />,
      enableSorting: false,
    })
  }

  if (showUtilisation) {
    columns.push({
      id: 'utilisation',
      header: 'Utilisation',
      cell: ({ row }) => <RunnerUtilisation runner={row.original} />,
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
          <DataTableRowActionsTrigger
            label={`Actions for ${row.original.name}`}
          />
          <DropdownMenuContent align="end" className="w-40">
            <DropdownMenuGroup>
              <DropdownMenuItem onClick={() => onEdit(row.original)}>
                Edit
              </DropdownMenuItem>
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
  onEdit,
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
  onEdit: (runner: Runner) => void
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
  const showLabels = runners.some((runner) => (runner.labels ?? []).length > 0)
  const showUtilisation = runners.some((runner) => !!runner.metrics)
  const columns = useMemo(
    () =>
      getRunnerColumns({
        canWrite,
        onPurgeCaches,
        onEdit,
        showLabels,
        showUtilisation,
        showVersion,
      }),
    [canWrite, onPurgeCaches, onEdit, showLabels, showUtilisation, showVersion],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: runners,
    getRowId: (runner) => runner.id,
    // Keep the table within the page now that labels and utilisation show.
    initialState: { columnVisibility: { registered_by: false } },
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, RUNNER_SORTS)
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <Stack aria-label="Runner inventory" fill render={<section />}>
      <DataTable
        table={table}
        search={{
          value: query,
          onChange: onSearch,
          placeholder: 'Search runners',
        }}
        pagination={{ onPageChange, page, pageSize, total }}
        isLoading={isLoading}
        emptyMessage={isLoading ? 'Loading runners…' : undefined}
      />
    </Stack>
  )
}
