import type { AuditLogEntry } from '@/lib/types'
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

export type AuditSort =
  'created_at' | 'actor_email' | 'action' | 'resource_type'

export function AuditLogInventory({
  direction,
  entries,
  isLoading,
  onPageChange,
  onPageSizeChange,
  onSortChange,
  page,
  pageSize,
  sort,
  total,
}: {
  direction: SortDirection
  entries: Array<AuditLogEntry>
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onSortChange: (sort: AuditSort, direction: SortDirection) => void
  page: number
  pageSize: number
  sort: AuditSort
  total: number
}) {
  return (
    <section aria-label="Audit activity" className="min-w-0">
      <ul className="divide-y sm:hidden">
        {isLoading
          ? Array.from({ length: 5 }, (_, index) => (
              <li key={index} className="space-y-2 py-4">
                <Skeleton className="h-5 w-2/3" />
                <Skeleton className="h-4 w-1/2" />
                <Skeleton className="h-4 w-full" />
              </li>
            ))
          : entries.map((entry) => (
              <li key={entry.id} className="space-y-2 py-4">
                <div className="flex items-start justify-between gap-3">
                  <Badge variant="outline" className="min-w-0 truncate">
                    {entry.action}
                  </Badge>
                  <time
                    dateTime={new Date(entry.created_at * 1000).toISOString()}
                    className="shrink-0 text-xs text-muted-foreground"
                  >
                    {relativeTime(entry.created_at)}
                  </time>
                </div>
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="secondary">{entry.resource_type}</Badge>
                  <span className="truncate font-mono">
                    {entry.resource_id
                      ? entry.resource_id.slice(0, 8)
                      : 'No resource ID'}
                  </span>
                </div>
                <p className="truncate text-sm">
                  {entry.actor_email ?? 'System'}
                </p>
                {entry.details ? (
                  <p className="line-clamp-2 text-xs text-muted-foreground">
                    {entry.details}
                  </p>
                ) : null}
              </li>
            ))}
      </ul>

      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              <SortableTableHead
                sort={sort}
                sortKey="created_at"
                direction={direction}
                onSortChange={onSortChange}
              >
                Time
              </SortableTableHead>
              <SortableTableHead
                sort={sort}
                sortKey="actor_email"
                direction={direction}
                onSortChange={onSortChange}
              >
                Actor
              </SortableTableHead>
              <SortableTableHead
                sort={sort}
                sortKey="action"
                direction={direction}
                onSortChange={onSortChange}
              >
                Action
              </SortableTableHead>
              <SortableTableHead
                sort={sort}
                sortKey="resource_type"
                direction={direction}
                onSortChange={onSortChange}
              >
                Resource
              </SortableTableHead>
              <TableHead className="hidden lg:table-cell">
                Resource ID
              </TableHead>
              <TableHead className="hidden lg:table-cell">Details</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading
              ? Array.from({ length: 5 }, (_, index) => (
                  <TableRow key={index}>
                    <TableCell>
                      <Skeleton className="h-4 w-20" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-4 w-32" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-6 w-28" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-6 w-20" />
                    </TableCell>
                    <TableCell className="hidden lg:table-cell">
                      <Skeleton className="h-4 w-16" />
                    </TableCell>
                    <TableCell className="hidden lg:table-cell">
                      <Skeleton className="h-4 w-40" />
                    </TableCell>
                  </TableRow>
                ))
              : entries.map((entry) => (
                  <TableRow key={entry.id}>
                    <TableCell className="whitespace-nowrap text-sm text-muted-foreground">
                      <time
                        dateTime={new Date(
                          entry.created_at * 1000,
                        ).toISOString()}
                      >
                        {relativeTime(entry.created_at)}
                      </time>
                    </TableCell>
                    <TableCell className="max-w-40 truncate text-sm">
                      {entry.actor_email ?? (
                        <span className="text-muted-foreground">System</span>
                      )}
                    </TableCell>
                    <TableCell className="max-w-48">
                      <Badge variant="outline" className="max-w-full truncate">
                        {entry.action}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{entry.resource_type}</Badge>
                    </TableCell>
                    <TableCell className="hidden font-mono text-[11px] text-muted-foreground lg:table-cell">
                      {entry.resource_id ? entry.resource_id.slice(0, 8) : '—'}
                    </TableCell>
                    <TableCell className="hidden max-w-xs truncate text-xs text-muted-foreground lg:table-cell">
                      {entry.details ?? '—'}
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
