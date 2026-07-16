import { lazy, Suspense, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { UserAdd01Icon } from '@hugeicons/core-free-icons'

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
        <HugeiconsIcon
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
