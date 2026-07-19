import { lazy, Suspense, useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { UserPlus as UserAdd01Icon } from 'lucide-react'

import { Button } from '@/components/ui/button'

const loadInviteUserDialog = () => import('./-invite-user-dialog')
const InviteUserDialog = lazy(loadInviteUserDialog)

export function InviteUserAction() {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        onMouseEnter={() => void loadInviteUserDialog()}
        onFocus={() => void loadInviteUserDialog()}
        onClick={() => setOpen(true)}
      >
        <DynamicLucideIcon
          icon={UserAdd01Icon}
          data-icon="inline-start"
          aria-hidden
        />
        Invite user
      </Button>
      {open ? (
        <Suspense fallback={null}>
          <InviteUserDialog open onOpenChange={setOpen} />
        </Suspense>
      ) : null}
    </>
  )
}
