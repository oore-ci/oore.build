import { Link } from '@tanstack/react-router'

import type { NotificationChannel } from '@/lib/types'
import { relativeTime } from '@/lib/format-utils'
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
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import type { SortDirection } from '@/components/collection-controls'
import { ChannelActions } from './-channel-actions'

export type NotificationSort = 'name' | 'type' | 'status' | 'updated_at'

function channelTypeLabel(type: string): string {
  if (type === 'webhook') return 'Webhook'
  if (type === 'mattermost') return 'Mattermost'
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

export function NotificationInventory({
  channels,
  direction,
  isLoading,
  onDelete,
  onPageChange,
  onPageSizeChange,
  onSortChange,
  onTest,
  page,
  pageSize,
  pending,
  sort,
  total,
}: {
  channels: Array<NotificationChannel>
  direction: SortDirection
  isLoading: boolean
  onDelete: (channel: NotificationChannel) => void
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onSortChange: (sort: NotificationSort, direction: SortDirection) => void
  onTest: (channel: NotificationChannel) => void
  page: number
  pageSize: number
  pending: boolean
  sort: NotificationSort
  total: number
}) {
  return (
    <section aria-label="Notification channel inventory" className="min-w-0">
      <div className="divide-y sm:hidden">
        {isLoading
          ? Array.from({ length: 4 }, (_, index) => (
              <div key={index} className="space-y-2 py-4">
                <Skeleton className="h-5 w-2/3" />
                <Skeleton className="h-4 w-1/2" />
              </div>
            ))
          : channels.map((channel) => (
              <article key={channel.id} className="space-y-3 py-4">
                <div className="flex items-start justify-between gap-3">
                  {channelIdentity(channel)}
                  <ChannelActions
                    channel={channel}
                    pending={pending}
                    onDelete={() => onDelete(channel)}
                    onTest={() => onTest(channel)}
                  />
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="outline">
                    {channelTypeLabel(channel.channel_type)}
                  </Badge>
                  <Badge variant={channel.enabled ? 'secondary' : 'outline'}>
                    {channel.enabled ? 'Enabled' : 'Disabled'}
                  </Badge>
                  <span className="text-xs text-muted-foreground">
                    Updated {relativeTime(channel.updated_at)}
                  </span>
                </div>
              </article>
            ))}
      </div>
      <div className="hidden sm:block">
        <Table>
          <TableHeader>
            <TableRow>
              {(['name', 'type', 'status'] as const).map((key) => (
                <SortableTableHead
                  key={key}
                  sort={sort}
                  sortKey={key}
                  direction={direction}
                  onSortChange={onSortChange}
                >
                  {key === 'name'
                    ? 'Channel'
                    : key === 'type'
                      ? 'Type'
                      : 'Status'}
                </SortableTableHead>
              ))}
              <TableHead className="hidden lg:table-cell">Events</TableHead>
              <SortableTableHead
                className="hidden lg:table-cell"
                sort={sort}
                sortKey="updated_at"
                direction={direction}
                onSortChange={onSortChange}
              >
                Updated
              </SortableTableHead>
              <TableHead className="text-right">
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading
              ? Array.from({ length: 5 }, (_row, index) => (
                  <TableRow key={index}>
                    {Array.from({ length: 6 }, (_column, cell) => (
                      <TableCell
                        key={cell}
                        className={
                          cell === 3 || cell === 4
                            ? 'hidden lg:table-cell'
                            : undefined
                        }
                      >
                        <Skeleton className="h-6 w-20" />
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              : channels.map((channel) => (
                  <TableRow key={channel.id}>
                    <TableCell>{channelIdentity(channel)}</TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {channelTypeLabel(channel.channel_type)}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={channel.enabled ? 'secondary' : 'outline'}
                      >
                        {channel.enabled ? 'Enabled' : 'Disabled'}
                      </Badge>
                    </TableCell>
                    <TableCell className="hidden max-w-[28ch] truncate text-xs text-muted-foreground lg:table-cell">
                      {channel.events.length > 0
                        ? channel.events.join(', ')
                        : 'All events'}
                    </TableCell>
                    <TableCell className="hidden text-xs text-muted-foreground lg:table-cell">
                      {relativeTime(channel.updated_at)}
                    </TableCell>
                    <TableCell className="text-right">
                      <ChannelActions
                        channel={channel}
                        pending={pending}
                        onDelete={() => onDelete(channel)}
                        onTest={() => onTest(channel)}
                      />
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
    </section>
  )
}
