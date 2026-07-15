import { useMemo, useState } from 'react'
import { createFileRoute, redirect, useSearch } from '@tanstack/react-router'
import { InformationCircleIcon, Search01Icon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import type { SortDirection } from '@/components/collection-controls'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { useAuditLogs } from '@/hooks/use-audit-logs'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { relativeTime } from '@/lib/format-utils'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { useAuthStore } from '@/stores/auth-store'

type AuditSort = 'created_at' | 'actor_email' | 'action' | 'resource_type'

interface AuditLogSearch {
  direction?: SortDirection
  from?: string
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  resource?: string
  sort?: AuditSort
  to?: string
}

const RESOURCE_TYPE_OPTIONS: Record<string, string> = {
  all: 'All resources',
  user: 'User',
  build: 'Build',
  project: 'Project',
  pipeline: 'Pipeline',
  integration: 'Integration',
  instance_settings: 'Settings',
  runner: 'Runner',
  artifact: 'Artifact',
  auth: 'Auth',
}

const AUDIT_SORT_OPTIONS: Record<AuditSort, string> = {
  created_at: 'Time',
  actor_email: 'Actor',
  action: 'Action',
  resource_type: 'Resource',
}

const AUDIT_SORT_VALUES = new Set<AuditSort>(
  Object.keys(AUDIT_SORT_OPTIONS) as Array<AuditSort>,
)

function validDate(value: unknown): string | undefined {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    return undefined
  }
  return Number.isNaN(new Date(`${value}T00:00:00`).getTime())
    ? undefined
    : value
}

function parseSearch(search: Record<string, unknown>): AuditLogSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const q = typeof search.q === 'string' ? search.q.trim() : ''
  const resource =
    typeof search.resource === 'string' &&
    search.resource !== 'all' &&
    search.resource in RESOURCE_TYPE_OPTIONS
      ? search.resource
      : undefined
  const sort = search.sort as AuditSort

  return {
    q: q || undefined,
    resource,
    from: validDate(search.from),
    to: validDate(search.to),
    sort: AUDIT_SORT_VALUES.has(sort) ? sort : undefined,
    direction:
      search.direction === 'asc' || search.direction === 'desc'
        ? search.direction
        : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/audit-log')({
  staticData: { breadcrumbLabel: 'Audit Log' },
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
  component: AuditLogPage,
})

function ActionSearch({
  initialValue,
  onSearch,
}: {
  initialValue: string
  onSearch: (value: string) => void
}) {
  const [value, setValue] = useState(initialValue)
  const debouncedSearch = useDebouncedCallback(onSearch, 300)

  return (
    <div className="relative w-full lg:max-w-sm">
      <HugeiconsIcon
        icon={Search01Icon}
        className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input
        type="search"
        value={value}
        onChange={(event) => {
          const next = event.target.value
          setValue(next)
          debouncedSearch(next)
        }}
        placeholder="Search actions"
        aria-label="Search audit actions"
        className="pl-9"
      />
    </div>
  )
}

