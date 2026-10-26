import { HugeiconsIcon } from '@hugeicons/react'
import {
  Cancel01Icon,
  FolderAddIcon,
  UserCheck01Icon,
} from '@hugeicons/core-free-icons'

import type { User, UserRole } from '@oore/client/models'
import { DataTableRowActionsTrigger } from '@/components/data-table'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from '@/components/ui/dropdown-menu'
import { ROLE_LABELS } from './-user-role-labels'

export function UserActions({
  authUserId,
  onDisable,
  onGrantAccess,
  onReEnable,
  onRoleChange,
  user,
}: {
  authUserId: string | undefined
  onDisable: (userId: string, email: string) => void
  onGrantAccess: (user: User) => void
  onReEnable: (userId: string, email: string) => void
  onRoleChange: (userId: string, email: string, newRole: UserRole) => void
  user: User
}) {
  const isOwner = user.role === 'owner'
  const isSelf = user.id === authUserId
  const isDisabled = user.status === 'disabled'

  if (isOwner || isSelf) return null

  return (
    <DropdownMenu>
      <DataTableRowActionsTrigger label={`Actions for ${user.email}`} />
      <DropdownMenuContent align="end" className="w-52">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Actions</DropdownMenuLabel>
        </DropdownMenuGroup>
        {!isDisabled ? (
          <>
            <DropdownMenuGroup>
              {user.role !== 'admin' ? (
                <DropdownMenuItem onClick={() => onGrantAccess(user)}>
                  <HugeiconsIcon icon={FolderAddIcon} />
                  Give project access
                </DropdownMenuItem>
              ) : null}
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
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuItem
                variant="destructive"
                onClick={() => onDisable(user.id, user.email)}
              >
                <HugeiconsIcon icon={Cancel01Icon} />
                Disable user
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </>
        ) : (
          <DropdownMenuGroup>
            <DropdownMenuItem onClick={() => onReEnable(user.id, user.email)}>
              <HugeiconsIcon icon={UserCheck01Icon} />
              Re-enable user
            </DropdownMenuItem>
          </DropdownMenuGroup>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
