import { useRef, useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  CircleAlert as AlertCircleIcon,
  CircleCheck as CheckmarkCircle02Icon,
  FileText as File02Icon,
  RefreshCw as RefreshIcon,
} from 'lucide-react'
import { toast } from '@/lib/toast'

import type {
  ConcurrencyPolicy,
  CreatePipelineRequest,
  RepositoryWorkflowPreview,
  TriggerConfig,
  UpdatePipelineAndroidSigningRequest,
  UpdatePipelineIosSigningRequest,
} from '@/lib/types'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { requireProjectPermissionOrRedirect } from '@/lib/project-route-guard'
import {
  useCreatePipeline,
  useRepositoryWorkflows,
  useUpdatePipelineAndroidSigning,
  useUpdatePipelineIosSigning,
  useValidatePipeline,
} from '@/hooks/use-pipelines'
import { useProject } from '@/hooks/use-projects'
import {
  executionConfigFromForm,
  fileToBase64,
  fileToUtf8,
  parseBundleIdsInput,
  parseCsv,
  selectedPlatforms,
  trimToUndefined,
} from '@/lib/pipeline-form-utils'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import PipelineForm from '@/components/pipeline-form'
import { PageMeta } from '@/lib/seo'
import { cn } from '@/lib/utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Spinner } from '@/components/ui/spinner'

