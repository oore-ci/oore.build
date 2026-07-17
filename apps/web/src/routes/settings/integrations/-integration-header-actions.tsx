import {
  Delete02Icon,
  LinkSquare02Icon,
  MoreHorizontalCircle01Icon,
  Refresh01Icon,
  Setting07Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

export function IntegrationHeaderActions({
  authorizePending,
  canSync,
  manageHref,
  manageLabel,
  needsAuthorization,
  onAuthorize,
  onDisconnect,
  onSync,
  syncLabel,
  syncPending,
}: {
  authorizePending: boolean
  canSync: boolean
  manageHref: string | null
  manageLabel: string
  needsAuthorization: boolean
  onAuthorize: () => void
  onDisconnect: () => void
  onSync: () => void
  syncLabel: string
  syncPending: boolean
}) {
  return (
    <>
      {needsAuthorization ? (
        <Button onClick={onAuthorize} disabled={authorizePending}>
          <HugeiconsIcon icon={LinkSquare02Icon} size={16} />
          {authorizePending ? 'Redirecting...' : 'Authorize GitLab'}
        </Button>
      ) : canSync ? (
        <Button onClick={onSync} disabled={syncPending}>
          <HugeiconsIcon icon={Refresh01Icon} />
          {syncPending ? 'Syncing...' : syncLabel}
        </Button>
      ) : null}

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button variant="outline" size="icon" aria-label="Source actions" />
          }
        >
          <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-auto">
          {manageHref ? (
            <DropdownMenuItem
              onClick={() =>
                window.open(manageHref, '_blank', 'noopener,noreferrer')
              }
            >
              <HugeiconsIcon icon={Setting07Icon} />
              {manageLabel}
            </DropdownMenuItem>
          ) : null}
          <DropdownMenuItem variant="destructive" onClick={onDisconnect}>
            <HugeiconsIcon icon={Delete02Icon} />
            Disconnect source
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  )
}
