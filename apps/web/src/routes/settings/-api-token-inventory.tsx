import type { ApiTokenSummary } from '@/lib/types'
import type { ApiTokenSort } from './api-tokens'
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

const roles: Record<string, string> = {
  owner: 'Owner',
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

function status(token: ApiTokenSummary) {
  if (token.is_revoked) return 'revoked'
  if (token.expires_at && token.expires_at * 1000 < Date.now()) return 'expired'
  return 'active'
}

function relative(epoch?: number | null) {
  if (!epoch) return 'Never'
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - epoch)
  if (seconds < 60) return `${seconds}s ago`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`
  return `${Math.floor(seconds / 86400)}d ago`
}

export function ApiTokenInventory({
  canDelete,
  direction,
  isLoading,
  onPageChange,
  onPageSizeChange,
  onRevoke,
  onSortChange,
  page,
  pageSize,
  sort,
  tokens,
  total,
}: {
  canDelete: boolean
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (size: number) => void
  onRevoke: (token: ApiTokenSummary) => void
  onSortChange: (sort: ApiTokenSort, direction: SortDirection) => void
  page: number
  pageSize: number
  sort: ApiTokenSort
  tokens: Array<ApiTokenSummary>
  total: number
}) {
  return (
    <section aria-label="API token inventory" className="min-w-0">
      <div className="divide-y sm:hidden">
        {isLoading
          ? Array.from({ length: 4 }, (_, index) => (
              <Skeleton key={index} className="my-4 h-16 w-full" />
            ))
          : tokens.map((token) => {
              const tokenStatus = status(token)
              return (
                <article key={token.id} className="space-y-3 py-4">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <h2 className="truncate font-medium">{token.name}</h2>
                      <code className="block truncate font-mono text-xs text-muted-foreground">
                        {token.prefix}...
                      </code>
                    </div>
                    <Badge
                      variant={
                        tokenStatus === 'active'
                          ? 'secondary'
                          : tokenStatus === 'revoked'
                            ? 'destructive'
                            : 'outline'
                      }
                    >
                      {tokenStatus}
                    </Badge>
                  </div>
                  <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted-foreground">
                    <Badge variant="secondary">
                      {roles[token.role] ?? token.role}
                    </Badge>
                    <span>Created {relative(token.created_at)}</span>
                    <span>Used {relative(token.last_used_at)}</span>
                  </div>
                  {tokenStatus === 'active' && canDelete ? (
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => onRevoke(token)}
                    >
                      Revoke
                    </Button>
                  ) : null}
                </article>
              )
            })}
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
                Name
              </SortableTableHead>
              <TableHead className="hidden lg:table-cell">Prefix</TableHead>
              <SortableTableHead
                sort={sort}
                sortKey="role"
                direction={direction}
                onSortChange={onSortChange}
              >
                Role
              </SortableTableHead>
              <TableHead className="hidden lg:table-cell">Created by</TableHead>
              <SortableTableHead
                className="hidden lg:table-cell"
                sort={sort}
                sortKey="created_at"
                direction={direction}
                onSortChange={onSortChange}
              >
                Created
              </SortableTableHead>
              <SortableTableHead
                className="hidden lg:table-cell"
                sort={sort}
                sortKey="last_used_at"
                direction={direction}
                onSortChange={onSortChange}
              >
                Last used
              </SortableTableHead>
              <SortableTableHead
                sort={sort}
                sortKey="status"
                direction={direction}
                onSortChange={onSortChange}
              >
                Status
              </SortableTableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading
              ? Array.from({ length: 5 }, (_row, index) => (
                  <TableRow key={index}>
                    {Array.from({ length: 8 }, (_column, cell) => (
                      <TableCell key={cell}>
                        <Skeleton className="h-6 w-20" />
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              : tokens.map((token) => {
                  const tokenStatus = status(token)
                  return (
                    <TableRow key={token.id}>
                      <TableCell className="font-medium">
                        {token.name}
                      </TableCell>
                      <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                        {token.prefix}...
                      </TableCell>
                      <TableCell>
                        <Badge variant="secondary">
                          {roles[token.role] ?? token.role}
                        </Badge>
                      </TableCell>
                      <TableCell className="hidden text-sm text-muted-foreground lg:table-cell">
                        {token.created_by_email}
                      </TableCell>
                      <TableCell className="hidden text-muted-foreground lg:table-cell">
                        {relative(token.created_at)}
                      </TableCell>
                      <TableCell className="hidden text-muted-foreground lg:table-cell">
                        {relative(token.last_used_at)}
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={
                            tokenStatus === 'active'
                              ? 'secondary'
                              : tokenStatus === 'revoked'
                                ? 'destructive'
                                : 'outline'
                          }
                        >
                          {tokenStatus}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right">
                        {tokenStatus === 'active' && canDelete ? (
                          <Button
                            variant="destructive"
                            size="sm"
                            onClick={() => onRevoke(token)}
                          >
                            Revoke
                          </Button>
                        ) : null}
                      </TableCell>
                    </TableRow>
                  )
                })}
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
