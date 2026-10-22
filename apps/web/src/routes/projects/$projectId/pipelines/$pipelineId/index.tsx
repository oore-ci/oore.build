import { lazy, Suspense, useState } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  Edit02Icon,
  InformationCircleIcon,
  PlayIcon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import type { Build } from '@oore/client/models'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import {
  useDeletePipeline,
  usePipeline,
  usePipelineAndroidSigning,
  usePipelineIosSigning,
  useUpdatePipeline,
} from '@/hooks/use-pipelines'
import { useProject } from '@/hooks/use-projects'
import { enabledStatus } from '@/lib/status-variants'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import { DangerZone } from '@/components/danger-zone'
import { RelativeTime } from '@/components/relative-time'
import { StatusBadge } from '@/components/status-badge'
import { PageMeta } from '@/lib/seo'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import { Alert, AlertDescription } from '@/components/ui/alert'
import ConfirmDialog from '@/components/ConfirmDialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { searchChoice } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'
import { PipelineConfigurationCard } from '../-pipeline-configuration-card'

const recentBuildColumns: Array<DataTableColumnDef<Build>> = [
  {
    accessorKey: 'build_number',
    header: 'Build',
    cell: ({ row }) => (
      <Link to="/builds/$buildId" params={{ buildId: row.original.id }}>
        #{row.original.build_number}
      </Link>
    ),
    enableSorting: false,
  },
  {
    accessorKey: 'status',
    header: 'Status',
    cell: ({ row }) => <StatusBadge kind="build" value={row.original.status} />,
    enableSorting: false,
  },
  {
    accessorKey: 'branch',
    header: 'Branch',
    cell: ({ row }) => row.original.branch ?? 'n/a',
    enableSorting: false,
  },
  {
    accessorKey: 'created_at',
    header: 'Created',
    cell: ({ row }) => <RelativeTime at={row.original.created_at} />,
    enableSorting: false,
  },
]

function RecentBuildsTable({ builds }: { builds: Array<Build> }) {
  const table = useDataTable({
    columns: recentBuildColumns,
    data: builds,
    getRowId: (build) => build.id,
  })
  return <DataTable table={table} />
}

const loadTriggerBuildDrawer = () => import('@/components/trigger-build-drawer')
const TriggerBuildDrawer = lazy(loadTriggerBuildDrawer)
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

const TAB_VALUES = ['overview', 'publishing', 'secrets'] as const
type TabValue = (typeof TAB_VALUES)[number]

interface PipelineDetailSearch {
  tab?: TabValue
}

function validatePipelineSearch(search: SearchInput): PipelineDetailSearch {
  const tab = searchChoice(search, 'tab', TAB_VALUES)
  return { tab: tab === 'overview' ? undefined : tab }
}

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId/',
)({
  validateSearch: validatePipelineSearch,
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(
      instance.id,
      ['owner', 'admin', 'developer'],
      context,
    )
  },
  component: PipelineDetailPage,
})

/* ------------------------------------------------------------------ */
/*  Main page                                                          */
/* ------------------------------------------------------------------ */

