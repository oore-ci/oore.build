import { useMemo } from 'react'
import {
  CalendarDays as Calendar03Icon,
  Info as InformationCircleIcon,
  Search as Search01Icon,
} from 'lucide-react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { createFileRoute, redirect, useSearch } from '@tanstack/react-router'
import { format } from 'date-fns'
import type { DateRange } from 'react-day-picker'

import type { SortDirection } from '@/components/collection-controls'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { useAuditLogs } from '@/hooks/use-audit-logs'
import { CollectionSearchInput } from '@/components/collection-search-input'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { useAuthStore } from '@/stores/auth-store'
import { AuditLogInventory } from './-audit-log-inventory'
import type { AuditSort } from './-audit-log-inventory'

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
  staticData: {
    breadcrumb: {
      title: 'Audit log',
    },
  },
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

function dateFromSearch(value?: string): Date | undefined {
  if (!value) return undefined
  const date = new Date(`${value}T00:00:00`)
  return Number.isNaN(date.getTime()) ? undefined : date
}

function AuditDateRangePicker({
  from,
  onChange,
  to,
}: {
  from?: string
  onChange: (range: DateRange | undefined) => void
  to?: string
}) {
  const fromDate = dateFromSearch(from)
  const toDate = dateFromSearch(to)
  const selected: DateRange | undefined =
    fromDate || toDate ? { from: fromDate, to: toDate } : undefined
  const label = fromDate
    ? toDate
      ? `${format(fromDate, 'MMM d, yyyy')} to ${format(toDate, 'MMM d, yyyy')}`
      : `From ${format(fromDate, 'MMM d, yyyy')}`
    : toDate
      ? `Through ${format(toDate, 'MMM d, yyyy')}`
      : 'Pick a date range'

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            variant="outline"
            data-empty={!selected}
            className="col-span-2 w-full justify-start overflow-hidden text-left font-normal data-[empty=true]:text-muted-foreground sm:w-auto"
            aria-label={`Date range: ${label}`}
          />
        }
      >
        <DynamicLucideIcon
          icon={Calendar03Icon}
          data-icon="inline-start"
          aria-hidden
        />
        <span className="truncate">{label}</span>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="max-h-[calc(100dvh-2rem)] w-auto max-w-[calc(100vw-2rem)] overflow-auto p-0"
      >
        <Calendar
          mode="range"
          selected={selected}
          defaultMonth={fromDate ?? toDate}
          numberOfMonths={2}
          onSelect={onChange}
        />
      </PopoverContent>
    </Popover>
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
      <PageMeta title="Audit log" noindex />
      <PageHeader
        title="Audit log"
        description="Activity trail of user and system actions for compliance and security auditing."
      />

      <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
        <CollectionSearchInput
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          placeholder="Search actions"
          ariaLabel="Search audit actions"
          className="lg:max-w-sm"
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
          <AuditDateRangePicker
            from={search.from}
            to={search.to}
            onChange={(range) =>
              updateSearch({
                from: range?.from
                  ? format(range.from, 'yyyy-MM-dd')
                  : undefined,
                to: range?.to ? format(range.to, 'yyyy-MM-dd') : undefined,
                page: undefined,
              })
            }
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
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
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
              <DynamicLucideIcon icon={Search01Icon} />
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
              <DynamicLucideIcon icon={InformationCircleIcon} />
            </EmptyMedia>
            <EmptyTitle>No activity yet</EmptyTitle>
            <EmptyDescription>
              User and system actions will appear here as they happen.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : null}

      {!auditQuery.error && (auditQuery.isLoading || total > 0) ? (
        <AuditLogInventory
          direction={direction}
          entries={entries}
          isLoading={auditQuery.isLoading}
          onPageChange={(nextPage) =>
            updateSearch({ page: nextPage > 1 ? nextPage : undefined })
          }
          onPageSizeChange={(nextPageSize) =>
            updateSearch({
              pageSize:
                nextPageSize === 20 ? undefined : (nextPageSize as 50 | 100),
              page: undefined,
            })
          }
          onSortChange={handleSortChange}
          page={page}
          pageSize={pageSize}
          sort={sort}
          total={total}
        />
      ) : null}
    </PageLayout>
  )
}
