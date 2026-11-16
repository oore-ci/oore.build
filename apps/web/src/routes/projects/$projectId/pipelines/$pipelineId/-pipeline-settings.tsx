import { lazy, Suspense, useState, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon, InformationCircleIcon } from '@hugeicons/core-free-icons'
import type {
  Pipeline,
  PipelineAndroidSigningResponse,
  PipelineIosSigningResponse,
  PipelineWorkflowsResponse,
  RegisteredIosDevice,
  RepositoryWorkflowExecutionPreview,
  UpdatePipelineRequest,
} from '@oore/client/models'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import type { IosSigningFiles } from '@/lib/pipeline-signing'
import {
  androidIdentityValue,
  buildAndroidSigningPayload,
  buildIosSigningPayload,
  iosIdentityValue,
  iosProfilesNeedResync,
} from '@/lib/pipeline-signing'
import {
  buildNumberOffsetField,
  hasCustomFallback,
  pipelineUpdateFromForm,
  runsOnField,
  selectedPlatforms,
  sizeBudgetFormFields,
  toMultiline,
} from '@/lib/pipeline-form-utils'
import {
  usesPipeline,
  type PipelineFile,
  type SourcedValue,
} from '@/lib/pipeline-sources'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import { toast } from '@/lib/toast'
import {
  usePipelineIosDevices,
  useRegisterPipelineIosDevice,
  useRepositoryWorkflows,
  useSyncPipelineIosSigning,
  useUpdatePipeline,
  useUpdatePipelineAndroidSigning,
  useUpdatePipelineIosSigning,
  usePipelines,
} from '@/hooks/use-pipelines'
import {
  usePipelineAppStorePublish,
  usePipelineGooglePlayPublish,
  usePipelinePublishReadiness,
} from '@/hooks/use-pipeline-publish'
import PipelineForm, { type PipelineFormPart } from '@/components/pipeline-form'
import { PipelineFormSubsection } from '@/components/pipeline-form-section'
import {
  SourcedList,
  sourcedRows,
  RepositoryFileLink,
} from '@/components/pipeline-page/source-label'
import { StoreReadinessChecklist } from '@/components/trigger-build-store-section'
import { CollectionError } from '@/components/collection'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { SettingsSurfaceSeparator } from '@/components/settings/settings-section'
import { Mono } from '@/components/layout/mono'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

const GooglePlayPublishSection = lazy(() =>
  import('@/components/publish/google-play-publish-section').then((module) => ({
    default: module.GooglePlayPublishSection,
  })),
)
const AppStorePublishSection = lazy(() =>
  import('@/components/publish/app-store-publish-section').then((module) => ({
    default: module.AppStorePublishSection,
  })),
)
const SecretsPanel = lazy(() =>
  import('@/components/secrets/secrets-panel').then((module) => ({
    default: module.SecretsPanel,
  })),
)
const PipelineWorkflowsPanel = lazy(() =>
  import('@/components/pipeline-workflows/pipeline-workflows-panel').then(
    (module) => ({ default: module.PipelineWorkflowsPanel }),
  ),
)

/** What every row needs to know about the pipeline. */
export interface PipelineRowContext {
  pipeline: Pipeline
  projectId: string
  canWrite: boolean
  canChangeApproval: boolean
  manualOnlyTriggers: boolean
  /** The pipeline's file when the daemon read it on the default branch. */
  file: PipelineFile | null
  listing?: PipelineWorkflowsResponse
  /** The project's default branch, for the Workflow lookup. */
  defaultBranch?: string | null
  hasRepository: boolean
  androidSigning?: PipelineAndroidSigningResponse
  iosSigning?: PipelineIosSigningResponse
}

