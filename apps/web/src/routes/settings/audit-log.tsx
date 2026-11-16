import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import { createFileRoute, useSearch } from '@tanstack/react-router'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'

import { DataTableSelectFilter } from '@/components/data-table'
import {
  DateRangeFilter,
  searchDateRange,
  validSearchDate,
} from '@/components/date-range-filter'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { useAuditLogActivity, useAuditLogs } from '@/hooks/use-audit-logs'
import { useUsers } from '@/hooks/use-auth'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { AuditLogActivity } from './-audit-log-activity'
import { AUDIT_KINDS, AuditLogCollection } from './-audit-log-collection'
import type { AuditKind } from './-audit-log-collection'

interface AuditLogSearch {
  actor?: string
  from?: string
  kind?: Exclude<AuditKind, 'all'>
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  to?: string
}

const ACTIVITY_DAYS = 14

const KIND_VALUES = new Set(
  AUDIT_KINDS.map((kind) => kind.value).filter((kind) => kind !== 'all'),
)

function parseSearch(search: SearchInput): AuditLogSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''

  return {
    q: q || undefined,
    kind: searchChoice(search, 'kind', KIND_VALUES),
    actor: searchString(search, 'actor') || undefined,
    from: validSearchDate(searchString(search, 'from')),
    to: validSearchDate(searchString(search, 'to')),
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

function AuditLogPage() {
  const navigate = Route.useNavigate()
  const search = useSearch({ from: '/settings/audit-log' })
  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const fromTs = search.from
    ? Math.floor(new Date(`${search.from}T00:00:00`).getTime() / 1000)
    : undefined
  const toTs = search.to
    ? Math.floor(new Date(`${search.to}T23:59:59`).getTime() / 1000)
    : undefined

  const auditQuery = useAuditLogs({
    limit: pageSize,
    offset: (page - 1) * pageSize,
    q: search.q,
    category: search.kind,
    actor_id: search.actor,
    from_ts: fromTs,
    to_ts: toTs,
  })
  const activityQuery = useAuditLogActivity(ACTIVITY_DAYS)
  const usersQuery = useUsers()

  const entries = auditQuery.data?.entries ?? []
  const total = auditQuery.data?.total ?? 0
  const hasFilters =
    !!search.q ||
    !!search.kind ||
    !!search.actor ||
    !!search.from ||
    !!search.to
  const showTrueEmpty =
    !auditQuery.isLoading && !auditQuery.error && total === 0 && !hasFilters

  const people = Object.fromEntries([
    ['all', 'Everyone'],
    ...(usersQuery.data?.users ?? []).map((user) => [
      user.id,
      user.display_name || user.email,
    ]),
    ['system', 'Oore'],
  ])

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
      kind: undefined,
      actor: undefined,
      from: undefined,
      to: undefined,
      page: undefined,
    })
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Audit log" noindex />
      <PageHeader title="Audit log" />

      {showTrueEmpty || activityQuery.error ? null : (
        <AuditLogActivity
          activity={activityQuery.data}
          selectedDay={
            search.from && search.from === search.to ? search.from : undefined
          }
          onSelectDay={(date) =>
            updateSearch({ from: date, to: date, page: undefined })
          }
        />
      )}

      <AuditLogCollection
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
          Number(!!search.actor) + Number(!!(search.from || search.to))
        }
        onClearFilters={clearFilters}
        filters={
          <>
            <DataTableSelectFilter
              value={search.actor ?? 'all'}
              options={people}
              contentClassName="w-64"
              onValueChange={(value) =>
                updateSearch({
                  actor: value && value !== 'all' ? value : undefined,
                  page: undefined,
                })
              }
            />
            <DateRangeFilter
              from={search.from}
              to={search.to}
              onChange={(range) =>
                updateSearch({ ...searchDateRange(range), page: undefined })
              }
            />
          </>
        }
        isFiltered={hasFilters}
        isLoading={auditQuery.isLoading}
        isRefreshing={auditQuery.isFetching && !auditQuery.isLoading}
        kind={search.kind ?? 'all'}
        onKindChange={(kind) =>
          updateSearch({
            kind: kind === 'all' ? undefined : kind,
            page: undefined,
          })
        }
        onPageChange={(nextPage) =>
          updateSearch({ page: nextPage > 1 ? nextPage : undefined })
        }
        onRetry={() => void auditQuery.refetch()}
        onSearch={(value) =>
          updateSearch({ q: value.trim() || undefined, page: undefined })
        }
        onShowActor={(actor) => updateSearch({ actor, page: undefined })}
        onShowApp={(name) => updateSearch({ q: name, page: undefined })}
        page={page}
        pageSize={pageSize}
        query={search.q ?? ''}
        total={total}
      />
    </PageLayout>
  )
}
