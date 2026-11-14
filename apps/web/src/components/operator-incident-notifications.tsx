import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Notification03Icon } from '@hugeicons/core-free-icons'
import type { InboxItem } from '@oore/client/models'

import { useAttention } from '@/hooks/use-attention'
import { useInbox } from '@/hooks/use-personal'
import { useInboxAnnouncer } from '@/hooks/use-inbox-announcer'
import { useApiContext } from '@/hooks/use-api-context'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'
import { relativeTime } from '@/lib/format-utils'
import { unreadCount } from '@/lib/inbox'

function seenKey(instanceId: string | null | undefined) {
  return `oore-inbox-seen:${instanceId ?? 'default'}`
}

function readSeen(instanceId: string | null | undefined) {
  try {
    return Number(localStorage.getItem(seenKey(instanceId))) || 0
  } catch {
    return 0
  }
}

function itemLink(item: InboxItem) {
  if (item.kind === 'release' && item.release_id) {
    return (
      <Link to="/releases/$releaseId" params={{ releaseId: item.release_id }} />
    )
  }
  if (item.build_id) {
    return <Link to="/builds/$buildId" params={{ buildId: item.build_id }} />
  }
  return (
    <Link to="/projects/$projectId" params={{ projectId: item.project_id }} />
  )
}

/**
 * The bell is a personal inbox: approvals waiting for you, your failed
 * builds and changes to releases of apps you pinned. Home keeps the
 * instance-wide Needs attention list; the bell only links to it.
 */
export function OperatorIncidentNotifications() {
  const { instanceId } = useApiContext()
  const inboxQuery = useInbox()
  const { items: attention } = useAttention()
  const [seen, setSeen] = useState(() => readSeen(instanceId))
  const items = inboxQuery.data?.items ?? []
  useInboxAnnouncer(inboxQuery.data?.items)
  const unread = unreadCount(items, seen)
  const hasApproval = items.some(
    (item) => item.kind === 'approval' && item.at > seen,
  )

  function markSeen() {
    const newest = items[0]?.at ?? Math.floor(Date.now() / 1000)
    try {
      localStorage.setItem(seenKey(instanceId), String(newest))
    } catch {
      // Unread again on the next visit.
    }
    setSeen(newest)
  }

  return (
    <DropdownMenu
      onOpenChange={(open) => {
        if (open) void inboxQuery.refetch()
        else markSeen()
      }}
    >
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={
              unread === 0
                ? 'Your inbox, nothing new'
                : `Your inbox, ${unread} new`
            }
            className="relative"
            size="icon"
            variant="ghost"
          />
        }
      >
        <HugeiconsIcon icon={Notification03Icon} />
        {unread > 0 ? (
          <span
            className={cn(
              'absolute top-1 right-1 size-2 rounded-full',
              hasApproval ? 'bg-warning' : 'bg-destructive',
            )}
          />
        ) : null}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-88 max-w-[90vw]">
        <DropdownMenuGroup>
          <DropdownMenuLabel>For you</DropdownMenuLabel>
          {inboxQuery.isError ? (
            <DropdownMenuItem
              closeOnClick={false}
              onClick={() => void inboxQuery.refetch()}
              className="text-xs text-destructive"
            >
              Your inbox could not be loaded. Retry
            </DropdownMenuItem>
          ) : null}
          {items.length === 0 ? (
            <Text variant="caption" className="px-2 py-3">
              {inboxQuery.isLoading
                ? 'Loading…'
                : 'Nothing for you this week. Approvals, your failed builds and releases of apps you pin show here.'}
            </Text>
          ) : (
            items.slice(0, 12).map((item) => (
              <DropdownMenuItem key={item.id} render={itemLink(item)}>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span
                    className={cn(
                      'truncate',
                      item.at > seen ? 'font-semibold' : 'font-medium',
                    )}
                  >
                    {item.title}
                  </span>
                  <span
                    className={cn(
                      textVariants({ variant: 'caption' }),
                      'truncate',
                    )}
                  >
                    {item.project_name}
                    {item.detail ? ` · ${item.detail}` : ''} ·{' '}
                    {relativeTime(item.at)}
                  </span>
                </span>
              </DropdownMenuItem>
            ))
          )}
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          {attention.length > 0 ? (
            <DropdownMenuItem render={<Link to="/" />}>
              <span className="text-sm">
                {attention.length} thing{attention.length === 1 ? '' : 's'} need
                attention on Home
              </span>
            </DropdownMenuItem>
          ) : null}
          <DropdownMenuItem render={<Link to="/account/notifications" />}>
            <span className="text-sm">Notification settings</span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
