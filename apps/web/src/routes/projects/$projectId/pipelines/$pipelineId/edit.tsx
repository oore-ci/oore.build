import type { IosSigningFiles } from '@/lib/pipeline-signing'
import { useState } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { toast } from '@/lib/toast'

import type { RegisteredIosDevice } from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import { canChangeApprovalPolicy } from '@/hooks/use-permissions'
import { useAuthStore } from '@/stores/auth-store'
import { searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { requireProjectPermissionOrRedirect } from '@/lib/project-route-guard'
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
import { usePipelineAppStorePublish } from '@/hooks/use-pipeline-publish'
import { useProject } from '@/hooks/use-projects'
import {
  buildNumberOffsetField,
  runsOnField,
  hasCustomFallback,
  pipelineRequestFromForm,
  selectedPlatforms,
  toMultiline,
} from '@/lib/pipeline-form-utils'
import {
  buildAndroidSigningPayload,
  buildIosSigningPayload,
  iosProfilesNeedResync,
} from '@/lib/pipeline-signing'
import { PageMeta } from '@/lib/seo'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Skeleton } from '@/components/ui/skeleton'
import PipelineForm from '@/components/pipeline-form'
import { PipelineFormSection } from '@/components/pipeline-form-section'

const iosDeviceColumns: Array<DataTableColumnDef<RegisteredIosDevice>> = [
  { accessorKey: 'name', header: 'Name', enableSorting: false },
  {
    accessorKey: 'udid',
    header: 'UDID',
    enableSorting: false,
  },
  { accessorKey: 'status', header: 'Status', enableSorting: false },
]

function describeProfileSync(updatedProfiles: number) {
  return `${updatedProfiles} provisioning profile${updatedProfiles === 1 ? '' : 's'} synced from App Store Connect`
}

function IosDeviceTable({ devices }: { devices: Array<RegisteredIosDevice> }) {
  const table = useDataTable({
    columns: iosDeviceColumns,
    data: devices,
    getRowId: (device) => device.id,
  })
  return <DataTable table={table} />
}

interface EditPipelineSearch {
  signing?: 'android' | 'ios'
  signingError?: string
}

function parseEditPipelineSearch(search: SearchInput): EditPipelineSearch {
  const signing = searchString(search, 'signing')
  return {
    signing: signing === 'android' || signing === 'ios' ? signing : undefined,
    signingError: searchString(search, 'signingError'),
  }
}

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId/edit',
)({
  staticData: {
    breadcrumb: {
      title: 'Edit',
    },
  },
  validateSearch: parseEditPipelineSearch,
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
  component: EditPipelinePage,
})

