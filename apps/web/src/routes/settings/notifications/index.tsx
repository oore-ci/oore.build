import { useMemo, useState } from 'react'
import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Delete02Icon,
  InformationCircleIcon,
  MoreHorizontalCircle01Icon,
  Search01Icon,
  TestTube01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type { NotificationChannel } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import {
  useDeleteNotificationChannel,
  useNotificationChannels,
  useTestNotificationChannel,
} from '@/hooks/use-notification-channels'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { PageMeta } from '@/lib/seo'
import { relativeTime } from '@/lib/format-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
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
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'

type NotificationSort = 'name' | 'type' | 'status' | 'updated_at'

interface NotificationsSearch {
  direction?: SortDirection
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: NotificationSort
}

const NOTIFICATION_SORTS = new Set<NotificationSort>([
  'name',
  'type',
  'status',
  'updated_at',
])

const NOTIFICATION_SORT_OPTIONS: Record<NotificationSort, string> = {
  name: 'Name',
  type: 'Channel type',
  status: 'Status',
  updated_at: 'Recently updated',
}

function parseSearch(search: Record<string, unknown>): NotificationsSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const sort = search.sort as NotificationSort
  const q = typeof search.q === 'string' ? search.q.trim() : ''

  return {
    q: q || undefined,
    sort: NOTIFICATION_SORTS.has(sort) ? sort : undefined,
    direction: search.direction === 'desc' ? 'desc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/notifications/')({
  staticData: { breadcrumbLabel: 'Notifications' },
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'])
  },
  component: NotificationsPage,
})

function channelTypeLabel(type: string): string {
  switch (type) {
    case 'webhook':
      return 'Webhook'
    case 'mattermost':
      return 'Mattermost'
    case 'email':
      return 'Email (SMTP)'
    default:
      return type
  }
}

function ChannelSearch({
  initialValue,
  onSearch,
}: {
  initialValue: string
  onSearch: (value: string) => void
}) {
  const [value, setValue] = useState(initialValue)
  const debouncedSearch = useDebouncedCallback(onSearch, 300)

  return (
    <div className="relative w-full sm:max-w-sm">
      <HugeiconsIcon
        icon={Search01Icon}
        className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input
        type="search"
        value={value}
        onChange={(event) => {
          const nextValue = event.target.value
          setValue(nextValue)
          debouncedSearch(nextValue)
        }}
        placeholder="Search channels"
        aria-label="Search notification channels"
        className="pl-9"
      />
    </div>
  )
}

