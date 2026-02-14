import { useEffect, useMemo, useState } from 'react'
import { createFileRoute, redirect, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { toast } from 'sonner'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
  CheckmarkCircle02Icon,
  AlertCircleIcon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useAuthStore } from '@/stores/auth-store'
import { PageMeta } from '@/lib/seo'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  useExternalAccessPreflight,
  useExternalAccessNetworkSettings,
  useArtifactStorageSettings,
  useConfigureExternalAccessOidc,
  useUpdateExternalAccessNetworkSettings,
  useInstancePreferences,
  useUpdateArtifactStorageSettings,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import {
  ApiClientError,
  getApiErrorMessage,
} from '@/lib/api'
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
    provider: z.enum(['disabled', 'local', 's3', 'r2']),
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

    if (value.provider === 'local' && !localDir) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['local_base_dir'],
        message: 'Local base directory is required for local storage.',
      })
    }

    if ((value.provider === 's3' || value.provider === 'r2') && !bucket) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['s3_bucket'],
        message: 'Bucket is required for S3/R2 storage.',
      })
    }

    if (value.provider === 'r2' && !endpoint) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['s3_endpoint'],
        message: 'Endpoint is required for R2 storage.',
      })
    }
  })

type StorageFormValues = z.infer<typeof storageSchema>

const externalAccessOidcSchema = z.object({
  issuer_url: z.url('Please enter a valid issuer URL'),
  client_id: z.string().min(1, 'Client ID is required'),
  client_secret: z.string().optional(),
})

type ExternalAccessOidcFormValues = z.infer<typeof externalAccessOidcSchema>

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
  if (checkId === 'redirect_policy_consistent') {
    return 'Ensure redirect URI policy matches your configured public origin.'
  }
  return 'Resolve this check before enabling External Access.'
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

function parseAllowedOriginsInput(value: string): Array<string> {
  return value
    .split(/[\n,]/g)
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0)
}

