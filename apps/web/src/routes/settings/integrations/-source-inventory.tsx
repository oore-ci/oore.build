import { useMemo } from 'react'
import { Link } from '@tanstack/react-router'

import type { Integration } from '@oore/client/models'
import {
  DataTable,
  DataTableColumnHeader,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import {
  dataTableSortingState,
  resolveDataTableSorting,
} from '@/components/data-table-features'
import type { SortDirection } from '@/components/data-table-features'
import { formatEnumLabel, relativeTime } from '@/lib/format-utils'
import { StatusBadge } from '@/components/status-badge'
import { Badge } from '@/components/ui/badge'

export type IntegrationSort = 'name' | 'provider' | 'status' | 'updated_at'

const INTEGRATION_SORTS = [
  'name',
  'provider',
  'status',
  'updated_at',
] satisfies ReadonlyArray<IntegrationSort>

function providerLabel(provider: Integration['provider']): string {
  if (provider === 'github') return 'GitHub'
  if (provider === 'gitlab') return 'GitLab'
  return 'Local Git'
}

function authModeLabel(mode: string): string {
  const labels = new Map([
    ['github_app', 'GitHub App'],
    ['github_app_manifest', 'GitHub App manifest'],
    ['oauth_app', 'OAuth app'],
    ['pat', 'Personal access token'],
    ['personal_token', 'Personal access token'],
  ])
  return labels.get(mode) ?? formatEnumLabel(mode)
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

const sourceColumns: Array<DataTableColumnDef<Integration>> = [
  {
    id: 'name',
    accessorFn: (integration) => integration.display_name,
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Source" />
    ),
    cell: ({ row }) => sourceIdentity(row.original),
  },
  {
    accessorKey: 'provider',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Provider" />
    ),
    cell: ({ row }) => (
      <Badge variant="outline">{providerLabel(row.original.provider)}</Badge>
    ),
  },
  {
    accessorKey: 'status',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Status" />
    ),
    cell: ({ row }) => (
      <StatusBadge kind="integration" value={row.original.status} />
    ),
  },
  {
    accessorKey: 'auth_mode',
    header: 'Authentication',
    cell: ({ row }) => authModeLabel(row.original.auth_mode),
    enableSorting: false,
  },
  {
    accessorKey: 'host_url',
    header: 'Host',
    enableSorting: false,
  },
  {
    accessorKey: 'updated_at',
    header: ({ column }) => (
      <DataTableColumnHeader column={column} title="Updated" />
    ),
    cell: ({ row }) => relativeTime(row.original.updated_at),
  },
]

export function SourceInventory({
  direction,
  integrations,
  isLoading,
  onPageChange,
  onSearch,
  onSortChange,
  page,
  pageSize,
  query,
  sort,
  total,
}: {
  direction: SortDirection
  integrations: Array<Integration>
  isLoading: boolean
  onPageChange: (page: number) => void
  onSearch: (query: string) => void
  onSortChange: (sort: IntegrationSort, direction: SortDirection) => void
  page: number
  pageSize: number
  query: string
  sort: IntegrationSort
  total: number
}) {
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns: sourceColumns,
    data: integrations,
    getRowId: (integration) => integration.id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, INTEGRATION_SORTS)
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <div
      aria-label="Connected source inventory"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <DataTable
        table={table}
        search={{
          value: query,
          onChange: onSearch,
          placeholder: 'Search connected sources',
        }}
        pagination={{ onPageChange, page, pageSize, total }}
        emptyMessage={isLoading ? 'Loading connected sources…' : undefined}
      />
    </div>
  )
}
