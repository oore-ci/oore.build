import { useMemo, useState } from 'react'
import { createFileRoute, redirect, useNavigate } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowDown01Icon,
  ArrowRight01Icon,
  CheckmarkCircle02Icon,
  Folder02Icon,
} from '@hugeicons/core-free-icons'
import type { RemoteAuthMode } from '@/lib/types'
import { useMountEffect } from '@/hooks/use-mount-effect'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { PageMeta } from '@/lib/seo'
import { useHasPermission } from '@/hooks/use-permissions'
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
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import LocalFolderPickerDialog from '@/components/LocalFolderPickerDialog'
import { OidcIssuerUrlAutocomplete } from '@/components/oidc-issuer-url-autocomplete'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { ApiClientError, getApiErrorMessage } from '@/lib/api'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'

export const Route = createFileRoute('/settings/preferences')({
  staticData: { breadcrumbLabel: 'Preferences' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)

    const user = useAuthStore.getState().user
    if (!user || (user.role !== 'owner' && user.role !== 'admin')) {
      throw redirect({ to: '/' })
    }
  },
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

type StorageFormValues = z.infer<typeof storageSchema>

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
})

type TrustedProxyFormValues = z.infer<typeof trustedProxySchema>

function guidanceForPreflight(checkId: string, failureCode?: string): string {
  if (failureCode === 'external_access_public_url_missing') {
    return 'Set a non-loopback HTTPS Public URL in External Access network settings.'
  }
  if (failureCode === 'external_access_https_required') {
    return 'Public URL must use HTTPS before enabling External Access.'
  }
  if (failureCode === 'external_access_origin_not_allowed') {
    return 'Add the Public URL origin to allowed origins in External Access network settings.'
  }
  if (checkId === 'setup_ready') {
    return 'Finish setup until the instance reaches ready state.'
  }
  if (checkId === 'oidc_configured') {
    return 'Configure OIDC and verify runtime auth settings.'
  }
  if (checkId === 'trusted_proxy_configured') {
    return 'Configure Trusted Proxy identity settings and shared secret.'
  }
  if (checkId === 'redirect_policy_consistent') {
    return 'Ensure redirect URI policy matches your configured public origin.'
  }
  return 'Resolve this check before enabling External Access.'
}

function authModeLabel(mode: RemoteAuthMode | undefined): string {
  return mode === 'trusted_proxy' ? 'Trusted Proxy' : 'OIDC'
}

function authModeDescription(mode: RemoteAuthMode | undefined): string {
  return mode === 'trusted_proxy'
    ? 'Sign-in is delegated to a trusted upstream proxy.'
    : 'Sign-in uses your configured OIDC provider.'
}

function parseTrustedProxyCidrs(value: string | undefined): Array<string> {
  return (value ?? '')
    .split(/[\n,]/g)
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0)
}

const externalAccessNetworkSchema = z.object({
  public_url: z.string().optional(),
  allowed_origins: z
    .string()
    .min(1, 'Add at least one allowed frontend origin.'),
})

type ExternalAccessNetworkFormValues = z.infer<
  typeof externalAccessNetworkSchema
>

interface RuntimeHealth {
  ok?: boolean
  version?: string
  channel?: string | null
  github_repo?: string | null
  backend_url?: string
  package_version?: string
}

async function fetchRuntimeHealth(path: string): Promise<RuntimeHealth> {
  const response = await fetch(path, { cache: 'no-store' })
  if (!response.ok) {
    throw new Error(`Health check failed (${response.status})`)
  }
  return (await response.json()) as RuntimeHealth
}