function PreferencesPage() {
  const navigate = useNavigate()
  const [readinessOpen, setReadinessOpen] = useState(false)
  const [networkEditorOpen, setNetworkEditorOpen] = useState(false)
  const [oidcDialogOpen, setOidcDialogOpen] = useState(false)
  const canWrite = useHasPermission('instance_settings', 'write')
  const user = useAuthStore((s) => s.user)
  const clearAuth = useAuthStore((s) => s.clearAuth)
  const isOwner = user?.role === 'owner'
  const settingsQuery = useArtifactStorageSettings()
  const preferencesQuery = useInstancePreferences()
  const preflightQuery = useExternalAccessPreflight()
  const networkSettingsQuery = useExternalAccessNetworkSettings()
  const configureExternalAccessOidcMutation = useConfigureExternalAccessOidc()
  const updateNetworkSettingsMutation = useUpdateExternalAccessNetworkSettings()
  const updateStorageMutation = useUpdateArtifactStorageSettings()
  const updatePreferencesMutation = useUpdateInstancePreferences()

  const storageForm = useForm<StorageFormValues>({
    resolver: zodResolver(storageSchema),
    defaultValues: {
      provider: 'disabled',
      local_base_dir: '',
      s3_bucket: '',
      s3_region: 'us-east-1',
      s3_endpoint: '',
      access_key_id: '',
      secret_access_key: '',
    },
  })

  const externalAccessOidcForm = useForm<ExternalAccessOidcFormValues>({
    resolver: zodResolver(externalAccessOidcSchema),
    defaultValues: {
      issuer_url: '',
      client_id: '',
      client_secret: '',
    },
  })
  const externalAccessNetworkForm = useForm<ExternalAccessNetworkFormValues>({
    resolver: zodResolver(externalAccessNetworkSchema),
    defaultValues: {
      public_url: '',
      allowed_origins: '',
    },
  })

  const provider = storageForm.watch('provider')

  useEffect(() => {
    const settings = settingsQuery.data?.settings
    if (!settings) return

    storageForm.reset({
      provider: settings.provider,
      local_base_dir: settings.local_base_dir ?? '',
      s3_bucket: settings.s3_bucket ?? '',
      s3_region: settings.s3_region ?? 'us-east-1',
      s3_endpoint: settings.s3_endpoint ?? '',
      access_key_id: '',
      secret_access_key: '',
    })
  }, [settingsQuery.data, storageForm])

  useEffect(() => {
    const network = networkSettingsQuery.data?.settings
    if (!network) return

    externalAccessNetworkForm.reset({
      public_url: network.public_url ?? '',
      allowed_origins: network.allowed_origins.join('\n'),
    })
  }, [networkSettingsQuery.data, externalAccessNetworkForm])

  function onSubmitStorage(values: StorageFormValues) {
    const payload = {
      provider: values.provider,
      local_base_dir:
        values.provider === 'local' ? values.local_base_dir?.trim() : undefined,
      s3_bucket:
        values.provider === 's3' || values.provider === 'r2'
          ? values.s3_bucket?.trim()
          : undefined,
      s3_region:
        values.provider === 's3' || values.provider === 'r2'
          ? values.s3_region?.trim() || 'us-east-1'
          : undefined,
      s3_endpoint:
        values.provider === 's3' || values.provider === 'r2'
          ? values.s3_endpoint?.trim() || undefined
          : undefined,
      access_key_id:
        values.provider === 's3' || values.provider === 'r2'
          ? values.access_key_id?.trim() || undefined
          : undefined,
      secret_access_key:
        values.provider === 's3' || values.provider === 'r2'
          ? values.secret_access_key?.trim() || undefined
          : undefined,
    } as const

    updateStorageMutation.mutate(payload, {
      onSuccess: (res) => {
        toast.success(`Artifact storage updated to ${res.settings.provider}`)
        storageForm.setValue('access_key_id', '')
        storageForm.setValue('secret_access_key', '')
      },
      onError: (error) => {
        toast.error(
          error instanceof Error
            ? error.message
            : 'Failed to update artifact storage settings',
        )
      },
    })
  }

  function onSubmitExternalAccessNetwork(values: ExternalAccessNetworkFormValues) {
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
              external_access_https_required:
                'Public URL must use HTTPS.',
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

  const settings = settingsQuery.data?.settings
  const preferences = preferencesQuery.data?.preferences
  const networkSettings = networkSettingsQuery.data?.settings
  const externalAccessEnabled = preferences?.runtime_mode === 'remote'
  const failedReadinessChecks = useMemo(
    () => preflightQuery.data?.checks.filter((check) => !check.ok) ?? [],
    [preflightQuery.data?.checks],
  )
  const readinessById = useMemo(() => {
    const entries = preflightQuery.data?.checks ?? []
    return new Map(entries.map((check) => [check.id, check]))
  }, [preflightQuery.data?.checks])
  const setupReady = readinessById.get('setup_ready')?.ok ?? false
  const oidcReady = readinessById.get('oidc_configured')?.ok ?? false
  const networkReady =
    (readinessById.get('public_url_https')?.ok ?? false) &&
    (readinessById.get('public_origin_allowed')?.ok ?? false) &&
    (readinessById.get('redirect_policy_consistent')?.ok ?? false)
  const readinessReady = preflightQuery.data?.ready ?? false
  const setupStepsComplete = Number(networkReady) + Number(oidcReady)
  const setupStepCount = 2

  function handleExternalAccessToggle() {
    if (!preferences || updatePreferencesMutation.isPending || !isOwner) return

    const nextMode = externalAccessEnabled ? 'local' : 'remote'
    updatePreferencesMutation.mutate(
      {
        key_storage_mode: preferences.key_storage_mode,
        runtime_mode: nextMode,
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
          if (error instanceof ApiClientError && error.code.startsWith('external_access')) {
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
              {externalAccessEnabled ? 'External Access' : 'Local Only'}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-wrap items-start justify-between gap-3 border p-3">
            <div className="space-y-1">
              <p className="text-sm font-medium">Current access</p>
              <p className="text-xs text-muted-foreground">
                {externalAccessEnabled
                  ? 'Sign-in from network paths is active and uses OIDC.'
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
                  'Turn Off External Access'
                ) : (
                  'Turn On External Access'
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
                          {networkSettings?.public_url ?? 'Set Public URL and allowed origins.'}
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
                    onClick={() => setOidcDialogOpen(true)}
                    disabled={!isOwner}
                    className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <p className="text-sm font-medium">2. Identity</p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {oidcReady ? 'OIDC provider configured.' : 'Configure OIDC provider.'}
                        </p>
                      </div>
                      <Badge variant={oidcReady ? 'success' : 'outline'}>
                        {oidcReady ? 'Ready' : 'Setup'}
                      </Badge>
                    </div>
                    <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
                      Configure
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
                        <div key={check.id} className="border border-border/60 p-3">
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
                    {networkSettings?.public_url ?? 'Set Public URL and allowed origins.'}
                  </p>
                  <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
                    Edit
                    <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
                  </p>
                </button>

                <button
                  type="button"
                  onClick={() => setOidcDialogOpen(true)}
                  disabled={!isOwner}
                  className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  <p className="text-sm font-medium">Identity settings</p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Update issuer and client credentials.
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
                    'Save Network Settings'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <Dialog open={oidcDialogOpen} onOpenChange={setOidcDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Configure OIDC for External Access</DialogTitle>
            <DialogDescription>
              Owner-only. This updates runtime OIDC settings used by External
              Access sign-in.
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
                      <Input
                        type="url"
                        placeholder="https://accounts.google.com"
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
                        placeholder="Leave empty for public clients"
                        {...field}
                        disabled={configureExternalAccessOidcMutation.isPending}
                      />
                    </FormControl>
                    <FormDescription>
                      If omitted, any existing stored client secret is removed.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

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
                    'Save OIDC Settings'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <section className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Artifact provider
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {settings?.provider ?? 'disabled'}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Current artifact backend
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Config source
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {settings?.source ?? 'default'}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Database, environment, or default
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
                  name="provider"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Provider</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                        disabled={!canWrite || updateStorageMutation.isPending}
                        items={{
                          disabled: 'Disabled',
                          local: 'Local filesystem',
                          s3: 'S3-compatible (AWS/MinIO)',
                          r2: 'Cloudflare R2',
                        }}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue placeholder="Select provider" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          <SelectItem value="disabled">Disabled</SelectItem>
                          <SelectItem value="local">
                            Local filesystem
                          </SelectItem>
                          <SelectItem value="s3">
                            S3-compatible (AWS/MinIO)
                          </SelectItem>
                          <SelectItem value="r2">Cloudflare R2</SelectItem>
                        </SelectContent>
                      </Select>
                      <FormDescription>
                        {provider === 'disabled'
                          ? 'Artifacts remain in metadata only. Binary downloads are unavailable.'
                          : provider === 'local'
                            ? 'Runner uploads artifact binaries to this daemon and stores files on local disk.'
                            : 'Runner uploads binaries using pre-signed S3-compatible URLs.'}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {provider === 'local' ? (
                  <FormField
                    control={storageForm.control}
                    name="local_base_dir"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Local base directory</FormLabel>
                        <FormControl>
                          <Input
                            placeholder="/Users/arya/Library/Application Support/oore/artifacts"
                            {...field}
                            disabled={
                              !canWrite || updateStorageMutation.isPending
                            }
                          />
                        </FormControl>
                        <FormDescription>
                          Absolute path on the daemon host where artifact files
                          are stored.
                        </FormDescription>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                ) : null}

                {provider === 's3' || provider === 'r2' ? (
                  <>
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
                              placeholder="us-east-1"
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
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
                            Endpoint (optional for S3, required for R2)
                          </FormLabel>
                          <FormControl>
                            <Input
                              placeholder="https://<account-id>.r2.cloudflarestorage.com"
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
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
                            Leave empty to keep current stored value.
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
    </PageLayout>
  )
}
