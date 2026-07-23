import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { ArrowRight as ArrowRight01Icon } from 'lucide-react'
import type {
  useExternalAccessNetworkSettings,
  useExternalAccessOidc,
  useExternalAccessTrustedProxySettings,
} from '@/hooks/use-artifact-storage'
import type { RemoteAuthMode, TrustedProxySettingsPublic } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'

export function ExternalAccessManagement({
  identityQuery,
  isOwner,
  networkSettingsQuery,
  onEditIdentity,
  onEditNetwork,
  onPreloadIdentity,
  onPreloadNetwork,
  remoteAuthMode,
  trustedProxySettings,
}: {
  identityQuery:
    | ReturnType<typeof useExternalAccessOidc>
    | ReturnType<typeof useExternalAccessTrustedProxySettings>
  isOwner: boolean
  networkSettingsQuery: ReturnType<typeof useExternalAccessNetworkSettings>
  onEditIdentity: () => void
  onEditNetwork: () => void
  onPreloadIdentity: () => void
  onPreloadNetwork: () => void
  remoteAuthMode: RemoteAuthMode
  trustedProxySettings: TrustedProxySettingsPublic | undefined
}) {
  return (
    <div className="flex flex-col gap-2">
      <h3 className="text-sm font-medium text-muted-foreground">
        Manage External Access
      </h3>
      <div className="grid gap-1 md:grid-cols-2">
        <Button
          type="button"
          variant="ghost"
          onMouseEnter={onPreloadNetwork}
          onFocus={onPreloadNetwork}
          onClick={onEditNetwork}
          disabled={
            !isOwner ||
            networkSettingsQuery.isLoading ||
            !!networkSettingsQuery.error
          }
          className="group h-auto w-full justify-start px-0 py-2 text-left whitespace-normal"
        >
          <span className="min-w-0 flex-1">
            <span className="block text-sm font-medium">Network settings</span>
            <span className="mt-1 block truncate text-xs text-muted-foreground">
              {networkSettingsQuery.data?.public_url ??
                'Set Public URL and allowed origins.'}
            </span>
            <span className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
              Edit
              <DynamicLucideIcon
                icon={ArrowRight01Icon}
                data-icon="inline-end"
              />
            </span>
          </span>
        </Button>

        <Button
          type="button"
          variant="ghost"
          onMouseEnter={onPreloadIdentity}
          onFocus={onPreloadIdentity}
          onClick={onEditIdentity}
          disabled={
            !isOwner || identityQuery.isLoading || !!identityQuery.error
          }
          className="group h-auto w-full justify-start px-0 py-2 text-left whitespace-normal"
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
              <DynamicLucideIcon
                icon={ArrowRight01Icon}
                data-icon="inline-end"
              />
            </span>
          </span>
        </Button>
      </div>
      {networkSettingsQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              Failed to load network settings:{' '}
              {networkSettingsQuery.error.message}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void networkSettingsQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}
      {identityQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              Failed to load identity settings: {identityQuery.error.message}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void identityQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}
    </div>
  )
}
