import type { PreferencesPageState } from '@/routes/settings/preferences'
import {
  authModeDescription,
  authModeLabel,
} from '@/components/settings/preferences-utils'
import { ExternalAccessManagement } from '@/components/settings/preferences-external-access-management'
import { ExternalAccessSetup } from '@/components/settings/preferences-external-access-setup'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Spinner } from '@/components/ui/spinner'

export function ExternalAccessCard({ state }: { state: PreferencesPageState }) {
  const {
    externalAccessEnabled,
    handleExternalAccessToggle,
    isOwner,
    preflightQuery,
    readinessReady,
    remoteAuthMode,
    updatePreferencesMutation,
  } = state
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-3">
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            External Access
          </CardTitle>
          <Badge variant={externalAccessEnabled ? 'default' : 'secondary'}>
            {externalAccessEnabled
              ? `External Access - ${authModeLabel(remoteAuthMode)}`
              : 'Local Only'}
          </Badge>
        </div>
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
              onClick={handleExternalAccessToggle}
              disabled={
                updatePreferencesMutation.isPending ||
                (!externalAccessEnabled &&
                  (!readinessReady || preflightQuery.isLoading))
              }
            >
              {updatePreferencesMutation.isPending ? (
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
        {externalAccessEnabled ? (
          <ExternalAccessManagement state={state} />
        ) : (
          <ExternalAccessSetup state={state} />
        )}
      </CardContent>
    </Card>
  )
}
