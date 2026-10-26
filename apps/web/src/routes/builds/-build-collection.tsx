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
import { StatusBadge } from '@/components/status-badge'
import { BuildSourceValue, CELL_STACK } from '@/components/table-values'
import { relativeTime } from '@/lib/format-utils'
import {
  getStatusPresentation,
  getRunnerPolicyBlockLabel,
  TRIGGER_TYPE_LABELS,
} from '@/lib/status-variants'
import type { Build } from '@oore/client/models'
import { useTableDensity } from '@/stores/table-density-store'
import BuildActionsMenu from './-build-actions-menu'
import {
  BUILD_TRIGGER_FILTER_OPTIONS,
  webhookEventTrigger,
} from './-build-filter-controls'
import type { BuildSort } from './-build-sort'
import { LIST_NAV_ITEM } from '@/lib/keyboard-shortcuts'
import { Stack } from '@/components/layout/stack'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

const BUILD_TABLE_SORTS = [
  'project_name',
  'status',
  'branch',
  'created_at',
] satisfies ReadonlyArray<BuildSort>

/** The webhook event says more than "Webhook" does. */
function buildTriggerLabel(build: Build) {
  if (build.trigger_type === 'webhook' && build.trigger_event) {
    const trigger = webhookEventTrigger(build.trigger_event)
    if (trigger) return BUILD_TRIGGER_FILTER_OPTIONS[trigger]
  }
  return TRIGGER_TYPE_LABELS[build.trigger_type]
}

function projectName(build: Build) {
  return build.context?.project_name ?? 'Unknown project'
}

const columns: Array<DataTableColumnDef<Build>> = [
  {
    accessorKey: 'build_number',
    header: 'Build',
    cell: ({ row }) => (
      <Link
        {...LIST_NAV_ITEM}
        to="/builds/$buildId"
        params={{ buildId: row.original.id }}
        className="font-mono font-medium"
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
      <div className={CELL_STACK}>
        <Link
          to="/projects/$projectId"
          params={{ projectId: row.original.project_id }}
          className="hover:underline"
        >
          {projectName(row.original)}
        </Link>
        {row.original.context?.pipeline_name ? (
          <Link
            to="/projects/$projectId/pipelines/$pipelineId"
            params={{
              projectId: row.original.project_id,
              pipelineId: row.original.pipeline_id,
            }}
            className="text-muted-foreground hover:text-foreground hover:underline"
          >
            {row.original.context.pipeline_name}
          </Link>
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
      <Stack
        gap={0.5}
        className="items-start in-data-[density=compact]:flex-row in-data-[density=compact]:items-center in-data-[density=compact]:gap-2"
      >
        <StatusBadge kind="build" value={row.original.status} />
        {row.original.approval?.state === 'pending' ? (
          <span className="text-xs text-warning">
            {
              getStatusPresentation('approval', row.original.approval.state)
                .label
            }
          </span>
        ) : null}
        {row.original.runner_policy_block_reason ? (
          <span className="text-xs text-warning">
            {getRunnerPolicyBlockLabel(row.original.runner_policy_block_reason)}
          </span>
        ) : null}
      </Stack>
    ),
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
    accessorKey: 'created_at',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Created" />
    ),
    // How and by whom it started reads with when it started, which keeps the
    // table inside an 1100px window without Trigger and Actor columns.
    cell: ({ row }) => (
      <div className={CELL_STACK}>
        <span>{relativeTime(row.original.created_at)}</span>
        <span
          className={cn(
            textVariants({ variant: 'caption' }),
            'max-w-56 truncate',
          )}
        >
          {buildTriggerLabel(row.original)}
          {row.original.trigger_actor ? (
            <span title={row.original.trigger_actor}>
              {' · '}
              {row.original.trigger_actor}
            </span>
          ) : null}
        </span>
      </div>
    ),
  },
  {
    id: 'actions',
    header: () => <span className="sr-only">Actions</span>,
    cell: ({ row }) => <BuildActionsMenu build={row.original} />,
    enableHiding: false,
    enableSorting: false,
  },
]

export function BuildCollection({
  activeFilterCount,
  builds,
  direction,
  emptyState,
  error,
  filters,
  isFiltered,
  isLoading,
  isRefreshing,
  onClearFilters,
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
  activeFilterCount: number
  builds: Array<Build>
  direction: SortDirection
  emptyState: ReactNode
  error: Error | null
  filters: ReactNode
  isFiltered: boolean
  isLoading: boolean
  isRefreshing: boolean
  onClearFilters: () => void
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
  const [density, setDensity] = useTableDensity('builds')
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
          collapseFilters
          density={density}
          onDensityChange={setDensity}
          activeFilterCount={activeFilterCount}
          onClearFilters={isFiltered ? onClearFilters : undefined}
          search={{
            value: query,
            onChange: onSearch,
            placeholder: 'Search builds',
          }}
          pagination={{ onPageChange, page, pageSize, total }}
          isLoading={isLoading}
          emptyMessage={
            isLoading
              ? 'Loading builds…'
              : query
                ? `No builds match “${query}”.`
                : 'No builds match these filters.'
          }
        />
      )}
    </CollectionFrame>
  )
}
