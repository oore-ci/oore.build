import { HugeiconsIcon } from '@hugeicons/react'
import { Notification03Icon } from '@hugeicons/core-free-icons'

import { useAttention } from '@/hooks/use-attention'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'

/** The header view of Home's Needs attention, from the same items. */
export function OperatorIncidentNotifications() {
  const { isError, isLoading, items, refetch, unreadIncidents } = useAttention()
  const hasBlocking = items.some((item) => item.severity === 'blocking')

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={
              items.length === 0
                ? 'Needs attention, nothing open'
                : `Needs attention, ${items.length} open`
            }
            className="relative"
            size="icon"
            variant="ghost"
          />
        }
      >
        <HugeiconsIcon icon={Notification03Icon} />
        {items.length > 0 ? (
          <span
            className={cn(
              'absolute top-1 right-1 size-2 rounded-full',
              hasBlocking || unreadIncidents > 0
                ? 'bg-destructive'
                : 'bg-warning',
            )}
          />
        ) : null}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Needs attention</DropdownMenuLabel>
          {isError ? (
            <DropdownMenuItem
              closeOnClick={false}
              onClick={refetch}
              className="text-xs text-destructive"
            >
              Some checks could not be loaded. Retry
            </DropdownMenuItem>
          ) : null}
          {items.length === 0 ? (
            <p className="px-2 py-3 text-xs text-muted-foreground">
              {isLoading
                ? 'Checking…'
                : isError
                  ? 'Nothing else to show.'
                  : 'Nothing needs attention.'}
            </p>
          ) : (
            items.map((item) => (
              <DropdownMenuItem
                key={item.key}
                render={item.link}
                onClick={item.onSelect}
              >
                <span className="flex min-w-0 flex-col">
                  <span className="truncate font-medium">{item.title}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {item.description}
                  </span>
                </span>
              </DropdownMenuItem>
            ))
          )}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
