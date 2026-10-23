import { useState } from 'react'
import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  InformationCircleIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'

import type { NotificationChannel } from '@oore/client/models'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import {
  useDeleteNotificationChannel,
  useNotificationChannels,
  useTestNotificationChannel,
} from '@/hooks/use-notification-channels'
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
import type { SortDirection } from '@/components/data-table-features'
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

function parseSearch(search: SearchInput): NotificationsSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const sort = searchChoice(search, 'sort', NOTIFICATION_SORTS)
  const q = searchString(search, 'q')?.trim() ?? ''

  return {
    q: q || undefined,
    sort,
    direction:
      searchString(search, 'direction') === 'desc' ? 'desc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/settings/notifications/')({
  validateSearch: parseSearch,
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'], context)
  },
  component: NotificationsPage,
})

function NotificationsPage() {
  const search = useSearch({ from: '/settings/notifications/' })
  const navigate = Route.useNavigate()
  const deleteMutation = useDeleteNotificationChannel()
  const testMutation = useTestNotificationChannel()
  const [deleteTarget, setDeleteTarget] = useState<NotificationChannel | null>(
    null,
  )
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'name'
  const direction = search.direction ?? 'asc'
  const requestedPage = search.page ?? 1
  const channelsQuery = useNotificationChannels({
    q: search.q,
    sort,
    direction,
    limit: pageSize,
    offset: (requestedPage - 1) * pageSize,
  })
  const total = channelsQuery.data?.total ?? 0
  const page = Math.min(requestedPage, Math.max(1, Math.ceil(total / pageSize)))
  const visibleChannels = channelsQuery.data?.channels ?? []

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
    <PageLayout width="wide" fill>
      <PageMeta title="Notifications" noindex />
      <PageHeader
        title="Notifications"
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
        <Empty className="border bg-card">
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
        <Empty className="border bg-card">
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
        <NotificationInventory
          channels={visibleChannels}
          direction={direction}
          isLoading={channelsQuery.isLoading}
          onDelete={setDeleteTarget}
          onPageChange={(nextPage) =>
            updateSearch({ page: nextPage > 1 ? nextPage : undefined })
          }
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          onSortChange={handleSortChange}
          onTest={handleTest}
          page={page}
          pageSize={pageSize}
          pending={testMutation.isPending}
          query={search.q ?? ''}
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
              its delivery history. Pipelines that notified it stop sending.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (deleteTarget) handleDelete(deleteTarget)
              }}
              disabled={deleteMutation.isPending}
            >
              {deleteMutation.isPending ? 'Deleting...' : 'Delete channel'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  )
}
