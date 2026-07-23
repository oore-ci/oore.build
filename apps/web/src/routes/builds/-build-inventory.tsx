import { lazy, Suspense, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { CircleEllipsis as MoreHorizontalCircle01Icon } from 'lucide-react'

import type { Build, Project } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
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
import {
  getRunnerPolicyBlockLabel,
  getStatusVariant,
} from '@/lib/status-variants'

export type BuildSort =
  | 'created_at'
  | 'status'
  | 'project_name'
  | 'pipeline_name'
  | 'branch'

const loadBuildActionsMenu = () => import('./-build-actions-menu')
const BuildActionsMenu = lazy(loadBuildActionsMenu)

function projectName(
  build: Build,
  projectNamesById: ReadonlyMap<string, string>,
) {
  return (
    build.context?.project_name ??
    projectNamesById.get(build.project_id) ??
    'Unknown project'
  )
}

function BuildIdentity({ build }: { build: Build }) {
  return (
    <Link
      to="/builds/$buildId"
      params={{ buildId: build.id }}
      className="group block rounded-sm font-mono outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span className="block text-sm group-hover:underline">
        #{build.build_number}
      </span>
      <span className="block text-[11px] text-muted-foreground">
        {build.id.slice(0, 8)}
      </span>
    </Link>
  )
}

function BuildActionsControl({ build }: { build: Build }) {
  const [requested, setRequested] = useState(false)
  const [open, setOpen] = useState(false)

  const trigger = (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={`Actions for build ${build.build_number}`}
      title="Build actions"
      onMouseEnter={() => void loadBuildActionsMenu()}
      onFocus={() => void loadBuildActionsMenu()}
      onClick={() => {
        setRequested(true)
        setOpen(true)
      }}
    >
      <DynamicLucideIcon icon={MoreHorizontalCircle01Icon} />
    </Button>
  )

  if (!requested) return trigger

  return (
    <Suspense fallback={trigger}>
      <BuildActionsMenu build={build} open={open} onOpenChange={setOpen} />
    </Suspense>
  )
}

export function BuildInventory({
  builds,
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
  builds: Array<Build>
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onSortChange: (sort: BuildSort, direction: SortDirection) => void
  page: number
  pageSize: number
  projects: Array<Project>
  sort: BuildSort
  total: number
}) {
  const projectNamesById = new Map(
    projects.map((project) => [project.id, project.name]),
  )

  return (
    <section aria-label="Build queue and history" className="min-w-0">
      <div className="divide-y sm:hidden">
        {isLoading
          ? Array.from({ length: 5 }, (_, index) => (
              <div key={index} className="space-y-2 py-4">
                <Skeleton className="h-5 w-1/3" />
                <Skeleton className="h-4 w-2/3" />
              </div>
            ))
          : builds.map((build) => (
              <div key={build.id} className="space-y-2 py-4">
                <div className="flex items-start justify-between gap-4">
                  <BuildIdentity build={build} />
                  <div className="flex items-center gap-2">
                    <Badge variant={getStatusVariant(build.status)}>
                      {build.status}
                    </Badge>
                    <BuildActionsControl build={build} />
                  </div>
                </div>
                {build.runner_policy_block_reason ? (
                  <p className="text-xs text-warning">
                    {getRunnerPolicyBlockLabel(
                      build.runner_policy_block_reason,
                    )}
                  </p>
                ) : null}
                <p className="truncate text-sm">
                  {projectName(build, projectNamesById)}
                  {build.context?.pipeline_name ? (
                    <span className="text-muted-foreground">
                      {' · '}
                      {build.context.pipeline_name}
                    </span>
                  ) : null}
                </p>
                <div className="flex items-center justify-between gap-4 text-xs text-muted-foreground">
                  <span className="truncate font-mono">
                    {build.branch ?? 'No branch'}
                  </span>
                  <span className="shrink-0">
                    {relativeTime(build.created_at)}
                  </span>
                </div>
              </div>
            ))}
      </div>

      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Build</TableHead>
              <SortableTableHead
                sort={sort}
                sortKey="project_name"
                direction={direction}
                onSortChange={onSortChange}
              >
                Project
              </SortableTableHead>
              <SortableTableHead
                sort={sort}
                sortKey="status"
                direction={direction}
                onSortChange={onSortChange}
              >
                Status
              </SortableTableHead>
              <TableHead className="hidden lg:table-cell">Trigger</TableHead>
              <SortableTableHead
                className="hidden lg:table-cell"
                sort={sort}
                sortKey="branch"
                direction={direction}
                onSortChange={onSortChange}
              >
                Branch
              </SortableTableHead>
              <TableHead className="hidden lg:table-cell">Commit</TableHead>
              <SortableTableHead
                sort={sort}
                sortKey="created_at"
                direction={direction}
                onSortChange={onSortChange}
              >
                Created
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
                      <Skeleton className="h-8 w-20" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-8 w-36" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-6 w-20" />
                    </TableCell>
                    <TableCell className="hidden lg:table-cell">
                      <Skeleton className="h-6 w-24" />
                    </TableCell>
                    <TableCell className="hidden lg:table-cell">
                      <Skeleton className="h-4 w-24" />
                    </TableCell>
                    <TableCell className="hidden lg:table-cell">
                      <Skeleton className="h-4 w-20" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-4 w-20" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="size-8" />
                    </TableCell>
                  </TableRow>
                ))
              : builds.map((build) => (
                  <TableRow key={build.id}>
                    <TableCell>
                      <BuildIdentity build={build} />
                    </TableCell>
                    <TableCell>
                      <p className="text-sm">
                        {projectName(build, projectNamesById)}
                      </p>
                      {build.context?.pipeline_name ? (
                        <p className="text-xs text-muted-foreground">
                          {build.context.pipeline_name}
                        </p>
                      ) : null}
                    </TableCell>
                    <TableCell>
                      <Badge variant={getStatusVariant(build.status)}>
                        {build.status}
                      </Badge>
                      {build.runner_policy_block_reason ? (
                        <p className="mt-1 text-xs text-warning">
                          {getRunnerPolicyBlockLabel(
                            build.runner_policy_block_reason,
                          )}
                        </p>
                      ) : null}
                    </TableCell>
                    <TableCell className="hidden lg:table-cell">
                      <Badge variant="outline">{build.trigger_type}</Badge>
                    </TableCell>
                    <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                      {build.branch ?? 'n/a'}
                    </TableCell>
                    <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                      {build.commit_sha ? build.commit_sha.slice(0, 10) : 'n/a'}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {relativeTime(build.created_at)}
                    </TableCell>
                    <TableCell>
                      <BuildActionsControl build={build} />
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
