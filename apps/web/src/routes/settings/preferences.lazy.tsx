import { lazy, Suspense, useMemo, useState } from 'react'
import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import { useMountEffect } from '@/hooks/use-mount-effect'

import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { PageMeta } from '@/lib/seo'
import { useHasPermission } from '@/hooks/use-permissions'
import { useRuntimeUpdates } from '@/hooks/use-runtime-updates'
import {
  useArtifactStorageSettings,
  useConfigureExternalAccessOidc,
  useExternalAccessNetworkSettings,
  useExternalAccessOidc,
  useExternalAccessPreflight,
  useExternalAccessTrustedProxySettings,
  useInstancePreferences,
  useTestOidcConnection,
  useUpdateArtifactStorageSettings,
  useUpdateExternalAccessNetworkSettings,
  useUpdateExternalAccessTrustedProxySettings,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { ApiClientError, getApiErrorMessage } from '@/lib/api'
import { ExternalAccessCard } from '@/components/settings/preferences-external-access-card'
import { RuntimeOverview } from '@/components/settings/preferences-runtime-overview'
import { ArtifactStorageSettings } from '@/components/settings/preferences-artifact-storage-settings'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Switch } from '@/components/ui/switch'

const preloadExternalAccessNetworkDialog = () =>
  import('@/components/settings/preferences-external-access-network-dialog')
const preloadTrustedProxySettingsDialog = () =>
  import('@/components/settings/preferences-trusted-proxy-settings-dialog')
const preloadOidcSettingsDialog = () =>
  import('@/components/settings/preferences-oidc-settings-dialog')
const preloadArtifactFolderPicker = () =>
  import('@/components/settings/preferences-artifact-folder-picker')

const ExternalAccessNetworkDialog = lazy(preloadExternalAccessNetworkDialog)
const TrustedProxySettingsDialog = lazy(preloadTrustedProxySettingsDialog)
const OidcSettingsDialog = lazy(preloadOidcSettingsDialog)
const ArtifactFolderPicker = lazy(preloadArtifactFolderPicker)

export const Route = createLazyFileRoute('/settings/preferences')({
  component: PreferencesPage,
})

const storageSchema = z
  .object({
    backend_kind: z.enum(['disabled', 'local', 'object']),
    object_service: z
      .enum(['aws_s3', 'cloudflare_r2', 'minio', 'custom'])
      .optional(),
    local_base_dir: z.string().optional(),
    s3_bucket: z.string().optional(),
    s3_region: z.string().optional(),
    s3_endpoint: z.string().optional(),
    access_key_id: z.string().optional(),
    secret_access_key: z.string().optional(),
  })
  .superRefine((value, ctx) => {
    const localDir = (value.local_base_dir ?? '').trim()
    const bucket = (value.s3_bucket ?? '').trim()
    const endpoint = (value.s3_endpoint ?? '').trim()
    const service = value.object_service

    if (value.backend_kind === 'local' && !localDir) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['local_base_dir'],
        message: 'Local base directory is required.',
      })
    }

    if (value.backend_kind === 'object') {
      if (!service) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['object_service'],
          message: 'Choose an object storage service.',
        })
      }

      if (!bucket) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['s3_bucket'],
          message: 'Bucket is required for object storage.',
        })
      }

      const endpointRequired =
        service === 'cloudflare_r2' ||
        service === 'minio' ||
        service === 'custom'
      if (endpointRequired && !endpoint) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['s3_endpoint'],
          message: 'Endpoint is required for this service preset.',
        })
      }
    }
  })

type StorageFormInput = z.input<typeof storageSchema>
type StorageFormValues = z.output<typeof storageSchema>

const externalAccessOidcSchema = z.object({
  issuer_url: z.url('Please enter a valid issuer URL'),
  client_id: z.string().min(1, 'Client ID is required'),
  client_secret: z.string().optional(),
})