/** The form's values for a saved pipeline and its signing. */
export function pipelineFormValues(
  pipeline: Pipeline,
  android: PipelineAndroidSigningResponse | undefined,
  ios: PipelineIosSigningResponse | undefined,
  manualOnlyTriggers: boolean,
): PipelineFormValues {
  const platformSet = new Set(pipeline.execution_config.platforms)
  return {
    name: pipeline.name,
    config_mode:
      pipeline.config_path.trim() === '.oore.yaml' ? 'auto' : 'explicit',
    config_path: pipeline.config_path,
    workflow: pipeline.workflow ?? '',
    platform_android: platformSet.has('android'),
    platform_ios: platformSet.has('ios'),
    platform_macos: platformSet.has('macos'),
    android_signing_release_enabled: android?.release.enabled ?? false,
    android_signing_release_store_password: '',
    android_signing_release_key_alias: android?.release.key_alias ?? '',
    android_signing_release_key_password: '',
    android_signing_debug_enabled: android?.debug.enabled ?? false,
    android_signing_debug_store_password: '',
    android_signing_debug_key_alias: android?.debug.key_alias ?? '',
    android_signing_debug_key_password: '',
    android_signing_release_identity_id: androidIdentityValue(android?.release),
    android_signing_debug_identity_id: androidIdentityValue(android?.debug),
    ios_signing_enabled: ios?.enabled ?? false,
    ios_signing_identity_id: iosIdentityValue(ios),
    ios_signing_mode: ios?.mode ?? 'manual',
    ios_signing_export_method: ios?.export_method ?? 'ad_hoc',
    ios_signing_team_id: ios?.team_id ?? '',
    ios_signing_bundle_ids: (ios?.bundle_ids ?? []).join('\n'),
    ios_signing_p12_password: '',
    ios_signing_api_key_id: ios?.api_key_id ?? '',
    ios_signing_api_issuer_id: ios?.api_issuer_id ?? '',
    ios_signing_api_key_source:
      ios?.shared_api_key?.id ?? (ios?.has_api_key ? 'own' : ''),
    flutter_version: pipeline.execution_config.flutter_version ?? '',
    build_number_offset: buildNumberOffsetField(pipeline.execution_config),
    runs_on: runsOnField(pipeline.execution_config),
    enable_customization: hasCustomFallback(pipeline),
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
    isolation: pipeline.execution_config.isolation ?? 'host',
    timeout_minutes: String(pipeline.timeout_minutes),
    trigger_events: manualOnlyTriggers ? [] : pipeline.trigger_config.events,
    tags: manualOnlyTriggers
      ? ''
      : (pipeline.trigger_config.tags ?? []).join(', '),
    paths: manualOnlyTriggers
      ? ''
      : toMultiline(pipeline.trigger_config.paths ?? []),
    paths_ignore: manualOnlyTriggers
      ? ''
      : toMultiline(pipeline.trigger_config.paths_ignore ?? []),
    schedules: (pipeline.trigger_config.schedules ?? []).map((schedule) => ({
      cron: schedule.cron,
      branch: schedule.branch ?? '',
    })),
    cancel_previous: pipeline.concurrency.cancel_previous,
    branches: manualOnlyTriggers
      ? ''
      : pipeline.trigger_config.branches.join(', '),
    max_concurrent: pipeline.concurrency.max_concurrent
      ? String(pipeline.concurrency.max_concurrent)
      : undefined,
    approval_required: pipeline.approval.required,
    approval_events: pipeline.approval.events ?? [],
    ...sizeBudgetFormFields(pipeline.size_budget),
  }
}

/**
 * A row's form: only its parts, Save and Cancel once something changes.
 * It starts again from the saved pipeline after every save.
 */
function RowForm({
  children,
  context,
  isPending,
  onSubmit,
  parts,
  title,
  ...props
}: {
  children?: ReactNode
  context: PipelineRowContext
  isPending: boolean
  onSubmit: (
    values: PipelineFormValues,
    iosFiles: IosSigningFiles,
  ) => Promise<void>
  parts: ReadonlyArray<PipelineFormPart>
  title: string
} & Pick<
  React.ComponentProps<typeof PipelineForm>,
  | 'fileTriggers'
  | 'signingReadOnly'
  | 'signingNotes'
  | 'iosSigningActions'
  | 'iosAppStorePublishing'
  | 'googlePlayUpload'
  | 'publishSettings'
  | 'otherPipelines'
  | 'validationErrors'
  | 'initialValues'
>) {
  const { pipeline } = context
  return (
    <PipelineForm
      key={`${pipeline.updated_at}-${context.androidSigning?.release.updated_at ?? ''}-${context.iosSigning?.updated_at ?? ''}`}
      parts={parts}
      embeddedIn={title}
      markSetHere={!!context.file}
      manualOnlyTriggers={context.manualOnlyTriggers}
      canChangeApproval={context.canChangeApproval}
      onSubmit={(values, _release, _debug, iosFiles) =>
        onSubmit(values, iosFiles)
      }
      onCancel={() => undefined}
      submitLabel="Save changes"
      isPending={isPending}
      signingData={context.androidSigning}
      iosSigningData={context.iosSigning}
      workflowLookup={
        context.hasRepository
          ? {
              projectId: context.projectId,
              defaultBranch: context.defaultBranch,
            }
          : undefined
      }
      {...props}
    >
      {children}
    </PipelineForm>
  )
}

