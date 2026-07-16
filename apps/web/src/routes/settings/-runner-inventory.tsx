import type { Runner } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import { Button } from '@/components/ui/button'
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
import RunnerStatusDot from '@/components/runner-status-dot'
import { getRunnerStatusVariant } from '@/lib/status-variants'

export type RunnerSort = 'created_at' | 'name' | 'status' | 'last_heartbeat_at'

function relative(epoch?: number) {
  if (!epoch) return 'Never'
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - epoch)
  if (seconds < 60) return `${seconds}s ago`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  return `${Math.floor(seconds / 3600)}h ago`
}

function capabilities(value: Runner['capabilities']) {
  return (
    Object.entries(value)
      .flatMap(([name, enabled]) => (enabled === true ? [name] : []))
      .join(', ') || 'None reported'
  )
}

export function RunnerInventory({
  canWrite,
  direction,
  isLoading,
  onPageChange,
  onPageSizeChange,
  onRename,
  onSortChange,
  page,
  pageSize,
  runners,
  sort,
  total,
}: {
  canWrite: boolean
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (size: number) => void
  onRename: (runner: Runner) => void
  onSortChange: (sort: RunnerSort, direction: SortDirection) => void
  page: number
  pageSize: number
  runners: Array<Runner>
  sort: RunnerSort
  total: number
}) {
  return (
    <section aria-label="Runner inventory" className="min-w-0">
      <div className="divide-y sm:hidden">
        {isLoading
          ? Array.from({ length: 4 }, (_, index) => (
              <Skeleton key={index} className="my-4 h-16 w-full" />
            ))
          : runners.map((runner) => (
              <article key={runner.id} className="space-y-3 py-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h2 className="truncate font-medium">{runner.name}</h2>
                    <p className="truncate font-mono text-xs text-muted-foreground">
                      {runner.id.slice(0, 8)}
                    </p>
                  </div>
                  <div className="flex items-center">
                    <RunnerStatusDot status={runner.status} />
                    <Badge variant={getRunnerStatusVariant(runner.status)}>
                      {runner.status}
                    </Badge>
                  </div>
                </div>
                <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                  <span>Heartbeat {relative(runner.last_heartbeat_at)}</span>
                  <span>{runner.registered_by ?? 'Embedded runner'}</span>
                </div>
                {canWrite && runner.registered_by ? (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => onRename(runner)}
                  >
                    Rename
                  </Button>
                ) : null}
              </article>
            ))}
      </div>
      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              {(['name', 'status', 'last_heartbeat_at'] as const).map((key) => (
                <SortableTableHead
                  key={key}
                  sort={sort}
                  sortKey={key}
                  direction={direction}
                  onSortChange={onSortChange}
                >
                  {key === 'last_heartbeat_at'
                    ? 'Last heartbeat'
                    : key[0].toUpperCase() + key.slice(1)}
                </SortableTableHead>
              ))}
              <TableHead className="hidden lg:table-cell">Version</TableHead>
              <TableHead className="hidden lg:table-cell">
                Capabilities
              </TableHead>
              <TableHead className="hidden lg:table-cell">
                Registered by
              </TableHead>
              {canWrite ? (
                <TableHead className="text-right">Action</TableHead>
              ) : null}
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading
              ? Array.from({ length: 5 }, (_row, index) => (
                  <TableRow key={index}>
                    {Array.from(
                      { length: canWrite ? 7 : 6 },
                      (_column, cell) => (
                        <TableCell key={cell}>
                          <Skeleton className="h-6 w-20" />
                        </TableCell>
                      ),
                    )}
                  </TableRow>
                ))
              : runners.map((runner) => (
                  <TableRow key={runner.id}>
                    <TableCell>
                      <p className="font-medium">{runner.name}</p>
                      <p className="font-mono text-xs text-muted-foreground">
                        {runner.id.slice(0, 8)}
                      </p>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center">
                        <RunnerStatusDot status={runner.status} />
                        <Badge variant={getRunnerStatusVariant(runner.status)}>
                          {runner.status}
                        </Badge>
                      </div>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {relative(runner.last_heartbeat_at)}
                    </TableCell>
                    <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                      {typeof runner.capabilities.version === 'string'
                        ? runner.capabilities.version
                        : 'Unknown'}
                    </TableCell>
                    <TableCell className="hidden text-xs text-muted-foreground lg:table-cell">
                      {capabilities(runner.capabilities)}
                    </TableCell>
                    <TableCell className="hidden text-sm text-muted-foreground lg:table-cell">
                      {runner.registered_by ?? 'embedded'}
                    </TableCell>
                    {canWrite ? (
                      <TableCell className="text-right">
                        {runner.registered_by ? (
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={() => onRename(runner)}
                          >
                            Rename
                          </Button>
                        ) : (
                          <span className="text-xs text-muted-foreground">
                            Managed by daemon
                          </span>
                        )}
                      </TableCell>
                    ) : null}
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
