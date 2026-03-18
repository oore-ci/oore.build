import { createFileRoute, redirect, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuditLogs } from '@/hooks/use-audit-logs'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
} from '@/components/ui/pagination'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'

const PAGE_SIZE = 25

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

export const Route = createFileRoute('/settings/audit-log')({
  staticData: { breadcrumbLabel: 'Audit Log' },
  validateSearch: (
    search: Record<string, unknown>,
  ): { page?: number } => ({
    page: Number(search.page) > 1 ? Number(search.page) : undefined,
  }),
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

function AuditLogPage() {
  const navigate = Route.useNavigate()
  const search = useSearch({ from: '/settings/audit-log' })
  const page = search.page ?? 1
  const offset = (page - 1) * PAGE_SIZE

  const [resourceTypeFilter, setResourceTypeFilter] = useState<string>('all')
  const [actionFilter, setActionFilter] = useState('')
  const [fromDate, setFromDate] = useState('')
  const [toDate, setToDate] = useState('')

  const fromTs = fromDate
    ? Math.floor(new Date(fromDate + 'T00:00:00').getTime() / 1000)
    : undefined
  const toTs = toDate
    ? Math.floor(new Date(toDate + 'T23:59:59').getTime() / 1000)
    : undefined

  const auditQuery = useAuditLogs({
    limit: PAGE_SIZE,
    offset,
    resource_type:
      resourceTypeFilter !== 'all' ? resourceTypeFilter : undefined,
    action: actionFilter.trim() || undefined,
    from_ts: fromTs,
    to_ts: toTs,
  })

  const total = auditQuery.data?.total ?? 0
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE))

  const entries = useMemo(
    () => auditQuery.data?.entries ?? [],
    [auditQuery.data?.entries],
  )

  const hasFilters =
    resourceTypeFilter !== 'all' || actionFilter || fromDate || toDate

  return (
    <PageLayout width="wide">
      <PageMeta title="Audit Log" noindex />
      <PageHeader
        title="Audit Log"
        description="Activity trail of user and system actions for compliance and security auditing."
      />

      <div className="flex items-center gap-3">
        <Select
          value={resourceTypeFilter}
          onValueChange={(v) => {
            setResourceTypeFilter(v ?? 'all')
            void navigate({ search: { page: 1 } })
          }}
          items={RESOURCE_TYPE_OPTIONS}
        >
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {Object.entries(RESOURCE_TYPE_OPTIONS).map(([key, label]) => (
              <SelectItem key={key} value={key}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Input
          placeholder="Filter by action..."
          value={actionFilter}
          onChange={(e) => {
            setActionFilter(e.target.value)
            void navigate({ search: { page: 1 } })
          }}
          className="max-w-xs"
        />
        <Input
          type="date"
          value={fromDate}
          onChange={(e) => {
            setFromDate(e.target.value)
            void navigate({ search: { page: 1 } })
          }}
          className="w-36"
          aria-label="From date"
        />
        <Input
          type="date"
          value={toDate}
          onChange={(e) => {
            setToDate(e.target.value)
            void navigate({ search: { page: 1 } })
          }}
          className="w-36"
          aria-label="To date"
        />
        {hasFilters ? (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setResourceTypeFilter('all')
              setActionFilter('')
              setFromDate('')
              setToDate('')
              void navigate({ search: { page: 1 } })
            }}
          >
            Clear filters
          </Button>
        ) : null}
      </div>

      {auditQuery.isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {auditQuery.error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load audit log: {auditQuery.error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!auditQuery.isLoading && !auditQuery.error ? (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                Activity log
              </CardTitle>
              <span className="text-xs text-muted-foreground">
                {total} total
              </span>
            </div>
          </CardHeader>
          <CardContent>
            {entries.length === 0 ? (
              <p className="py-6 text-sm text-muted-foreground">
                No audit log entries found.
              </p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Time</TableHead>
                    <TableHead>Actor</TableHead>
                    <TableHead>Action</TableHead>
                    <TableHead>Resource</TableHead>
                    <TableHead>Resource ID</TableHead>
                    <TableHead>Details</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {entries.map((entry) => (
                    <TableRow key={entry.id}>
                      <TableCell className="text-sm text-muted-foreground whitespace-nowrap">
                        {relativeTime(entry.created_at)}
                      </TableCell>
                      <TableCell className="text-sm">
                        {entry.actor_email ?? (
                          <span className="text-muted-foreground">System</span>
                        )}
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline">{entry.action}</Badge>
                      </TableCell>
                      <TableCell>
                        <Badge variant="secondary">
                          {entry.resource_type}
                        </Badge>
                      </TableCell>
                      <TableCell className="font-mono text-[11px] text-muted-foreground">
                        {entry.resource_id
                          ? entry.resource_id.slice(0, 8)
                          : '—'}
                      </TableCell>
                      <TableCell className="max-w-xs truncate text-xs text-muted-foreground">
                        {entry.details
                          ? entry.details.length > 60
                            ? entry.details.slice(0, 60) + '…'
                            : entry.details
                          : '—'}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      ) : null}

      {!auditQuery.isLoading && !auditQuery.error && totalPages > 1 ? (
        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">
            Page {page} of {totalPages}
          </span>
          <Pagination>
            <PaginationContent>
              <PaginationItem>
                <PaginationPrevious
                  onClick={(e) => {
                    e.preventDefault()
                    if (page > 1)
                      void navigate({
                        search: { page: page - 1 > 1 ? page - 1 : undefined },
                      })
                  }}
                  aria-disabled={page <= 1}
                  className={page <= 1 ? 'pointer-events-none opacity-50' : ''}
                />
              </PaginationItem>
              {Array.from({ length: Math.min(totalPages, 5) }, (_, i) => {
                let pageNum: number
                if (totalPages <= 5) {
                  pageNum = i + 1
                } else if (page <= 3) {
                  pageNum = i + 1
                } else if (page >= totalPages - 2) {
                  pageNum = totalPages - 4 + i
                } else {
                  pageNum = page - 2 + i
                }
                return (
                  <PaginationItem key={pageNum}>
                    <PaginationLink
                      isActive={pageNum === page}
                      onClick={(e) => {
                        e.preventDefault()
                        void navigate({
                          search: {
                            page: pageNum > 1 ? pageNum : undefined,
                          },
                        })
                      }}
                    >
                      {pageNum}
                    </PaginationLink>
                  </PaginationItem>
                )
              })}
              <PaginationItem>
                <PaginationNext
                  onClick={(e) => {
                    e.preventDefault()
                    if (page < totalPages)
                      void navigate({ search: { page: page + 1 } })
                  }}
                  aria-disabled={page >= totalPages}
                  className={
                    page >= totalPages ? 'pointer-events-none opacity-50' : ''
                  }
                />
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        </div>
      ) : null}
    </PageLayout>
  )
}