function AuditLogPage() {
  const navigate = Route.useNavigate()
  const search = useSearch({ from: '/settings/audit-log' })
  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const fromTs = search.from
    ? Math.floor(new Date(`${search.from}T00:00:00`).getTime() / 1000)
    : undefined
  const toTs = search.to
    ? Math.floor(new Date(`${search.to}T23:59:59`).getTime() / 1000)
    : undefined

  const auditQuery = useAuditLogs({
    limit: pageSize,
    offset: (page - 1) * pageSize,
    resource_type: search.resource,
    action: search.q,
    from_ts: fromTs,
    to_ts: toTs,
    sort,
    direction,
  })

  const entries = useMemo(
    () => auditQuery.data?.entries ?? [],
    [auditQuery.data?.entries],
  )
  const total = auditQuery.data?.total ?? 0
  const hasFilters =
    !!search.q || !!search.resource || !!search.from || !!search.to
  const showFilteredEmpty =
    !auditQuery.isLoading && !auditQuery.error && total === 0 && hasFilters
  const showTrueEmpty =
    !auditQuery.isLoading && !auditQuery.error && total === 0 && !hasFilters

  function updateSearch(updates: Partial<AuditLogSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(page, pageSize, auditQuery.data?.total, (nextPage) => {
    updateSearch({ page: nextPage === 1 ? undefined : nextPage })
  })

  function clearFilters() {
    updateSearch({
      q: undefined,
      resource: undefined,
      from: undefined,
      to: undefined,
      page: undefined,
    })
  }

  function handleSortChange(nextSort: AuditSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Audit Log" noindex />
      <PageHeader
        title="Audit Log"
        description="Activity trail of user and system actions for compliance and security auditing."
      />

      <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
        <ActionSearch
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
        />
        <div className="grid min-w-0 grid-cols-2 gap-3 sm:flex sm:flex-wrap lg:ml-auto">
          <Select
            value={search.resource ?? 'all'}
            onValueChange={(value) =>
              updateSearch({
                resource: value && value !== 'all' ? value : undefined,
                page: undefined,
              })
            }
            items={RESOURCE_TYPE_OPTIONS}
          >
            <SelectTrigger
              className="w-full sm:w-40"
              aria-label="Filter by resource"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Object.entries(RESOURCE_TYPE_OPTIONS).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select
            value={sort}
            onValueChange={(value) =>
              handleSortChange(value ?? 'created_at', direction)
            }
            items={AUDIT_SORT_OPTIONS}
          >
            <SelectTrigger
              className="w-full sm:hidden"
              aria-label="Sort audit log"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Object.entries(AUDIT_SORT_OPTIONS).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Input
            type="date"
            value={search.from ?? ''}
            onChange={(event) =>
              updateSearch({
                from: event.target.value || undefined,
                page: undefined,
              })
            }
            max={search.to}
            className="min-w-0 sm:w-36"
            aria-label="From date"
          />
          <Input
            type="date"
            value={search.to ?? ''}
            onChange={(event) =>
              updateSearch({
                to: event.target.value || undefined,
                page: undefined,
              })
            }
            min={search.from}
            className="min-w-0 sm:w-36"
            aria-label="To date"
          />
          <Button
            variant="outline"
            className="w-full sm:hidden"
            onClick={() =>
              handleSortChange(sort, direction === 'desc' ? 'asc' : 'desc')
            }
            aria-label={`Sort ${direction === 'desc' ? 'ascending' : 'descending'}`}
          >
            {direction === 'desc' ? 'Descending' : 'Ascending'}
          </Button>
          {hasFilters ? (
            <Button
              variant="ghost"
              size="sm"
              className="w-full sm:w-auto"
              onClick={clearFilters}
            >
              Clear filters
            </Button>
          ) : null}
        </div>
      </div>

      {auditQuery.error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>Failed to load audit log: {auditQuery.error.message}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void auditQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {showFilteredEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={Search01Icon} />
            </EmptyMedia>
            <EmptyTitle>No matching activity</EmptyTitle>
            <EmptyDescription>
              Change the current filters or clear them to see all activity.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button variant="outline" onClick={clearFilters}>
              Clear filters
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {showTrueEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={InformationCircleIcon} />
            </EmptyMedia>
            <EmptyTitle>No activity yet</EmptyTitle>
            <EmptyDescription>
              User and system actions will appear here as they happen.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : null}

      {!auditQuery.error && (auditQuery.isLoading || total > 0) ? (
        <section aria-label="Audit activity" className="min-w-0">
          <ul className="divide-y sm:hidden">
            {auditQuery.isLoading
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
                        dateTime={new Date(
                          entry.created_at * 1000,
                        ).toISOString()}
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
                    onSortChange={handleSortChange}
                  >
                    Time
                  </SortableTableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="actor_email"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Actor
                  </SortableTableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="action"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Action
                  </SortableTableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="resource_type"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Resource
                  </SortableTableHead>
                  <TableHead className="hidden lg:table-cell">
                    Resource ID
                  </TableHead>
                  <TableHead className="hidden lg:table-cell">
                    Details
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {auditQuery.isLoading
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
                            <span className="text-muted-foreground">
                              System
                            </span>
                          )}
                        </TableCell>
                        <TableCell className="max-w-48">
                          <Badge
                            variant="outline"
                            className="max-w-full truncate"
                          >
                            {entry.action}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <Badge variant="secondary">
                            {entry.resource_type}
                          </Badge>
                        </TableCell>
                        <TableCell className="hidden font-mono text-[11px] text-muted-foreground lg:table-cell">
                          {entry.resource_id
                            ? entry.resource_id.slice(0, 8)
                            : '—'}
                        </TableCell>
                        <TableCell className="hidden max-w-xs truncate text-xs text-muted-foreground lg:table-cell">
                          {entry.details ?? '—'}
                        </TableCell>
                      </TableRow>
                    ))}
              </TableBody>
            </Table>
          </div>

          {!auditQuery.isLoading ? (
            <CollectionPagination
              page={page}
              pageSize={pageSize}
              total={total}
              onPageChange={(nextPage) =>
                updateSearch({ page: nextPage > 1 ? nextPage : undefined })
              }
              onPageSizeChange={(nextPageSize) =>
                updateSearch({
                  pageSize:
                    nextPageSize === 20
                      ? undefined
                      : (nextPageSize as 50 | 100),
                  page: undefined,
                })
              }
            />
          ) : null}
        </section>
      ) : null}
    </PageLayout>
  )
}
