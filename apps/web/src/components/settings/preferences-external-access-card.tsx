import type { ReactNode } from 'react'
import type { RemoteAuthMode } from '@/lib/types'
import {
  authModeDescription,
  authModeLabel,
} from '@/components/settings/preferences-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'

export function ExternalAccessCard({
  children,
  externalAccessEnabled,
  isOwner,
  onToggle,
  preflightLoading,
  readinessReady,
  remoteAuthMode,
  updatePending,
}: {
  children: ReactNode
  externalAccessEnabled: boolean
  isOwner: boolean
  onToggle: () => void
  preflightLoading: boolean
  readinessReady: boolean
  remoteAuthMode: RemoteAuthMode
  updatePending: boolean
}) {
  return (
    <section className="border bg-card" aria-labelledby="external-access-title">
      <div className="border-b px-4 py-3">
        <div className="flex items-center justify-between gap-3">
          <h2 id="external-access-title" className="text-sm font-semibold">
            External Access
          </h2>
          <Badge variant={externalAccessEnabled ? 'secondary' : 'outline'}>
            {externalAccessEnabled
              ? `External Access: ${authModeLabel(remoteAuthMode)}`
              : 'Local Only'}
          </Badge>
        </div>
      </div>
      <div className="flex flex-col gap-5 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex flex-col gap-1">
            <p className="text-sm font-medium">Current access</p>
            <p className="text-xs text-muted-foreground">
              {externalAccessEnabled
                ? authModeDescription(remoteAuthMode)
                : 'Local Only is active. Sign-in is limited to localhost on this machine.'}
            </p>
          </div>
          {isOwner ? (
            <Button
              type="button"
              onClick={onToggle}
              disabled={
                updatePending ||
                (!externalAccessEnabled &&
                  (!readinessReady || preflightLoading))
              }
            >
              {updatePending ? (
                <>
                  <Spinner className="size-4" />
                  Saving...
                </>
              ) : externalAccessEnabled ? (
                'Turn off External Access'
              ) : (
                'Turn on External Access'
              )}
            </Button>
          ) : null}
        </div>
        {!isOwner ? (
          <Alert>
            <AlertDescription>
              Only the owner can change External Access.
            </AlertDescription>
          </Alert>
        ) : null}
        {children}
      </div>
    </section>
  )
}