export const Route = createFileRoute('/projects/$projectId/pipelines/new')({
  staticData: {
    breadcrumb: {
      title: 'New Pipeline',
    },
  },
  beforeLoad: async ({ params }) => {
    const instance = getActiveInstanceOrRedirect()
    const token = requireAuthOrRedirect(instance.id)
    await requireProjectPermissionOrRedirect({
      action: 'write',
      instance,
      projectId: params.projectId,
      resource: 'pipelines',
      token,
    })
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
  ios_signing_enabled: false,
  ios_signing_mode: 'manual',
  ios_signing_team_id: '',
  ios_signing_bundle_ids: '',
  ios_signing_p12_password: '',
  ios_signing_api_key_id: '',
  ios_signing_api_issuer_id: '',
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
  artifact_patterns: 'build/app/outputs/flutter-apk/*.apk',
  branches: '',
  max_concurrent: undefined,
}

const PIPELINE_TEMPLATES = [
  {
    key: 'debug-apk',
    label: 'Quick Debug APK',
    description: 'Android debug build, no signing. Fastest way to test.',
    values: {
      ...emptyDefaults,
      name: 'Debug APK',
      platform_android: true,
      platform_ios: false,
      platform_macos: false,
      enable_customization: true,
      android_command_override: 'flutter build apk --debug',
      artifact_patterns: 'build/app/outputs/flutter-apk/*.apk',
    } satisfies PipelineFormValues,
    events: ['push'],
  },
  {
    key: 'release-android',
    label: 'Release Android',
    description: 'Android release build with signing enabled.',
    values: {
      ...emptyDefaults,
      name: 'Release Android',
      platform_android: true,
      platform_ios: false,
      platform_macos: false,
      android_signing_release_enabled: true,
      artifact_patterns:
        'build/app/outputs/flutter-apk/*.apk\nbuild/app/outputs/bundle/release/*.aab',
    } satisfies PipelineFormValues,
    events: ['push', 'tag_push'],
  },
  {
    key: 'ios-android',
    label: 'iOS + Android',
    description: 'Both mobile platforms. Configure signing after creation.',
    values: {
      ...emptyDefaults,
      name: 'Mobile Release',
      platform_android: true,
      platform_ios: true,
      platform_macos: false,
      artifact_patterns:
        'build/app/outputs/flutter-apk/*.apk\nbuild/app/outputs/bundle/release/*.aab\nbuild/ios/ipa/*.ipa',
    } satisfies PipelineFormValues,
    events: ['push', 'tag_push'],
  },
  {
    key: 'full-stack',
    label: 'All Platforms',
    description: 'Android, iOS, and macOS. Full Flutter build matrix.',
    values: {
      ...emptyDefaults,
      name: 'Full Build',
      platform_android: true,
      platform_ios: true,
      platform_macos: true,
      artifact_patterns:
        'build/app/outputs/flutter-apk/*.apk\nbuild/app/outputs/bundle/release/*.aab\nbuild/ios/ipa/*.ipa\nbuild/macos/Build/Products/Release/*.app',
    } satisfies PipelineFormValues,
    events: ['push', 'tag_push'],
  },
  {
    key: 'custom',
    label: 'Custom',
    description: 'Start from scratch with full control.',
    values: emptyDefaults,
    events: ['push'],
  },
] as const

function workflowDefaults(
  workflow: RepositoryWorkflowPreview,
): PipelineFormValues {
  const execution = workflow.execution
  const workflowName =
    workflow.path
      .split('/')
      .at(-1)
      ?.replace(/^\.oore\.?/, '')
      .replace(/\.ya?ml$/, '')
      .replaceAll('-', ' ')
      .trim() || 'Repository workflow'

  return {
    ...emptyDefaults,
    name:
      workflow.path === '.oore.yaml' || workflow.path === '.oore.yml'
        ? 'Repository workflow'
        : workflowName.replace(/\b\w/g, (letter) => letter.toUpperCase()),
    config_mode: 'explicit',
    config_path: workflow.path,
    platform_android: execution?.platforms.includes('android') ?? false,
    platform_ios: execution?.platforms.includes('ios') ?? false,
    platform_macos: execution?.platforms.includes('macos') ?? false,
    flutter_version: execution?.flutter_version ?? '',
    enable_customization: true,
    pre_build_commands: execution?.commands.pre_build.join('\n') ?? '',
    build_commands: execution?.commands.build.join('\n') ?? '',
    post_build_commands: execution?.commands.post_build.join('\n') ?? '',
    android_build_args: execution?.platform_build_args.android.join('\n') ?? '',
    ios_build_args: execution?.platform_build_args.ios.join('\n') ?? '',
    macos_build_args: execution?.platform_build_args.macos.join('\n') ?? '',
    android_command_override: execution?.platform_commands.android ?? '',
    ios_command_override: execution?.platform_commands.ios ?? '',
    macos_command_override: execution?.platform_commands.macos ?? '',
    artifact_patterns: execution?.artifact_patterns.join('\n') ?? '',
  }
}

function RepositoryWorkflowSummary({
  workflow,
  reference,
}: {
  workflow: RepositoryWorkflowPreview
  reference: string
}) {
  const execution = workflow.execution
  if (!execution) return null
  const commandCount =
    execution.commands.pre_build.length +
    execution.commands.build.length +
    execution.commands.post_build.length +
    Object.values(execution.platform_commands).filter(Boolean).length

  return (
    <div className="space-y-4">
      <Alert>
        <DynamicLucideIcon icon={CheckmarkCircle02Icon} size={16} />
        <AlertDescription>
          Oore will read <code>{workflow.path}</code> from the exact commit
          being built. This preview is from <code>{reference}</code>.
        </AlertDescription>
      </Alert>
      <div className="grid gap-3 text-sm sm:grid-cols-2">
        <div>
          <p className="text-xs text-muted-foreground">Platforms</p>
          <p>{execution.platforms.join(', ')}</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Flutter</p>
          <p>
            {execution.flutter_version ??
              'Detected from .fvmrc or Oore-managed stable'}
          </p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Commands</p>
          <p>
            {commandCount > 0
              ? `${commandCount} configured`
              : 'Platform defaults'}
          </p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Artifacts</p>
          <p className="font-mono text-xs">
            {execution.artifact_patterns.join(', ') || 'Platform defaults'}
          </p>
        </div>
      </div>
      {execution.env_keys.length > 0 ? (
        <p className="text-xs text-muted-foreground">
          Environment keys: {execution.env_keys.join(', ')}. Values stay hidden.
        </p>
      ) : null}
    </div>
  )
}

function NewPipelinePage() {
  const { projectId } = Route.useParams()
  const navigate = useNavigate()
  const { data: projectData } = useProject(projectId)
  const workflowsQuery = useRepositoryWorkflows(projectId, undefined, {
    enabled: !!projectData?.project.repository_id,
  })
  const manualOnlyTriggers =
    projectData?.project.repository_provider === 'local_git'
  const createMutation = useCreatePipeline()
  const validateMutation = useValidatePipeline()
  const updateSigningMutation = useUpdatePipelineAndroidSigning()
  const updateIosSigningMutation = useUpdatePipelineIosSigning()
  const [validationErrors, setValidationErrors] = useState<Array<string>>([])
  const createdPipelineId = useRef<string | null>(null)
  const [selectedTemplate, setSelectedTemplate] = useState<string>('custom')
  const [selectedWorkflowPath, setSelectedWorkflowPath] = useState<
    string | null
  >(null)
  const [manualSetup, setManualSetup] = useState(false)
  const { validWorkflows, invalidWorkflows } = (
    workflowsQuery.data?.workflows ?? []
  ).reduce<{
    validWorkflows: Array<RepositoryWorkflowPreview>
    invalidWorkflows: Array<RepositoryWorkflowPreview>
  }>(
    (groups, workflow) => {
      groups[workflow.valid ? 'validWorkflows' : 'invalidWorkflows'].push(
        workflow,
      )
      return groups
    },
    { validWorkflows: [], invalidWorkflows: [] },
  )
  const selectedWorkflow: RepositoryWorkflowPreview | undefined =
    validWorkflows.find((workflow) => workflow.path === selectedWorkflowPath) ??
    validWorkflows.at(0)
  const activeTemplate =
    !manualSetup && selectedWorkflow !== undefined
      ? {
          key: `repository:${selectedWorkflow.path}`,
          label: selectedWorkflow.path,
          description: 'Repository-owned workflow',
          values: workflowDefaults(selectedWorkflow),
          events: ['push'],
        }
      : (PIPELINE_TEMPLATES.find((t) => t.key === selectedTemplate) ??
        PIPELINE_TEMPLATES[PIPELINE_TEMPLATES.length - 1])

  async function handleSubmit(
    data: PipelineFormValues,
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
    if (createdPipelineId.current) return
    const platforms = selectedPlatforms(data)
    if (platforms.length === 0) {
      setValidationErrors(['Pick at least one platform to build'])
      return
    }

    const trigger_config: TriggerConfig = manualOnlyTriggers
      ? { events: [], branches: [] }
      : {
          events,
          branches: parseCsv(data.branches),
        }

    const concurrency: ConcurrencyPolicy = {
      cancel_previous: cancelPrevious,
      max_concurrent: data.max_concurrent
        ? Number(data.max_concurrent)
        : undefined,
    }

    const payload: CreatePipelineRequest = {
      name: data.name.trim(),
      config_path:
        data.config_mode === 'explicit'
          ? data.config_path?.trim()
          : '.oore.yaml',
      config_path_explicit: data.config_mode === 'explicit',
      execution_config: executionConfigFromForm(data),
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

    const [signingPayload, iosSigningPayload] = await Promise.all([
      buildAndroidSigningPayload(
        { ...data },
        releaseKeystoreFile,
        debugKeystoreFile,
      ),
      buildIosSigningPayload({ ...data }, iosSigningFiles),
    ])
    if (
      (data.android_signing_release_enabled ||
        data.android_signing_debug_enabled) &&
      !signingPayload
    ) {
      return
    }
    if (data.platform_ios && data.ios_signing_enabled && !iosSigningPayload) {
      return
    }

    let created: { pipeline: { id: string } }
    try {
      created = await createMutation.mutateAsync({
        projectId,
        data: payload,
      })
      createdPipelineId.current = created.pipeline.id
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(`Failed to create pipeline: ${message}`)
      return
    }

    try {
      if (signingPayload) {
        await updateSigningMutation.mutateAsync({
          pipelineId: created.pipeline.id,
          data: signingPayload,
        })
      }
      if (iosSigningPayload) {
        await updateIosSigningMutation.mutateAsync({
          pipelineId: created.pipeline.id,
          data: iosSigningPayload,
        })
      }
      toast.success('Pipeline created')
      await navigate({ to: '/projects/$projectId', params: { projectId } })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      const signing = signingPayload ? 'android' : 'ios'
      toast.error(
        `Pipeline was created, but ${signing} signing failed: ${message}`,
      )
      await navigate({
        to: '/projects/$projectId/pipelines/$pipelineId/edit',
        params: { projectId, pipelineId: created.pipeline.id },
        search: { signing, signingError: message },
      })
    }
  }

  async function buildAndroidSigningPayload(
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

    if (!data.platform_android) return null
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

  async function buildIosSigningPayload(
    data: PipelineFormValues,
    iosSigningFiles: {
      p12File: File | null
      apiKeyFile: File | null
      profileFiles: Record<string, File | null>
    },
  ): Promise<UpdatePipelineIosSigningRequest | null> {
    const bundleIds = parseBundleIdsInput(data.ios_signing_bundle_ids)
    const teamId = trimToUndefined(data.ios_signing_team_id)
    const p12Password = trimToUndefined(data.ios_signing_p12_password)
    const apiKeyId = trimToUndefined(data.ios_signing_api_key_id)
    const apiIssuerId = trimToUndefined(data.ios_signing_api_issuer_id)

    const anyIosInput =
      data.ios_signing_enabled ||
      bundleIds.length > 0 ||
      !!teamId ||
      !!iosSigningFiles.p12File ||
      !!p12Password ||
      !!apiKeyId ||
      !!apiIssuerId ||
      !!iosSigningFiles.apiKeyFile ||
      Object.values(iosSigningFiles.profileFiles).some(Boolean)

    if (!data.platform_ios) return null
    if (!anyIosInput) return null

    const errors: Array<string> = []
    if (data.ios_signing_enabled) {
      if (!teamId) errors.push('iOS signing requires Team ID')
      if (bundleIds.length === 0)
        errors.push('iOS signing requires at least one bundle identifier')
    }

    if (
      data.ios_signing_enabled &&
      (data.ios_signing_mode === 'manual' || data.ios_signing_mode === 'hybrid')
    ) {
      if (!iosSigningFiles.p12File)
        errors.push(
          'Manual/Hybrid iOS signing requires a .p12 certificate file',
        )
      if (!p12Password)
        errors.push('Manual/Hybrid iOS signing requires p12 password')
      if (
        bundleIds.some((bundleId) => !iosSigningFiles.profileFiles[bundleId])
      ) {
        errors.push(
          'Manual/Hybrid iOS signing requires provisioning profile files for all bundle IDs',
        )
      }
    }

    if (
      data.ios_signing_enabled &&
      (data.ios_signing_mode === 'api' || data.ios_signing_mode === 'hybrid')
    ) {
      if (!apiKeyId) errors.push('API/Hybrid iOS signing requires API key ID')
      if (!apiIssuerId)
        errors.push('API/Hybrid iOS signing requires API issuer ID')
      if (!iosSigningFiles.apiKeyFile)
        errors.push('API/Hybrid iOS signing requires .p8 private key file')
    }

    if (errors.length > 0) {
      setValidationErrors(errors)
      return null
    }

    const [provisioningProfiles, apiPrivateKey] = await Promise.all([
      Promise.all(
        bundleIds.flatMap((bundleId) => {
          const profileFile = iosSigningFiles.profileFiles[bundleId]
          return profileFile
            ? [
                fileToBase64(profileFile).then((profile_base64) => ({
                  bundle_id: bundleId,
                  profile_filename: profileFile.name,
                  profile_base64,
                })),
              ]
            : []
        }),
      ),
      iosSigningFiles.apiKeyFile
        ? fileToUtf8(iosSigningFiles.apiKeyFile)
        : Promise.resolve(undefined),
    ])

    return {
      enabled: data.ios_signing_enabled,
      mode: data.ios_signing_mode,
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
              key_id: apiKeyId,
              issuer_id: apiIssuerId,
              private_key_base64: apiPrivateKey
                ? btoa(apiPrivateKey)
                : undefined,
            }
          : undefined,
    }
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="New Pipeline" />
      <PageHeader
        title="Set up a build"
        description="Use the workflow already in your repository, or start with a guided template."
      />
      <div className="mx-auto mb-6 max-w-4xl">
        {workflowsQuery.isLoading ? (
          <Card>
            <CardContent className="flex items-center gap-3 py-6 text-sm text-muted-foreground">
              <Spinner className="size-4" />
              Looking for Oore workflows on{' '}
              {projectData?.project.default_branch ?? 'the default branch'}...
            </CardContent>
          </Card>
        ) : workflowsQuery.error ? (
          <Alert variant="destructive">
            <DynamicLucideIcon icon={AlertCircleIcon} size={16} />
            <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
              <span>
                Oore could not inspect this repository. Nothing has been
                changed. {workflowsQuery.error.message}
              </span>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => void workflowsQuery.refetch()}
              >
                <DynamicLucideIcon icon={RefreshIcon} />
                Retry
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setManualSetup(true)}
              >
                Continue manually
              </Button>
            </AlertDescription>
          </Alert>
        ) : validWorkflows.length > 0 && !manualSetup ? (
          <Card>
            <CardHeader className="gap-2">
              <div className="flex items-center justify-between gap-3">
                <CardTitle className="flex items-center gap-2 text-base">
                  <DynamicLucideIcon icon={File02Icon} size={18} />
                  Repository workflow found
                </CardTitle>
                <Badge variant="secondary">
                  {workflowsQuery.data?.reference}
                </Badge>
              </div>
              <p className="text-sm text-muted-foreground">
                This is the reproducible setup checked into your repository, so
                Oore recommends using it.
              </p>
            </CardHeader>
            <CardContent className="space-y-4">
              {validWorkflows.length > 1 ? (
                <div className="grid gap-2 sm:grid-cols-2">
                  {validWorkflows.map((workflow) => (
                    <Button
                      key={workflow.path}
                      type="button"
                      variant={
                        (selectedWorkflowPath ?? validWorkflows.at(0)?.path) ===
                        workflow.path
                          ? 'secondary'
                          : 'outline'
                      }
                      className="justify-start font-mono"
                      onClick={() => setSelectedWorkflowPath(workflow.path)}
                    >
                      <DynamicLucideIcon icon={File02Icon} />
                      {workflow.path}
                    </Button>
                  ))}
                </div>
              ) : null}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => setManualSetup(true)}
              >
                Set up manually instead
              </Button>
            </CardContent>
          </Card>
        ) : (
          <div className="space-y-3">
            {invalidWorkflows.length > 0 ? (
              <Alert variant="destructive">
                <DynamicLucideIcon icon={AlertCircleIcon} size={16} />
                <AlertDescription>
                  Oore found repository workflow files, but they need attention:
                  <ul className="mt-2 list-disc pl-5">
                    {invalidWorkflows.map((workflow) => (
                      <li key={workflow.path}>
                        <code>{workflow.path}</code>:{' '}
                        {workflow.errors.join('; ')}
                      </li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            ) : !manualSetup ? (
              <Alert>
                <DynamicLucideIcon icon={File02Icon} size={16} />
                <AlertDescription>
                  No Oore workflow was found on{' '}
                  <code>{workflowsQuery.data?.reference}</code>. Choose a
                  starter below; you can move the configuration into{' '}
                  <code>.oore.yaml</code> later.
                </AlertDescription>
              </Alert>
            ) : null}
            <div className="flex items-center justify-between gap-3">
              <p className="text-sm font-medium">Choose a starting point</p>
              {manualSetup && validWorkflows.length > 0 ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setManualSetup(false)}
                >
                  Use repository workflow
                </Button>
              ) : null}
            </div>
            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
              {PIPELINE_TEMPLATES.map((tmpl) => (
                <button
                  key={tmpl.key}
                  type="button"
                  aria-pressed={selectedTemplate === tmpl.key}
                  onClick={() => setSelectedTemplate(tmpl.key)}
                  className={cn(
                    'flex flex-col items-start gap-1 border p-3 text-left text-sm transition-colors hover:bg-accent',
                    selectedTemplate === tmpl.key && 'border-primary bg-accent',
                  )}
                >
                  <span className="font-medium">{tmpl.label}</span>
                  <span className="text-xs text-muted-foreground">
                    {tmpl.description}
                  </span>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
      {(!workflowsQuery.isLoading || manualSetup) &&
      (!workflowsQuery.error || manualSetup) ? (
        <div className="mx-auto max-w-4xl">
          <PipelineForm
            key={activeTemplate.key}
            initialValues={activeTemplate.values}
            initialEvents={manualOnlyTriggers ? [] : [...activeTemplate.events]}
            initialCancelPrevious={true}
            manualOnlyTriggers={manualOnlyTriggers}
            onSubmit={handleSubmit}
            onCancel={() =>
              void navigate({
                to: '/projects/$projectId',
                params: { projectId },
              })
            }
            submitLabel="Create"
            isPending={
              createMutation.isPending ||
              updateSigningMutation.isPending ||
              updateIosSigningMutation.isPending
            }
            validationErrors={validationErrors}
            repositoryWorkflow={
              !manualSetup &&
              selectedWorkflow !== undefined &&
              workflowsQuery.data !== undefined ? (
                <RepositoryWorkflowSummary
                  workflow={selectedWorkflow}
                  reference={workflowsQuery.data.reference}
                />
              ) : undefined
            }
          />
        </div>
      ) : null}
    </PageLayout>
  )
}
