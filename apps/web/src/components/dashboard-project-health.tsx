import { Link } from '@tanstack/react-router'

import type { ProjectBuildInsights } from '@oore/client/models'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import { StatusBadge } from '@/components/status-badge'
import { useBuildInsights, type BuildInsightsWindow } from '@/hooks/use-builds'
import { formatDuration, relativeTime } from '@/lib/format-utils'

function SuccessRate({ project }: { project: ProjectBuildInsights }) {
  const rate = project.totals.success_rate
  if (rate == null) {
    return <span className="text-muted-foreground">—</span>
  }
  const percent = Math.round(rate * 100)
  return (
    <div className="flex items-center gap-2">
      <div
        className="h-1.5 w-20 overflow-hidden rounded-full bg-destructive/25"
        aria-hidden
      >
        <div
          className="h-full rounded-full bg-success"
          style={{ width: `${percent}%` }}
        />
      </div>
      <span className="font-mono text-xs tabular-nums">{percent}%</span>
    </div>
  )
}

const columns: Array<DataTableColumnDef<ProjectBuildInsights>> = [
  {
    id: 'project',
    header: 'Project',
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
    enableSorting: false,
  },
  {
    id: 'success_rate',
    header: 'Success rate',
    cell: ({ row }) => <SuccessRate project={row.original} />,
    enableSorting: false,
  },
  {
    id: 'builds',
    header: 'Builds',
    cell: ({ row }) => {
      const { totals } = row.original
      const failures = totals.failed + totals.timed_out
      return (
        <span className="font-mono text-xs tabular-nums">
          {totals.finished}
          {failures > 0 ? (
            <span className="text-muted-foreground"> · {failures} failed</span>
          ) : null}
        </span>
      )
    },
    enableSorting: false,
  },
  {
    id: 'duration',
    header: 'Median time',
    cell: ({ row }) => (
      <span className="font-mono text-xs tabular-nums">
        {row.original.duration.p50_seconds == null
          ? '—'
          : formatDuration(row.original.duration.p50_seconds)}
      </span>
    ),
    enableSorting: false,
  },
  {
    id: 'last_build',
    header: 'Last build',
    cell: ({ row }) => (
      <div className="flex items-center gap-2">
        <StatusBadge kind="build" value={row.original.last_status} />
        <span className="text-xs text-muted-foreground">
          {relativeTime(row.original.last_finished_at)}
        </span>
      </div>
    ),
    enableSorting: false,
  },
]

/** Per-project build health over the selected period. */
export function DashboardProjectHealth({
  days,
}: {
  days: BuildInsightsWindow
}) {
  const insightsQuery = useBuildInsights({ days })
  const projects = insightsQuery.data?.projects ?? []
  const table = useDataTable({
    columns,
    data: projects,
    getRowId: (project) => project.project_id,
  })

  // Build health already reports load failures; an idle period has nothing to rank.
  if (!insightsQuery.data || projects.length === 0) return null

  return (
    <section className="flex flex-col gap-1" aria-labelledby="project-health">
      <h2
        id="project-health"
        className="text-sm font-medium text-muted-foreground"
      >
        Projects
      </h2>
      <DataTable table={table} />
    </section>
  )
}
