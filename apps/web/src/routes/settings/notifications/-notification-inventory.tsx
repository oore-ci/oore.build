import { useMemo } from 'react'
import { Link } from '@tanstack/react-router'

import type { NotificationChannel } from '@oore/client/models'
import { relativeTime } from '@/lib/format-utils'
import { notificationEventLabel } from './-notification-form'
import { enabledStatus } from '@/lib/status-variants'
import { StatusBadge } from '@/components/status-badge'
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
import { Badge } from '@/components/ui/badge'
import type { SortDirection } from '@/components/data-table-features'
import { ChannelActions } from './-channel-actions'

export type NotificationSort = 'name' | 'type' | 'status' | 'updated_at'

const NOTIFICATION_SORTS = [
  'name',
  'type',
  'status',
  'updated_at',
] satisfies ReadonlyArray<NotificationSort>

function channelTypeLabel(type: string): string {
  if (type === 'webhook') return 'Webhook'
  if (type === 'mattermost') return 'Mattermost'
  if (type === 'slack') return 'Slack'
  if (type === 'email') return 'Email (SMTP)'
  return type
}

function channelIdentity(channel: NotificationChannel) {
  return (
    <Link
      to="/settings/notifications/$channelId"
      params={{ channelId: channel.id }}
      className="group block min-w-0 rounded-md outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
    >
      <span className="block truncate font-medium group-hover:underline">
        {channel.name}
      </span>
      <span className="block truncate font-mono text-[11px] text-muted-foreground">
        {channel.id.slice(0, 8)}
      </span>
    </Link>
  )
}

function getNotificationColumns({
  onDelete,
  onTest,
  pending,
}: {
  onDelete: (channel: NotificationChannel) => void
  onTest: (channel: NotificationChannel) => void
  pending: boolean
}): Array<DataTableColumnDef<NotificationChannel>> {
  return [
    {
      accessorKey: 'name',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Channel" />
      ),
      cell: ({ row }) => channelIdentity(row.original),
    },
    {
      id: 'type',
      accessorFn: (channel) => channel.channel_type,
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Type" />
      ),
      cell: ({ row }) => (
        <Badge variant="outline">
          {channelTypeLabel(row.original.channel_type)}
        </Badge>
      ),
    },
    {
      id: 'status',
      accessorFn: (channel) => channel.enabled,
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Status" />
      ),
      cell: ({ row }) => (
        <StatusBadge
          kind="enabled"
          value={enabledStatus(row.original.enabled)}
        />
      ),
    },
    {
      accessorKey: 'events',
      header: 'Events',
      cell: ({ row }) =>
        row.original.events.length > 0
          ? row.original.events.map(notificationEventLabel).join(', ')
          : 'All events',
      enableSorting: false,
    },
    {
      accessorKey: 'updated_at',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Updated" />
      ),
      cell: ({ row }) => relativeTime(row.original.updated_at),
    },
    {
      id: 'actions',
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) => (
        <ChannelActions
          pending={pending}
          onDelete={() => onDelete(row.original)}
          onTest={() => onTest(row.original)}
        />
      ),
      enableHiding: false,
      enableSorting: false,
    },
  ]
}

export function NotificationInventory({
  channels,
  direction,
  isLoading,
  onDelete,
  onPageChange,
  onSearch,
  onSortChange,
  onTest,
  page,
  pageSize,
  pending,
  query,
  sort,
  total,
}: {
  channels: Array<NotificationChannel>
  direction: SortDirection
  isLoading: boolean
  onDelete: (channel: NotificationChannel) => void
  onPageChange: (page: number) => void
  onSearch: (query: string) => void
  onSortChange: (sort: NotificationSort, direction: SortDirection) => void
  onTest: (channel: NotificationChannel) => void
  page: number
  pageSize: number
  pending: boolean
  query: string
  sort: NotificationSort
  total: number
}) {
  const columns = useMemo(
    () => getNotificationColumns({ onDelete, onTest, pending }),
    [onDelete, onTest, pending],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: channels,
    getRowId: (channel) => channel.id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, NOTIFICATION_SORTS)
      if (next) onSortChange(next.sort, next.direction)
    },
  })

  return (
    <section
      aria-label="Notification channel inventory"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <DataTable
        table={table}
        search={{
          value: query,
          onChange: onSearch,
          placeholder: 'Search channels',
        }}
        pagination={{ onPageChange, page, pageSize, total }}
        emptyMessage={isLoading ? 'Loading notification channels…' : undefined}
      />
    </section>
  )
}
