import { useEffect, useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { toast } from 'sonner'

import type {
  ConcurrencyPolicy,
  TriggerConfig,
  UpdatePipelineAndroidSigningRequest,
  UpdatePipelineIosSigningRequest,
  UpdatePipelineRequest,
} from '@/lib/types'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import {
  usePipeline,
  usePipelineAndroidSigning,
  usePipelineIosDevices,
  usePipelineIosSigning,
  useRegisterPipelineIosDevice,
  useSyncPipelineIosSigning,
  useUpdatePipeline,
  useUpdatePipelineAndroidSigning,
  useUpdatePipelineIosSigning,
} from '@/hooks/use-pipelines'
import {
  defaultArtifactPatterns,
  fileToBase64,
  fileToUtf8,
  hasCustomFallback,
  parseBundleIdsInput,
  parseCsv,
  parseEnvVars,
  parseMultiline,
  selectedPlatforms,
  toMultiline,
  trimToUndefined,
} from '@/lib/pipeline-form-utils'
import { webPageTitle } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import PipelineForm from '@/components/pipeline-form'

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId_/edit',
)({
  staticData: { breadcrumbLabel: 'Edit Pipeline' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: EditPipelinePage,
})

function EditPipelinePage() {
  const { projectId, pipelineId } = Route.useParams()
  const navigate = useNavigate()
  const { data, isLoading, error } = usePipeline(pipelineId)
  const signingQuery = usePipelineAndroidSigning(pipelineId)
  const iosSigningQuery = usePipelineIosSigning(pipelineId)
  const iosDevicesQuery = usePipelineIosDevices(pipelineId)
  const updateMutation = useUpdatePipeline()
  const updateSigningMutation = useUpdatePipelineAndroidSigning()
  const updateIosSigningMutation = useUpdatePipelineIosSigning()
  const registerIosDeviceMutation = useRegisterPipelineIosDevice()
  const syncIosSigningMutation = useSyncPipelineIosSigning()
  const [deviceName, setDeviceName] = useState('')
  const [deviceUdid, setDeviceUdid] = useState('')
  const [validationErrors, setValidationErrors] = useState<Array<string>>([])

  useEffect(() => {
    const label = data?.pipeline.name
      ? `Edit ${data.pipeline.name}`
      : 'Edit Pipeline'
    document.title = webPageTitle(label)
  }, [data?.pipeline.name])

  if (isLoading || signingQuery.isLoading || iosSigningQuery.isLoading) {
    return (
      <PageLayout width="wide">
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-96 w-full" />
      </PageLayout>
    )
  }

  if (error || !data) {
    return (
      <PageLayout width="wide">
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load pipeline: {error?.message ?? 'Not found'}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  const { pipeline } = data

  const platformSet = new Set(pipeline.execution_config.platforms)
  const custom = hasCustomFallback(pipeline)

  const formInitialValues: PipelineFormValues = {
    name: pipeline.name,
    config_mode: pipeline.config_path_explicit ? 'explicit' : 'auto',
    config_path: pipeline.config_path,
    platform_android: platformSet.has('android'),
    platform_ios: platformSet.has('ios'),
    platform_macos: platformSet.has('macos'),
    android_signing_release_enabled:
      signingQuery.data?.release.enabled ?? false,
    android_signing_release_store_password: '',
    android_signing_release_key_alias:
      signingQuery.data?.release.key_alias ?? '',
    android_signing_release_key_password: '',
    android_signing_debug_enabled: signingQuery.data?.debug.enabled ?? false,
    android_signing_debug_store_password: '',
    android_signing_debug_key_alias: signingQuery.data?.debug.key_alias ?? '',
    android_signing_debug_key_password: '',
    ios_signing_enabled: iosSigningQuery.data?.enabled ?? false,
    ios_signing_mode: iosSigningQuery.data?.mode ?? 'manual',
    ios_signing_team_id: iosSigningQuery.data?.team_id ?? '',
    ios_signing_bundle_ids: (iosSigningQuery.data?.bundle_ids ?? []).join('\n'),
    ios_signing_p12_password: '',
    ios_signing_api_key_id: iosSigningQuery.data?.api_key_id ?? '',
    ios_signing_api_issuer_id: iosSigningQuery.data?.api_issuer_id ?? '',
    flutter_version: pipeline.execution_config.flutter_version ?? '',
    enable_customization: custom,
    pre_build_commands: toMultiline(
      pipeline.execution_config.commands.pre_build,
    ),
    build_commands: toMultiline(pipeline.execution_config.commands.build),
    post_build_commands: toMultiline(
      pipeline.execution_config.commands.post_build,
    ),
    android_build_args: toMultiline(
      pipeline.execution_config.platform_build_args?.android ?? [],
    ),
    ios_build_args: toMultiline(
      pipeline.execution_config.platform_build_args?.ios ?? [],
    ),
    macos_build_args: toMultiline(
      pipeline.execution_config.platform_build_args?.macos ?? [],
    ),
    android_command_override:
      pipeline.execution_config.platform_commands?.android ?? '',
    ios_command_override:
      pipeline.execution_config.platform_commands?.ios ?? '',
    macos_command_override:
      pipeline.execution_config.platform_commands?.macos ?? '',
    env_vars: toMultiline(
      (pipeline.execution_config.env ?? []).map(
        (entry) => `${entry.key}=${entry.value}`,
      ),
    ),
    artifact_patterns: toMultiline(pipeline.execution_config.artifact_patterns),
    branches: pipeline.trigger_config.branches.join(', '),
    max_concurrent: pipeline.concurrency.max_concurrent
      ? String(pipeline.concurrency.max_concurrent)
      : undefined,
  }

  async function handleSubmit(
    values: PipelineFormValues,
    events: Array<string>,
    cancelPrevious: boolean,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
    iosSigningFiles: {
      p12File: File | null
      apiKeyFile: File | null
      profileFiles: Record<string, File | null>
    },
  ) {
    const platforms = selectedPlatforms(values)
    if (platforms.length === 0) {
      setValidationErrors(['Pick at least one platform to build'])
      return
    }

    const trigger_config: TriggerConfig = {
      events,
      branches: parseCsv(values.branches),
    }

    const concurrency: ConcurrencyPolicy = {
      cancel_previous: cancelPrevious,
      max_concurrent: values.max_concurrent
        ? Number(values.max_concurrent)
        : undefined,
    }

    const commands = values.enable_customization
      ? {
          pre_build: parseMultiline(values.pre_build_commands),
          build: parseMultiline(values.build_commands),
          post_build: parseMultiline(values.post_build_commands),
        }
      : { pre_build: [], build: [], post_build: [] }

    const customPatterns = values.enable_customization
      ? parseMultiline(values.artifact_patterns)
      : []

    const payload: UpdatePipelineRequest = {
      name: values.name.trim(),
      config_path:
        values.config_mode === 'explicit'
          ? values.config_path?.trim()
          : '.oore.yaml',
      config_path_explicit: values.config_mode === 'explicit',
      execution_config: {
        platforms,
        flutter_version: values.flutter_version?.trim() || undefined,
        commands,
        platform_build_args: values.enable_customization
          ? {
              android: parseMultiline(values.android_build_args),
              ios: parseMultiline(values.ios_build_args),
              macos: parseMultiline(values.macos_build_args),
            }
          : { android: [], ios: [], macos: [] },
        platform_commands: values.enable_customization
          ? {
              android: values.android_command_override?.trim() || undefined,
              ios: values.ios_command_override?.trim() || undefined,
              macos: values.macos_command_override?.trim() || undefined,
            }
          : {},
        env: values.enable_customization ? parseEnvVars(values.env_vars) : [],
        artifact_patterns:
          customPatterns.length > 0
            ? customPatterns
            : defaultArtifactPatterns(platforms),
      },
      trigger_config,
      concurrency,
    }

    setValidationErrors([])
    let hasPayloadErrors = false
    const origSetErrors = (errors: Array<string>) => {
      hasPayloadErrors = true
      setValidationErrors(errors)
    }

    const signingPayload = await buildAndroidSigningPayload(
      values,
      releaseKeystoreFile,
      debugKeystoreFile,
      origSetErrors,
    )
    const iosSigningPayload = await buildIosSigningPayload(
      values,
      iosSigningFiles,
      origSetErrors,
    )
    if (hasPayloadErrors) {
      return
    }

    try {
      await updateMutation.mutateAsync({
        pipelineId: pipeline.id,
        data: payload,
      })
      if (signingPayload) {
        await updateSigningMutation.mutateAsync({
          pipelineId: pipeline.id,
          data: signingPayload,
        })
      }
      if (iosSigningPayload) {
        await updateIosSigningMutation.mutateAsync({
          pipelineId: pipeline.id,
          data: iosSigningPayload,
        })
      }
      toast.success('Pipeline updated')
      void navigate({
        to: '/projects/$projectId/pipelines/$pipelineId',
        params: { projectId, pipelineId },
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(`Failed to update pipeline: ${message}`)
    }
  }

  async function buildAndroidSigningPayload(
    values: PipelineFormValues,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
    reportErrors: (errors: Array<string>) => void,
  ): Promise<UpdatePipelineAndroidSigningRequest | null> {
    const releaseEnabled = values.android_signing_release_enabled
    const debugEnabled = values.android_signing_debug_enabled

    const releaseAlias = trimToUndefined(
      values.android_signing_release_key_alias,
    )
    const releaseStorePassword = trimToUndefined(
      values.android_signing_release_store_password,
    )
    const releaseKeyPassword = trimToUndefined(
      values.android_signing_release_key_password,
    )
    const debugAlias = trimToUndefined(values.android_signing_debug_key_alias)
    const debugStorePassword = trimToUndefined(
      values.android_signing_debug_store_password,
    )
    const debugKeyPassword = trimToUndefined(
      values.android_signing_debug_key_password,
    )

    const releaseHasStored = !!signingQuery.data?.release.has_keystore
    const debugHasStored = !!signingQuery.data?.debug.has_keystore

    if (!values.platform_android) {
      // Platform disabled — skip signing entirely, don't send payload
      return null
    }

    const profileErrors: Array<string> = []
    if (releaseEnabled) {
      if (!releaseKeystoreFile && !releaseHasStored)
        profileErrors.push(
          'Release signing is enabled but no release keystore is configured',
        )
      if (!releaseAlias)
        profileErrors.push('Release signing key alias is required')
      if (
        !releaseStorePassword &&
        !signingQuery.data?.release.has_store_password
      )
        profileErrors.push('Release store password is required')
      if (!releaseKeyPassword && !signingQuery.data?.release.has_key_password)
        profileErrors.push('Release key password is required')
    }
    if (debugEnabled) {
      if (!debugKeystoreFile && !debugHasStored)
        profileErrors.push(
          'Debug signing is enabled but no debug keystore is configured',
        )
      if (!debugAlias) profileErrors.push('Debug signing key alias is required')
      if (!debugStorePassword && !signingQuery.data?.debug.has_store_password)
        profileErrors.push('Debug store password is required')
      if (!debugKeyPassword && !signingQuery.data?.debug.has_key_password)
        profileErrors.push('Debug key password is required')
    }
    if (profileErrors.length > 0) {
      reportErrors(profileErrors)
      return null
    }

    const releaseTouched =
      releaseEnabled !== (signingQuery.data?.release.enabled ?? false) ||
      !!releaseKeystoreFile ||
      !!releaseAlias ||
      !!releaseStorePassword ||
      !!releaseKeyPassword
    const debugTouched =
      debugEnabled !== (signingQuery.data?.debug.enabled ?? false) ||
      !!debugKeystoreFile ||
      !!debugAlias ||
      !!debugStorePassword ||
      !!debugKeyPassword

    if (!releaseTouched && !debugTouched) return null

    return {
      release: releaseTouched
        ? {
            enabled: releaseEnabled,
            keystore_filename: releaseKeystoreFile?.name,
            keystore_base64: releaseKeystoreFile
              ? await fileToBase64(releaseKeystoreFile)
              : undefined,
            key_alias: releaseAlias,
            store_password: releaseStorePassword,
            key_password: releaseKeyPassword,
          }
        : undefined,
      debug: debugTouched
        ? {
            enabled: debugEnabled,
            keystore_filename: debugKeystoreFile?.name,
            keystore_base64: debugKeystoreFile
              ? await fileToBase64(debugKeystoreFile)
              : undefined,
            key_alias: debugAlias,
            store_password: debugStorePassword,
            key_password: debugKeyPassword,
          }
        : undefined,
    }
  }

  async function buildIosSigningPayload(
    values: PipelineFormValues,
    iosSigningFiles: {
      p12File: File | null
      apiKeyFile: File | null
      profileFiles: Record<string, File | null>
    },
    reportErrors: (errors: Array<string>) => void,
  ): Promise<UpdatePipelineIosSigningRequest | null> {
    const bundleIds = parseBundleIdsInput(values.ios_signing_bundle_ids)
    const teamId = trimToUndefined(values.ios_signing_team_id)
    const p12Password = trimToUndefined(values.ios_signing_p12_password)
    const apiKeyId = trimToUndefined(values.ios_signing_api_key_id)
    const apiIssuerId = trimToUndefined(values.ios_signing_api_issuer_id)
    const existing = iosSigningQuery.data

    const anyIosInput =
      values.ios_signing_enabled !== (existing?.enabled ?? false) ||
      values.ios_signing_mode !== (existing?.mode ?? 'manual') ||
      bundleIds.join(',') !== (existing?.bundle_ids ?? []).join(',') ||
      teamId !== trimToUndefined(existing?.team_id) ||
      !!iosSigningFiles.p12File ||
      !!p12Password ||
      apiKeyId !== trimToUndefined(existing?.api_key_id) ||
      apiIssuerId !== trimToUndefined(existing?.api_issuer_id) ||
      !!iosSigningFiles.apiKeyFile ||
      Object.values(iosSigningFiles.profileFiles).some(Boolean)

    if (!values.platform_ios) {
      // Platform disabled — skip signing entirely, don't send payload
      return null
    }

    if (!anyIosInput) return null

    const errors: Array<string> = []
    if (values.ios_signing_enabled) {
      if (!teamId) errors.push('iOS signing requires Team ID')
      if (bundleIds.length === 0)
        errors.push('iOS signing requires at least one bundle identifier')
    }

    if (
      values.ios_signing_enabled &&
      (values.ios_signing_mode === 'manual' ||
        values.ios_signing_mode === 'hybrid')
    ) {
      const hasStoredP12 = existing?.has_p12 ?? false
      const hasStoredP12Password = existing?.has_p12_password ?? false
      if (!iosSigningFiles.p12File && !hasStoredP12)
        errors.push(
          'Manual/Hybrid iOS signing requires .p12 certificate (upload or keep existing)',
        )
      if (!p12Password && !hasStoredP12Password)
        errors.push('Manual/Hybrid iOS signing requires p12 password')

      for (const bundleId of bundleIds) {
        const hasStoredProfile = !!existing?.provisioning_profiles?.find(
          (profile) => profile.bundle_id === bundleId && profile.has_profile,
        )
        if (!iosSigningFiles.profileFiles[bundleId] && !hasStoredProfile) {
          errors.push(
            `Manual/Hybrid iOS signing requires provisioning profile for ${bundleId}`,
          )
        }
      }
    }

    if (
      values.ios_signing_enabled &&
      (values.ios_signing_mode === 'api' || values.ios_signing_mode === 'hybrid')
    ) {
      const hasStoredApi = existing?.has_api_key ?? false
      if (!apiKeyId && !existing?.api_key_id)
        errors.push('API/Hybrid iOS signing requires API key ID')
      if (!apiIssuerId && !existing?.api_issuer_id)
        errors.push('API/Hybrid iOS signing requires API issuer ID')
      if (!iosSigningFiles.apiKeyFile && !hasStoredApi)
        errors.push(
          'API/Hybrid iOS signing requires .p8 private key file (upload or keep existing)',
        )
    }

    if (errors.length > 0) {
      reportErrors(errors)
      return null
    }

    const provisioningProfiles: Array<{
      bundle_id: string
      profile_filename?: string
      profile_base64?: string
    }> = []
    for (const bundleId of bundleIds) {
      const profileFile = iosSigningFiles.profileFiles[bundleId]
      if (!profileFile) continue
      provisioningProfiles.push({
        bundle_id: bundleId,
        profile_filename: profileFile.name,
        profile_base64: await fileToBase64(profileFile),
      })
    }

    const apiPrivateKey = iosSigningFiles.apiKeyFile
      ? await fileToUtf8(iosSigningFiles.apiKeyFile)
      : undefined

    return {
      enabled: values.ios_signing_enabled,
      mode: values.ios_signing_mode,
      team_id: teamId,
      bundle_ids: bundleIds,
      certificate:
        iosSigningFiles.p12File || p12Password
          ? {
              p12_filename: iosSigningFiles.p12File?.name,
              p12_base64: iosSigningFiles.p12File
                ? await fileToBase64(iosSigningFiles.p12File)
                : undefined,
              p12_password: p12Password,
            }
          : undefined,
      provisioning_profiles: provisioningProfiles,
      api_credentials:
        apiKeyId || apiIssuerId || apiPrivateKey
          ? {
              key_id: apiKeyId ?? existing?.api_key_id,
              issuer_id: apiIssuerId ?? existing?.api_issuer_id,
              private_key_base64: apiPrivateKey ? btoa(apiPrivateKey) : undefined,
            }
          : undefined,
    }
  }

  async function handleRegisterDevice() {
    const name = deviceName.trim()
    const udid = deviceUdid.trim()
    if (!name || !udid) {
      toast.error('Device name and UDID are required')
      return
    }

    try {
      const response = await registerIosDeviceMutation.mutateAsync({
        pipelineId,
        data: { name, udid, platform: 'IOS' },
      })
      setDeviceName('')
      setDeviceUdid('')
      toast.success(
        response.profile_sync_triggered
          ? 'Device registered and profiles synced'
          : 'Device registered',
      )
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(`Failed to register device: ${message}`)
    }
  }

  async function handleSyncIosSigning() {
    try {
      const response = await syncIosSigningMutation.mutateAsync(pipelineId)
      const warningSuffix =
        response.warnings.length > 0
          ? ` (${response.warnings.length} warning${response.warnings.length === 1 ? '' : 's'})`
          : ''
      toast.success(
        `iOS signing sync completed: ${response.updated_profiles} profile${response.updated_profiles === 1 ? '' : 's'} updated${warningSuffix}`,
      )
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(`Failed to sync iOS signing: ${message}`)
    }
  }

  return (
    <PageLayout width="wide">
      <PageHeader
        title={`Edit: ${pipeline.name}`}
        back={{
          to: `/projects/${projectId}/pipelines/${pipelineId}`,
          label: 'Pipeline',
        }}
        description="Update pipeline configuration."
      />
      <div className="mx-auto max-w-4xl">
        <PipelineForm
          initialValues={formInitialValues}
          initialEvents={pipeline.trigger_config.events}
          initialCancelPrevious={pipeline.concurrency.cancel_previous}
          onSubmit={handleSubmit}
          onCancel={() =>
            void navigate({
              to: '/projects/$projectId/pipelines/$pipelineId',
              params: { projectId, pipelineId },
            })
          }
          submitLabel="Save"
          isPending={
            updateMutation.isPending ||
            updateSigningMutation.isPending ||
            updateIosSigningMutation.isPending
          }
          validationErrors={validationErrors}
          signingData={signingQuery.data}
          iosSigningData={iosSigningQuery.data}
        >
          {iosSigningQuery.data?.enabled &&
            (iosSigningQuery.data.mode === 'api' ||
              iosSigningQuery.data.mode === 'hybrid') && (
              <Card className="mt-6">
                <CardContent className="space-y-4">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <h3 className="text-sm font-medium">
                        Registered iOS Devices
                      </h3>
                      <p className="text-xs text-muted-foreground">
                        Register UDIDs and sync provisioning profiles for
                        API/hybrid modes.
                      </p>
                    </div>
                    <Button
                      type="button"
                      variant="outline"
                      disabled={syncIosSigningMutation.isPending}
                      onClick={() => void handleSyncIosSigning()}
                    >
                      {syncIosSigningMutation.isPending
                        ? 'Syncing...'
                        : 'Sync Profiles'}
                    </Button>
                  </div>

                  <div className="grid gap-3 md:grid-cols-[1fr_1fr_auto]">
                    <Input
                      placeholder="Device name"
                      value={deviceName}
                      onChange={(event) => setDeviceName(event.target.value)}
                    />
                    <Input
                      placeholder="UDID"
                      className="font-mono"
                      value={deviceUdid}
                      onChange={(event) => setDeviceUdid(event.target.value)}
                    />
                    <Button
                      type="button"
                      disabled={registerIosDeviceMutation.isPending}
                      onClick={() => void handleRegisterDevice()}
                    >
                      {registerIosDeviceMutation.isPending
                        ? 'Registering...'
                        : 'Register Device'}
                    </Button>
                  </div>

                  {iosDevicesQuery.isLoading ? (
                    <p className="text-xs text-muted-foreground">
                      Loading devices...
                    </p>
                  ) : iosDevicesQuery.data?.devices.length ? (
                    <div className="rounded-md border">
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>Name</TableHead>
                            <TableHead>UDID</TableHead>
                            <TableHead>Status</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {iosDevicesQuery.data.devices.map((device) => (
                            <TableRow key={device.id}>
                              <TableCell>{device.name}</TableCell>
                              <TableCell className="font-mono text-xs">
                                {device.udid}
                              </TableCell>
                              <TableCell>{device.status}</TableCell>
                            </TableRow>
                          ))}
                        </TableBody>
                      </Table>
                    </div>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      No iOS devices registered for this pipeline.
                    </p>
                  )}
                </CardContent>
              </Card>
            )}
        </PipelineForm>
      </div>
    </PageLayout>
  )
}
