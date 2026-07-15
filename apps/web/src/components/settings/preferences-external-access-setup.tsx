import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowDown01Icon,
  ArrowRight01Icon,
  CheckmarkCircle02Icon,
} from '@hugeicons/core-free-icons'
import type { PreferencesPageState } from '@/routes/settings/preferences'
import {
  authModeLabel,
  guidanceForPreflight,
} from '@/components/settings/preferences-utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Spinner } from '@/components/ui/spinner'

export function ExternalAccessSetup({
  state,
}: {
  state: PreferencesPageState
}) {
  const {
    failedReadinessChecks,
    identityReady,
    isOwner,
    networkReady,
    networkSettings,
    networkSettingsQuery,
    oidcConfig,
    preflightQuery,
    preloadExternalAccessNetworkDialog,
    preloadOidcSettingsDialog,
    preloadTrustedProxySettingsDialog,
    readinessOpen,
    readinessReady,
    remoteAuthMode,
    setNetworkEditorOpen,
    setOidcDialogOpen,
    setReadinessOpen,
    setTrustedProxyDialogOpen,
    setupReady,
    setupStepCount,
    setupStepsComplete,
    trustedProxySettings,
  } = state
  return (
    <>
      <div className="space-y-3 border p-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Setup Steps
            </p>
            {preflightQuery.isLoading ? (
              <p className="mt-1 flex items-center gap-2 text-sm text-muted-foreground">
                <Spinner className="size-4" />
                Checking requirements...
              </p>
            ) : (
              <p className="mt-1 text-sm text-muted-foreground">
                {setupStepsComplete}/{setupStepCount} setup steps ready.
              </p>
            )}
          </div>
          <Badge variant={readinessReady ? 'success' : 'secondary'}>
            {readinessReady
              ? 'Ready to enable'
              : `${setupStepsComplete}/${setupStepCount} ready`}
          </Badge>
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          <button
            type="button"
            onMouseEnter={() => void preloadExternalAccessNetworkDialog()}
            onFocus={() => void preloadExternalAccessNetworkDialog()}
            onClick={() => setNetworkEditorOpen(true)}
            disabled={!isOwner || networkSettingsQuery.isLoading}
            className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-sm font-medium">1. Network</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {networkSettings?.public_url ??
                    'Set Public URL and allowed origins.'}
                </p>
              </div>
              <Badge variant={networkReady ? 'success' : 'outline'}>
                {networkReady ? 'Ready' : 'Setup'}
              </Badge>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              {networkSettings?.allowed_origins.length ?? 0} allowed origins
            </p>
            <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
              Configure
              <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
            </p>
          </button>

          <button
            type="button"
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
            className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-sm font-medium">2. Identity</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {identityReady
                    ? `${authModeLabel(remoteAuthMode)} configured.`
                    : `Configure ${authModeLabel(remoteAuthMode)}.`}
                </p>
              </div>
              <Badge variant={identityReady ? 'success' : 'outline'}>
                {identityReady ? 'Ready' : 'Setup'}
              </Badge>
            </div>
            {remoteAuthMode === 'trusted_proxy' && trustedProxySettings ? (
              <div className="mt-2 space-y-1 text-xs text-muted-foreground">
                <p>
                  <span className="font-medium text-foreground">Header:</span>{' '}
                  <span className="font-mono">
                    {trustedProxySettings.user_email_header}
                  </span>
                </p>
                <p>
                  <span className="font-medium text-foreground">Secret:</span>{' '}
                  {trustedProxySettings.has_shared_secret
                    ? 'Stored'
                    : 'Missing'}
                </p>
                {trustedProxySettings.user_email_header ===
                'x-warpgate-username' ? (
                  <p>
                    <span className="font-medium text-foreground">
                      iOS installs:
                    </span>{' '}
                    {trustedProxySettings.has_warpgate_ticket
                      ? `Ticket from ${trustedProxySettings.warpgate_ticket_source === 'environment' ? 'environment' : 'encrypted settings'}`
                      : 'Ticket missing'}
                  </p>
                ) : null}
                <p>
                  <span className="font-medium text-foreground">
                    Peer CIDRs:
                  </span>{' '}
                  {trustedProxySettings.trusted_proxy_cidrs.length > 0
                    ? trustedProxySettings.trusted_proxy_cidrs.join(', ')
                    : 'Loopback only'}
                </p>
              </div>
            ) : remoteAuthMode === 'oidc' && oidcConfig ? (
              <div className="mt-2 space-y-1 text-xs text-muted-foreground">
                <p>
                  <span className="font-medium text-foreground">Issuer:</span>{' '}
                  <span className="font-mono">{oidcConfig.issuer_url}</span>
                </p>
                <p>
                  <span className="font-medium text-foreground">
                    Client ID:
                  </span>{' '}
                  <span className="font-mono">{oidcConfig.client_id}</span>
                </p>
                <p>
                  <span className="font-medium text-foreground">Secret:</span>{' '}
                  {oidcConfig.has_client_secret
                    ? 'Stored'
                    : 'None (public client)'}
                </p>
              </div>
            ) : null}
            <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
              {identityReady ? 'Reconfigure' : 'Configure'}
              <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
            </p>
          </button>
        </div>
      </div>

      {!setupReady ? (
        <Alert variant="destructive">
          <AlertDescription>
            Complete setup before enabling External Access.
          </AlertDescription>
        </Alert>
      ) : null}

      {networkSettingsQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load network settings:{' '}
            {networkSettingsQuery.error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      <Collapsible
        open={readinessOpen}
        onOpenChange={setReadinessOpen}
        className="space-y-3 border p-3"
      >
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Technical checks
            </p>
            {preflightQuery.isLoading ? (
              <p className="mt-1 flex items-center gap-2 text-sm text-muted-foreground">
                <Spinner className="size-4" />
                Checking...
              </p>
            ) : preflightQuery.error ? (
              <p className="mt-1 text-sm text-destructive">Check run failed.</p>
            ) : preflightQuery.data?.ready ? (
              <p className="mt-1 text-sm text-muted-foreground">
                All checks are passing.
              </p>
            ) : (
              <p className="mt-1 text-sm text-muted-foreground">
                {failedReadinessChecks.length} check
                {failedReadinessChecks.length === 1 ? '' : 's'} need attention.
              </p>
            )}
          </div>
          <CollapsibleTrigger className="inline-flex items-center gap-1 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground">
            <HugeiconsIcon
              icon={readinessOpen ? ArrowDown01Icon : ArrowRight01Icon}
              size={14}
            />
            {readinessOpen ? 'Hide checks' : 'Show checks'}
          </CollapsibleTrigger>
        </div>

        {preflightQuery.error ? (
          <Alert variant="destructive">
            <AlertDescription>
              Failed to run readiness checks: {preflightQuery.error.message}
            </AlertDescription>
          </Alert>
        ) : null}

        <CollapsibleContent className="space-y-2">
          {preflightQuery.data
            ? preflightQuery.data.checks.map((check) => (
                <div key={check.id} className="border border-border/60 p-3">
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-start gap-2">
                      <HugeiconsIcon
                        icon={
                          check.ok ? CheckmarkCircle02Icon : AlertCircleIcon
                        }
                        size={14}
                        className={
                          check.ok ? 'text-success' : 'text-destructive'
                        }
                      />
                      <div>
                        <p className="text-sm font-medium">{check.label}</p>
                        <p className="text-xs text-muted-foreground">
                          {check.ok
                            ? check.message
                            : guidanceForPreflight(
                                check.id,
                                check.failure_code,
                              )}
                        </p>
                      </div>
                    </div>
                    <Badge
                      variant={check.ok ? 'success' : 'warning'}
                      className="mt-0.5"
                    >
                      {check.ok ? 'Ready' : 'Needs setup'}
                    </Badge>
                  </div>
                </div>
              ))
            : null}
        </CollapsibleContent>
      </Collapsible>
    </>
  )
}
