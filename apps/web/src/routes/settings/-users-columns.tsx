import {
  ArrowUpDownIcon,
  Cancel01Icon,
  MoreHorizontalCircle01Icon,
  UserCheck01Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import type { ColumnDef } from '@tanstack/react-table'

import type { User, UserRole } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

const ROLE_LABELS: Record<string, string> = {
  owner: 'Owner',
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

const ROLE_BADGE_VARIANT: Record<
  string,
  'warning' | 'info' | 'secondary' | 'outline'
> = {
  owner: 'warning',
  admin: 'info',
  developer: 'secondary',
  qa_viewer: 'outline',
}

const STATUS_BADGE_VARIANT: Record<string, 'success' | 'info' | 'destructive'> =
  {
    active: 'success',
    invited: 'info',
    disabled: 'destructive',
  }

export interface UserColumnOptions {
  authUserId: string | undefined
  onRoleChange: (userId: string, email: string, newRole: UserRole) => void
  onDisable: (userId: string, email: string) => void
  onReEnable: (userId: string, email: string) => void
}

export function getColumns(options: UserColumnOptions): Array<ColumnDef<User>> {
  const { authUserId, onRoleChange, onDisable, onReEnable } = options

  return [
    {
      id: 'select',
      header: ({ table }) => (
        <Checkbox
          checked={table.getIsAllPageRowsSelected()}
          indeterminate={
            table.getIsSomePageRowsSelected() &&
            !table.getIsAllPageRowsSelected()
          }
          onCheckedChange={(value) => table.toggleAllPageRowsSelected(!!value)}
          aria-label="Select all"
        />
      ),
      cell: ({ row }) => {
        if (!row.getCanSelect()) return null
        return (
          <Checkbox
            checked={row.getIsSelected()}
            onCheckedChange={(value) => row.toggleSelected(!!value)}
            aria-label="Select row"
          />
        )
      },
      enableSorting: false,
    },
    {
      accessorKey: 'email',
      header: ({ column }) => (
        <Button
          variant="ghost"
          size="sm"
          className="-ml-2"
          onClick={() => column.toggleSorting(column.getIsSorted() === 'asc')}
        >
          Email
          <HugeiconsIcon icon={ArrowUpDownIcon} size={14} />
        </Button>
      ),
      cell: ({ row }) => {
        const isSelf = row.original.id === authUserId
        return (
          <span>
            {row.original.email}
            {isSelf ? (
              <span className="ml-2 text-xs text-muted-foreground">(you)</span>
            ) : null}
          </span>
        )
      },
    },
    {
      accessorKey: 'role',
      header: ({ column }) => (
        <Button
          variant="ghost"
          size="sm"
          className="-ml-2"
          onClick={() => column.toggleSorting(column.getIsSorted() === 'asc')}
        >
          Role
          <HugeiconsIcon icon={ArrowUpDownIcon} size={14} />
        </Button>
      ),
      cell: ({ row }) => {
        const role = row.original.role
        return (
          <Badge
            variant={ROLE_BADGE_VARIANT[role] ?? 'outline'}
            className="text-xs"
          >
            {ROLE_LABELS[role] ?? role}
          </Badge>
        )
      },
    },
    {
      accessorKey: 'status',
      header: 'Status',
      cell: ({ row }) => {
        const status = row.original.status
        return (
          <Badge
            variant={STATUS_BADGE_VARIANT[status] ?? 'outline'}
            className="text-xs capitalize"
          >
            {status}
          </Badge>
        )
      },
    },
    {
      id: 'actions',
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) => {
        const user = row.original
        const isOwner = user.role === 'owner'
        const isSelf = user.id === authUserId
        const isDisabled = user.status === 'disabled'

        if (isOwner || isSelf) return null

        return (
          <div className="text-right">
            <DropdownMenu>
              <DropdownMenuTrigger
                render={<Button variant="ghost" size="icon-sm" />}
              >
                <HugeiconsIcon icon={MoreHorizontalCircle01Icon} size={16} />
                <span className="sr-only">Open menu</span>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-auto">
                {!isDisabled ? (
                  <>
                    <DropdownMenuSub>
                      <DropdownMenuSubTrigger>
                        Change Role
                      </DropdownMenuSubTrigger>
                      <DropdownMenuSubContent>
                        <DropdownMenuRadioGroup value={user.role}>
                          {(['admin', 'developer', 'qa_viewer'] as const).map(
                            (role) => (
                              <DropdownMenuRadioItem
                                key={role}
                                value={role}
                                onClick={() => {
                                  if (role !== user.role) {
                                    onRoleChange(user.id, user.email, role)
                                  }
                                }}
                              >
                                {ROLE_LABELS[role]}
                              </DropdownMenuRadioItem>
                            ),
                          )}
                        </DropdownMenuRadioGroup>
                      </DropdownMenuSubContent>
                    </DropdownMenuSub>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem
                      variant="destructive"
                      onClick={() => onDisable(user.id, user.email)}
                    >
                      <HugeiconsIcon icon={Cancel01Icon} size={14} />
                      Disable User
                    </DropdownMenuItem>
                  </>
                ) : (
                  <DropdownMenuItem
                    onClick={() => onReEnable(user.id, user.email)}
                  >
                    <HugeiconsIcon icon={UserCheck01Icon} size={14} />
                    Re-enable User
                  </DropdownMenuItem>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )
      },
    },
  ]
}