function EditPipelinePage() {
  const { projectId, pipelineId } = Route.useParams()
  const { signing: retrySigning, signingError } = Route.useSearch()
  const navigate = useNavigate()
  const { data: projectData } = useProject(projectId)
  const manualOnlyTriggers =
    projectData?.project.repository_provider === 'local_git'
  const instanceRole = useAuthStore((state) => state.user?.role)
  const canChangeApproval = canChangeApprovalPolicy(
    instanceRole,
    projectData?.project.current_user_role,
  )
  const { data, isLoading, error } = usePipeline(pipelineId)
  const signingQuery = usePipelineAndroidSigning(pipelineId)
  const iosSigningQuery = usePipelineIosSigning(pipelineId)
  const iosDevicesQuery = usePipelineIosDevices(pipelineId)
  const appStorePublishQuery = usePipelineAppStorePublish(pipelineId)
  const updateMutation = useUpdatePipeline()
  const updateSigningMutation = useUpdatePipelineAndroidSigning()
  const updateIosSigningMutation = useUpdatePipelineIosSigning()
  const registerIosDeviceMutation = useRegisterPipelineIosDevice()
  const syncIosSigningMutation = useSyncPipelineIosSigning()
  const [deviceName, setDeviceName] = useState('')
  const [deviceUdid, setDeviceUdid] = useState('')
  const [validationErrors, setValidationErrors] = useState<Array<string>>([])

  const label = data?.pipeline.name
    ? `Edit ${data.pipeline.name}`
    : 'Edit Pipeline'

  if (isLoading || signingQuery.isLoading || iosSigningQuery.isLoading) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-96 w-full" />
      </PageLayout>
    )
  }

  if (error || !data) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
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
    ios_signing_export_method: iosSigningQuery.data?.export_method ?? 'ad_hoc',
    ios_signing_team_id: iosSigningQuery.data?.team_id ?? '',
    ios_signing_bundle_ids: (iosSigningQuery.data?.bundle_ids ?? []).join('\n'),
    ios_signing_p12_password: '',
    ios_signing_api_key_id: iosSigningQuery.data?.api_key_id ?? '',
    ios_signing_api_issuer_id: iosSigningQuery.data?.api_issuer_id ?? '',
    flutter_version: pipeline.execution_config.flutter_version ?? '',
    build_number_offset: buildNumberOffsetField(pipeline.execution_config),
    runs_on: runsOnField(pipeline.execution_config),
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
  }

  async function handleSubmit(
    values: PipelineFormValues,
    releaseKeystoreFile: File | null,
    debugKeystoreFile: File | null,
    iosSigningFiles: IosSigningFiles,
  ) {
    if (selectedPlatforms(values).length === 0) {
      setValidationErrors(['Pick at least one platform to build'])
      return
    }
    const payload = pipelineRequestFromForm(values, {
      canChangeApproval,
      manualOnlyTriggers,
    })

    const [androidSigning, iosSigning] = await Promise.all([
      buildAndroidSigningPayload(
        values,
        { release: releaseKeystoreFile, debug: debugKeystoreFile },
        signingQuery.data,
      ),
      buildIosSigningPayload(values, iosSigningFiles, iosSigningQuery.data),
    ])
    const signingErrors = [...androidSigning.errors, ...iosSigning.errors]
    setValidationErrors(signingErrors)
    if (signingErrors.length > 0) {
      return
    }
    const signingPayload = androidSigning.payload
    const iosSigningPayload = iosSigning.payload

    try {
      if (!retrySigning) {
        await updateMutation.mutateAsync({
          pipelineId: pipeline.id,
          data: payload,
        })
      }
      if (signingPayload && retrySigning !== 'ios') {
        await updateSigningMutation.mutateAsync({
          pipelineId: pipeline.id,
          data: signingPayload,
        })
      }
      let profileSyncNote = ''
      if (iosSigningPayload && retrySigning !== 'android') {
        await updateIosSigningMutation.mutateAsync({
          pipelineId: pipeline.id,
          data: iosSigningPayload,
        })
        // The daemon discarded the stored profiles with the distribution
        // change; regenerate them now so the next build does not fail for
        // lack of a profile.
        if (iosProfilesNeedResync(iosSigningPayload, iosSigningQuery.data)) {
          try {
            const response = await syncIosSigningMutation.mutateAsync(
              pipeline.id,
            )
            profileSyncNote = `; ${describeProfileSync(response.updated_profiles)}`
          } catch (err) {
            const message = err instanceof Error ? err.message : 'Unknown error'
            toast.error(
              `Pipeline updated, but iOS profiles could not be synced: ${message}. Press Sync profiles under iOS signing before the next build.`,
            )
            return
          }
        }
      }
      toast.success(
        `${retrySigning ? 'Signing updated' : 'Pipeline updated'}${profileSyncNote}`,
      )
      await navigate({
        to: '/projects/$projectId/pipelines/$pipelineId',
        params: { projectId, pipelineId },
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(
        `Failed to ${retrySigning ? 'update signing' : 'update pipeline'}: ${message}`,
      )
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
        `iOS signing sync completed: ${describeProfileSync(response.updated_profiles)}${warningSuffix}`,
      )
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error'
      toast.error(`Failed to sync iOS signing: ${message}`)
    }
  }

  const storedIosSigning = iosSigningQuery.data
  // App Store Connect can regenerate profiles only with API credentials.
  const iosProfileSyncAvailable =
    !!storedIosSigning?.enabled &&
    (storedIosSigning.mode === 'api' || storedIosSigning.mode === 'hybrid')
  const iosSigningActions = iosProfileSyncAvailable ? (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={syncIosSigningMutation.isPending}
      onClick={() => void handleSyncIosSigning()}
    >
      {syncIosSigningMutation.isPending ? 'Syncing...' : 'Sync profiles'}
    </Button>
  ) : undefined
  const iosAppStorePublishing = appStorePublishQuery.data
    ? {
        configured: appStorePublishQuery.data.config.enabled,
        link: (
          <Link
            to="/projects/$projectId/pipelines/$pipelineId"
            params={{ projectId, pipelineId }}
            search={{ tab: 'publishing' }}
            className="underline underline-offset-4"
          >
            {appStorePublishQuery.data.config.enabled
              ? 'Open publishing'
              : 'Set up publishing'}
          </Link>
        ),
      }
    : undefined

  return (
    <PageLayout>
      <PageMeta title={label} noindex />
      <PageHeader
        title={`Edit ${pipeline.name}`}
        description={
          retrySigning
            ? `Fix and retry ${retrySigning} signing without creating the pipeline again.`
            : undefined
        }
      />
      <PipelineForm
        initialValues={formInitialValues}
        manualOnlyTriggers={manualOnlyTriggers}
        canChangeApproval={canChangeApproval}
        onSubmit={handleSubmit}
        onCancel={() =>
          void navigate({
            to: '/projects/$projectId/pipelines/$pipelineId',
            params: { projectId, pipelineId },
          })
        }
        submitLabel={
          retrySigning ? `Retry ${retrySigning} signing` : 'Save changes'
        }
        isPending={
          updateMutation.isPending ||
          updateSigningMutation.isPending ||
          updateIosSigningMutation.isPending
        }
        validationErrors={validationErrors}
        retrySigning={retrySigning}
        signingError={signingError}
        signingData={signingQuery.data}
        iosSigningData={iosSigningQuery.data}
        iosSigningActions={iosSigningActions}
        iosAppStorePublishing={iosAppStorePublishing}
      >
        {iosProfileSyncAvailable &&
          storedIosSigning.export_method === 'ad_hoc' && (
            <PipelineFormSection
              title="Registered iOS devices"
              description="Ad hoc builds install only on these devices. Registering one re-syncs the provisioning profiles."
            >
              <div className="grid gap-3 md:grid-cols-[1fr_1fr_auto]">
                <Input
                  aria-label="Device name"
                  placeholder="Device name"
                  value={deviceName}
                  onChange={(event) => setDeviceName(event.target.value)}
                />
                <Input
                  aria-label="Device UDID"
                  placeholder="UDID"
                  className="font-mono"
                  value={deviceUdid}
                  onChange={(event) => setDeviceUdid(event.target.value)}
                />
                <Button
                  type="button"
                  variant="outline"
                  disabled={registerIosDeviceMutation.isPending}
                  onClick={() => void handleRegisterDevice()}
                >
                  {registerIosDeviceMutation.isPending
                    ? 'Registering...'
                    : 'Register device'}
                </Button>
              </div>

              {iosDevicesQuery.isLoading ? (
                <p className="text-sm text-muted-foreground">
                  Loading devices...
                </p>
              ) : iosDevicesQuery.data?.devices.length ? (
                <IosDeviceTable devices={iosDevicesQuery.data.devices} />
              ) : (
                <p className="text-sm text-muted-foreground">
                  No iOS devices registered for this pipeline.
                </p>
              )}
            </PipelineFormSection>
          )}
      </PipelineForm>
    </PageLayout>
  )
}