function healthVersionLabel(
  health: RuntimeHealth | undefined,
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

function PreferencesPage() {
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
  const webHealthQuery = useQuery({
    queryKey: ['runtime-health', 'oore-web'],
    queryFn: () => fetchRuntimeHealth('/__oore_web_healthz'),
    retry: false,
    staleTime: 30_000,
  })
  const backendHealthQuery = useQuery({
    queryKey: [instance?.id ?? '__none__', 'runtime-health', 'oored'],
    queryFn: () => {
      const baseUrl = instanceApiBaseUrl
      if (!baseUrl) throw new Error('No active instance URL')
      return fetchRuntimeHealth(new URL('/healthz', baseUrl).toString())
    },
    enabled: !!instanceApiBaseUrl,
    retry: false,
    staleTime: 30_000,
  })
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
  const storageValues = useMemo(() => {
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
    } as StorageFormValues
  }, [storageSettings])

  const storageForm = useForm<StorageFormValues>({
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
      allowed_origins: networkSettings.allowed_origins.join('\n'),
    }
  }, [networkSettings])

  const externalAccessNetworkForm = useForm<ExternalAccessNetworkFormValues>({
    resolver: zodResolver(externalAccessNetworkSchema),
    defaultValues: {
      public_url: '',
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
    }
  }, [trustedProxySettings])

  const trustedProxyForm = useForm<TrustedProxyFormValues>({
    resolver: zodResolver(trustedProxySchema),
    defaultValues: {
      user_email_header: 'x-oore-user-email',
      trusted_proxy_cidrs: '',
      shared_secret: '',
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
    updateTrustedProxyMutation.mutate(
      {
        user_email_header: values.user_email_header.trim(),
        trusted_proxy_cidrs: parseTrustedProxyCidrs(values.trusted_proxy_cidrs),
        ...(sharedSecret ? { shared_secret: sharedSecret } : {}),
      },
      {
        onSuccess: () => {
          toast.success('Trusted Proxy settings saved.')
          setTrustedProxyDialogOpen(false)
          trustedProxyForm.setValue('shared_secret', '')
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

  return (
    <PageLayout width="wide">
      <PageMeta title="Preferences" noindex />
      <PageHeader
        title="Preferences"
        description="Manage artifact storage and External Access policy for this instance."
      />

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
        <CardContent className="space-y-4">
          <div className="flex flex-wrap items-start justify-between gap-3 border p-3">
            <div className="space-y-1">
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

          {!externalAccessEnabled ? (
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
                      {networkSettings?.allowed_origins.length ?? 0} allowed
                      origins
                    </p>
                    <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
                      Configure
                      <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
                    </p>
                  </button>

                  <button
                    type="button"
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
                    {remoteAuthMode === 'trusted_proxy' &&
                    trustedProxySettings ? (
                      <div className="mt-2 space-y-1 text-xs text-muted-foreground">
                        <p>
                          <span className="font-medium text-foreground">
                            Header:
                          </span>{' '}
                          <span className="font-mono">
                            {trustedProxySettings.user_email_header}
                          </span>
                        </p>
                        <p>
                          <span className="font-medium text-foreground">
                            Secret:
                          </span>{' '}
                          {trustedProxySettings.has_shared_secret
                            ? 'Stored'
                            : 'Missing'}
                        </p>
                        <p>
                          <span className="font-medium text-foreground">
                            Peer CIDRs:
                          </span>{' '}
                          {trustedProxySettings.trusted_proxy_cidrs.length > 0
                            ? trustedProxySettings.trusted_proxy_cidrs.join(
                                ', ',
                              )
                            : 'Loopback only'}
                        </p>
                      </div>
                    ) : remoteAuthMode === 'oidc' && oidcConfig ? (
                      <div className="mt-2 space-y-1 text-xs text-muted-foreground">
                        <p>
                          <span className="font-medium text-foreground">
                            Issuer:
                          </span>{' '}
                          <span className="font-mono">
                            {oidcConfig.issuer_url}
                          </span>
                        </p>
                        <p>
                          <span className="font-medium text-foreground">
                            Client ID:
                          </span>{' '}
                          <span className="font-mono">
                            {oidcConfig.client_id}
                          </span>
                        </p>
                        <p>
                          <span className="font-medium text-foreground">
                            Secret:
                          </span>{' '}
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
                      <p className="mt-1 text-sm text-destructive">
                        Check run failed.
                      </p>
                    ) : preflightQuery.data?.ready ? (
                      <p className="mt-1 text-sm text-muted-foreground">
                        All checks are passing.
                      </p>
                    ) : (
                      <p className="mt-1 text-sm text-muted-foreground">
                        {failedReadinessChecks.length} check
                        {failedReadinessChecks.length === 1 ? '' : 's'} need
                        attention.
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
                      Failed to run readiness checks:{' '}
                      {preflightQuery.error.message}
                    </AlertDescription>
                  </Alert>
                ) : null}

                <CollapsibleContent className="space-y-2">
                  {preflightQuery.data
                    ? preflightQuery.data.checks.map((check) => (
                        <div
                          key={check.id}
                          className="border border-border/60 p-3"
                        >
                          <div className="flex items-start justify-between gap-3">
                            <div className="flex items-start gap-2">
                              <HugeiconsIcon
                                icon={
                                  check.ok
                                    ? CheckmarkCircle02Icon
                                    : AlertCircleIcon
                                }
                                size={14}
                                className={
                                  check.ok ? 'text-success' : 'text-destructive'
                                }
                              />
                              <div>
                                <p className="text-sm font-medium">
                                  {check.label}
                                </p>
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
          ) : (
            <div className="space-y-3 border p-3">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Manage External Access
              </p>
              <div className="grid gap-3 md:grid-cols-2">
                <button
                  type="button"
                  onClick={() => setNetworkEditorOpen(true)}
                  disabled={!isOwner || networkSettingsQuery.isLoading}
                  className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  <p className="text-sm font-medium">Network settings</p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {networkSettings?.public_url ??
                      'Set Public URL and allowed origins.'}
                  </p>
                  <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
                    Edit
                    <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
                  </p>
                </button>

                <button
                  type="button"
                  onClick={() =>
                    remoteAuthMode === 'trusted_proxy'
                      ? setTrustedProxyDialogOpen(true)
                      : setOidcDialogOpen(true)
                  }
                  disabled={!isOwner}
                  className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  <p className="text-sm font-medium">Identity settings</p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {remoteAuthMode === 'trusted_proxy'
                      ? 'Update trusted proxy header, peer CIDRs, and secret.'
                      : 'Update issuer and client credentials.'}
                  </p>
                  <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
                    Edit
                    <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
                  </p>
                </button>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog open={networkEditorOpen} onOpenChange={setNetworkEditorOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>External Access Network Settings</DialogTitle>
            <DialogDescription>
              Configure the public endpoint and allowed frontend origins.
            </DialogDescription>
          </DialogHeader>

          <Form {...externalAccessNetworkForm}>
            <form
              onSubmit={externalAccessNetworkForm.handleSubmit(
                onSubmitExternalAccessNetwork,
              )}
              className="space-y-4"
            >
              <FormField
                control={externalAccessNetworkForm.control}
                name="public_url"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Public URL (HTTPS)</FormLabel>
                    <FormControl>
                      <Input
                        type="url"
                        placeholder="https://ci.example.com"
                        {...field}
                        disabled={
                          updateNetworkSettingsMutation.isPending || !isOwner
                        }
                      />
                    </FormControl>
                    <FormDescription>
                      Must be non-loopback and HTTPS.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={externalAccessNetworkForm.control}
                name="allowed_origins"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Allowed frontend origins</FormLabel>
                    <FormControl>
                      <Textarea
                        rows={5}
                        placeholder="http://localhost:3000&#10;http://127.0.0.1:3000&#10;https://ci.example.com"
                        {...field}
                        disabled={
                          updateNetworkSettingsMutation.isPending || !isOwner
                        }
                      />
                    </FormControl>
                    <FormDescription>
                      One origin per line (or comma-separated).
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setNetworkEditorOpen(false)}
                  disabled={updateNetworkSettingsMutation.isPending}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={!isOwner || updateNetworkSettingsMutation.isPending}
                >
                  {updateNetworkSettingsMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Saving...
                    </>
                  ) : (
                    'Save network settings'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <Dialog
        open={trustedProxyDialogOpen}
        onOpenChange={(open) => {
          setTrustedProxyDialogOpen(open)
          if (!open) trustedProxyForm.setValue('shared_secret', '')
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Trusted Proxy Identity Settings</DialogTitle>
            <DialogDescription>
              Configure the backend trust contract used when an upstream proxy
              provides the signed-in user email.
            </DialogDescription>
          </DialogHeader>

          <Form {...trustedProxyForm}>
            <form
              onSubmit={trustedProxyForm.handleSubmit(onSubmitTrustedProxy)}
              className="space-y-4"
            >
              <FormField
                control={trustedProxyForm.control}
                name="user_email_header"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>User email header</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="x-oore-user-email"
                        {...field}
                        disabled={updateTrustedProxyMutation.isPending}
                      />
                    </FormControl>
                    <FormDescription>
                      Header forwarded by oore-web after the upstream proxy has
                      proven the request.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={trustedProxyForm.control}
                name="trusted_proxy_cidrs"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Trusted proxy peer CIDRs</FormLabel>
                    <FormControl>
                      <Textarea
                        rows={4}
                        placeholder="127.0.0.1/32&#10;10.0.0.10/32"
                        {...field}
                        disabled={updateTrustedProxyMutation.isPending}
                      />
                    </FormControl>
                    <FormDescription>
                      One CIDR per line. Leave blank to accept loopback peers
                      only.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={trustedProxyForm.control}
                name="shared_secret"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Shared secret</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder={
                          trustedProxySettings?.has_shared_secret
                            ? 'Leave empty to keep existing secret'
                            : 'Paste shared secret'
                        }
                        {...field}
                        disabled={updateTrustedProxyMutation.isPending}
                      />
                    </FormControl>
                    <FormDescription>
                      Leave empty to keep the existing secret. Enter a new value
                      only when rotating it.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setTrustedProxyDialogOpen(false)}
                  disabled={updateTrustedProxyMutation.isPending}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={!isOwner || updateTrustedProxyMutation.isPending}
                >
                  {updateTrustedProxyMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Saving...
                    </>
                  ) : (
                    'Save trusted proxy settings'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <Dialog
        open={oidcDialogOpen}
        onOpenChange={(open) => {
          setOidcDialogOpen(open)
          if (!open) testOidcConnectionMutation.reset()
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {oidcConfig
                ? 'Update OIDC Provider'
                : 'Configure OIDC for External Access'}
            </DialogTitle>
            <DialogDescription>
              Owner-only. This updates runtime OIDC settings used by External
              Access sign-in.
              {oidcConfig?.has_client_secret ? (
                <> Leave the secret field empty to keep the existing secret.</>
              ) : null}
            </DialogDescription>
          </DialogHeader>

          <Form {...externalAccessOidcForm}>
            <form
              onSubmit={externalAccessOidcForm.handleSubmit(
                onSubmitExternalAccessOidc,
              )}
              className="space-y-4"
            >
              <FormField
                control={externalAccessOidcForm.control}
                name="issuer_url"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Issuer URL</FormLabel>
                    <FormControl>
                      <OidcIssuerUrlAutocomplete
                        name={field.name}
                        value={field.value}
                        onValueChange={(next) => field.onChange(next)}
                        onBlur={field.onBlur}
                        ref={field.ref}
                        disabled={configureExternalAccessOidcMutation.isPending}
                      />
                    </FormControl>
                    <FormDescription>
                      Pick a common provider or enter a custom issuer URL.
                      Template entries must be edited before saving.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={externalAccessOidcForm.control}
                name="client_id"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Client ID</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="your-client-id"
                        {...field}
                        disabled={configureExternalAccessOidcMutation.isPending}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={externalAccessOidcForm.control}
                name="client_secret"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Client Secret (optional)</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder={
                          oidcConfig?.has_client_secret
                            ? 'Leave empty to keep existing secret'
                            : 'Leave empty for public clients'
                        }
                        {...field}
                        disabled={configureExternalAccessOidcMutation.isPending}
                      />
                    </FormControl>
                    <FormDescription>
                      {oidcConfig?.has_client_secret
                        ? 'Leave empty to keep the existing secret. Enter a new value to rotate.'
                        : 'If omitted, any existing stored client secret is removed.'}
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {testOidcConnectionMutation.isSuccess ? (
                <Alert>
                  <HugeiconsIcon
                    icon={CheckmarkCircle02Icon}
                    size={16}
                    className="text-emerald-500"
                  />
                  <AlertDescription>
                    Connection successful.{' '}
                    <span className="font-mono text-xs">
                      {testOidcConnectionMutation.data.discovered_issuer}
                    </span>
                  </AlertDescription>
                </Alert>
              ) : testOidcConnectionMutation.isError ? (
                <Alert variant="destructive">
                  <HugeiconsIcon icon={AlertCircleIcon} size={16} />
                  <AlertDescription>
                    Connection failed. Verify the issuer URL and try again.
                  </AlertDescription>
                </Alert>
              ) : null}

              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setOidcDialogOpen(false)}
                  disabled={configureExternalAccessOidcMutation.isPending}
                >
                  Cancel
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  disabled={
                    !isOwner ||
                    testOidcConnectionMutation.isPending ||
                    configureExternalAccessOidcMutation.isPending ||
                    !externalAccessOidcForm.watch('issuer_url').trim()
                  }
                  onClick={() => {
                    const issuerUrl = externalAccessOidcForm
                      .getValues('issuer_url')
                      .trim()
                    if (issuerUrl) {
                      testOidcConnectionMutation.mutate(
                        { issuer_url: issuerUrl },
                        {
                          onError: (error) => {
                            toast.error(
                              getApiErrorMessage(error, {
                                oidc_discovery_failed:
                                  'OIDC discovery failed. Verify issuer URL and provider availability.',
                                invalid_input:
                                  'Invalid issuer URL. Enter a valid URL.',
                              }),
                            )
                          },
                        },
                      )
                    }
                  }}
                >
                  {testOidcConnectionMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Testing...
                    </>
                  ) : (
                    'Test connection'
                  )}
                </Button>
                <Button
                  type="submit"
                  disabled={
                    !isOwner || configureExternalAccessOidcMutation.isPending
                  }
                >
                  {configureExternalAccessOidcMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Saving...
                    </>
                  ) : (
                    'Update OIDC provider'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Artifact backend
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {artifactBackendLabel}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Where build artifacts are stored
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Config source
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {artifactSourceLabel}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Effective settings source
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Frontend version
            </p>
            <p className="mt-3 font-mono text-2xl font-bold tracking-tight">
              {webVersionLabel}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {webHealthQuery.data?.channel
                ? `${webHealthQuery.data.channel} channel`
                : 'Loaded oore-web bundle'}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Backend version
            </p>
            <p className="mt-3 font-mono text-2xl font-bold tracking-tight">
              {backendVersionLabel}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {backendHealthQuery.data?.channel
                ? `${backendHealthQuery.data.channel} channel`
                : 'Loaded oored daemon'}
            </p>
          </CardContent>
        </Card>
      </section>

      {settingsQuery.isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {settingsQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load artifact settings: {settingsQuery.error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!settingsQuery.isLoading && !settingsQuery.error ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Artifact Storage
            </CardTitle>
          </CardHeader>
          <CardContent>
            {!canWrite ? (
              <Alert>
                <AlertDescription>
                  You have read access only. Owner/Admin write permission is
                  required to update settings.
                </AlertDescription>
              </Alert>
            ) : null}

            <Form {...storageForm}>
              <form
                onSubmit={storageForm.handleSubmit(onSubmitStorage)}
                className="space-y-4"
              >
                <FormField
                  control={storageForm.control}
                  name="backend_kind"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Backend</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                        disabled={!canWrite || updateStorageMutation.isPending}
                        items={{
                          disabled: 'Disabled',
                          local: 'Local filesystem',
                          object: 'Object storage (S3-compatible)',
                        }}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue placeholder="Select backend" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          <SelectItem value="disabled">Disabled</SelectItem>
                          <SelectItem value="local">
                            Local filesystem
                          </SelectItem>
                          <SelectItem value="object">
                            Object storage (S3-compatible)
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <FormDescription>
                        {backendKind === 'disabled'
                          ? 'Artifacts are recorded as metadata only. Files are not stored, so downloads are unavailable.'
                          : backendKind === 'local'
                            ? 'Artifacts are uploaded to the daemon and stored on local disk (best for single-host setups).'
                            : 'Artifacts are stored in an S3-compatible bucket and served via time-limited download links.'}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {backendKind === 'local' ? (
                  <FormField
                    control={storageForm.control}
                    name="local_base_dir"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Local base directory</FormLabel>
                        <div className="flex flex-col gap-2 md:flex-row">
                          <FormControl>
                            <Input
                              placeholder="/absolute/path/to/artifacts"
                              className="font-mono text-xs"
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
                          {canBrowseLocalFs ? (
                            <div className="flex gap-2">
                              <Button
                                type="button"
                                variant="outline"
                                size="icon"
                                aria-label="Browse"
                                title="Browse"
                                onClick={() => {
                                  setArtifactDirPickerOpen(true)
                                }}
                                disabled={
                                  !canWrite || updateStorageMutation.isPending
                                }
                              >
                                <HugeiconsIcon icon={Folder02Icon} size={16} />
                              </Button>
                            </div>
                          ) : null}
                        </div>
                        <FormDescription>
                          Absolute path on the daemon host where artifact files
                          are stored.
                          {!canBrowseLocalFs ? (
                            <>
                              {' '}
                              For security, folder browsing is only available
                              from localhost.
                            </>
                          ) : null}
                        </FormDescription>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                ) : null}

                {backendKind === 'object' ? (
                  <>
                    <FormField
                      control={storageForm.control}
                      name="object_service"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Service</FormLabel>
                          <Select
                            value={field.value ?? 'aws_s3'}
                            onValueChange={field.onChange}
                            disabled={
                              !canWrite || updateStorageMutation.isPending
                            }
                            items={{
                              aws_s3: 'AWS S3',
                              cloudflare_r2: 'Cloudflare R2',
                              minio: 'MinIO',
                              custom: 'Custom S3-compatible',
                            }}
                          >
                            <FormControl>
                              <SelectTrigger>
                                <SelectValue placeholder="Select a service" />
                              </SelectTrigger>
                            </FormControl>
                            <SelectContent>
                              <SelectItem value="aws_s3">AWS S3</SelectItem>
                              <SelectItem value="cloudflare_r2">
                                Cloudflare R2
                              </SelectItem>
                              <SelectItem value="minio">MinIO</SelectItem>
                              <SelectItem value="custom">
                                Custom S3-compatible
                              </SelectItem>
                            </SelectContent>
                          </Select>
                          <FormDescription>
                            Presets apply sane defaults (R2 uses region `auto`
                            and requires an endpoint).
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="s3_bucket"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Bucket</FormLabel>
                          <FormControl>
                            <Input
                              placeholder="oore-artifacts"
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
                          <FormDescription>
                            Keep this bucket private. Oore serves files via
                            time-limited signed URLs.
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="s3_region"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Region</FormLabel>
                          <FormControl>
                            <Input
                              placeholder={
                                objectService === 'cloudflare_r2'
                                  ? 'auto'
                                  : 'us-east-1'
                              }
                              {...field}
                              disabled={
                                objectService === 'cloudflare_r2' ||
                                !canWrite ||
                                updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
                          <FormDescription>
                            {objectService === 'cloudflare_r2'
                              ? 'Cloudflare R2 uses region `auto` for signing.'
                              : 'AWS region used for request signing.'}
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="s3_endpoint"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>
                            {objectService === 'aws_s3'
                              ? 'Endpoint (optional)'
                              : 'Endpoint (required)'}
                          </FormLabel>
                          <FormControl>
                            <Input
                              placeholder={
                                objectService === 'cloudflare_r2'
                                  ? 'https://<account-id>.r2.cloudflarestorage.com'
                                  : objectService === 'minio'
                                    ? 'http://127.0.0.1:9000'
                                    : objectService === 'custom'
                                      ? 'https://s3.example.com'
                                      : 'Leave empty for AWS S3'
                              }
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
                          <FormDescription>
                            {objectService === 'aws_s3'
                              ? 'Leave empty to use AWS defaults for the selected region.'
                              : 'Base URL for the S3-compatible API endpoint.'}
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="access_key_id"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Access key ID</FormLabel>
                          <FormControl>
                            <Input
                              placeholder={
                                settings?.has_access_key_id
                                  ? 'Stored (leave empty to keep current key)'
                                  : 'Enter access key'
                              }
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
                          <FormDescription>
                            Leave empty to keep the current stored value.
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={storageForm.control}
                      name="secret_access_key"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>Secret access key</FormLabel>
                          <FormControl>
                            <Input
                              type="password"
                              placeholder={
                                settings?.has_secret_access_key
                                  ? 'Stored (leave empty to keep current key)'
                                  : 'Enter secret key'
                              }
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
                          <FormDescription>
                            Stored encrypted at rest using the daemon encryption
                            key.
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />
                  </>
                ) : null}

                <div className="flex justify-end">
                  <Button
                    type="submit"
                    disabled={!canWrite || updateStorageMutation.isPending}
                  >
                    {updateStorageMutation.isPending ? (
                      <>
                        <Spinner className="size-4" />
                        Saving...
                      </>
                    ) : (
                      'Save'
                    )}
                  </Button>
                </div>
              </form>
            </Form>
          </CardContent>
        </Card>
      ) : null}

      <LocalFolderPickerDialog
        open={artifactDirPickerOpen}
        onOpenChange={setArtifactDirPickerOpen}
        enabled={canBrowseLocalFs}
        initialPath={storageForm.getValues('local_base_dir')}
        title="Browse Artifact Folder"
        description="Select a folder on the daemon host where artifact files will be stored."
        selectCurrentLabel="Use Current Folder"
        selectDirectoryLabel="Select Folder"
        onSelectPath={(path) => {
          storageForm.setValue('local_base_dir', path, {
            shouldDirty: true,
            shouldTouch: true,
            shouldValidate: true,
          })
        }}
      />
    </PageLayout>
  )
}
