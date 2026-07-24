import { lazy, Suspense, useMemo, useState } from 'react'
import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import { useAuthStore } from '@/stores/auth-store'
import { PageMeta } from '@/lib/seo'
import { useHasPermission } from '@/hooks/use-permissions'
import { useRuntimeUpdates } from '@/hooks/use-runtime-updates'
import {
  useConfigureExternalAccessOidc,
  useExternalAccessNetworkSettings,
  useExternalAccessOidc,
  useExternalAccessPreflight,
  useExternalAccessTrustedProxySettings,
  useInstancePreferences,
  useTestOidcConnection,
  useUpdateExternalAccessNetworkSettings,
  useUpdateExternalAccessTrustedProxySettings,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { ApiClientError, getApiErrorMessage } from '@/lib/api'
import { ExternalAccessCard } from '@/components/settings/preferences-external-access-card'
import { ExternalAccessManagement } from '@/components/settings/preferences-external-access-management'
import { ExternalAccessSetup } from '@/components/settings/preferences-external-access-setup'
import { RuntimeOverview } from '@/components/settings/preferences-runtime-overview'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'

const preloadExternalAccessNetworkDialog = () =>
  import('@/components/settings/preferences-external-access-network-dialog')
const preloadTrustedProxySettingsDialog = () =>
  import('@/components/settings/preferences-trusted-proxy-settings-dialog')
const preloadOidcSettingsDialog = () =>
  import('@/components/settings/preferences-oidc-settings-dialog')
const ExternalAccessNetworkDialog = lazy(preloadExternalAccessNetworkDialog)
const TrustedProxySettingsDialog = lazy(preloadTrustedProxySettingsDialog)
const OidcSettingsDialog = lazy(preloadOidcSettingsDialog)

export const Route = createLazyFileRoute('/settings/preferences')({
  component: PreferencesPage,
})

const externalAccessOidcSchema = z.object({
  issuer_url: z.url('Please enter a valid issuer URL'),
  client_id: z.string().min(1, 'Client ID is required'),
  client_secret: z.string().optional(),
})

export type ExternalAccessOidcFormValues = z.infer<
  typeof externalAccessOidcSchema
>

const trustedProxySchema = z.object({
  user_email_header: z.string().trim().min(1, 'User email header is required.'),
  trusted_proxy_cidrs: z.string().optional(),
  shared_secret: z.string().optional(),
  warpgate_ticket: z
    .string()
    .max(1024, 'Warpgate ticket must be 1024 characters or fewer.')
    .optional(),
  clear_warpgate_ticket: z.boolean(),
})

export type TrustedProxyFormValues = z.infer<typeof trustedProxySchema>

function parseTrustedProxyCidrs(value: string | undefined): Array<string> {
  return (value ?? '')
    .split(/[\n,]/g)
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0)
}

const externalAccessNetworkSchema = z.object({
  public_url: z.string().optional(),
  artifact_delivery_url: z.string().optional(),
  allowed_origins: z
    .string()
    .min(1, 'Add at least one allowed frontend origin.'),
})

export type ExternalAccessNetworkFormValues = z.infer<
  typeof externalAccessNetworkSchema
>

function healthVersionLabel(
  health: { version?: string } | undefined,
  isLoading: boolean,
  isError: boolean,
): string {
  if (isLoading) return 'Checking...'
  if (isError) return 'Unavailable'
  return health?.version?.trim() || 'Unknown'
}

function parseAllowedOriginsInput(value: string): Array<string> {
  return value
    .split(/[\n,]/g)
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0)
}

function DirectRunnerSettings({
  canWrite,
  isError,
  isPending,
  onToggle,
  paused,
}: {
  canWrite: boolean
  isError: boolean
  isPending: boolean
  onToggle: (acceptingBuilds: boolean) => void
  paused: boolean | undefined
}) {
  const acceptingBuilds = paused === false
  const runnerStateLabel = isError
    ? 'Unavailable'
    : paused === undefined
      ? 'Checking...'
      : acceptingBuilds
        ? 'Accepting builds'
        : 'Paused'

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-4">
          <CardTitle className="text-sm font-medium">
            Direct macOS runner
          </CardTitle>
          <Badge variant={acceptingBuilds ? 'secondary' : 'outline'}>
            {runnerStateLabel}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex items-start justify-between gap-6">
          <div className="space-y-1">
            <p className="text-sm font-medium">Accept new builds</p>
            <p className="text-sm text-muted-foreground">
              When paused, running builds finish and queued builds wait.
            </p>
          </div>
          <Switch
            checked={acceptingBuilds}
            disabled={!canWrite || paused === undefined || isError || isPending}
            onCheckedChange={onToggle}
            aria-label="Accept new builds"
            className="after:-inset-y-3.5"
          />
        </div>
        <Alert>
          <AlertDescription>
            {isError
              ? 'Oore could not load the runner pause state. Refresh this page before changing build intake.'
              : paused === undefined
                ? 'Checking whether this Mac is accepting new builds.'
                : !canWrite
                  ? 'An owner or admin can pause new claims while assigned and running builds finish.'
                  : 'Pausing stops new claims while assigned and running builds finish. Resume whenever this Mac is ready for more work.'}
          </AlertDescription>
        </Alert>
      </CardContent>
    </Card>
  )
}

