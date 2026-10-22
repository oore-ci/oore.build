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
import { EmptyValue } from '@/components/table-values'
import { relativeTime } from '@/lib/format-utils'
import type { Project } from '@oore/client/models'
import ProjectActionsMenu from './-project-actions-menu'

export type ProjectSort = 'created_at' | 'updated_at' | 'name'

const PROJECT_TABLE_SORTS = [
  'name',
  'updated_at',
] satisfies ReadonlyArray<ProjectSort>

function ProjectIdentity({ project }: { project: Project }) {
  return (
    <div className="flex min-w-0 items-center gap-3">
      <RepositoryAvatar
        fullName={project.repository_full_name ?? project.name}
        avatarUrl={project.repository_avatar_url}
        repositoryId={project.repository_id}
        provider={project.repository_provider}
      />
      <span className="min-w-0">
        <Link
          to="/projects/$projectId"
          params={{ projectId: project.id }}
          className="block truncate font-medium"
        >
          {project.name}
        </Link>
        <span className="block truncate text-xs text-muted-foreground">
          {project.repository_full_name ?? 'Local repository'}
        </span>
      </span>
    </div>
  )
}

function getProjectColumns(
  canManageProject: (project: Project) => boolean,
): Array<DataTableColumnDef<Project>> {
  return [
    {
      accessorKey: 'name',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Project" />
      ),
      cell: ({ row }) => <ProjectIdentity project={row.original} />,
    },
    {
      accessorKey: 'default_branch',
      header: 'Default branch',
      cell: ({ row }) => row.original.default_branch ?? <EmptyValue />,
      enableSorting: false,
    },
    {
      accessorKey: 'description',
      header: 'Description',
      cell: ({ row }) => row.original.description || <EmptyValue />,
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
  const columns = useMemo(
    () => getProjectColumns(canManageProject),
    [canManageProject],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: projects,
    getRowId: (project) => project.id,
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
