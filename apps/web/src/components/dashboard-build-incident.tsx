import { HugeiconsIcon } from '@hugeicons/react'
import {
  Cancel01Icon,
  ChevronRightIcon,
  AlertCircleIcon as CircleAlertIcon,
  Alert02Icon as TriangleAlertIcon,
} from '@hugeicons/core-free-icons'

import { DashboardList, DashboardSection } from '@/components/dashboard-section'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import type { AttentionItem } from '@/hooks/use-attention'
import { cn } from '@/lib/utils'

function AttentionRow({
  item,
  onDismiss,
}: {
  item: AttentionItem
  onDismiss?: (item: AttentionItem) => void
}) {
  const blocking = item.severity === 'blocking'
  return (
    <div className="group/attention relative flex items-center">
      <Item
        size="sm"
        className={cn('flex-1 rounded-none', onDismiss && 'pr-12')}
        render={item.link}
        onClick={item.onSelect}
      >
        <ItemMedia variant="icon">
          <HugeiconsIcon
            icon={blocking ? CircleAlertIcon : TriangleAlertIcon}
            className={cn(blocking ? 'text-destructive!' : 'text-warning!')}
          />
        </ItemMedia>
        <ItemContent className="min-w-0">
          <ItemTitle>{item.title}</ItemTitle>
          <ItemDescription className="line-clamp-1">
            {item.description}
          </ItemDescription>
        </ItemContent>
        {/* With a dismiss button in its place, the row reads as a link by itself. */}
        {onDismiss ? null : (
          <ItemActions>
            <HugeiconsIcon icon={ChevronRightIcon} aria-hidden />
          </ItemActions>
        )}
      </Item>
      {onDismiss ? (
        <Button
          variant="ghost"
          size="icon-sm"
          className="absolute right-2 text-muted-foreground"
          aria-label={`Dismiss ${item.title}`}
          title="Dismiss until it changes"
          onClick={() => onDismiss(item)}
        >
          <HugeiconsIcon icon={Cancel01Icon} aria-hidden />
        </Button>
      ) : null}
    </div>
  )
}

/** What is broken right now, blocking causes first, gone once it recovers. */
export default function DashboardBuildIncident({
  items,
  onDismiss,
}: {
  items: Array<AttentionItem>
  /** Hide an item until it changes. */
  onDismiss?: (item: AttentionItem) => void
}) {
  if (items.length === 0) return null

  return (
    <DashboardSection title="Needs attention" count={items.length}>
      <DashboardList>
        {items.map((item) => ({
          key: item.key,
          node: <AttentionRow item={item} onDismiss={onDismiss} />,
        }))}
      </DashboardList>
    </DashboardSection>
  )
}
