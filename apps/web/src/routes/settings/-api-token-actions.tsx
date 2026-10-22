import { HugeiconsIcon } from '@hugeicons/react'
import { Cancel01Icon } from '@hugeicons/core-free-icons'

import { DataTableRowActionsTrigger } from '@/components/data-table'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
} from '@/components/ui/dropdown-menu'

export function ApiTokenActions({
  name,
  onRevoke,
}: {
  name: string
  onRevoke: () => void
}) {
  return (
    <DropdownMenu>
      <DataTableRowActionsTrigger label={`Actions for ${name}`} />
      <DropdownMenuContent align="end" className="w-40">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Actions</DropdownMenuLabel>
          <DropdownMenuItem variant="destructive" onClick={onRevoke}>
            <HugeiconsIcon icon={Cancel01Icon} />
            Revoke token
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
