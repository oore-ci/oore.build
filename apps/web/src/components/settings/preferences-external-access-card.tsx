import { useState, type ReactNode } from 'react'
import type { RemoteAuthMode } from '@oore/client/models'
import { authModeLabel } from '@/components/settings/preferences-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { Spinner } from '@/components/ui/spinner'
import ConfirmDialog from '@/components/ConfirmDialog'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'

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
  const [confirmOffOpen, setConfirmOffOpen] = useState(false)

  return (
    <SettingsSection
      title="External access"
      meta={
        <Badge variant={externalAccessEnabled ? 'secondary' : 'outline'}>
          {externalAccessEnabled ? authModeLabel(remoteAuthMode) : 'Local only'}
        </Badge>
      }
    >
      <SettingsSurface inset={false}>
        <Item>
          <ItemContent>
            <ItemTitle>Allow access from other devices</ItemTitle>
            <ItemDescription>
              {externalAccessEnabled
                ? 'Remote sign-in is available using the configured identity provider.'
                : 'Configure a public URL and identity provider before enabling remote sign-in.'}
            </ItemDescription>
          </ItemContent>
          <ItemActions>
            {isOwner ? (
              <Button
                type="button"
                variant={externalAccessEnabled ? 'outline' : 'default'}
                onClick={() =>
                  externalAccessEnabled ? setConfirmOffOpen(true) : onToggle()
                }
                disabled={
                  updatePending ||
                  (!externalAccessEnabled &&
                    (!readinessReady || preflightLoading))
                }
              >
                {updatePending ? (
                  <>
                    <Spinner />
                    Saving...
                  </>
                ) : externalAccessEnabled ? (
                  'Turn off'
                ) : (
                  'Turn on'
                )}
              </Button>
            ) : null}
          </ItemActions>
        </Item>
      </SettingsSurface>
      {!isOwner ? (
        <Alert>
          <AlertDescription>
            Only the owner can change external access.
          </AlertDescription>
        </Alert>
      ) : null}
      {children}
      <ConfirmDialog
        open={confirmOffOpen && externalAccessEnabled}
        onOpenChange={setConfirmOffOpen}
        title="Turn off external access?"
        description="Everyone who signs in from another device loses access, and remote sessions stop working, possibly including yours. Only sign-in on this machine keeps working. You will need to sign in again."
        confirmLabel="Turn off external access"
        confirmVariant="destructive"
        isPending={updatePending}
        onConfirm={onToggle}
      />
    </SettingsSection>
  )
}