/** Values the form shows read-only when the user cannot change them. */
function ReadOnlyNote({ children }: { children: ReactNode }) {
  return (
    <Text variant="muted" className="text-[13px]">
      {children}
    </Text>
  )
}

/* ------------------------------------------------------------------ */
/*  Source and triggers                                                */
/* ------------------------------------------------------------------ */

export function SourceRowContent({
  approval,
  context,
  showWorkflows,
  triggers,
}: {
  approval: Array<SourcedValue>
  context: PipelineRowContext
  showWorkflows: boolean
  triggers: Array<SourcedValue>
}) {
  const { pipeline } = context
  const updateMutation = useUpdatePipeline()
  const fileDecidesTriggers = triggers.length > 0 && !usesPipeline(triggers)
  const fileTriggers = triggers.filter((entry) => entry.source.kind !== 'here')
  const approvalOverrides = approval.filter(
    (entry) => entry.source.kind === 'workflow',
  )

  async function save(values: PipelineFormValues) {
    const full = pipelineUpdateFromForm(values, pipeline.workflow, {
      canChangeApproval: context.canChangeApproval,
      manualOnlyTriggers: context.manualOnlyTriggers,
    })
    // Only this row's fields, so a stale value elsewhere is never sent.
    const data: UpdatePipelineRequest = {
      name: full.name,
      config_path: full.config_path,
      trigger_config: full.trigger_config,
      concurrency: full.concurrency,
      timeout_minutes: full.timeout_minutes,
      approval: full.approval,
    }
    if ('workflow' in full) data.workflow = full.workflow
    try {
      await updateMutation.mutateAsync({ pipelineId: pipeline.id, data })
      toast.success('Pipeline updated')
    } catch (err) {
      toast.error(
        `Failed to update pipeline: ${err instanceof Error ? err.message : 'Unknown error'}`,
      )
    }
  }

  return (
    <Stack gap={6}>
      {showWorkflows ? (
        <Suspense fallback={<Skeleton className="h-40 w-full" />}>
          <PipelineWorkflowsPanel
            canChangeApproval={context.canChangeApproval}
            canWrite={context.canWrite}
            pipelineId={pipeline.id}
            projectId={context.projectId}
          />
        </Suspense>
      ) : null}
      {!fileDecidesTriggers && fileTriggers.length > 0 ? (
        <SourcedList rows={sourcedRows('Triggers', triggers)} />
      ) : null}
      {approvalOverrides.length > 0 ? (
        <SourcedList rows={sourcedRows('Approval', approvalOverrides)} />
      ) : null}
      {showWorkflows ||
      (!fileDecidesTriggers && fileTriggers.length > 0) ||
      approvalOverrides.length > 0 ? (
        <SettingsSurfaceSeparator />
      ) : null}
      {context.canWrite ? (
        <RowForm
          context={context}
          initialValues={pipelineFormValues(
            pipeline,
            undefined,
            undefined,
            context.manualOnlyTriggers,
          )}
          isPending={updateMutation.isPending}
          onSubmit={save}
          parts={['source', 'triggers']}
          title="Source and triggers"
          fileTriggers={
            fileDecidesTriggers ? (
              <SourcedList rows={sourcedRows('Starts on', triggers)} />
            ) : undefined
          }
        />
      ) : (
        <ReadOnlyNote>
          Only people who can edit this pipeline can change its file and
          triggers.
        </ReadOnlyNote>
      )}
    </Stack>
  )
}

