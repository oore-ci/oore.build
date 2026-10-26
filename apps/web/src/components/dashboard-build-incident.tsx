import { HugeiconsIcon } from '@hugeicons/react'
import {
  ChevronRightIcon,
  AlertCircleIcon as CircleAlertIcon,
  Alert02Icon as TriangleAlertIcon,
} from '@hugeicons/core-free-icons'

import { DashboardList, DashboardSection } from '@/components/dashboard-section'
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

function AttentionRow({ item }: { item: AttentionItem }) {
  const blocking = item.severity === 'blocking'
  return (
    <Item
      size="sm"
      className="rounded-none"
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
      <ItemActions>
        <HugeiconsIcon icon={ChevronRightIcon} aria-hidden />
      </ItemActions>
    </Item>
  )
}

/** What is broken right now, blocking causes first, gone once it recovers. */
export default function DashboardBuildIncident({
  items,
}: {
  items: Array<AttentionItem>
}) {
  if (items.length === 0) return null

  return (
    <DashboardSection title="Needs attention" count={items.length}>
      <DashboardList>
        {items.map((item) => ({
          key: item.key,
          node: <AttentionRow item={item} />,
        }))}
      </DashboardList>
    </DashboardSection>
  )
}