function PipelineDetailPage() {
  const { projectId, pipelineId } = Route.useParams()
  const navigate = useNavigate()
  const { tab = 'overview' } = Route.useSearch()
  const { data, isLoading, error } = usePipeline(pipelineId)
  const [canWriteGlobally, canTriggerBuildGlobally, canReadInstanceSettings] =
    useHasPermissions([
      'pipelines:write',
      'builds:write',
      'instance_settings:read',
    ])
  const { data: projectData } = useProject(projectId)
  const projectRole = projectData?.project.current_user_role
  const canWrite =
    canWriteGlobally && hasProjectPermission(projectRole, 'pipelines:write')
  const canDelete = hasProjectPermission(projectRole, 'pipelines:delete')
  const canTriggerBuild =
    canTriggerBuildGlobally && hasProjectPermission(projectRole, 'builds:write')
  const signingQuery = usePipelineAndroidSigning(pipelineId, {
    enabled: canWrite,
  })
  const iosSigningQuery = usePipelineIosSigning(pipelineId, {
    enabled: canWrite,
  })
  const { data: buildsData } = useBuilds({
    pipeline_id: pipelineId,
    limit: 20,
  })
  const updateMutation = useUpdatePipeline()
  const deleteMutation = useDeletePipeline()

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [buildDrawerOpen, setBuildDrawerOpen] = useState(false)

  const label = data?.pipeline.name ?? 'Pipeline Details'

  if (isLoading) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-28 w-full" />
        <Skeleton className="h-56 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load pipeline: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { pipeline } = data
  const buildsAndroid = pipeline.execution_config.platforms.includes('android')
  const buildsIos = pipeline.execution_config.platforms.includes('ios')
  const builds = buildsData?.builds ?? []
  const projectHasSource = !!projectData?.project.repository_id
  const manualOnlyTriggers =
    projectData?.project.repository_provider === 'local_git'

  function handleToggleEnabled() {
    updateMutation.mutate(
      { pipelineId: pipeline.id, data: { enabled: !pipeline.enabled } },
      {
        onSuccess: () =>
          toast.success(
            pipeline.enabled ? 'Pipeline disabled' : 'Pipeline enabled',
          ),
        onError: (err) =>
          toast.error(`Failed to update pipeline: ${err.message}`),
      },
    )
  }

  function handleDelete() {
    deleteMutation.mutate(
      { pipelineId, projectId },
      {
        onSuccess: () => {
          toast.success('Pipeline deleted')
          void navigate({ to: '/projects/$projectId', params: { projectId } })
        },
        onError: (err) =>
          toast.error(`Failed to delete pipeline: ${err.message}`),
      },
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
      <PageHeader
        title={pipeline.name}
        description="Pipeline overview and configuration."
        meta={
          <>
            <StatusBadge
              kind="enabled"
              value={enabledStatus(pipeline.enabled)}
            />
            {pipeline.execution_config.platforms.map((p) => (
              <Badge key={p} variant="outline" className="text-[11px]">
                {BUILD_PLATFORM_LABELS[p]}
              </Badge>
            ))}
            <span>
              Updated <RelativeTime at={pipeline.updated_at} />
            </span>
          </>
        }
        actions={
          canWrite || canTriggerBuild ? (
            <>
              {canTriggerBuild ? (
                <Suspense
                  fallback={
                    <Button disabled>
                      <HugeiconsIcon icon={PlayIcon} />
                      Run build
                    </Button>
                  }
                >
                  <TriggerBuildDrawer
                    fixedProjectId={projectId}
                    fixedPipelineId={pipeline.id}
                    fixedPipelineName={pipeline.name}
                    defaultBranch={
                      projectData?.project.default_branch ?? undefined
                    }
                    description="Run this pipeline now with a branch or pinned commit."
                    open={buildDrawerOpen}
                    onOpenChange={setBuildDrawerOpen}
                    onBuildCreated={(buildId) => {
                      void navigate({
                        to: '/builds/$buildId',
                        params: { buildId },
                      })
                    }}
                  >
                    <Button disabled={!projectHasSource}>
                      <HugeiconsIcon icon={PlayIcon} />
                      Run build
                    </Button>
                  </TriggerBuildDrawer>
                </Suspense>
              ) : null}
              {canWrite ? (
                <Button
                  variant="outline"
                  onClick={handleToggleEnabled}
                  disabled={updateMutation.isPending}
                >
                  {pipeline.enabled ? 'Disable' : 'Enable'}
                </Button>
              ) : null}
              {canWrite ? (
                <Button
                  variant="outline"
                  render={
                    <Link
                      to="/projects/$projectId/pipelines/$pipelineId/edit"
                      params={{ projectId, pipelineId }}
                      search={{}}
                    />
                  }
                  nativeButton={false}
                >
                  <HugeiconsIcon icon={Edit02Icon} />
                  Edit
                </Button>
              ) : null}
            </>
          ) : undefined
        }
      />
      {!projectHasSource ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            This project has no linked source repository. Link a repository
            before triggering builds.
          </AlertDescription>
        </Alert>
      ) : null}

      <Tabs
        value={tab}
        onValueChange={(value) => {
          const next = TAB_VALUES.find((candidate) => candidate === value)
          if (!next) return
          void navigate({
            to: '/projects/$projectId/pipelines/$pipelineId',
            params: { projectId, pipelineId },
            search: { tab: next === 'overview' ? undefined : next },
            replace: true,
          })
        }}
      >
        <TabsList variant="line">
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="publishing">Publishing</TabsTrigger>
          <TabsTrigger value="secrets">Secrets</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="flex flex-col gap-4 pt-2">
          <PipelineConfigurationCard
            androidSigning={signingQuery.data}
            iosSigning={iosSigningQuery.data}
            manualOnlyTriggers={manualOnlyTriggers}
            pipeline={pipeline}
          />
          {/* Recent builds */}
          <Card>
            <CardContent>
              <h3 className="pb-3 text-sm font-medium">Recent builds</h3>
              {builds.length === 0 ? (
                <Empty className="p-8">
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <HugeiconsIcon icon={PlayIcon} />
                    </EmptyMedia>
                    <EmptyTitle>No builds yet</EmptyTitle>
                    <EmptyDescription>
                      Run this pipeline to see its status, output, and artifacts
                      here.
                    </EmptyDescription>
                  </EmptyHeader>
                  {canTriggerBuild && projectHasSource ? (
                    <EmptyContent>
                      <Button
                        size="sm"
                        onClick={() => setBuildDrawerOpen(true)}
                      >
                        <HugeiconsIcon icon={PlayIcon} />
                        Run first build
                      </Button>
                    </EmptyContent>
                  ) : null}
                </Empty>
              ) : (
                <RecentBuildsTable builds={builds} />
              )}
            </CardContent>
          </Card>
          {canDelete ? (
            <DangerZone
              title="Delete this pipeline"
              description={`Permanently removes "${pipeline.name}" and its configuration from this project. This cannot be undone.`}
              action={
                <Button
                  variant="destructive"
                  onClick={() => setDeleteOpen(true)}
                >
                  <HugeiconsIcon icon={Delete02Icon} />
                  Delete pipeline
                </Button>
              }
            />
          ) : null}
        </TabsContent>

        <TabsContent value="publishing" className="pt-2">
          {tab === 'publishing' ? (
            <Suspense fallback={<Skeleton className="h-96 w-full" />}>
              <div className="flex flex-col gap-8">
                {buildsAndroid ? (
                  <GooglePlayPublishSection
                    androidSigning={signingQuery.data}
                    canReadInstanceSecrets={canReadInstanceSettings}
                    canWrite={canWrite}
                    pipelineId={pipeline.id}
                    projectId={projectId}
                  />
                ) : null}
                {buildsIos ? (
                  <AppStorePublishSection
                    canWrite={canWrite}
                    pipelineId={pipeline.id}
                    projectId={projectId}
                  />
                ) : null}
                {!buildsAndroid && !buildsIos ? (
                  <Alert>
                    <HugeiconsIcon icon={InformationCircleIcon} />
                    <AlertDescription>
                      Publishing is available for Android and iOS builds. Add
                      one of them to this pipeline’s platforms to upload to
                      Google Play or App Store Connect.
                    </AlertDescription>
                  </Alert>
                ) : null}
              </div>
            </Suspense>
          ) : null}
        </TabsContent>

        <TabsContent value="secrets" className="pt-2">
          {tab === 'secrets' ? (
            <Suspense fallback={<Skeleton className="h-48 w-full" />}>
              <SecretsPanel
                canWrite={canWrite}
                inheritedFrom={[
                  { scope: 'project', projectId },
                  ...(canReadInstanceSettings
                    ? [{ scope: 'instance' as const }]
                    : []),
                ]}
                target={{ scope: 'pipeline', pipelineId: pipeline.id }}
                title="Pipeline secrets"
              />
            </Suspense>
          ) : null}
        </TabsContent>
      </Tabs>

      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title="Delete pipeline?"
        description={`This will permanently delete "${pipeline.name}". This action cannot be undone.`}
        confirmLabel="Delete pipeline"
        confirmVariant="destructive"
        isPending={deleteMutation.isPending}
        pendingLabel="Deleting..."
        onConfirm={handleDelete}
      />
    </PageLayout>
  )
}