/* ------------------------------------------------------------------ */
/*  Build                                                               */
/* ------------------------------------------------------------------ */

const ISOLATION_LABELS = {
  host: 'On the runner',
  vm: 'In a fresh macOS VM',
} as const

function Lines({ lines }: { lines: Array<string> }) {
  return (
    <Stack gap={0.5}>
      {lines.map((line, index) => (
        <Mono key={`${index}-${line}`} className="text-[13px] wrap-break-word">
          {line}
        </Mono>
      ))}
    </Stack>
  )
}

/** The build the file decides, read-only, each value from the file. */
function FileBuildValues({
  execution,
  file,
  platforms,
}: {
  execution?: RepositoryWorkflowExecutionPreview | null
  file: PipelineFile
  platforms: Array<string>
}) {
  const source = { kind: 'file' as const, path: file.path }
  const row = (key: string, label: string, value: ReactNode) => ({
    key,
    label,
    value,
    source,
  })
  const rows = [row('platforms', 'Platforms', platforms.join(', ') || '—')]
  if (execution) {
    rows.push(
      row(
        'flutter',
        'Flutter',
        execution.flutter_version ? (
          <Mono>{execution.flutter_version}</Mono>
        ) : (
          'From .fvmrc, else stable'
        ),
      ),
    )
    const { commands } = execution
    if (commands.pre_build.length > 0)
      rows.push(
        row('pre', 'Before build', <Lines lines={commands.pre_build} />),
      )
    const platformCommands = (['android', 'ios', 'macos'] as const).flatMap(
      (platform) => {
        const command = execution.platform_commands[platform]
        return command ? [command] : []
      },
    )
    rows.push(
      row(
        'build',
        'Build',
        platformCommands.length + commands.build.length > 0 ? (
          <Lines lines={[...platformCommands, ...commands.build]} />
        ) : (
          'Default platform commands'
        ),
      ),
    )
    if (commands.post_build.length > 0)
      rows.push(
        row('post', 'After build', <Lines lines={commands.post_build} />),
      )
    const args = (['android', 'ios', 'macos'] as const).flatMap((platform) => {
      const list = execution.platform_build_args[platform] ?? []
      return list.length > 0
        ? [`${BUILD_PLATFORM_LABELS[platform]}: ${list.join(' ')}`]
        : []
    })
    if (args.length > 0)
      rows.push(row('args', 'Build arguments', <Lines lines={args} />))
    if (execution.env_keys.length > 0)
      rows.push(
        row(
          'env',
          'Environment variables',
          <Mono className="text-[13px]">{execution.env_keys.join(', ')}</Mono>,
        ),
      )
    rows.push(
      row(
        'artifacts',
        'Artifacts',
        <Lines lines={execution.artifact_patterns} />,
      ),
    )
    rows.push(row('isolation', 'Runs', ISOLATION_LABELS[execution.isolation]))
    if ((execution.runs_on ?? []).length > 0)
      rows.push(
        row(
          'runs-on',
          'Runner labels',
          <Mono className="text-[13px]">
            {(execution.runs_on ?? []).join(', ')}
          </Mono>,
        ),
      )
    if (execution.build_number_offset)
      rows.push(
        row(
          'offset',
          'Build number offset',
          <Mono>{execution.build_number_offset}</Mono>,
        ),
      )
  }
  return (
    <Stack gap={3}>
      <SourcedList rows={rows} />
      {execution ? null : (
        <ReadOnlyNote>
          The commands, environment variables and artifacts are in{' '}
          <RepositoryFileLink path={file.path} />.
        </ReadOnlyNote>
      )}
    </Stack>
  )
}

