import type { ReactNode } from 'react'
import type { RemoteAuthMode } from '@/lib/types'
import {
  authModeDescription,
  authModeLabel,
} from '@/components/settings/preferences-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
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
    <Card size="sm" aria-labelledby="external-access-title">
      <CardHeader>
        <CardTitle id="external-access-title">External access</CardTitle>
        <CardAction>
          <Badge variant={externalAccessEnabled ? 'secondary' : 'outline'}>
            {externalAccessEnabled
              ? `External Access: ${authModeLabel(remoteAuthMode)}`
              : 'Local Only'}
          </Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
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
      </CardContent>
    </Card>
  )
}
