import type { ColumnDef } from '@tanstack/react-table'

import type { User, UserRole } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import { relativeTime } from '@/lib/format-utils'
import { UserActions } from './-user-actions'
import { ROLE_LABELS } from './-user-role-labels'

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

function UserRoleBadge({ role }: { role: UserRole }) {
  return (
    <Badge variant={ROLE_BADGE_VARIANT[role] ?? 'outline'} className="text-xs">
      {ROLE_LABELS[role] ?? role}
    </Badge>
  )
}

function UserStatusBadge({ status }: { status: User['status'] }) {
  return (
    <Badge
      variant={STATUS_BADGE_VARIANT[status] ?? 'outline'}
      className="text-xs capitalize"
    >
      {status}
    </Badge>
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