export function BuildRowContent({ context }: { context: PipelineRowContext }) {
  const { file, listing, pipeline } = context
  const updateMutation = useUpdatePipeline()
  const [validationErrors, setValidationErrors] = useState<Array<string>>([])
  const googlePlayQuery = usePipelineGooglePlayPublish(pipeline.id, {
    enabled: pipeline.execution_config.platforms.includes('android'),
  })
  const appStoreQuery = usePipelineAppStorePublish(pipeline.id, {
    enabled: pipeline.execution_config.platforms.includes('ios'),
  })
  const projectPipelinesQuery = usePipelines(context.projectId, { limit: 200 })
  const previewQuery = useRepositoryWorkflows(
    context.projectId,
    file ? { path: file.path } : undefined,
    { enabled: !!file && context.canWrite && context.hasRepository },
  )
  const execution = file
    ? previewQuery.data?.workflows.find(
        (candidate) => candidate.path === file.path,
      )?.execution
    : undefined
  const filePlatforms = file
    ? [
        ...new Set(
          (execution?.platforms ?? []).length > 0
            ? (execution?.platforms ?? [])
            : file.workflows.flatMap((workflow) => workflow.platforms),
        ),
      ].map((platform) => BUILD_PLATFORM_LABELS[platform])
    : []

  async function save(values: PipelineFormValues) {
    if (!file && selectedPlatforms(values).length === 0) {
      setValidationErrors(['Pick at least one platform to build'])
      return
    }
    setValidationErrors([])
    const full = pipelineUpdateFromForm(values, pipeline.workflow, {
      canChangeApproval: context.canChangeApproval,
      manualOnlyTriggers: context.manualOnlyTriggers,
    })
    const data: UpdatePipelineRequest = file
      ? { size_budget: full.size_budget }
      : {
          execution_config: full.execution_config,
          size_budget: full.size_budget,
        }
    try {
      await updateMutation.mutateAsync({ pipelineId: pipeline.id, data })
      toast.success('Pipeline updated')
    } catch (err) {
      toast.error(
        `Failed to update pipeline: ${err instanceof Error ? err.message : 'Unknown error'}`,
      )
    }
  }

  const form = context.canWrite ? (
    <RowForm
      context={context}
      initialValues={pipelineFormValues(
        pipeline,
        undefined,
        undefined,
        context.manualOnlyTriggers,
      )}
      isPending={updateMutation.isPending}
      onSubmit={save}
      parts={
        file
          ? ['size-budget']
          : ['build-basics', 'build', 'plan', 'size-budget']
      }
      title="Build"
      validationErrors={validationErrors}
      googlePlayUpload={!!googlePlayQuery.data?.config?.enabled}
      otherPipelines={(projectPipelinesQuery.data?.pipelines ?? []).filter(
        (other) => other.id !== pipeline.id,
      )}
      publishSettings={{
        googlePlay: googlePlayQuery.data?.config ?? undefined,
        appStore: appStoreQuery.data?.config,
      }}
    />
  ) : (
    <ReadOnlyNote>
      Only people who can edit this pipeline can change how it builds.
    </ReadOnlyNote>
  )

  if (file) {
    return (
      <Stack gap={6}>
        {previewQuery.isLoading ? (
          <Skeleton className="h-40 w-full" />
        ) : (
          <FileBuildValues
            execution={execution}
            file={file}
            platforms={filePlatforms}
          />
        )}
        <SettingsSurfaceSeparator />
        {form}
      </Stack>
    )
  }
  if (listing?.state === 'invalid') {
    return (
      <Stack gap={6}>
        <Alert variant="destructive">
          <HugeiconsIcon icon={Alert02Icon} size={16} />
          <AlertTitle>The pipeline file has errors</AlertTitle>
          <AlertDescription>
            <span>
              {listing.path ? <RepositoryFileLink path={listing.path} /> : null}
              {listing.path ? ': ' : null}
              {(listing.errors ?? []).join('; ') || 'Unknown error'}
            </span>
          </AlertDescription>
        </Alert>
        {form}
      </Stack>
    )
  }
  return (
    <Stack gap={4}>
      {listing?.state === 'unavailable' ? (
        <Alert role="status">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Oore could not read the repository, so it cannot tell whether a file
            decides the build. These settings apply when there is none.
          </AlertDescription>
        </Alert>
      ) : null}
      {form}
    </Stack>
  )
}

/* ------------------------------------------------------------------ */
/*  Signing                                                             */
/* ------------------------------------------------------------------ */

const iosDeviceColumns: Array<DataTableColumnDef<RegisteredIosDevice>> = [
  { accessorKey: 'name', header: 'Name', enableSorting: false },
  { accessorKey: 'udid', header: 'UDID', enableSorting: false },
  { accessorKey: 'status', header: 'Status', enableSorting: false },
]

