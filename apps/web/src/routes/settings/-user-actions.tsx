import {
  Cancel01Icon,
  MoreHorizontalCircle01Icon,
  UserCheck01Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import type { User, UserRole } from '@/lib/types'
import { Button } from '@/components/ui/button'
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
import { ROLE_LABELS } from './-user-role-labels'

export function UserActions({
  authUserId,
  onDisable,
  onReEnable,
  onRoleChange,
  user,
}: {
  authUserId: string | undefined
  onDisable: (userId: string, email: string) => void
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
