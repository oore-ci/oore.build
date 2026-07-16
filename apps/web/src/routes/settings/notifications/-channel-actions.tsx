import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  MoreHorizontalCircle01Icon,
  TestTube01Icon,
} from '@hugeicons/core-free-icons'

import type { NotificationChannel } from '@/lib/types'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

export function ChannelActions({
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
