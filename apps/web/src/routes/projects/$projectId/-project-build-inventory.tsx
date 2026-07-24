import { Link } from '@tanstack/react-router'

import type { Build } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import { Badge } from '@/components/ui/badge'
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
import { getStatusVariant } from '@/lib/status-variants'
import type { ProjectBuildSort } from './-project-build-sort'

function BuildIdentity({ build }: { build: Build }) {
  return (
    <Link
      to="/builds/$buildId"
      params={{ buildId: build.id }}
      className="group block rounded-md font-mono outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
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

export function ProjectBuildInventory({
  builds,
  direction,
  isLoading,
  onPageChange,
  onPageSizeChange,
  onSortChange,
  page,
  pageSize,
  sort,
  total,
}: {
  builds: Array<Build>
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onSortChange: (sort: ProjectBuildSort, direction: SortDirection) => void
  page: number
  pageSize: number
  sort: ProjectBuildSort
  total: number
}) {
  return (
    <section aria-label="Project build history" className="min-w-0">
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
                  <Badge variant={getStatusVariant(build.status)}>
                    {build.status}
                  </Badge>
                </div>
                {build.context?.pipeline_name ? (
                  <p className="truncate text-sm">
                    {build.context.pipeline_name}
                  </p>
                ) : null}
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
                sortKey="pipeline_name"
                direction={direction}
                onSortChange={onSortChange}
              >
                Pipeline
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
                      <Skeleton className="h-8 w-32" />
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
                  </TableRow>
                ))
              : builds.map((build) => (
                  <TableRow key={build.id}>
                    <TableCell>
                      <BuildIdentity build={build} />
                    </TableCell>
                    <TableCell>
                      <p className="text-sm">
                        {build.context?.pipeline_name ?? 'Unknown pipeline'}
                      </p>
                    </TableCell>
                    <TableCell>
                      <Badge variant={getStatusVariant(build.status)}>
                        {build.status}
                      </Badge>
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
