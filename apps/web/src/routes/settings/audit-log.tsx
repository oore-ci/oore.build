import { HugeiconsIcon } from '@hugeicons/react'
import {
  Calendar03Icon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'
import { createFileRoute, useSearch } from '@tanstack/react-router'
import { format } from 'date-fns'
import type { DateRange } from 'react-day-picker'
import * as z from 'zod'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput, SearchValue } from '@/lib/search-input'

import type { SortDirection } from '@/components/data-table-features'
import { DataTableSelectFilter } from '@/components/data-table'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import {
  Empty,
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
import { useAuditLogs } from '@/hooks/use-audit-logs'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { AuditLogCollection } from './-audit-log-collection'
import type { AuditSort } from './-audit-log-collection'

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

const RESOURCE_TYPE_OPTIONS = {
  all: 'All resources',
  user: 'User',
  build: 'Build',
  project: 'Project',
  pipeline: 'Pipeline',
  integration: 'Source',
  instance_settings: 'Settings',
  runner: 'Runner',
  artifact: 'Artifact',
  auth: 'Auth',
} satisfies Record<string, string>

const AUDIT_SORT_VALUES = new Set<AuditSort>([
  'created_at',
  'actor_email',
  'action',
  'resource_type',
])

function validDate(value: SearchValue): string | undefined {
  const parsed = z.string().safeParse(value)
  if (!parsed.success || !/^\d{4}-\d{2}-\d{2}$/.test(parsed.data)) {
    return undefined
  }
  return Number.isNaN(new Date(`${parsed.data}T00:00:00`).getTime())
    ? undefined
    : parsed.data
}

function parseSearch(search: SearchInput): AuditLogSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const resourceValue = searchString(search, 'resource')
  const resource =
    resourceValue &&
    resourceValue !== 'all' &&
    resourceValue in RESOURCE_TYPE_OPTIONS
      ? resourceValue
      : undefined
  const sort = searchChoice(search, 'sort', AUDIT_SORT_VALUES)
  const direction = searchString(search, 'direction')

  return {
    q: q || undefined,
    resource,
    from: validDate(searchString(search, 'from')),
    to: validDate(searchString(search, 'to')),
    sort,
    direction:
      direction === 'asc' || direction === 'desc' ? direction : undefined,
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
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'], context)
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
            variant="secondary"
            className="max-w-64 justify-start overflow-hidden text-left font-normal"
            aria-label={`Date range: ${label}`}
          />
        }
      >
        <HugeiconsIcon
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

  const entries = auditQuery.data?.entries ?? []
  const total = auditQuery.data?.total ?? 0
  const hasFilters =
    !!search.q || !!search.resource || !!search.from || !!search.to
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
    <PageLayout width="wide" fill>
      <PageMeta title="Audit log" noindex />
      <PageHeader title="Audit log" />

      <AuditLogCollection
        direction={direction}
        emptyState={
          showTrueEmpty ? (
            <Empty className="border bg-card">
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
          ) : null
        }
        entries={entries}
        error={auditQuery.error}
        activeFilterCount={
          Number(!!search.resource) + Number(!!(search.from || search.to))
        }
        onClearFilters={clearFilters}
        filters={
          <>
            <DataTableSelectFilter
              value={search.resource ?? 'all'}
              options={RESOURCE_TYPE_OPTIONS}
              onValueChange={(value) =>
                updateSearch({
                  resource: value && value !== 'all' ? value : undefined,
                  page: undefined,
                })
              }
            />
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
          </>
        }
        isFiltered={hasFilters}
        isLoading={auditQuery.isLoading}
        isRefreshing={auditQuery.isFetching && !auditQuery.isLoading}
        onPageChange={(nextPage) =>
          updateSearch({ page: nextPage > 1 ? nextPage : undefined })
        }
        onRetry={() => void auditQuery.refetch()}
        onSearch={(value) =>
          updateSearch({ q: value.trim() || undefined, page: undefined })
        }
        onSortChange={handleSortChange}
        page={page}
        pageSize={pageSize}
        query={search.q ?? ''}
        sort={sort}
        total={total}
      />
    </PageLayout>
  )
}
