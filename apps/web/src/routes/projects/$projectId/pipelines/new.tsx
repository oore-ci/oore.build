import { useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { toast } from 'sonner'

import type {
  ConcurrencyPolicy,
  CreatePipelineRequest,
  TriggerConfig,
  UpdatePipelineAndroidSigningRequest,
} from '@/lib/types'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import {
  useCreatePipeline,
  useUpdatePipelineAndroidSigning,
  useValidatePipeline,
} from '@/hooks/use-pipelines'
import {
  defaultArtifactPatterns,
  fileToBase64,
  parseCsv,
  parseEnvVars,
  parseMultiline,
  selectedPlatforms,
  trimToUndefined,
} from '@/lib/pipeline-form-utils'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import PipelineForm from '@/components/pipeline-form'

export const Route = createFileRoute('/projects/$projectId/pipelines/new')({
  staticData: { breadcrumbLabel: 'New Pipeline' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: NewPipelinePage,
})

const emptyDefaults: PipelineFormValues = {
  name: '',
  config_mode: 'auto',
  config_path: '.oore.yaml',
  platform_android: true,
  platform_ios: false,
  platform_macos: false,
  android_signing_release_enabled: false,
  android_signing_release_store_password: '',
  android_signing_release_key_alias: '',
  android_signing_release_key_password: '',
  android_signing_debug_enabled: false,
  android_signing_debug_store_password: '',
  android_signing_debug_key_alias: '',
  android_signing_debug_key_password: '',
  flutter_version: '',
  enable_customization: false,
  pre_build_commands: '',
  build_commands: '',
  post_build_commands: '',
  android_build_args: '',
  ios_build_args: '',
  macos_build_args: '',
  android_command_override: '',
  ios_command_override: '',
  macos_command_override: '',
  env_vars: '',
  artifact_patterns: '*.apk',
  branches: '',
  max_concurrent: undefined,
}

function NewPipelinePage() {
  const { projectId } = Route.useParams()
  const navigate = useNavigate()
  const createMutation = useCreatePipeline()
  const validateMutation = useValidatePipeline()
  const updateSigningMutation = useUpdatePipelineAndroidSigning()
  const [validationErrors, setValidationErrors] = useState<Array<string>>([])

  async function handleSubmit(
    data: PipelineFormValues,
    events: Array<string>,
    cancelPrevious: boolean,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
  ) {
    const platforms = selectedPlatforms(data)
    if (platforms.length === 0) {
      setValidationErrors(['Pick at least one platform to build'])
      return
    }

    const trigger_config: TriggerConfig = {
      events,
      branches: parseCsv(data.branches),
    }

    const concurrency: ConcurrencyPolicy = {
      cancel_previous: cancelPrevious,
      max_concurrent: data.max_concurrent
        ? Number(data.max_concurrent)
        : undefined,
    }

    const commands = data.enable_customization
      ? {
          pre_build: parseMultiline(data.pre_build_commands),
          build: parseMultiline(data.build_commands),
          post_build: parseMultiline(data.post_build_commands),
        }
      : { pre_build: [], build: [], post_build: [] }

    const customPatterns = data.enable_customization
      ? parseMultiline(data.artifact_patterns)
      : []

    const payload: CreatePipelineRequest = {
      name: data.name.trim(),
      config_path:
        data.config_mode === 'explicit'
          ? data.config_path?.trim()
          : '.oore.yaml',
      config_path_explicit: data.config_mode === 'explicit',
      execution_config: {
        platforms,
        flutter_version: data.flutter_version?.trim() || undefined,
        commands,
        platform_build_args: data.enable_customization
          ? {
              android: parseMultiline(data.android_build_args),
              ios: parseMultiline(data.ios_build_args),
              macos: parseMultiline(data.macos_build_args),
            }
          : { android: [], ios: [], macos: [] },
        platform_commands: data.enable_customization
          ? {
              android: data.android_command_override?.trim() || undefined,
              ios: data.ios_command_override?.trim() || undefined,
              macos: data.macos_command_override?.trim() || undefined,
            }
          : {},
        env: data.enable_customization ? parseEnvVars(data.env_vars) : [],
        artifact_patterns:
          customPatterns.length > 0
            ? customPatterns
            : defaultArtifactPatterns(platforms),
      },
      trigger_config,
      concurrency,
    }

    try {
      const result = await validateMutation.mutateAsync(payload)
      if (!result.valid && result.errors?.length) {
        setValidationErrors(result.errors)
        return
      }
    } catch {
      // Validation endpoint is best-effort.
    }

    setValidationErrors([])

    const signingPayload = await buildSigningPayload(
      data,
      releaseKeystoreFile,
      debugKeystoreFile,
    )
    if (
      (data.android_signing_release_enabled ||
        data.android_signing_debug_enabled) &&
      !signingPayload
    ) {
      return
    }

    try {
      const created = await createMutation.mutateAsync({
        projectId,
        data: payload,
      })
      if (signingPayload) {
        await updateSigningMutation.mutateAsync({
          pipelineId: created.pipeline.id,
          data: signingPayload,
        })
      }
      toast.success('Pipeline created')
      void navigate({ to: '/projects/$projectId', params: { projectId } })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(`Failed to create pipeline: ${message}`)
    }
  }

  async function buildSigningPayload(
    data: PipelineFormValues,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
  ): Promise<UpdatePipelineAndroidSigningRequest | null> {
    const releaseEnabled = data.android_signing_release_enabled
    const debugEnabled = data.android_signing_debug_enabled
    const releaseAlias = trimToUndefined(data.android_signing_release_key_alias)
    const releaseStorePassword = trimToUndefined(
      data.android_signing_release_store_password,
    )
    const releaseKeyPassword = trimToUndefined(
      data.android_signing_release_key_password,
    )
    const debugAlias = trimToUndefined(data.android_signing_debug_key_alias)
    const debugStorePassword = trimToUndefined(
      data.android_signing_debug_store_password,
    )
    const debugKeyPassword = trimToUndefined(
      data.android_signing_debug_key_password,
    )

    const anySigningInput =
      releaseEnabled ||
      debugEnabled ||
      !!releaseKeystoreFile ||
      !!debugKeystoreFile ||
      !!releaseAlias ||
      !!releaseStorePassword ||
      !!releaseKeyPassword ||
      !!debugAlias ||
      !!debugStorePassword ||
      !!debugKeyPassword

    if (anySigningInput && !data.platform_android) {
      setValidationErrors([
        'Android signing profiles require Android platform to be enabled',
      ])
      return null
    }

    if (!anySigningInput) return null

    const profileErrors: Array<string> = []
    if (releaseEnabled) {
      if (!releaseKeystoreFile)
        profileErrors.push(
          'Release signing is enabled but no release keystore file is selected',
        )
      if (!releaseAlias)
        profileErrors.push('Release signing key alias is required')
      if (!releaseStorePassword)
        profileErrors.push('Release store password is required')
      if (!releaseKeyPassword)
        profileErrors.push('Release key password is required')
    }
    if (debugEnabled) {
      if (!debugKeystoreFile)
        profileErrors.push(
          'Debug signing is enabled but no debug keystore file is selected',
        )
      if (!debugAlias) profileErrors.push('Debug signing key alias is required')
      if (!debugStorePassword)
        profileErrors.push('Debug store password is required')
      if (!debugKeyPassword)
        profileErrors.push('Debug key password is required')
    }
    if (profileErrors.length > 0) {
      setValidationErrors(profileErrors)
      return null
    }

    const release =
      releaseEnabled ||
      releaseKeystoreFile ||
      releaseAlias ||
      releaseStorePassword ||
      releaseKeyPassword
        ? {
            enabled: releaseEnabled,
            keystore_filename: releaseKeystoreFile?.name,
            keystore_base64: releaseKeystoreFile
              ? await fileToBase64(releaseKeystoreFile)
              : undefined,
            store_password: releaseStorePassword,
            key_alias: releaseAlias,
            key_password: releaseKeyPassword,
          }
        : undefined

    const debug =
      debugEnabled ||
      debugKeystoreFile ||
      debugAlias ||
      debugStorePassword ||
      debugKeyPassword
        ? {
            enabled: debugEnabled,
            keystore_filename: debugKeystoreFile?.name,
            keystore_base64: debugKeystoreFile
              ? await fileToBase64(debugKeystoreFile)
              : undefined,
            store_password: debugStorePassword,
            key_alias: debugAlias,
            key_password: debugKeyPassword,
          }
        : undefined

    return { debug, release }
  }

  return (
    <PageLayout width="wide">
      <PageHeader
        title="New Pipeline"
        back={{ to: `/projects/${projectId}`, label: 'Project' }}
        description="Configure a new build pipeline for this project."
      />
      <div className="mx-auto max-w-4xl">
        <PipelineForm
          initialValues={emptyDefaults}
          initialEvents={['push']}
          initialCancelPrevious={true}
          onSubmit={handleSubmit}
          onCancel={() =>
            void navigate({ to: '/projects/$projectId', params: { projectId } })
          }
          submitLabel="Create"
          isPending={
            createMutation.isPending || updateSigningMutation.isPending
          }
          validationErrors={validationErrors}
        />
      </div>
    </PageLayout>
  )
}
