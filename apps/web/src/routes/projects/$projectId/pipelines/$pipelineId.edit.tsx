import { useEffect, useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { toast } from 'sonner'

import type {
  ConcurrencyPolicy,
  TriggerConfig,
  UpdatePipelineAndroidSigningRequest,
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
  useUpdatePipeline,
  useUpdatePipelineAndroidSigning,
} from '@/hooks/use-pipelines'
import {
  defaultArtifactPatterns,
  fileToBase64,
  hasCustomFallback,
  parseCsv,
  parseEnvVars,
  parseMultiline,
  selectedPlatforms,
  toMultiline,
  trimToUndefined,
} from '@/lib/pipeline-form-utils'
import { webPageTitle } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Skeleton } from '@/components/ui/skeleton'
import PipelineForm from '@/components/pipeline-form'

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId/edit',
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
  const updateMutation = useUpdatePipeline()
  const updateSigningMutation = useUpdatePipelineAndroidSigning()
  const [validationErrors, setValidationErrors] = useState<Array<string>>([])

  useEffect(() => {
    const label = data?.pipeline.name
      ? `Edit ${data.pipeline.name}`
      : 'Edit Pipeline'
    document.title = webPageTitle(label)
  }, [data?.pipeline.name])

  if (isLoading) {
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

    const signingPayload = await buildSigningPayload(
      values,
      releaseKeystoreFile,
      debugKeystoreFile,
    )

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

  async function buildSigningPayload(
    values: PipelineFormValues,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
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

    if (
      (releaseEnabled ||
        debugEnabled ||
        !!releaseKeystoreFile ||
        !!debugKeystoreFile ||
        !!releaseAlias ||
        !!releaseStorePassword ||
        !!releaseKeyPassword ||
        !!debugAlias ||
        !!debugStorePassword ||
        !!debugKeyPassword) &&
      !values.platform_android
    ) {
      setValidationErrors([
        'Android signing profiles require Android platform to be enabled',
      ])
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
      setValidationErrors(profileErrors)
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
            updateMutation.isPending || updateSigningMutation.isPending
          }
          validationErrors={validationErrors}
          signingData={signingQuery.data}
        />
      </div>
    </PageLayout>
  )
}