function ChannelIdentity({ channel }: { channel: NotificationChannel }) {
  return (
    <Link
      to="/settings/notifications/$channelId"
      params={{ channelId: channel.id }}
      className="group block min-w-0 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
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

function ChannelActions({
  channel,
  pending,
  onDelete,
  onTest,
}: {
  channel: NotificationChannel
  pending: boolean
  onDelete: () => void
  onTest: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Actions for ${channel.name}`}
          />
        }
      >
        <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        <DropdownMenuItem onClick={onTest} disabled={pending}>
          <HugeiconsIcon icon={TestTube01Icon} />
          Send test
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onClick={onDelete}>
          <HugeiconsIcon icon={Delete02Icon} />
          Delete channel
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function NotificationsPage() {
  const search = useSearch({ from: '/settings/notifications/' })
  const navigate = Route.useNavigate()
  const channelsQuery = useNotificationChannels()
  const deleteMutation = useDeleteNotificationChannel()
  const testMutation = useTestNotificationChannel()
  const [deleteTarget, setDeleteTarget] = useState<NotificationChannel | null>(
    null,
  )
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'name'
  const direction = search.direction ?? 'asc'

  const filteredChannels = useMemo(() => {
    const query = search.q?.toLocaleLowerCase()
    const channels = (channelsQuery.data?.channels ?? []).filter((channel) =>
      query
        ? [channel.name, channel.channel_type, ...channel.events]
            .join(' ')
            .toLocaleLowerCase()
            .includes(query)
        : true,
    )

    return channels.sort((left, right) => {
      const leftValue =
        sort === 'type'
          ? left.channel_type
          : sort === 'status'
            ? Number(left.enabled)
            : sort === 'updated_at'
              ? left.updated_at
              : left.name.toLocaleLowerCase()
      const rightValue =
        sort === 'type'
          ? right.channel_type
          : sort === 'status'
            ? Number(right.enabled)
            : sort === 'updated_at'
              ? right.updated_at
              : right.name.toLocaleLowerCase()
      const result =
        typeof leftValue === 'number'
          ? leftValue - Number(rightValue)
          : leftValue.localeCompare(String(rightValue))
      return direction === 'asc' ? result : -result
    })
  }, [channelsQuery.data?.channels, direction, search.q, sort])

  const total = filteredChannels.length
  const requestedPage = search.page ?? 1
  const page = Math.min(requestedPage, Math.max(1, Math.ceil(total / pageSize)))
  const visibleChannels = filteredChannels.slice(
    (page - 1) * pageSize,
    page * pageSize,
  )

  function updateSearch(updates: Partial<NotificationsSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(
    requestedPage,
    pageSize,
    channelsQuery.isLoading ? undefined : total,
    (nextPage) => {
      updateSearch({ page: nextPage === 1 ? undefined : nextPage })
    },
  )

  function handleSortChange(nextSort: NotificationSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  function handleDelete(channel: NotificationChannel) {
    deleteMutation.mutate(channel.id, {
      onSuccess: () => {
        toast.success(`Deleted channel: ${channel.name}`)
        setDeleteTarget(null)
      },
      onError: (error) => toast.error(`Failed to delete: ${error.message}`),
    })
  }

  function handleTest(channel: NotificationChannel) {
    testMutation.mutate(channel.id, {
      onSuccess: (result) => {
        if (result.success) {
          toast.success(`Test notification sent to ${channel.name}`)
        } else {
          toast.error(`Test failed: ${result.error ?? 'Unknown error'}`)
        }
      },
      onError: (error) => toast.error(`Test failed: ${error.message}`),
    })
  }

  const hasSearch = !!search.q
  const showTrueEmpty =
    !channelsQuery.isLoading &&
    !channelsQuery.error &&
    total === 0 &&
    !hasSearch
  const showFilteredEmpty =
    !channelsQuery.isLoading && !channelsQuery.error && total === 0 && hasSearch

  return (
    <PageLayout width="wide">
      <PageMeta title="Notifications" noindex />
      <PageHeader
        title="Notifications"
        description="Configure outbound notification channels for build status updates."
        actions={
          <Button
            render={<Link to="/settings/notifications/new" />}
            nativeButton={false}
          >
            <HugeiconsIcon icon={Add01Icon} />
            Add channel
          </Button>
        }
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <ChannelSearch
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
        />
        <NativeSelect
          className="w-full sm:hidden"
          aria-label="Sort notification channels"
          value={sort}
          onChange={(event) =>
            handleSortChange(event.target.value as NotificationSort, direction)
          }
        >
          {Object.entries(NOTIFICATION_SORT_OPTIONS).map(([value, label]) => (
            <NativeSelectOption key={value} value={value}>
              {label}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>

      {channelsQuery.error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              Failed to load notification channels:{' '}
              {channelsQuery.error.message}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void channelsQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {showTrueEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={InformationCircleIcon} />
            </EmptyMedia>
            <EmptyTitle>No notification channels</EmptyTitle>
            <EmptyDescription>
              Add a channel to send build status updates outside Oore.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              render={<Link to="/settings/notifications/new" />}
              nativeButton={false}
            >
              <HugeiconsIcon icon={Add01Icon} />
              Add channel
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {showFilteredEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={Search01Icon} />
            </EmptyMedia>
            <EmptyTitle>No matching channels</EmptyTitle>
            <EmptyDescription>
              Try a different search or clear the current query.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              variant="outline"
              onClick={() => updateSearch({ q: undefined, page: undefined })}
            >
              Clear search
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!channelsQuery.error && (channelsQuery.isLoading || total > 0) ? (
        <section
          aria-label="Notification channel inventory"
          className="min-w-0"
        >
          <div className="divide-y sm:hidden">
            {channelsQuery.isLoading
              ? Array.from({ length: 4 }, (_, index) => (
                  <div key={index} className="space-y-2 py-4">
                    <Skeleton className="h-5 w-2/3" />
                    <Skeleton className="h-4 w-1/2" />
                  </div>
                ))
              : visibleChannels.map((channel) => (
                  <article key={channel.id} className="space-y-3 py-4">
                    <div className="flex items-start justify-between gap-3">
                      <ChannelIdentity channel={channel} />
                      <ChannelActions
                        channel={channel}
                        pending={testMutation.isPending}
                        onDelete={() => setDeleteTarget(channel)}
                        onTest={() => handleTest(channel)}
                      />
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="outline">
                        {channelTypeLabel(channel.channel_type)}
                      </Badge>
                      <Badge
                        variant={channel.enabled ? 'default' : 'secondary'}
                      >
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
                  <SortableTableHead
                    sort={sort}
                    sortKey="name"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Channel
                  </SortableTableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="type"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Type
                  </SortableTableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="status"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Status
                  </SortableTableHead>
                  <TableHead className="hidden lg:table-cell">Events</TableHead>
                  <SortableTableHead
                    className="hidden lg:table-cell"
                    sort={sort}
                    sortKey="updated_at"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Updated
                  </SortableTableHead>
                  <TableHead className="text-right">
                    <span className="sr-only">Actions</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {channelsQuery.isLoading
                  ? Array.from({ length: 5 }, (_, index) => (
                      <TableRow key={index}>
                        <TableCell>
                          <Skeleton className="h-8 w-40" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-6 w-20" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-6 w-16" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-32" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-20" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="ml-auto h-8 w-8" />
                        </TableCell>
                      </TableRow>
                    ))
                  : visibleChannels.map((channel) => (
                      <TableRow key={channel.id}>
                        <TableCell>
                          <ChannelIdentity channel={channel} />
                        </TableCell>
                        <TableCell>
                          <Badge variant="outline">
                            {channelTypeLabel(channel.channel_type)}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <Badge
                            variant={channel.enabled ? 'default' : 'secondary'}
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
                            pending={testMutation.isPending}
                            onDelete={() => setDeleteTarget(channel)}
                            onTest={() => handleTest(channel)}
                          />
                        </TableCell>
                      </TableRow>
                    ))}
              </TableBody>
            </Table>
          </div>

          {!channelsQuery.isLoading ? (
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

      <AlertDialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete notification channel?</AlertDialogTitle>
            <AlertDialogDescription>
              This permanently removes {deleteTarget?.name ?? 'the channel'} and
              its delivery history.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (deleteTarget) handleDelete(deleteTarget)
              }}
              disabled={deleteMutation.isPending}
            >
              {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  )
}