function IosDeviceTable({ devices }: { devices: Array<RegisteredIosDevice> }) {
  const table = useDataTable({
    columns: iosDeviceColumns,
    data: devices,
    getRowId: (device) => device.id,
  })
  return <DataTable table={table} />
}

function describeProfileSync(updatedProfiles: number) {
  return `${updatedProfiles} provisioning profile${updatedProfiles === 1 ? '' : 's'} synced from App Store Connect`
}

/** Ad hoc builds install only on registered devices. */
function RegisteredIosDevices({ pipelineId }: { pipelineId: string }) {
  const devicesQuery = usePipelineIosDevices(pipelineId)
  const registerMutation = useRegisterPipelineIosDevice()
  const [name, setName] = useState('')
  const [udid, setUdid] = useState('')

  async function register() {
    if (!name.trim() || !udid.trim()) {
      toast.error('Device name and UDID are required')
      return
    }
    try {
      const response = await registerMutation.mutateAsync({
        pipelineId,
        data: { name: name.trim(), udid: udid.trim(), platform: 'IOS' },
      })
      setName('')
      setUdid('')
      toast.success(
        response.profile_sync_triggered
          ? 'Device registered and profiles synced'
          : 'Device registered',
      )
    } catch (err) {
      toast.error(
        `Failed to register device: ${err instanceof Error ? err.message : 'Unknown error'}`,
      )
    }
  }

  return (
    <PipelineFormSubsection
      title="Registered iOS devices"
      hint="Ad hoc builds install only on these devices. Registering one re-syncs the profiles."
    >
      <div className="grid gap-3 md:grid-cols-[1fr_1fr_auto]">
        <Input
          aria-label="Device name"
          placeholder="Device name"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <Input
          aria-label="Device UDID"
          placeholder="UDID"
          className="font-mono"
          value={udid}
          onChange={(event) => setUdid(event.target.value)}
        />
        <Button
          type="button"
          variant="outline"
          disabled={registerMutation.isPending}
          onClick={() => void register()}
        >
          {registerMutation.isPending ? 'Registering...' : 'Register device'}
        </Button>
      </div>
      {devicesQuery.isLoading ? (
        <Text variant="muted">Loading devices...</Text>
      ) : devicesQuery.isError ? (
        <CollectionError
          title="Failed to load registered devices"
          description={devicesQuery.error.message}
          onRetry={() => void devicesQuery.refetch()}
        />
      ) : devicesQuery.data?.devices.length ? (
        <IosDeviceTable devices={devicesQuery.data.devices} />
      ) : (
        <Text variant="muted">
          No iOS devices registered for this pipeline.
        </Text>
      )}
    </PipelineFormSubsection>
  )
}

/** "Not signed", the identity's name, or how App Store Connect signs. */
export function pipelineSigningValue(
  platform: 'android' | 'ios',
  android?: PipelineAndroidSigningResponse,
  ios?: PipelineIosSigningResponse,
): string {
  if (platform === 'android') {
    const release = android?.release
    if (!release?.enabled) return 'Not signed'
    return release.identity_name ?? release.keystore_filename ?? 'Keystore'
  }
  if (!ios?.enabled) return 'Not signed'
  if (ios.mode === 'manual') return ios.identity_name ?? 'Certificate'
  return ios.mode === 'api'
    ? 'App Store Connect (automatic)'
    : 'Certificate with synced profiles'
}

