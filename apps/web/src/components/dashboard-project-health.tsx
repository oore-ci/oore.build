import { useMemo, useState } from 'react'
import { Link } from '@tanstack/react-router'

import type { ProjectBuildInsights } from '@oore/client/models'
import {
  DataTable,
  DataTableColumnHeader,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import {
  dataTableSortingState,
  resolveDataTableSorting,
  type SortDirection,
} from '@/components/data-table-features'
import {
  DashboardSection,
  DashboardSectionError,
  DashboardViewAll,
} from '@/components/dashboard-section'
import { Skeleton } from '@/components/ui/skeleton'
import { StatusBadge } from '@/components/status-badge'
import { useBuildInsights, type BuildInsightsWindow } from '@/hooks/use-builds'
import { formatDuration, relativeTime } from '@/lib/format-utils'

const SORTS = [
  'project',
  'success_rate',
  'builds',
  'duration',
  'last_build',
] as const
type ProjectSort = (typeof SORTS)[number]

const SORT_VALUE = {
  project: (project) => project.project_name.toLowerCase(),
  success_rate: (project) => project.totals.success_rate ?? -1,
  builds: (project) => project.totals.finished,
  duration: (project) => project.duration.p50_seconds ?? -1,
  last_build: (project) => project.last_finished_at,
} satisfies Record<
  ProjectSort,
  (project: ProjectBuildInsights) => string | number
>

function sortProjects(
  projects: Array<ProjectBuildInsights>,
  sort: ProjectSort,
  direction: SortDirection,
) {
  const value = SORT_VALUE[sort]
  const sign = direction === 'asc' ? 1 : -1
  return [...projects].sort((a, b) => {
    const left = value(a)
    const right = value(b)
    return (left < right ? -1 : left > right ? 1 : 0) * sign
  })
}

function SuccessRate({ project }: { project: ProjectBuildInsights }) {
  const rate = project.totals.success_rate
  if (rate == null) {
    return <span className="text-muted-foreground">—</span>
  }
  const percent = Math.round(rate * 100)
  return (
    <div className="flex items-center gap-2">
      <div
        className="h-1.5 w-20 overflow-hidden rounded-full bg-muted"
        aria-hidden
      >
        <div
          className="h-full rounded-full bg-foreground/70"
          style={{ width: `${percent}%` }}
        />
      </div>
      <span className="text-xs tabular-nums">{percent}%</span>
    </div>
  )
}

const columns: Array<DataTableColumnDef<ProjectBuildInsights>> = [
  {
    id: 'project',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Project" />
    ),
    cell: ({ row }) => (
      <Link
        to="/projects/$projectId"
        params={{ projectId: row.original.project_id }}
        className="font-medium underline-offset-4 hover:underline"
      >
        {row.original.project_name}
      </Link>
    ),
    enableHiding: false,
  },
  {
    id: 'success_rate',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Success rate" />
    ),
    cell: ({ row }) => <SuccessRate project={row.original} />,
  },
  {
    id: 'builds',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Builds" />
    ),
    cell: ({ row }) => {
      const { totals } = row.original
      const failures = totals.failed + totals.timed_out
      return (
        <span className="text-xs whitespace-nowrap tabular-nums">
          {totals.finished}
          {failures > 0 ? (
            <span className="text-muted-foreground"> · {failures} failed</span>
          ) : null}
        </span>
      )
    },
  },
  {
    id: 'duration',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Median time" />
    ),
    cell: ({ row }) => (
      <span className="text-xs tabular-nums">
        {row.original.duration.p50_seconds == null
          ? '—'
          : formatDuration(row.original.duration.p50_seconds)}
      </span>
    ),
  },
  {
    id: 'last_build',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Last build" />
    ),
    cell: ({ row }) => (
      <div className="flex items-center gap-2 whitespace-nowrap">
        <StatusBadge kind="build" value={row.original.last_status} />
        <span className="text-xs text-muted-foreground">
          {relativeTime(row.original.last_finished_at)}
        </span>
      </div>
    ),
  },
]

/** Per-project build health over the selected period. */
export function DashboardProjectHealth({
  days,
}: {
  days: BuildInsightsWindow
}) {
  const insightsQuery = useBuildInsights({ days })
  const [sort, setSort] = useState<ProjectSort>('builds')
  const [direction, setDirection] = useState<SortDirection>('desc')
  const projects = useMemo(
    () => sortProjects(insightsQuery.data?.projects ?? [], sort, direction),
    [direction, insightsQuery.data?.projects, sort],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: projects,
    getRowId: (project) => project.project_id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, SORTS)
      if (!next) return
      setSort(next.sort)
      setDirection(next.direction)
    },
  })

  // An idle period has nothing to rank.
  if (insightsQuery.isSuccess && projects.length === 0) return null

  return (
    <DashboardSection
      title="Project health"
      scope={`Last ${days} days`}
      collapsedOnMobile
      action={<DashboardViewAll to="/projects" />}
    >
      {insightsQuery.error && !insightsQuery.data ? (
        <DashboardSectionError
          label="Project health"
          onRetry={() => void insightsQuery.refetch()}
        />
      ) : !insightsQuery.data ? (
        <Skeleton className="h-48 w-full" />
      ) : (
        <DataTable table={table} />
      )}
    </DashboardSection>
  )
}
