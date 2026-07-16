import {
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
import { relativeTime } from '@/lib/format-utils'

export const ROLE_LABELS: Record<string, string> = {
  owner: 'Owner',
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

const ROLE_BADGE_VARIANT: Record<string, 'secondary' | 'outline'> = {
  owner: 'outline',
  admin: 'outline',
  developer: 'secondary',
  qa_viewer: 'outline',
}

const STATUS_BADGE_VARIANT: Record<
  string,
  'secondary' | 'outline' | 'destructive'
> = {
  active: 'secondary',
  invited: 'outline',
  disabled: 'destructive',
}

export interface UserColumnOptions {
  authUserId: string | undefined
  onRoleChange: (userId: string, email: string, newRole: UserRole) => void
  onDisable: (userId: string, email: string) => void
  onReEnable: (userId: string, email: string) => void
}

export function UserRoleBadge({ role }: { role: UserRole }) {
  return (
    <Badge variant={ROLE_BADGE_VARIANT[role] ?? 'outline'} className="text-xs">
      {ROLE_LABELS[role] ?? role}
    </Badge>
  )
}

export function UserStatusBadge({ status }: { status: User['status'] }) {
  return (
    <Badge
      variant={STATUS_BADGE_VARIANT[status] ?? 'outline'}
      className="text-xs capitalize"
    >
      {status}
    </Badge>
  )
}

interface UserActionsProps extends UserColumnOptions {
  user: User
}

export function UserActions({
  authUserId,
  onDisable,
  onReEnable,
  onRoleChange,
  user,
}: UserActionsProps) {
  const isOwner = user.role === 'owner'
  const isSelf = user.id === authUserId
  const isDisabled = user.status === 'disabled'

  if (isOwner || isSelf) return null

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Open actions for ${user.email}`}
            title={`Open actions for ${user.email}`}
          />
        }
      >
        <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-auto">
        {!isDisabled ? (
          <>
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>Change role</DropdownMenuSubTrigger>
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
              Disable user
            </DropdownMenuItem>
          </>
        ) : (
          <DropdownMenuItem onClick={() => onReEnable(user.id, user.email)}>
            <HugeiconsIcon icon={UserCheck01Icon} size={14} />
            Re-enable user
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function getColumns(options: UserColumnOptions): Array<ColumnDef<User>> {
  const { authUserId } = options

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
      header: 'Email',
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
      header: 'Role',
      cell: ({ row }) => <UserRoleBadge role={row.original.role} />,
    },
    {
      accessorKey: 'status',
      header: 'Status',
      cell: ({ row }) => <UserStatusBadge status={row.original.status} />,
    },
    {
      accessorKey: 'created_at',
      header: 'Joined',
      cell: ({ row }) => (
        <span
          className="text-xs text-muted-foreground"
          title={new Date(row.original.created_at * 1000).toLocaleString()}
        >
          {relativeTime(row.original.created_at)}
        </span>
      ),
    },
    {
      id: 'actions',
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) => (
        <div className="text-right">
          <UserActions user={row.original} {...options} />
        </div>
      ),
    },
  ]
}
