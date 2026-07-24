import {
  Trash2 as Delete02Icon,
  CircleEllipsis as MoreHorizontalCircle01Icon,
  FlaskConical as TestTube01Icon,
} from 'lucide-react'

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
        <MoreHorizontalCircle01Icon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        <DropdownMenuItem onClick={onTest} disabled={pending}>
          <TestTube01Icon />
          Send test
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onClick={onDelete}>
          <Delete02Icon />
          Delete channel
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
