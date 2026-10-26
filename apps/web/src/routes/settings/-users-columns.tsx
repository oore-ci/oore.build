import type { User, UserRole } from '@oore/client/models'
import {
  DataTableColumnHeader,
  type DataTableColumnDef,
} from '@/components/data-table'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import { StatusBadge } from '@/components/status-badge'
import { RelativeTime } from '@/components/relative-time'
import { UserActions } from './-user-actions'
import { ROLE_LABELS } from './-user-role-labels'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

const ROLE_BADGE_VARIANT = {
  owner: 'outline',
  admin: 'outline',
  developer: 'secondary',
  qa_viewer: 'outline',
} satisfies Record<string, 'secondary' | 'outline'>

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

export function getColumns(
  options: UserColumnOptions,
): Array<DataTableColumnDef<User>> {
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
      enableHiding: false,
    },
    {
      accessorKey: 'email',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Email" />
      ),
      cell: ({ row }) => {
        const isSelf = row.original.id === authUserId
        return (
          <span>
            {row.original.email}
            {isSelf ? (
              <span
                className={cn(textVariants({ variant: 'caption' }), 'ml-2')}
              >
                (you)
              </span>
            ) : null}
          </span>
        )
      },
    },
    {
      accessorKey: 'role',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Role" />
      ),
      cell: ({ row }) => <UserRoleBadge role={row.original.role} />,
    },
    {
      accessorKey: 'status',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Status" />
      ),
      cell: ({ row }) => (
        <StatusBadge
          kind="user"
          value={row.original.status}
          className="text-xs"
        />
      ),
    },
    {
      accessorKey: 'created_at',
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title="Joined" />
      ),
      cell: ({ row }) => (
        <RelativeTime
          at={row.original.created_at}
          className={textVariants({ variant: 'caption' })}
        />
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
      enableSorting: false,
      enableHiding: false,
    },
  ]
}
