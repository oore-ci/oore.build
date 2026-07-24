import { lazy, Suspense, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { CircleEllipsis as MoreHorizontalCircle01Icon } from 'lucide-react'

import type { AuthorizedProject, Project } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import RepositoryAvatar from '@/components/repository-avatar'
import { Skeleton } from '@/components/ui/skeleton'
import { Button } from '@/components/ui/button'
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

const loadProjectActionsMenu = () => import('./-project-actions-menu')
const ProjectActionsMenu = lazy(loadProjectActionsMenu)

function ProjectIdentity({ project }: { project: Project }) {
  return (
    <Link
      to="/projects/$projectId"
      params={{ projectId: project.id }}
      className="group flex min-w-0 items-center gap-3 rounded-md outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
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

function ProjectActionsControl({
  canManage,
  project,
}: {
  canManage: boolean
  project: Project
}) {
  const [requested, setRequested] = useState(false)
  const [open, setOpen] = useState(false)

  const trigger = (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={`Actions for ${project.name}`}
      title="Project actions"
      onMouseEnter={() => void loadProjectActionsMenu()}
      onFocus={() => void loadProjectActionsMenu()}
      onClick={() => {
        setRequested(true)
        setOpen(true)
      }}
    >
      <MoreHorizontalCircle01Icon />
    </Button>
  )

  if (!requested) return trigger

  return (
    <Suspense fallback={trigger}>
      <ProjectActionsMenu
        canManage={canManage}
        open={open}
        onOpenChange={setOpen}
        project={project}
      />
    </Suspense>
  )
}

export function ProjectInventory({
  canManageProject,
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
  canManageProject: (project: AuthorizedProject) => boolean
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onSortChange: (sort: ProjectSort, direction: SortDirection) => void
  page: number
  pageSize: number
  projects: Array<AuthorizedProject>
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
                <div className="flex items-start justify-between gap-3">
                  <ProjectIdentity project={project} />
                  <ProjectActionsControl
                    canManage={canManageProject(project)}
                    project={project}
                  />
                </div>
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
              <TableHead className="w-10">
                <span className="sr-only">Actions</span>
              </TableHead>
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
                    <TableCell>
                      <Skeleton className="size-8" />
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
                    <TableCell>
                      <ProjectActionsControl
                        canManage={canManageProject(project)}
                        project={project}
                      />
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
