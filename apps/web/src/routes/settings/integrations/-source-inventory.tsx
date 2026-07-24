import { Link } from '@tanstack/react-router'

import type { Integration } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import { relativeTime } from '@/lib/format-utils'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
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

export type IntegrationSort = 'name' | 'provider' | 'status' | 'updated_at'

function providerLabel(provider: Integration['provider']): string {
  if (provider === 'github') return 'GitHub'
  if (provider === 'gitlab') return 'GitLab'
  return 'Local Git'
}

function authModeLabel(mode: string): string {
  const labels: Record<string, string> = {
    github_app: 'GitHub App',
    github_app_manifest: 'GitHub App manifest',
    oauth_app: 'OAuth app',
    pat: 'Personal access token',
    personal_token: 'Personal access token',
  }
  return labels[mode] ?? mode.replace(/_/g, ' ')
}

function sourceIdentity(integration: Integration) {
  return (
    <Link
      to="/settings/integrations/$integrationId"
      params={{ integrationId: integration.id }}
      className="group block min-w-0 rounded-md outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
    >
      <span className="block truncate font-medium group-hover:underline">
        {integration.display_name ?? integration.provider}
      </span>
      <span className="block truncate font-mono text-[11px] text-muted-foreground">
        {integration.id.slice(0, 8)}
      </span>
    </Link>
  )
}

export function SourceInventory({
  direction,
  integrations,
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
  integrations: Array<Integration>
  isLoading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (size: number) => void
  onSortChange: (sort: IntegrationSort, direction: SortDirection) => void
  page: number
  pageSize: number
  sort: IntegrationSort
  total: number
}) {
  return (
    <div aria-label="Connected source inventory" className="min-w-0">
      <div className="divide-y sm:hidden">
        {isLoading
          ? Array.from({ length: 3 }, (_, index) => (
              <Skeleton key={index} className="my-4 h-16 w-full" />
            ))
          : integrations.map((integration) => (
              <article key={integration.id} className="space-y-3 py-4">
                <div className="flex items-start justify-between gap-3">
                  {sourceIdentity(integration)}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="outline">
                    {providerLabel(integration.provider)}
                  </Badge>
                  <Badge
                    variant={getIntegrationStatusVariant(integration.status)}
                  >
                    {integration.status}
                  </Badge>
                  <span className="text-xs text-muted-foreground">
                    Updated {relativeTime(integration.updated_at)}
                  </span>
                </div>
              </article>
            ))}
      </div>
      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              {(['name', 'provider', 'status'] as const).map((key) => (
                <SortableTableHead
                  key={key}
                  sort={sort}
                  sortKey={key}
                  direction={direction}
                  onSortChange={onSortChange}
                >
                  {key === 'name'
                    ? 'Source'
                    : key[0].toUpperCase() + key.slice(1)}
                </SortableTableHead>
              ))}
              <TableHead className="hidden lg:table-cell">
                Authentication
              </TableHead>
              <TableHead className="hidden lg:table-cell">Host</TableHead>
              <SortableTableHead
                className="hidden lg:table-cell"
                sort={sort}
                sortKey="updated_at"
                direction={direction}
                onSortChange={onSortChange}
              >
                Updated
              </SortableTableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading
              ? Array.from({ length: 4 }, (_row, index) => (
                  <TableRow key={index}>
                    {Array.from({ length: 6 }, (_column, cell) => (
                      <TableCell key={cell}>
                        <Skeleton className="h-6 w-20" />
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              : integrations.map((integration) => (
                  <TableRow key={integration.id}>
                    <TableCell>{sourceIdentity(integration)}</TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {providerLabel(integration.provider)}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={getIntegrationStatusVariant(
                          integration.status,
                        )}
                      >
                        {integration.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                      {authModeLabel(integration.auth_mode)}
                    </TableCell>
                    <TableCell className="hidden max-w-[24ch] truncate text-xs text-muted-foreground lg:table-cell">
                      {integration.host_url}
                    </TableCell>
                    <TableCell className="hidden text-xs text-muted-foreground lg:table-cell">
                      {relativeTime(integration.updated_at)}
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
    </div>
  )
}