function PreferencesPage() {
  const navigate = useNavigate()
  const [readinessOpen, setReadinessOpen] = useState(false)
  const [networkEditorOpen, setNetworkEditorOpen] = useState(false)
  const [oidcDialogOpen, setOidcDialogOpen] = useState(false)
  const [trustedProxyDialogOpen, setTrustedProxyDialogOpen] = useState(false)
  const canWrite = useHasPermission('instance_settings', 'write')
  const user = useAuthStore((s) => s.user)
  const clearAuth = useAuthStore((s) => s.clearAuth)
  const isOwner = user?.role === 'owner'
  const preferencesQuery = useInstancePreferences()
  const runtimeUpdates = useRuntimeUpdates()
  const webHealthQuery = runtimeUpdates.frontendHealth
  const backendHealthQuery = runtimeUpdates.backendHealth
  const frontendUpdatePhase =
    runtimeUpdates.startFrontendUpdate.data?.phase ??
    runtimeUpdates.frontendRelease.data?.phase
  const backendUpdateQueryIsCurrent =
    runtimeUpdates.backendUpdate.dataUpdatedAt >=
    runtimeUpdates.startBackendUpdate.submittedAt
  const backendUpdatePhase = backendUpdateQueryIsCurrent
    ? runtimeUpdates.backendUpdate.data?.phase
    : (runtimeUpdates.startBackendUpdate.data?.phase ??
      runtimeUpdates.backendUpdate.data?.phase)
  const preflightQuery = useExternalAccessPreflight()
  const networkSettingsQuery = useExternalAccessNetworkSettings()
  const oidcConfigQuery = useExternalAccessOidc()
  const trustedProxyQuery = useExternalAccessTrustedProxySettings()
  const configureExternalAccessOidcMutation = useConfigureExternalAccessOidc()
  const updateTrustedProxyMutation =
    useUpdateExternalAccessTrustedProxySettings()
  const testOidcConnectionMutation = useTestOidcConnection()
  const updateNetworkSettingsMutation = useUpdateExternalAccessNetworkSettings()
  const updatePreferencesMutation = useUpdateInstancePreferences()

  const oidcConfig = oidcConfigQuery.data
  const externalAccessOidcForm = useForm<ExternalAccessOidcFormValues>({
    resolver: zodResolver(externalAccessOidcSchema),
    defaultValues: {
      issuer_url: '',
      client_id: '',
      client_secret: '',
    },
    values: oidcConfig
      ? {
          issuer_url: oidcConfig.issuer_url,
          client_id: oidcConfig.client_id,
          client_secret: '',
        }
      : undefined,
    mode: 'onBlur',
  })
  const networkSettings = networkSettingsQuery.data
  const networkValues = useMemo(() => {
    if (!networkSettings) return undefined
    return {
      public_url: networkSettings.public_url ?? '',
      artifact_delivery_url: networkSettings.artifact_delivery_url ?? '',
      allowed_origins: networkSettings.allowed_origins.join('\n'),
    }
  }, [networkSettings])

  const externalAccessNetworkForm = useForm<ExternalAccessNetworkFormValues>({
    resolver: zodResolver(externalAccessNetworkSchema),
    defaultValues: {
      public_url: '',
      artifact_delivery_url: '',
      allowed_origins: '',
    },
    values: networkValues,
    mode: 'onBlur',
  })

  const trustedProxySettings = trustedProxyQuery.data
  const trustedProxyValues = useMemo(() => {
    if (!trustedProxySettings) return undefined
    return {
      user_email_header: trustedProxySettings.user_email_header,
      trusted_proxy_cidrs: trustedProxySettings.trusted_proxy_cidrs.join('\n'),
      shared_secret: '',
      warpgate_ticket: '',
      clear_warpgate_ticket: false,
    }
  }, [trustedProxySettings])

  const trustedProxyForm = useForm<TrustedProxyFormValues>({
    resolver: zodResolver(trustedProxySchema),
    defaultValues: {
      user_email_header: 'x-oore-user-email',
      trusted_proxy_cidrs: '',
      shared_secret: '',
      warpgate_ticket: '',
      clear_warpgate_ticket: false,
    },
    values: trustedProxyValues,
    mode: 'onBlur',
  })

  function onSubmitExternalAccessNetwork(
    values: ExternalAccessNetworkFormValues,
  ) {
    if (!isOwner) return

    const allowedOrigins = parseAllowedOriginsInput(values.allowed_origins)
    if (allowedOrigins.length === 0) {
      toast.error('Add at least one allowed frontend origin.')
      return
    }

    updateNetworkSettingsMutation.mutate(
      {
        public_url: values.public_url?.trim() || undefined,
        artifact_delivery_url:
          values.artifact_delivery_url?.trim() || undefined,
        allowed_origins: allowedOrigins,
      },
      {
        onSuccess: () => {
          toast.success('External Access network settings saved.')
          setNetworkEditorOpen(false)
          void preflightQuery.refetch()
        },
        onError: (error) => {
          toast.error(
            getApiErrorMessage(error, {
              external_access_owner_required:
                'Only the owner can update External Access network settings.',
              external_access_loopback_required:
                'In Local Only mode, network settings can only be changed from localhost on the host machine.',
              external_access_https_required: 'Public URL must use HTTPS.',
              artifact_delivery_https_required:
                'Artifact delivery URL must use HTTPS.',
              artifact_delivery_public_url_required:
                'Artifact delivery URL must use a non-loopback host.',
              artifact_delivery_url_invalid:
                'Artifact delivery URL is invalid.',
              external_access_origin_not_allowed:
                'Public URL origin must be included in allowed origins.',
              invalid_input:
                'Check Public URL and allowed origins. Each origin must be a valid http(s) origin.',
            }),
          )
        },
      },
    )
  }

  function onSubmitExternalAccessOidc(values: ExternalAccessOidcFormValues) {
    if (!isOwner) return

    configureExternalAccessOidcMutation.mutate(
      {
        issuer_url: values.issuer_url.trim(),
        client_id: values.client_id.trim(),
        ...(values.client_secret?.trim()
          ? { client_secret: values.client_secret.trim() }
          : {}),
      },
      {
        onSuccess: (response) => {
          toast.success(`OIDC configured: ${response.discovered_issuer}`)
          setOidcDialogOpen(false)
          externalAccessOidcForm.setValue('client_secret', '')
          void preflightQuery.refetch()
        },
        onError: (error) => {
          toast.error(
            getApiErrorMessage(error, {
              external_access_owner_required:
                'Only the owner can configure OIDC for External Access.',
              oidc_discovery_failed:
                'OIDC discovery failed. Verify issuer URL and provider availability.',
              invalid_input: 'Check issuer URL and client ID values.',
              invalid_state:
                'OIDC can be configured here only after setup is complete.',
            }),
          )
        },
      },
    )
  }

  function onSubmitTrustedProxy(values: TrustedProxyFormValues) {
    if (!isOwner) return

    const sharedSecret = values.shared_secret?.trim()
    const warpgateTicket = values.warpgate_ticket?.trim()
    const isWarpgate =
      values.user_email_header.trim().toLowerCase() === 'x-warpgate-username'
    updateTrustedProxyMutation.mutate(
      {
        user_email_header: values.user_email_header.trim(),
        trusted_proxy_cidrs: parseTrustedProxyCidrs(values.trusted_proxy_cidrs),
        ...(sharedSecret ? { shared_secret: sharedSecret } : {}),
        ...(isWarpgate
          ? values.clear_warpgate_ticket
            ? { warpgate_ticket: '' }
            : warpgateTicket
              ? { warpgate_ticket: warpgateTicket }
              : {}
          : {}),
      },
      {
        onSuccess: () => {
          toast.success('Trusted Proxy settings saved.')
          setTrustedProxyDialogOpen(false)
          trustedProxyForm.setValue('shared_secret', '')
          trustedProxyForm.setValue('warpgate_ticket', '')
          trustedProxyForm.setValue('clear_warpgate_ticket', false)
          void preflightQuery.refetch()
        },
        onError: (error) => {
          toast.error(
            getApiErrorMessage(error, {
              external_access_owner_required:
                'Only the owner can update Trusted Proxy settings.',
              external_access_loopback_required:
                'In Local Only mode, Trusted Proxy settings can only be changed from localhost on the host machine.',
              invalid_trusted_proxy_header:
                'Enter a valid HTTP header name for the user email header.',
              invalid_trusted_proxy_cidr:
                'Trusted proxy peers must be valid CIDR ranges.',
              invalid_input: 'Check Trusted Proxy values and try again.',
            }),
          )
        },
      },
    )
  }

  const preferences = preferencesQuery.data
  const externalAccessEnabled = preferences?.runtime_mode === 'remote'
  const remoteAuthMode = preferences?.remote_auth_mode ?? 'oidc'
  const webVersionLabel = healthVersionLabel(
    webHealthQuery.data,
    webHealthQuery.isLoading,
    webHealthQuery.isError,
  )
  const backendVersionLabel = healthVersionLabel(
    backendHealthQuery.data,
    backendHealthQuery.isLoading,
    backendHealthQuery.isError,
  )
  const identityCheckId =
    remoteAuthMode === 'trusted_proxy'
      ? 'trusted_proxy_configured'
      : 'oidc_configured'
  const readinessById = useMemo(() => {
    const entries = preflightQuery.data?.checks ?? []
    return new Map(entries.map((check) => [check.id, check]))
  }, [preflightQuery.data?.checks])
  const setupReady = readinessById.get('setup_ready')?.ok ?? false
  const identityReady = readinessById.get(identityCheckId)?.ok ?? false
  const networkCheckIds = [
    'public_url_https',
    'public_origin_allowed',
    ...(remoteAuthMode === 'oidc' ? ['redirect_policy_consistent'] : []),
  ]
  const networkReady = networkCheckIds.every(
    (checkId) => readinessById.get(checkId)?.ok ?? false,
  )
  const readinessReady = preflightQuery.data?.ready ?? false
  const setupStepsComplete = Number(networkReady) + Number(identityReady)

  function handleExternalAccessToggle() {
    if (!preferences || updatePreferencesMutation.isPending || !isOwner) return

    const nextMode = externalAccessEnabled ? 'local' : 'remote'
    updatePreferencesMutation.mutate(
      {
        key_storage_mode: preferences.key_storage_mode,
        runtime_mode: nextMode,
        remote_auth_mode: preferences.remote_auth_mode,
      },
      {
        onSuccess: () => {
          toast.success(
            nextMode === 'remote'
              ? 'External Access enabled. Please sign in again.'
              : 'External Access disabled. Please sign in again.',
          )
          clearAuth()
          void navigate({ to: '/login' })
        },
        onError: (error) => {
          const message = getApiErrorMessage(error, {
            external_access_owner_required:
              'Only the owner can change External Access mode.',
            external_access_preflight_failed:
              'External Access readiness checks are failing.',
            external_access_public_url_missing:
              'Set a non-loopback HTTPS Public URL in External Access network settings.',
            external_access_https_required:
              'External Access requires an HTTPS public URL.',
            external_access_origin_not_allowed:
              'Public URL origin must be listed in allowed frontend origins.',
          })
          if (error instanceof ApiClientError && error.details) {
            toast.error(`${message} ${error.details}`)
          } else {
            toast.error(message)
          }
          if (
            error instanceof ApiClientError &&
            error.code.startsWith('external_access')
          ) {
            setReadinessOpen(true)
          }
        },
      },
    )
  }

  function handleDirectRunnerToggle(acceptingBuilds: boolean) {
    if (
      !preferences ||
      preferencesQuery.isError ||
      updatePreferencesMutation.isPending ||
      !canWrite
    )
      return

    updatePreferencesMutation.mutate(
      {
        key_storage_mode: preferences.key_storage_mode,
        direct_macos_runner_paused: !acceptingBuilds,
      },
      {
        onSuccess: () =>
          toast.success(
            acceptingBuilds
              ? 'This Mac is accepting new builds.'
              : 'New builds are paused. Running builds will finish.',
          ),
        onError: (error) =>
          toast.error(`Failed to change build intake: ${error.message}`),
      },
    )
  }

  const identitySettingsQuery =
    remoteAuthMode === 'trusted_proxy' ? trustedProxyQuery : oidcConfigQuery

  function openNetworkSettingsDialog() {
    setNetworkEditorOpen(true)
  }

  function preloadIdentitySettingsDialog() {
    void (remoteAuthMode === 'trusted_proxy'
      ? preloadTrustedProxySettingsDialog()
      : preloadOidcSettingsDialog())
  }

  function openIdentitySettingsDialog() {
    if (remoteAuthMode === 'trusted_proxy') {
      setTrustedProxyDialogOpen(true)
    } else {
      setOidcDialogOpen(true)
    }
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="General settings" noindex />
      <PageHeader
        title="General"
        description="Manage runtime services and External Access for this instance."
      />
      <RuntimeOverview
        backendUpdatePhase={backendUpdatePhase}
        backendVersionLabel={backendVersionLabel}
        frontendUpdatePhase={frontendUpdatePhase}
        isOwner={isOwner}
        runtimeUpdates={runtimeUpdates}
        webVersionLabel={webVersionLabel}
      />
      <DirectRunnerSettings
        canWrite={canWrite}
        isError={preferencesQuery.isError}
        isPending={updatePreferencesMutation.isPending}
        onToggle={handleDirectRunnerToggle}
        paused={preferences?.direct_macos_runner_paused}
      />
      {preferencesQuery.isLoading ? (
        <div className="space-y-3 border bg-card p-4">
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-9 w-full" />
        </div>
      ) : preferencesQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              Failed to load instance settings: {preferencesQuery.error.message}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void preferencesQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : !preferences ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>The response did not include instance settings.</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void preferencesQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : (
        <ExternalAccessCard
          externalAccessEnabled={externalAccessEnabled}
          isOwner={isOwner}
          onToggle={handleExternalAccessToggle}
          preflightLoading={preflightQuery.isLoading}
          readinessReady={readinessReady}
          remoteAuthMode={remoteAuthMode}
          updatePending={updatePreferencesMutation.isPending}
        >
          {externalAccessEnabled ? (
            <ExternalAccessManagement
              identityQuery={identitySettingsQuery}
              isOwner={isOwner}
              networkSettingsQuery={networkSettingsQuery}
              onEditIdentity={openIdentitySettingsDialog}
              onEditNetwork={openNetworkSettingsDialog}
              onPreloadIdentity={preloadIdentitySettingsDialog}
              onPreloadNetwork={preloadExternalAccessNetworkDialog}
              remoteAuthMode={remoteAuthMode}
              trustedProxySettings={trustedProxySettings}
            />
          ) : (
            <ExternalAccessSetup
              identityQuery={identitySettingsQuery}
              identityReady={identityReady}
              isOwner={isOwner}
              networkReady={networkReady}
              networkSettingsQuery={networkSettingsQuery}
              oidcConfig={oidcConfig}
              onEditIdentity={openIdentitySettingsDialog}
              onEditNetwork={openNetworkSettingsDialog}
              onPreloadIdentity={preloadIdentitySettingsDialog}
              onPreloadNetwork={preloadExternalAccessNetworkDialog}
              onReadinessOpenChange={setReadinessOpen}
              preflightQuery={preflightQuery}
              readinessOpen={readinessOpen}
              readinessReady={readinessReady}
              remoteAuthMode={remoteAuthMode}
              setupReady={setupReady}
              setupStepsComplete={setupStepsComplete}
              trustedProxySettings={trustedProxySettings}
            />
          )}
        </ExternalAccessCard>
      )}
      {networkEditorOpen &&
      !networkSettingsQuery.isLoading &&
      !networkSettingsQuery.error ? (
        <Suspense fallback={null}>
          <ExternalAccessNetworkDialog
            form={externalAccessNetworkForm}
            isOwner={isOwner}
            isPending={updateNetworkSettingsMutation.isPending}
            onOpenChange={setNetworkEditorOpen}
            onSubmit={onSubmitExternalAccessNetwork}
            open={networkEditorOpen}
          />
        </Suspense>
      ) : null}
      {trustedProxyDialogOpen &&
      !trustedProxyQuery.isLoading &&
      !trustedProxyQuery.error ? (
        <Suspense fallback={null}>
          <TrustedProxySettingsDialog
            form={trustedProxyForm}
            isOwner={isOwner}
            isPending={updateTrustedProxyMutation.isPending}
            onOpenChange={setTrustedProxyDialogOpen}
            onSubmit={onSubmitTrustedProxy}
            open={trustedProxyDialogOpen}
            settings={trustedProxySettings}
          />
        </Suspense>
      ) : null}
      {oidcDialogOpen &&
      !oidcConfigQuery.isLoading &&
      !oidcConfigQuery.error ? (
        <Suspense fallback={null}>
          <OidcSettingsDialog
            form={externalAccessOidcForm}
            isOwner={isOwner}
            isSaving={configureExternalAccessOidcMutation.isPending}
            oidcConfig={oidcConfig}
            onOpenChange={setOidcDialogOpen}
            onSubmit={onSubmitExternalAccessOidc}
            open={oidcDialogOpen}
            testMutation={testOidcConnectionMutation}
          />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