type ExternalAccessOidcFormValues = z.infer<typeof externalAccessOidcSchema>

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

type TrustedProxyFormValues = z.infer<typeof trustedProxySchema>

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

type ExternalAccessNetworkFormValues = z.infer<
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

function isLoopbackHostname(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '::1' ||
    hostname === '[::1]'
  )
}

function resolveHostname(rawUrl: string | null | undefined): string {
  const trimmed = rawUrl?.trim() ?? ''
  if (!trimmed) return ''
  try {
    return new URL(trimmed).hostname
  } catch {
    return ''
  }
}

function usePreferencesPageState() {
  const navigate = useNavigate()
  const [readinessOpen, setReadinessOpen] = useState(false)
  const [networkEditorOpen, setNetworkEditorOpen] = useState(false)
  const [oidcDialogOpen, setOidcDialogOpen] = useState(false)
  const [trustedProxyDialogOpen, setTrustedProxyDialogOpen] = useState(false)
  const [artifactDirPickerOpen, setArtifactDirPickerOpen] = useState(false)
  const canWrite = useHasPermission('instance_settings', 'write')
  const user = useAuthStore((s) => s.user)
  const clearAuth = useAuthStore((s) => s.clearAuth)
  const isOwner = user?.role === 'owner'
  const instance = useActiveInstance()
  const instanceApiBaseUrl = resolveInstanceApiBaseUrl(instance)
  const uiIsLoopback = isLoopbackHostname(window.location.hostname)
  const backendIsLoopback = isLoopbackHostname(
    resolveHostname(instanceApiBaseUrl),
  )
  const canBrowseLocalFs = uiIsLoopback && backendIsLoopback
  const settingsQuery = useArtifactStorageSettings()
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
  const updateStorageMutation = useUpdateArtifactStorageSettings()
  const updatePreferencesMutation = useUpdateInstancePreferences()

  const storageSettings = settingsQuery.data?.settings
  const storageValues = useMemo<StorageFormInput | undefined>(() => {
    if (!storageSettings) return undefined
    const backend_kind =
      storageSettings.provider === 'disabled'
        ? 'disabled'
        : storageSettings.provider === 'local'
          ? 'local'
          : 'object'
    const object_service =
      storageSettings.provider === 'r2'
        ? 'cloudflare_r2'
        : storageSettings.provider === 's3'
          ? storageSettings.s3_endpoint
            ? 'custom'
            : 'aws_s3'
          : 'aws_s3'
    const region =
      storageSettings.provider === 'r2'
        ? (storageSettings.s3_region ?? 'auto')
        : (storageSettings.s3_region ?? 'us-east-1')
    return {
      backend_kind,
      object_service,
      local_base_dir: storageSettings.local_base_dir ?? '',
      s3_bucket: storageSettings.s3_bucket ?? '',
      s3_region: region,
      s3_endpoint: storageSettings.s3_endpoint ?? '',
      access_key_id: '',
      secret_access_key: '',
    }
  }, [storageSettings])

  const storageForm = useForm<StorageFormInput, unknown, StorageFormValues>({
    resolver: zodResolver(storageSchema),
    defaultValues: {
      backend_kind: 'disabled',
      object_service: 'aws_s3',
      local_base_dir: '',
      s3_bucket: '',
      s3_region: 'us-east-1',
      s3_endpoint: '',
      access_key_id: '',
      secret_access_key: '',
    },
    values: storageValues,
    mode: 'onBlur',
  })

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
  const networkSettings = networkSettingsQuery.data?.settings
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

  const trustedProxySettings = trustedProxyQuery.data?.settings
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

  const backendKind = storageForm.watch('backend_kind')
  const objectService = storageForm.watch('object_service')

  useMountEffect(() => {
    const subscription = storageForm.watch((values, { name }) => {
      if (name !== 'object_service' && name !== 'backend_kind') return
      if (values.backend_kind !== 'object') return
      if (values.object_service === 'cloudflare_r2') {
        if (values.s3_region !== 'auto') {
          storageForm.setValue('s3_region', 'auto', { shouldDirty: true })
        }
      } else if (values.object_service === 'aws_s3') {
        if (values.s3_region === 'auto') {
          storageForm.setValue('s3_region', 'us-east-1', { shouldDirty: true })
        }
      }
    })
    return () => subscription.unsubscribe()
  })

  function onSubmitStorage(values: StorageFormValues) {
    const provider =
      values.backend_kind === 'disabled'
        ? 'disabled'
        : values.backend_kind === 'local'
          ? 'local'
          : values.object_service === 'cloudflare_r2'
            ? 'r2'
            : 's3'

    const region =
      values.backend_kind === 'object'
        ? values.object_service === 'cloudflare_r2'
          ? 'auto'
          : values.s3_region?.trim() || 'us-east-1'
        : undefined

    const payload = {
      provider,
      local_base_dir:
        values.backend_kind === 'local'
          ? values.local_base_dir?.trim()
          : undefined,
      s3_bucket:
        values.backend_kind === 'object' ? values.s3_bucket?.trim() : undefined,
      s3_region: values.backend_kind === 'object' ? region : undefined,
      s3_endpoint:
        values.backend_kind === 'object'
          ? values.s3_endpoint?.trim() || undefined
          : undefined,
      access_key_id:
        values.backend_kind === 'object'
          ? values.access_key_id?.trim() || undefined
          : undefined,
      secret_access_key:
        values.backend_kind === 'object'
          ? values.secret_access_key?.trim() || undefined
          : undefined,
    } as const

    updateStorageMutation.mutate(payload, {
      onSuccess: (res) => {
        const label =
          res.settings.provider === 'local'
            ? 'Local filesystem'
            : res.settings.provider === 's3'
              ? 'Object storage'
              : res.settings.provider === 'r2'
                ? 'Object storage (R2)'
                : 'Disabled'
        toast.success(`Artifact storage updated: ${label}`)
        storageForm.setValue('access_key_id', '')
        storageForm.setValue('secret_access_key', '')
      },
      onError: (error) => {
        toast.error(
          getApiErrorMessage(error, {
            invalid_local_base_dir:
              'Local base directory must be an absolute path.',
            invalid_s3_bucket: 'Bucket is required for object storage.',
            invalid_s3_endpoint:
              'Endpoint is required for this service preset.',
            missing_s3_credentials:
              'Access key ID and secret access key are required for object storage.',
            encryption_error: 'Failed to store credentials securely.',
            store_error: 'Failed to update artifact storage settings.',
          }),
        )
      },
    })
  }

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

  const settings = settingsQuery.data?.settings
  const artifactBackendLabel = useMemo(() => {
    switch (settings?.provider) {
      case 'local':
        return 'Local filesystem'
      case 's3':
        return 'Object storage'
      case 'r2':
        return 'Object storage (R2)'
      case 'disabled':
      default:
        return 'Disabled'
    }
  }, [settings?.provider])
  const artifactSourceLabel = useMemo(() => {
    switch (settings?.source) {
      case 'database':
        return 'Database'
      case 'environment':
        return 'Environment'
      case 'default':
      default:
        return 'Default'
    }
  }, [settings?.source])
  const preferences = preferencesQuery.data?.preferences
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
  const failedReadinessChecks = useMemo(
    () => preflightQuery.data?.checks.filter((check) => !check.ok) ?? [],
    [preflightQuery.data?.checks],
  )
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
  const setupStepCount = 2

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

  return {
    artifactBackendLabel,
    artifactDirPickerOpen,
    artifactSourceLabel,
    backendHealthQuery,
    backendKind,
    backendUpdatePhase,
    backendVersionLabel,
    canBrowseLocalFs,
    canWrite,
    externalAccessEnabled,
    externalAccessNetworkForm,
    externalAccessOidcForm,
    failedReadinessChecks,
    frontendUpdatePhase,
    handleDirectRunnerToggle,
    handleExternalAccessToggle,
    identityReady,
    isOwner,
    networkEditorOpen,
    networkReady,
    networkSettings,
    networkSettingsQuery,
    objectService,
    oidcConfig,
    oidcDialogOpen,
    onSubmitExternalAccessNetwork,
    onSubmitExternalAccessOidc,
    onSubmitStorage,
    onSubmitTrustedProxy,
    preflightQuery,
    preloadArtifactFolderPicker,
    preloadExternalAccessNetworkDialog,
    preloadOidcSettingsDialog,
    preloadTrustedProxySettingsDialog,
    preferences,
    preferencesQuery,
    readinessOpen,
    readinessReady,
    remoteAuthMode,
    runtimeUpdates,
    setArtifactDirPickerOpen,
    setNetworkEditorOpen,
    setOidcDialogOpen,
    setReadinessOpen,
    setTrustedProxyDialogOpen,
    settings,
    settingsQuery,
    setupReady,
    setupStepCount,
    setupStepsComplete,
    storageForm,
    testOidcConnectionMutation,
    trustedProxyDialogOpen,
    trustedProxyForm,
    trustedProxySettings,
    updateNetworkSettingsMutation,
    updatePreferencesMutation,
    updateStorageMutation,
    updateTrustedProxyMutation,
    configureExternalAccessOidcMutation,
    webHealthQuery,
    webVersionLabel,
  }
}

