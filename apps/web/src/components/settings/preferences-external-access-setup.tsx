import {
  CircleAlert as AlertCircleIcon,
  ArrowDown as ArrowDown01Icon,
  ArrowRight as ArrowRight01Icon,
  CircleCheck as CheckmarkCircle02Icon,
} from 'lucide-react'
import type {
  useExternalAccessNetworkSettings,
  useExternalAccessOidc,
  useExternalAccessPreflight,
  useExternalAccessTrustedProxySettings,
} from '@/hooks/use-artifact-storage'
import type {
  GetExternalAccessOidcResponse,
  RemoteAuthMode,
  TrustedProxySettingsPublic,
} from '@/lib/types'
import {
  authModeLabel,
  guidanceForPreflight,
} from '@/components/settings/preferences-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { Spinner } from '@/components/ui/spinner'

export function ExternalAccessSetup({
  identityQuery,
  identityReady,
  isOwner,
  networkReady,
  networkSettingsQuery,
  oidcConfig,
  onEditIdentity,
  onEditNetwork,
  onPreloadIdentity,
  onPreloadNetwork,
  onReadinessOpenChange,
  preflightQuery,
  readinessOpen,
  readinessReady,
  remoteAuthMode,
  setupReady,
  setupStepsComplete,
  trustedProxySettings,
}: {
  identityQuery:
    | ReturnType<typeof useExternalAccessOidc>
    | ReturnType<typeof useExternalAccessTrustedProxySettings>
  identityReady: boolean
  isOwner: boolean
  networkReady: boolean
  networkSettingsQuery: ReturnType<typeof useExternalAccessNetworkSettings>
  oidcConfig: GetExternalAccessOidcResponse | undefined
  onEditIdentity: () => void
  onEditNetwork: () => void
  onPreloadIdentity: () => void
  onPreloadNetwork: () => void
  onReadinessOpenChange: (open: boolean) => void
  preflightQuery: ReturnType<typeof useExternalAccessPreflight>
  readinessOpen: boolean
  readinessReady: boolean
  remoteAuthMode: RemoteAuthMode
  setupReady: boolean
  setupStepsComplete: number
  trustedProxySettings: TrustedProxySettingsPublic | undefined
}) {
  const networkSettings = networkSettingsQuery.data
  const failedReadinessChecks =
    preflightQuery.data?.checks.filter((check) => !check.ok) ?? []
  const setupStepCount = 2
  const ReadinessIcon = readinessOpen ? ArrowDown01Icon : ArrowRight01Icon
  return (
    <>
      <Card size="sm">
        <CardHeader>
          <CardTitle>Setup steps</CardTitle>
          <CardDescription>
            {preflightQuery.isLoading ? (
              <span className="flex items-center gap-2">
                <Spinner className="size-4" />
                Checking requirements...
              </span>
            ) : (
              <>
                {setupStepsComplete}/{setupStepCount} setup steps ready.
              </>
            )}
          </CardDescription>
          <CardAction>
            <Badge variant={readinessReady ? 'secondary' : 'outline'}>
              {readinessReady
                ? 'Ready to enable'
                : `${setupStepsComplete}/${setupStepCount} ready`}
            </Badge>
          </CardAction>
        </CardHeader>

        <CardContent>
          <ItemGroup className="grid gap-3 md:grid-cols-2">
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
              variant="outline"
              className="disabled:pointer-events-none disabled:opacity-50"
              onMouseEnter={onPreloadNetwork}
              onFocus={onPreloadNetwork}
              onClick={onEditNetwork}
            >
              <ItemContent>
                <ItemTitle>1. Network</ItemTitle>
                <ItemDescription>
                  {networkSettings?.public_url ??
                    'Set Public URL and allowed origins.'}
                </ItemDescription>
                <ItemDescription>
                  {networkSettings?.allowed_origins.length ?? 0} allowed origins
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                <Badge variant={networkReady ? 'secondary' : 'outline'}>
                  {networkReady ? 'Ready' : 'Setup'}
                </Badge>
                <ArrowRight01Icon />
              </ItemActions>
            </Item>

            <Item
              render={
                <button
                  type="button"
                  disabled={
                    !isOwner || identityQuery.isLoading || !!identityQuery.error
                  }
                />
              }
              variant="outline"
              className="disabled:pointer-events-none disabled:opacity-50"
              onMouseEnter={onPreloadIdentity}
              onFocus={onPreloadIdentity}
              onClick={onEditIdentity}
            >
              <ItemContent>
                <ItemTitle>2. Identity</ItemTitle>
                <ItemDescription>
                  {identityReady
                    ? `${authModeLabel(remoteAuthMode)} configured.`
                    : `Configure ${authModeLabel(remoteAuthMode)}.`}
                </ItemDescription>
                {remoteAuthMode === 'trusted_proxy' && trustedProxySettings ? (
                  <>
                    <ItemDescription>
                      Header: {trustedProxySettings.user_email_header}
                    </ItemDescription>
                    <ItemDescription>
                      Secret:{' '}
                      {trustedProxySettings.has_shared_secret
                        ? 'Stored'
                        : 'Missing'}
                    </ItemDescription>
                    {trustedProxySettings.user_email_header ===
                    'x-warpgate-username' ? (
                      <ItemDescription>
                        iOS installs:{' '}
                        {trustedProxySettings.has_warpgate_ticket
                          ? `Ticket from ${trustedProxySettings.warpgate_ticket_source === 'environment' ? 'environment' : 'encrypted settings'}`
                          : 'Ticket missing'}
                      </ItemDescription>
                    ) : null}
                    <ItemDescription>
                      Peer CIDRs:{' '}
                      {trustedProxySettings.trusted_proxy_cidrs.length > 0
                        ? trustedProxySettings.trusted_proxy_cidrs.join(', ')
                        : 'Loopback only'}
                    </ItemDescription>
                  </>
                ) : remoteAuthMode === 'oidc' && oidcConfig ? (
                  <>
                    <ItemDescription>
                      Issuer: {oidcConfig.issuer_url}
                    </ItemDescription>
                    <ItemDescription>
                      Client ID: {oidcConfig.client_id}
                    </ItemDescription>
                    <ItemDescription>
                      Secret:{' '}
                      {oidcConfig.has_client_secret
                        ? 'Stored'
                        : 'None (public client)'}
                    </ItemDescription>
                  </>
                ) : null}
              </ItemContent>
              <ItemActions>
                <Badge variant={identityReady ? 'secondary' : 'outline'}>
                  {identityReady ? 'Ready' : 'Setup'}
                </Badge>
                <ArrowRight01Icon />
              </ItemActions>
            </Item>
          </ItemGroup>
        </CardContent>
      </Card>

      {!setupReady ? (
        <Alert variant="destructive">
          <AlertDescription>
            Complete setup before enabling External Access.
          </AlertDescription>
        </Alert>
      ) : null}

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

      <Collapsible open={readinessOpen} onOpenChange={onReadinessOpenChange}>
        <Card size="sm">
          <CardHeader>
            <CardTitle>Technical checks</CardTitle>
            <CardDescription>
              {preflightQuery.isLoading ? (
                <span className="flex items-center gap-2">
                  <Spinner className="size-4" />
                  Checking...
                </span>
              ) : preflightQuery.error ? (
                <span className="text-destructive">Check run failed.</span>
              ) : preflightQuery.data?.ready ? (
                <>All checks are passing.</>
              ) : (
                <>
                  {failedReadinessChecks.length} check
                  {failedReadinessChecks.length === 1 ? '' : 's'} need
                  attention.
                </>
              )}
            </CardDescription>
            <CardAction>
              <CollapsibleTrigger
                render={<Button type="button" variant="ghost" size="sm" />}
              >
                <ReadinessIcon />
                {readinessOpen ? 'Hide checks' : 'Show checks'}
              </CollapsibleTrigger>
            </CardAction>
          </CardHeader>

          <CardContent className="space-y-3">
            {preflightQuery.error ? (
              <Alert variant="destructive">
                <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <span>
                    Failed to run readiness checks:{' '}
                    {preflightQuery.error.message}
                  </span>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => void preflightQuery.refetch()}
                  >
                    Retry
                  </Button>
                </AlertDescription>
              </Alert>
            ) : null}

            <CollapsibleContent>
              <ItemGroup>
                {preflightQuery.data
                  ? preflightQuery.data.checks.map((check) => {
                      const CheckIcon = check.ok
                        ? CheckmarkCircle02Icon
                        : AlertCircleIcon

                      return (
                        <Item key={check.id} variant="outline" size="sm">
                          <ItemMedia>
                            <CheckIcon
                              className={
                                check.ok ? 'text-success' : 'text-destructive'
                              }
                            />
                          </ItemMedia>
                          <ItemContent>
                            <ItemTitle>{check.label}</ItemTitle>
                            <ItemDescription>
                              {check.ok
                                ? check.message
                                : guidanceForPreflight(
                                    check.id,
                                    check.failure_code,
                                  )}
                            </ItemDescription>
                          </ItemContent>
                          <ItemActions>
                            <Badge variant={check.ok ? 'secondary' : 'outline'}>
                              {check.ok ? 'Ready' : 'Needs setup'}
                            </Badge>
                          </ItemActions>
                        </Item>
                      )
                    })
                  : null}
              </ItemGroup>
            </CollapsibleContent>
          </CardContent>
        </Card>
      </Collapsible>
    </>
  )
}
