import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowRight01Icon } from '@hugeicons/core-free-icons'
import type { PreferencesPageState } from '@/routes/settings/preferences'
import { Button } from '@/components/ui/button'

export function ExternalAccessManagement({
  state,
}: {
  state: PreferencesPageState
}) {
  const {
    isOwner,
    networkSettings,
    networkSettingsQuery,
    preloadExternalAccessNetworkDialog,
    preloadOidcSettingsDialog,
    preloadTrustedProxySettingsDialog,
    remoteAuthMode,
    setNetworkEditorOpen,
    setOidcDialogOpen,
    setTrustedProxyDialogOpen,
    trustedProxySettings,
  } = state
  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
        Manage External Access
      </p>
      <div className="grid gap-1 md:grid-cols-2">
        <Button
          type="button"
          variant="ghost"
          onMouseEnter={() => void preloadExternalAccessNetworkDialog()}
          onFocus={() => void preloadExternalAccessNetworkDialog()}
          onClick={() => setNetworkEditorOpen(true)}
          disabled={!isOwner || networkSettingsQuery.isLoading}
          className="group h-auto w-full justify-start whitespace-normal p-3 text-left"
        >
          <span className="min-w-0 flex-1">
            <span className="block text-sm font-medium">Network settings</span>
            <span className="mt-1 block truncate text-xs text-muted-foreground">
              {networkSettings?.public_url ??
                'Set Public URL and allowed origins.'}
            </span>
            <span className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
              Edit
              <HugeiconsIcon icon={ArrowRight01Icon} data-icon="inline-end" />
            </span>
          </span>
        </Button>

        <Button
          type="button"
          variant="ghost"
          onMouseEnter={() =>
            void (remoteAuthMode === 'trusted_proxy'
              ? preloadTrustedProxySettingsDialog()
              : preloadOidcSettingsDialog())
          }
          onFocus={() =>
            void (remoteAuthMode === 'trusted_proxy'
              ? preloadTrustedProxySettingsDialog()
              : preloadOidcSettingsDialog())
          }
          onClick={() =>
            remoteAuthMode === 'trusted_proxy'
              ? setTrustedProxyDialogOpen(true)
              : setOidcDialogOpen(true)
          }
          disabled={!isOwner}
          className="group h-auto w-full justify-start whitespace-normal p-3 text-left"
        >
          <span className="min-w-0 flex-1">
            <span className="block text-sm font-medium">Identity settings</span>
            <span className="mt-1 block text-xs text-muted-foreground">
              {remoteAuthMode === 'trusted_proxy'
                ? trustedProxySettings?.user_email_header ===
                  'x-warpgate-username'
                  ? trustedProxySettings.has_warpgate_ticket
                    ? `Warpgate identity and iOS installs configured (${trustedProxySettings.warpgate_ticket_source === 'environment' ? 'environment' : 'encrypted settings'} ticket).`
                    : 'Warpgate identity configured. Add an access ticket for iOS installs.'
                  : 'Update trusted proxy header, peer CIDRs, and secret.'
                : 'Update issuer and client credentials.'}
            </span>
            <span className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
              Edit
              <HugeiconsIcon icon={ArrowRight01Icon} data-icon="inline-end" />
            </span>
          </span>
        </Button>
      </div>
    </div>
  )
}
