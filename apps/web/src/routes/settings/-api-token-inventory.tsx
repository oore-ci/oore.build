import type { ApiTokenSummary } from '@oore/client/models'
import { useMemo } from 'react'
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
import type { ApiTokenSort } from './api-tokens'
import type { SortDirection } from '@/components/data-table-features'
import { Badge } from '@/components/ui/badge'
import { RelativeTime } from '@/components/relative-time'
import { StatusBadge } from '@/components/status-badge'
import { apiTokenStatus } from '@/lib/status-variants'
import { ApiTokenActions } from './-api-token-actions'
import { Text } from '@/components/layout/text'
import { Stack } from '@/components/layout/stack'

const API_TOKEN_TABLE_SORTS = [
  'name',
  'role',
  'created_at',
  'last_used_at',
  'status',
] satisfies ReadonlyArray<ApiTokenSort>

interface RoleLabels {
  [role: string]: string
}

const roles: RoleLabels = {
  owner: 'Owner',
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

function LastUsed({ at }: { at?: number | null }) {
  if (!at) return <span className="text-muted-foreground">Never</span>
  return <RelativeTime at={at} />
}

function getApiTokenColumns({
  canDelete,
  onRevoke,
}: {
  canDelete: boolean
  onRevoke: (token: ApiTokenSummary) => void
}): Array<DataTableColumnDef<ApiTokenSummary>> {
  return [
    {
      accessorKey: 'name',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Name" />
      ),
      cell: ({ row }) => (
        <>
          <p className="font-medium">{row.original.name}</p>
          <Text variant="mono">{row.original.prefix}…</Text>
        </>
      ),
    },
    {
      accessorKey: 'role',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Role" />
      ),
      cell: ({ row }) => (
        <Badge variant="secondary">
          {roles[row.original.role] ?? row.original.role}
        </Badge>
      ),
    },
    {
      accessorKey: 'created_by_email',
      header: 'Created by',
      enableSorting: false,
    },
    {
      accessorKey: 'created_at',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Created" />
      ),
      cell: ({ row }) => <RelativeTime at={row.original.created_at} />,
    },
    {
      accessorKey: 'last_used_at',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Last used" />
      ),
      cell: ({ row }) => <LastUsed at={row.original.last_used_at} />,
    },
    {
      id: 'status',
      accessorFn: apiTokenStatus,
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Status" />
      ),
      cell: ({ row }) => (
        <StatusBadge kind="token" value={apiTokenStatus(row.original)} />
      ),
    },
    {
      id: 'actions',
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) =>
        apiTokenStatus(row.original) === 'active' && canDelete ? (
          <ApiTokenActions
            name={row.original.name}
            onRevoke={() => onRevoke(row.original)}
          />
        ) : null,
      enableHiding: false,
      enableSorting: false,
    },
  ]
}

export function ApiTokenInventory({
  canDelete,
  direction,
  isLoading,
  onPageChange,
  onRevoke,
  onSearch,
  onSortChange,
  page,
  pageSize,
  query,
  sort,
  tokens,
  total,
}: {
  canDelete: boolean
  direction: SortDirection
  isLoading: boolean
  onPageChange: (page: number) => void
  onRevoke: (token: ApiTokenSummary) => void
  onSearch: (query: string) => void
  onSortChange: (sort: ApiTokenSort, direction: SortDirection) => void
  page: number
  pageSize: number
  query: string
  sort: ApiTokenSort
  tokens: Array<ApiTokenSummary>
  total: number
}) {
  const columns = useMemo(
    () => getApiTokenColumns({ canDelete, onRevoke }),
    [canDelete, onRevoke],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: tokens,
    getRowId: (token) => token.id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(
        updater,
        sorting,
        API_TOKEN_TABLE_SORTS,
      )
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <Stack aria-label="API token inventory" fill render={<section />}>
      <DataTable
        table={table}
        search={{
          value: query,
          onChange: onSearch,
          placeholder: 'Search API tokens',
        }}
        pagination={{ onPageChange, page, pageSize, total }}
        isLoading={isLoading}
        emptyMessage={isLoading ? 'Loading API tokens…' : undefined}
      />
    </Stack>
  )
}