export function SigningRowContent({
  context,
  platforms,
  signing,
  signingError,
  onRetry,
}: {
  context: PipelineRowContext
  /** Platforms the pipeline or its file builds that are signed. */
  platforms: Array<'android' | 'ios'>
  signing: Partial<Record<'android' | 'ios', Array<SourcedValue>>>
  signingError?: Error | null
  onRetry: () => void
}) {
  const { pipeline, projectId } = context
  const updateAndroidMutation = useUpdatePipelineAndroidSigning()
  const updateIosMutation = useUpdatePipelineIosSigning()
  const syncMutation = useSyncPipelineIosSigning()
  const appStoreQuery = usePipelineAppStorePublish(pipeline.id, {
    enabled: platforms.includes('ios'),
  })
  const [validationErrors, setValidationErrors] = useState<Array<string>>([])

  if (!context.canWrite) {
    return (
      <ReadOnlyNote>
        Only people who can edit this pipeline can see and change its signing.
      </ReadOnlyNote>
    )
  }
  if (signingError) {
    return (
      <CollectionError
        title="Could not load signing settings"
        description={signingError.message}
        onRetry={onRetry}
      />
    )
  }
  if (!context.androidSigning || !context.iosSigning) {
    return <Skeleton className="h-40 w-full" />
  }
  const androidSigning = context.androidSigning
  const iosSigning = context.iosSigning

  async function syncProfiles() {
    try {
      const response = await syncMutation.mutateAsync(pipeline.id)
      const warnings =
        response.warnings.length > 0
          ? ` (${response.warnings.length} warning${response.warnings.length === 1 ? '' : 's'})`
          : ''
      toast.success(
        `iOS signing sync completed: ${describeProfileSync(response.updated_profiles)}${warnings}`,
      )
    } catch (err) {
      toast.error(
        `Failed to sync iOS signing: ${err instanceof Error ? err.message : 'Unknown error'}`,
      )
    }
  }

  async function save(values: PipelineFormValues, iosFiles: IosSigningFiles) {
    const [android, ios] = await Promise.all([
      buildAndroidSigningPayload(
        values,
        { release: null, debug: null },
        androidSigning,
      ),
      buildIosSigningPayload(values, iosFiles, iosSigning),
    ])
    const errors = [...android.errors, ...ios.errors]
    setValidationErrors(errors)
    if (errors.length > 0) return
    try {
      if (android.payload && values.platform_android) {
        await updateAndroidMutation.mutateAsync({
          pipelineId: pipeline.id,
          data: android.payload,
        })
      }
      let note = ''
      if (ios.payload && values.platform_ios) {
        await updateIosMutation.mutateAsync({
          pipelineId: pipeline.id,
          data: ios.payload,
        })
        // The daemon dropped the stored profiles with the distribution
        // change; regenerate them so the next build has one.
        if (iosProfilesNeedResync(ios.payload, iosSigning)) {
          try {
            const response = await syncMutation.mutateAsync(pipeline.id)
            note = `; ${describeProfileSync(response.updated_profiles)}`
          } catch (err) {
            toast.error(
              `Signing updated, but iOS profiles could not be synced: ${err instanceof Error ? err.message : 'Unknown error'}. Press Sync profiles before the next build.`,
            )
            return
          }
        }
      }
      toast.success(`Signing updated${note}`)
    } catch (err) {
      toast.error(
        `Failed to update signing: ${err instanceof Error ? err.message : 'Unknown error'}`,
      )
    }
  }

  // A platform whose identity something else decides for every build
  // shows what is in force instead of the pipeline's unused picker.
  const readOnly: Partial<Record<'android' | 'ios', ReactNode>> = {}
  const notes: Partial<Record<'android' | 'ios', ReactNode>> = {}
  for (const platform of platforms) {
    const entries = signing[platform] ?? []
    const label = BUILD_PLATFORM_LABELS[platform]
    if (entries.length > 0 && !usesPipeline(entries)) {
      readOnly[platform] = (
        <PipelineFormSubsection
          id={`${platform}-signing`}
          title={label}
          hint="The pipeline's own identity is not used while these are set."
        >
          <SourcedList rows={sourcedRows('Identity', entries)} />
        </PipelineFormSubsection>
      )
    } else {
      const elsewhere = entries.filter((entry) => entry.source.kind !== 'here')
      if (elsewhere.length > 0)
        notes[platform] = (
          <SourcedList rows={sourcedRows('Other workflows', elsewhere)} />
        )
    }
  }

  const initialValues = pipelineFormValues(
    pipeline,
    androidSigning,
    iosSigning,
    context.manualOnlyTriggers,
  )
  // The file can build platforms the pipeline's own settings do not list.
  initialValues.platform_android = platforms.includes('android')
  initialValues.platform_ios = platforms.includes('ios')

  const profileSyncAvailable =
    iosSigning.enabled &&
    (iosSigning.mode === 'api' || iosSigning.mode === 'hybrid')

  return (
    <RowForm
      context={context}
      initialValues={initialValues}
      isPending={
        updateAndroidMutation.isPending ||
        updateIosMutation.isPending ||
        syncMutation.isPending
      }
      onSubmit={save}
      parts={['signing']}
      title="Signing"
      validationErrors={validationErrors}
      signingReadOnly={readOnly}
      signingNotes={notes}
      iosSigningActions={
        profileSyncAvailable ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={syncMutation.isPending}
            onClick={() => void syncProfiles()}
          >
            {syncMutation.isPending ? 'Syncing...' : 'Sync profiles'}
          </Button>
        ) : undefined
      }
      iosAppStorePublishing={{
        configured: !!appStoreQuery.data?.config.enabled,
        link: (
          <Link
            to="/projects/$projectId/pipelines/$pipelineId"
            params={{ projectId, pipelineId: pipeline.id }}
            search={{ row: 'publishing' }}
            className="underline underline-offset-4"
          >
            Open publishing
          </Link>
        ),
      }}
    >
      {profileSyncAvailable && iosSigning.export_method === 'ad_hoc' ? (
        <>
          <SettingsSurfaceSeparator />
          <RegisteredIosDevices pipelineId={pipeline.id} />
        </>
      ) : null}
    </RowForm>
  )
}

