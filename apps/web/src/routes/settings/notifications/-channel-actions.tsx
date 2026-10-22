import { HugeiconsIcon } from '@hugeicons/react'
import { Delete02Icon, TestTube01Icon } from '@hugeicons/core-free-icons'

import { DataTableRowActionsTrigger } from '@/components/data-table'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'

export function ChannelActions({
  name,
  pending,
  onDelete,
  onTest,
}: {
  name: string
  pending: boolean
  onDelete: () => void
  onTest: () => void
}) {
  return (
    <DropdownMenu>
      <DataTableRowActionsTrigger label={`Actions for ${name}`} />
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Actions</DropdownMenuLabel>
          <DropdownMenuItem onClick={onTest} disabled={pending}>
            <HugeiconsIcon icon={TestTube01Icon} />
            Send test
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem variant="destructive" onClick={onDelete}>
            <HugeiconsIcon icon={Delete02Icon} />
            Delete channel
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