export type PreferencesPageState = ReturnType<typeof usePreferencesPageState>

function DirectRunnerSettings({ state }: { state: PreferencesPageState }) {
  const paused = state.preferences?.direct_macos_runner_paused
  const acceptingBuilds = paused === false
  const runnerStateLabel = state.preferencesQuery.isError
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
          <Badge variant={acceptingBuilds ? 'success' : 'outline'}>
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
            disabled={
              !state.canWrite ||
              !state.preferences ||
              state.preferencesQuery.isError ||
              state.updatePreferencesMutation.isPending
            }
            onCheckedChange={(checked) =>
              state.handleDirectRunnerToggle(checked)
            }
            aria-label="Accept new builds"
            className="after:-inset-y-3.5"
          />
        </div>
        <Alert>
          <AlertDescription>
            {state.preferencesQuery.isError
              ? 'Oore could not load the runner pause state. Refresh this page before changing build intake.'
              : paused === undefined
                ? 'Checking whether this Mac is accepting new builds.'
                : !state.canWrite
                  ? 'An owner or admin can pause new claims while assigned and running builds finish.'
                : 'Pausing stops new claims while assigned and running builds finish. Resume whenever this Mac is ready for more work.'}
          </AlertDescription>
        </Alert>
      </CardContent>
    </Card>
  )
}

function PreferencesPage() {
  const state = usePreferencesPageState()
  return (
    <PageLayout width="wide">
      <PageMeta title="Preferences" noindex />
      <PageHeader
        title="Preferences"
        description="Manage runner, artifact storage, and External Access policy for this instance."
      />
      <RuntimeOverview state={state} />
      <DirectRunnerSettings state={state} />
      <ExternalAccessCard state={state} />
      {state.networkEditorOpen ? (
        <Suspense fallback={null}>
          <ExternalAccessNetworkDialog state={state} />
        </Suspense>
      ) : null}
      {state.trustedProxyDialogOpen ? (
        <Suspense fallback={null}>
          <TrustedProxySettingsDialog state={state} />
        </Suspense>
      ) : null}
      {state.oidcDialogOpen ? (
        <Suspense fallback={null}>
          <OidcSettingsDialog state={state} />
        </Suspense>
      ) : null}
      <ArtifactStorageSettings state={state} />
      {state.artifactDirPickerOpen ? (
        <Suspense fallback={null}>
          <ArtifactFolderPicker state={state} />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