/* ------------------------------------------------------------------ */
/*  Publishing                                                          */
/* ------------------------------------------------------------------ */

export function PublishingRowContent({
  androidSignedElsewhere,
  canReadInstanceSecrets,
  context,
  overrides,
  platforms,
}: {
  androidSignedElsewhere: boolean
  canReadInstanceSecrets: boolean
  context: PipelineRowContext
  /** Workflows whose settings change where their builds go. */
  overrides: Array<{ label: string; entries: Array<SourcedValue> }>
  platforms: Array<'android' | 'ios'>
}) {
  const { pipeline } = context
  const readiness = usePipelinePublishReadiness(pipeline.id)
  const overrideRows = overrides.flatMap(({ label, entries }) =>
    sourcedRows(label, entries),
  )
  return (
    <Suspense fallback={<Skeleton className="h-96 w-full" />}>
      <Stack gap={8}>
        {readiness.data?.items.length ? (
          <StoreReadinessChecklist
            readiness={readiness}
            title="Ready to submit to the stores?"
          />
        ) : null}
        {overrideRows.length > 0 ? <SourcedList rows={overrideRows} /> : null}
        {platforms.includes('android') ? (
          <GooglePlayPublishSection
            androidCommandOverride={
              pipeline.execution_config.platform_commands?.android
            }
            // The file or workflow settings sign Android for every build, so
            // the pipeline's own unset signing does not block uploads.
            androidSigning={
              androidSignedElsewhere ? undefined : context.androidSigning
            }
            canReadInstanceSecrets={canReadInstanceSecrets}
            canWrite={context.canWrite}
            pipelineId={pipeline.id}
            projectId={context.projectId}
          />
        ) : null}
        {platforms.includes('ios') ? (
          <AppStorePublishSection
            canWrite={context.canWrite}
            pipelineId={pipeline.id}
            projectId={context.projectId}
          />
        ) : null}
        {platforms.length === 0 ? (
          <Text variant="muted">
            Publishing is for Android and iOS builds. This pipeline builds
            neither.
          </Text>
        ) : null}
      </Stack>
    </Suspense>
  )
}

/* ------------------------------------------------------------------ */
/*  Secrets                                                             */
/* ------------------------------------------------------------------ */

export function SecretsRowContent({
  canReadInstanceSecrets,
  context,
}: {
  canReadInstanceSecrets: boolean
  context: PipelineRowContext
}) {
  return (
    <Suspense fallback={<Skeleton className="h-48 w-full" />}>
      <SecretsPanel
        canWrite={context.canWrite}
        inheritedFrom={[
          { scope: 'project', projectId: context.projectId },
          ...(canReadInstanceSecrets ? [{ scope: 'instance' as const }] : []),
        ]}
        target={{ scope: 'pipeline', pipelineId: context.pipeline.id }}
        title="Pipeline secrets"
      />
    </Suspense>
  )
}
