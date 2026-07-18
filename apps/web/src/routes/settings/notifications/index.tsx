import { useMemo, useState } from 'react'
import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Plus as Add01Icon,
  Info as InformationCircleIcon,
  Search as Search01Icon,
} from 'lucide-react'
import { toast } from '@/lib/toast'

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
import { CollectionSearchInput } from '@/components/collection-search-input'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { PageMeta } from '@/lib/seo'
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
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import type { SortDirection } from '@/components/collection-controls'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { NotificationInventory } from './-notification-inventory'
import type { NotificationSort } from './-notification-inventory'

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
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'])
  },
  component: NotificationsPage,
})

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
            <DynamicLucideIcon icon={Add01Icon} />
            Add channel
          </Button>
        }
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <CollectionSearchInput
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          placeholder="Search channels"
          ariaLabel="Search notification channels"
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
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
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
              <DynamicLucideIcon icon={InformationCircleIcon} />
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
              <DynamicLucideIcon icon={Add01Icon} />
              Add channel
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {showFilteredEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <DynamicLucideIcon icon={Search01Icon} />
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
        <NotificationInventory
          channels={visibleChannels}
          direction={direction}
          isLoading={channelsQuery.isLoading}
          onDelete={setDeleteTarget}
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
          onTest={handleTest}
          page={page}
          pageSize={pageSize}
          pending={testMutation.isPending}
          sort={sort}
          total={total}
        />
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
