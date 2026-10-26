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
import RepositoryAvatar from '@/components/repository-avatar'
import { StatusBadge } from '@/components/status-badge'
import { usePipelines } from '@/hooks/use-pipelines'
import { EmptyValue } from '@/components/table-values'
import { relativeTime } from '@/lib/format-utils'
import type { Project, ProjectBuildInsights } from '@oore/client/models'
import ProjectActionsMenu from './-project-actions-menu'
import { LIST_NAV_ITEM } from '@/lib/keyboard-shortcuts'
import { Row } from '@/components/layout/row'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

export type ProjectSort = 'created_at' | 'updated_at' | 'name'

const PROJECT_TABLE_SORTS = [
  'name',
  'updated_at',
] satisfies ReadonlyArray<ProjectSort>

function ProjectIdentity({ project }: { project: Project }) {
  return (
    <Row gap={3} className="min-w-0">
      <RepositoryAvatar
        fullName={project.repository_full_name ?? project.name}
        avatarUrl={project.repository_avatar_url}
        repositoryId={project.repository_id}
        provider={project.repository_provider}
      />
      <span className="min-w-0">
        <Link
          {...LIST_NAV_ITEM}
          to="/projects/$projectId"
          params={{ projectId: project.id }}
          className="block truncate font-medium"
        >
          {project.name}
        </Link>
        <span
          className={cn(textVariants({ variant: 'caption' }), 'block truncate')}
        >
          {project.repository_full_name ?? 'Local repository'}
        </span>
      </span>
    </Row>
  )
}

/** Newest finished build in the insights window, with its success rate. */
function ProjectLastBuild({
  insights,
  isError,
  isLoading,
  projectId,
}: {
  insights: ProjectBuildInsights | undefined
  isError: boolean
  isLoading: boolean
  projectId: string
}) {
  if (!insights) {
    return (
      <span className="text-muted-foreground">
        {isLoading ? '…' : isError ? 'Unavailable' : 'No recent builds'}
      </span>
    )
  }
  const rate = insights.totals.success_rate
  return (
    <Link
      to="/projects/$projectId"
      params={{ projectId }}
      search={{ tab: 'builds' }}
      className="flex flex-col items-start gap-0.5"
      aria-label={`Builds for ${insights.project_name}`}
    >
      <StatusBadge kind="build" value={insights.last_status} />
      <span className={textVariants({ variant: 'caption' })}>
        {relativeTime(insights.last_finished_at)}
        {rate != null ? ` · ${Math.round(rate * 100)}% succeeded` : ''}
      </span>
    </Link>
  )
}

function ProjectPipelineCount({ projectId }: { projectId: string }) {
  const pipelinesQuery = usePipelines(projectId, { limit: 1 })
  const total = pipelinesQuery.data?.total
  if (total === undefined) {
    return (
      <span className="text-muted-foreground">
        {pipelinesQuery.isLoading ? '…' : 'Unavailable'}
      </span>
    )
  }
  return (
    <Link
      to="/projects/$projectId"
      params={{ projectId }}
      search={{ tab: 'pipelines' }}
      className={
        total === 0
          ? 'text-muted-foreground underline-offset-4 hover:underline'
          : 'tabular-nums underline-offset-4 hover:underline'
      }
    >
      {total === 0 ? 'None' : total}
    </Link>
  )
}

function getProjectColumns(
  canManageProject: (project: Project) => boolean,
  insightsByProject: ReadonlyMap<string, ProjectBuildInsights>,
  insightsLoading: boolean,
  insightsError: boolean,
): Array<DataTableColumnDef<Project>> {
  return [
    {
      accessorKey: 'name',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Project" />
      ),
      cell: ({ row }) => <ProjectIdentity project={row.original} />,
      enableHiding: false,
    },
    {
      id: 'last_build',
      header: 'Last build',
      cell: ({ row }) => (
        <ProjectLastBuild
          insights={insightsByProject.get(row.original.id)}
          isError={insightsError}
          isLoading={insightsLoading}
          projectId={row.original.id}
        />
      ),
      enableSorting: false,
    },
    {
      id: 'pipelines',
      header: 'Pipelines',
      cell: ({ row }) => <ProjectPipelineCount projectId={row.original.id} />,
      enableSorting: false,
    },
    {
      accessorKey: 'default_branch',
      header: 'Default branch',
      cell: ({ row }) =>
        row.original.default_branch ? (
          <span className="font-mono text-xs">
            {row.original.default_branch}
          </span>
        ) : (
          <EmptyValue />
        ),
      enableSorting: false,
    },
    {
      accessorKey: 'description',
      header: 'Description',
      cell: ({ row }) =>
        row.original.description ? (
          <span className="line-clamp-2 max-w-80 whitespace-normal">
            {row.original.description}
          </span>
        ) : (
          <EmptyValue />
        ),
      enableSorting: false,
    },
    {
      accessorKey: 'updated_at',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Updated" />
      ),
      cell: ({ row }) => relativeTime(row.original.updated_at),
    },
    {
      id: 'actions',
      cell: ({ row }) => (
        <ProjectActionsMenu
          canManage={canManageProject(row.original)}
          project={row.original}
        />
      ),
      enableHiding: false,
      enableSorting: false,
    },
  ]
}

export function ProjectCollection({
  canManageProject,
  direction,
  emptyState,
  error,
  insights,
  insightsError,
  insightsLoading,
  isLoading,
  isRefreshing,
  onPageChange,
  onRetry,
  onSearch,
  onSortChange,
  page,
  pageSize,
  projects,
  query,
  sort,
  total,
}: {
  canManageProject: (project: Project) => boolean
  direction: SortDirection
  emptyState: ReactNode
  error: Error | null
  /** Build health per project for the insights window. */
  insights: ReadonlyArray<ProjectBuildInsights> | undefined
  insightsError: boolean
  insightsLoading: boolean
  isLoading: boolean
  isRefreshing: boolean
  onPageChange: (page: number) => void
  onRetry: () => void
  onSearch: (query: string) => void
  onSortChange: (sort: ProjectSort, direction: SortDirection) => void
  page: number
  pageSize: number
  projects: Array<Project>
  query: string
  sort: ProjectSort
  total: number
}) {
  const insightsByProject = useMemo(
    () =>
      new Map((insights ?? []).map((project) => [project.project_id, project])),
    [insights],
  )
  const columns = useMemo(
    () =>
      getProjectColumns(
        canManageProject,
        insightsByProject,
        insightsLoading,
        insightsError,
      ),
    [canManageProject, insightsByProject, insightsLoading, insightsError],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: projects,
    getRowId: (project) => project.id,
    // Description stays one click away in Columns so status gets the width.
    initialState: { columnVisibility: { description: false } },
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(
        updater,
        sorting,
        PROJECT_TABLE_SORTS,
      )
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <CollectionFrame ariaLabel="Projects" isBusy={isLoading || isRefreshing}>
      {error ? (
        <CollectionError
          title="Projects could not be loaded"
          description={error.message}
          onRetry={onRetry}
        />
      ) : null}

      {!isLoading && total === 0 && !error && !query ? (
        emptyState
      ) : (
        <DataTable
          table={table}
          search={{
            value: query,
            onChange: onSearch,
            placeholder: 'Search projects',
          }}
          pagination={{ onPageChange, page, pageSize, total }}
          isLoading={isLoading}
          emptyMessage={isLoading ? 'Loading projects…' : undefined}
        />
      )}
    </CollectionFrame>
  )
}
