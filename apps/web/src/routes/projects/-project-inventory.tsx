import { Link } from '@tanstack/react-router'

import type { Project } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import RepositoryAvatar from '@/components/repository-avatar'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { relativeTime } from '@/lib/format-utils'

export type ProjectSort = 'created_at' | 'updated_at' | 'name'

function ProjectIdentity({ project }: { project: Project }) {
  return (
    <Link
      to="/projects/$projectId"
      params={{ projectId: project.id }}
      className="group flex min-w-0 items-center gap-3 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      {project.repository_full_name ? (
        <RepositoryAvatar
          fullName={project.repository_full_name}
          avatarUrl={project.repository_avatar_url}
          repositoryId={project.repository_id}
          provider={project.repository_provider}
        />
      ) : null}
      <span className="min-w-0">
        <span className="block truncate font-medium group-hover:underline">
          {project.name}
        </span>
        <span className="block truncate font-mono text-[11px] text-muted-foreground">
          {project.repository_full_name ?? project.id.slice(0, 8)}
        </span>
      </span>
    </Link>
  )
}

export function ProjectInventory({
  direction,
  isLoading,
  onPageChange,
  onPageSizeChange,
  onSortChange,
  page,
  pageSize,
  projects,
  sort,
  total,
}: {
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onSortChange: (sort: ProjectSort, direction: SortDirection) => void
  page: number
  pageSize: number
  projects: Array<Project>
  sort: ProjectSort
  total: number
}) {
  return (
    <section aria-label="Project inventory" className="min-w-0">
      <div className="divide-y sm:hidden">
        {isLoading
          ? Array.from({ length: 4 }, (_, index) => (
              <div key={index} className="space-y-2 py-4">
                <Skeleton className="h-5 w-2/3" />
                <Skeleton className="h-4 w-1/2" />
              </div>
            ))
          : projects.map((project) => (
              <div key={project.id} className="space-y-2 py-4">
                <ProjectIdentity project={project} />
                <div className="flex items-center justify-between gap-4 pl-11 text-xs text-muted-foreground">
                  <span className="truncate font-mono">
                    {project.default_branch ?? 'Branch not set'}
                  </span>
                  <span className="shrink-0">
                    {relativeTime(project.updated_at)}
                  </span>
                </div>
              </div>
            ))}
      </div>

      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              <SortableTableHead
                sort={sort}
                sortKey="name"
                direction={direction}
                onSortChange={onSortChange}
              >
                Project
              </SortableTableHead>
              <TableHead>Default branch</TableHead>
              <TableHead className="hidden lg:table-cell">
                Description
              </TableHead>
              <SortableTableHead
                sort={sort}
                sortKey="updated_at"
                direction={direction}
                onSortChange={onSortChange}
              >
                Updated
              </SortableTableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading
              ? Array.from({ length: 5 }, (_, index) => (
                  <TableRow key={index}>
                    <TableCell>
                      <Skeleton className="h-8 w-48" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-4 w-24" />
                    </TableCell>
                    <TableCell className="hidden lg:table-cell">
                      <Skeleton className="h-4 w-40" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-4 w-20" />
                    </TableCell>
                  </TableRow>
                ))
              : projects.map((project) => (
                  <TableRow key={project.id}>
                    <TableCell>
                      <ProjectIdentity project={project} />
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {project.default_branch ?? 'not set'}
                    </TableCell>
                    <TableCell className="hidden max-w-[30ch] truncate text-sm text-muted-foreground lg:table-cell">
                      {project.description ?? 'No description'}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {relativeTime(project.updated_at)}
                    </TableCell>
                  </TableRow>
                ))}
          </TableBody>
        </Table>
      </div>

      {!isLoading ? (
        <CollectionPagination
          page={page}
          pageSize={pageSize}
          total={total}
          onPageChange={onPageChange}
          onPageSizeChange={onPageSizeChange}
        />
      ) : null}
    </section>
  )
}
