import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowRight01Icon } from '@hugeicons/core-free-icons'
import type {
  useExternalAccessNetworkSettings,
  useExternalAccessOidc,
  useExternalAccessTrustedProxySettings,
} from '@/hooks/use-artifact-storage'
import type {
  RemoteAuthMode,
  TrustedProxySettingsPublic,
} from '@oore/client/models'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item'
import { SettingsSurface } from '@/components/settings/settings-section'
import { Stack } from '@/components/layout/stack'
import { splitRowVariants } from '@/lib/recipes'

export function ExternalAccessManagement({
  identityQuery,
  isOwner,
  networkSettingsQuery,
  onEditIdentity,
  onEditNetwork,
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
  remoteAuthMode: RemoteAuthMode
  trustedProxySettings: TrustedProxySettingsPublic | undefined
}) {
  return (
    <Stack gap={2}>
      <SettingsSurface inset={false}>
        <ItemGroup className="gap-0">
          <Item
            render={
              <button
                type="button"
                disabled={
                  !isOwner ||
                  networkSettingsQuery.isLoading ||
                  !!networkSettingsQuery.error
                }
              />
            }
            onClick={onEditNetwork}
            className="disabled:pointer-events-none disabled:opacity-50"
          >
            <ItemContent>
              <ItemTitle>Network settings</ItemTitle>
              <ItemDescription>
                {networkSettingsQuery.data?.public_url ??
                  'Set Public URL and allowed origins.'}
              </ItemDescription>
            </ItemContent>
            <ItemActions>
              <HugeiconsIcon icon={ArrowRight01Icon} />
            </ItemActions>
          </Item>

          <ItemSeparator className="my-0" />

          <Item
            render={
              <button
                type="button"
                disabled={
                  !isOwner || identityQuery.isLoading || !!identityQuery.error
                }
              />
            }
            onClick={onEditIdentity}
            className="disabled:pointer-events-none disabled:opacity-50"
          >
            <ItemContent>
              <ItemTitle>Identity settings</ItemTitle>
              <ItemDescription className="line-clamp-none">
                {remoteAuthMode === 'trusted_proxy'
                  ? trustedProxySettings?.user_email_header ===
                    'x-warpgate-username'
                    ? trustedProxySettings.has_warpgate_ticket
                      ? `Warpgate identity and iOS installs configured (${trustedProxySettings.warpgate_ticket_source === 'environment' ? 'environment' : 'encrypted settings'} ticket).`
                      : 'Warpgate identity configured. Add an access ticket for iOS installs.'
                    : 'Update trusted proxy header, peer CIDRs, and secret.'
                  : 'Update issuer and client credentials.'}
              </ItemDescription>
            </ItemContent>
            <ItemActions>
              <HugeiconsIcon icon={ArrowRight01Icon} />
            </ItemActions>
          </Item>
        </ItemGroup>
      </SettingsSurface>
      {networkSettingsQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription className={splitRowVariants()}>
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
          <AlertDescription className={splitRowVariants()}>
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
    </Stack>
  )
}
