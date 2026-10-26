import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { lazy, Suspense, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, PlayIcon } from '@hugeicons/core-free-icons'

import type { BuildStatus, RuntimeMode } from '@oore/client/models'
import type {
  ListBuildsResponse,
  ListIntegrationsResponse,
  ListRunnersResponse,
} from '@oore/client/models'
import { useIndexAuthGuard } from '@/hooks/use-index-auth-guard'
import { useMountEffect } from '@/hooks/use-mount-effect'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import {
  DashboardBuildActivity,
  DashboardGettingStarted,
} from '@/components/dashboard-sections'
import DashboardBuildIncident from '@/components/dashboard-build-incident'
import { DashboardBuildHealth } from '@/components/dashboard-build-health'
import { DashboardProjectHealth } from '@/components/dashboard-project-health'
import { DashboardStoreHealth } from '@/components/dashboard-store-health'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Spinner } from '@/components/ui/spinner'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { useAttention } from '@/hooks/use-attention'
import { useBuilds, type BuildInsightsWindow } from '@/hooks/use-builds'
import { useIntegrations } from '@/hooks/use-integrations'
import {
  canChangeApprovalPolicy,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useRunners } from '@/hooks/use-runners'
import { useSetupStatus } from '@/hooks/use-setup'
import { getSetupStatus } from '@oore/client/operations'
import { isLoopbackHostname } from '@/lib/connectivity'
import { PageMeta } from '@/lib/seo'
import { isManagedFrontend } from '@/lib/managed-frontend'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'
import { createWebOoreClient } from '@/lib/api-client/client'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { useListNavigation } from '@/hooks/use-list-navigation'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

const SetupChecklist = lazy(() => import('@/components/setup-checklist'))
const ReleaseBoard = lazy(() =>
  import('@/components/releases/release-board').then((module) => ({
    default: module.ReleaseBoard,
  })),
)
const ReleaseActivity = lazy(() =>
  import('@/components/releases/release-activity').then((module) => ({
    default: module.ReleaseActivity,
  })),
)

const QaReleasesPage = lazy(() => import('@/components/qa-releases-page'))
const TriggerBuildDrawer = lazy(
  () => import('@/components/trigger-build-drawer'),
)

export const Route = createFileRoute('/')({
  staticData: {
    breadcrumb: {
      title: 'Home',
    },
  },
  component: IndexPage,
})

const KNOWN_LOCAL_DAEMON_URLS = [
  'http://127.0.0.1:8787',
  'http://127.0.0.1:8788',
  'http://127.0.0.1:8790',
]

const ACTIVE_BUILD_STATUSES = new Set<BuildStatus>([
  'queued',
  'scheduled',
  'assigned',
  'running',
])

function selectDashboardBuilds({ builds }: ListBuildsResponse) {
  const completed = builds.filter(
    (build) => !ACTIVE_BUILD_STATUSES.has(build.status),
  )

  return {
    builds,
    active: builds.filter((build) => ACTIVE_BUILD_STATUSES.has(build.status)),
    recentCompleted: completed.slice(0, 6),
  }
}

function selectHasActiveIntegration({
  active_total,
}: ListIntegrationsResponse): boolean {
  return active_total > 0
}

function selectRunnerSummary({ online_total, total }: ListRunnersResponse) {
  return {
    online: online_total,
    total,
  }
}

async function detectReachableLocalDaemonUrl(): Promise<string | null> {
  for (const candidate of KNOWN_LOCAL_DAEMON_URLS) {
    try {
      await getSetupStatus({
        client: createWebOoreClient({ baseUrl: candidate }),
        signal: AbortSignal.timeout(900),
      })
      return candidate
    } catch {
      // try next candidate
    }
  }
  return null
}

function IndexPage() {
  const instance = useActiveInstance()
  const { data: status, isLoading, error, refetch } = useSetupStatus()
  const [showAddInstance, setShowAddInstance] = useState(false)
  const [isDetectingLocalInstance, setIsDetectingLocalInstance] =
    useState(false)
  const autoDetectAttemptedRef = useRef(false)
  const authUser = useAuthStore((s) => s.user)
  useListNavigation(authUser?.role !== 'qa_viewer')

  useMountEffect(() => {
    if (instance || autoDetectAttemptedRef.current) return

    autoDetectAttemptedRef.current = true
    setIsDetectingLocalInstance(true)

    void Promise.all([
      isManagedFrontend(),
      isLoopbackHostname(window.location.hostname)
        ? detectReachableLocalDaemonUrl()
        : Promise.resolve(null),
    ])
      .then(([managedFrontend, detectedUrl]) => {
        const store = useInstanceStore.getState()
        if (Object.keys(store.instances).length > 0) return
        if (managedFrontend) {
          const instanceId = store.addInstance(window.location.hostname, '')
          store.setActiveInstance(instanceId)
          return
        }
        if (!detectedUrl) return
        const instanceId = store.addInstance('Local', detectedUrl)
        store.setActiveInstance(instanceId)
      })
      .catch(() => {
        // No reachable local daemon; keep manual add-instance path.
      })
      .finally(() => {
        setIsDetectingLocalInstance(false)
      })
  })

  const isAutoSigningIn = useIndexAuthGuard(status, instance)

  if (!instance && isDetectingLocalInstance) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <PageMeta />
        <Row gap={3}>
          <Spinner className="size-5" />
          <Text variant="muted">Looking for Oore on this Mac...</Text>
        </Row>
      </div>
    )
  }

  if (isAutoSigningIn) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <PageMeta />
        <Row gap={3}>
          <Spinner className="size-5" />
          <Text variant="muted">Signing in...</Text>
        </Row>
      </div>
    )
  }

  if (!instance) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center p-6">
        <PageMeta />
        <div className="w-full max-w-md space-y-8">
          <div className="space-y-3 text-center">
            <div className="mx-auto flex size-14 items-center justify-center">
              <img src="/logo.svg" alt="Oore CI logo" className="size-full" />
            </div>
            <h1 className="text-3xl font-bold tracking-tight">Oore CI</h1>
            <Text variant="muted">
              Build your mobile app on a Mac you control. Share an install link
              with your team.
            </Text>
          </div>

          <Card size="sm">
            <CardHeader>
              <CardTitle>Join your team</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <Text variant="muted">
                Have an Oore address? Connect and sign in to find your team’s
                projects and builds.
              </Text>
              <Button
                onClick={() => setShowAddInstance(true)}
                className="w-full"
              >
                <HugeiconsIcon icon={Add01Icon} />
                Connect to Oore
              </Button>
            </CardContent>
          </Card>
          <div className="space-y-3 text-center">
            <h2 className="font-medium">Set up Oore on your Mac</h2>
            <Text variant="muted">
              Install Oore on macOS and keep the Mac running during builds.
            </Text>
            <Button
              variant="outline"
              render={
                <a
                  href="https://docs.oore.build/start/install"
                  target="_blank"
                  rel="noopener noreferrer"
                />
              }
              nativeButton={false}
            >
              Setup guide
            </Button>
          </div>
        </div>

        <AddInstanceDialog
          open={showAddInstance}
          onOpenChange={setShowAddInstance}
        />
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <PageMeta />
        <Row gap={3}>
          <Spinner className="size-5" />
          <Text variant="muted">Connecting to Oore...</Text>
        </Row>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <PageMeta />
        <div className="w-full max-w-md">
          <Alert variant="destructive">
            <AlertTitle>Connection failed</AlertTitle>
            <AlertDescription>
              Unable to reach the oore daemon. Make sure{' '}
              <code className="bg-muted px-1 py-0.5 text-xs">oored</code> is
              running.
            </AlertDescription>
          </Alert>
          <div className="mt-4 flex flex-wrap gap-2">
            <Button onClick={() => void refetch()}>Retry</Button>
            <Button variant="outline" onClick={() => setShowAddInstance(true)}>
              Change connection
            </Button>
          </div>
          <AddInstanceDialog
            open={showAddInstance}
            onOpenChange={setShowAddInstance}
          />
        </div>
      </div>
    )
  }

  if (status?.is_configured) {
    if (authUser?.role === 'qa_viewer') {
      return (
        <Suspense
          fallback={
            <PageLayout width="wide">
              <Skeleton className="h-8 w-48" />
              <Skeleton className="h-64 w-full" />
            </PageLayout>
          }
        >
          <QaReleasesPage />
        </Suspense>
      )
    }
    return (
      <>
        <PageMeta />
        <ConfiguredDashboard runtimeMode={status.runtime_mode} />
      </>
    )
  }

  return (
    <div className="flex flex-1 items-center justify-center">
      <PageMeta />
      <Row gap={3}>
        <Spinner className="size-5" />
        <Text variant="muted">Loading...</Text>
      </Row>
    </div>
  )
}

function ConfiguredDashboard({ runtimeMode }: { runtimeMode: RuntimeMode }) {
  const navigate = useNavigate()
  const [canWriteIntegrations, canWriteProjects, canWriteBuilds] =
    useHasPermissions(['integrations:write', 'projects:write', 'builds:write'])
  const attention = useAttention()
  const canApprove = canChangeApprovalPolicy(
    useAuthStore((s) => s.user?.role),
    undefined,
  )

  const scope = useFirstAppScope()
  const guidedProjectId = useFirstAppStore(
    (state) => state.progress[scope]?.projectId,
  )
  const projectsQuery = useProjects({
    limit: 1,
    sort: 'created_at',
    direction: 'desc',
  })
  const projects = projectsQuery.data?.projects ?? []
  const integrationsQuery = useIntegrations(
    { limit: 1 },
    {
      select: selectHasActiveIntegration,
    },
  )
  const runnersQuery = useRunners(
    { limit: 1 },
    {
      select: selectRunnerSummary,
    },
  )

  const recentBuildsQuery = useBuilds(
    { limit: 50 },
    { select: selectDashboardBuilds },
  )
  const [insightsDays, setInsightsDays] = useState<BuildInsightsWindow>(7)
  const activeBuilds = recentBuildsQuery.data?.active ?? []
  const recentCompletedBuilds = recentBuildsQuery.data?.recentCompleted ?? []
  const hasProjects = projects.length > 0
  const integrationsResolved =
    !integrationsQuery.isLoading && !integrationsQuery.error
  const noConnectedSources =
    runtimeMode === 'remote' &&
    integrationsResolved &&
    integrationsQuery.data === false
  const noOnlineRunners = !!runnersQuery.data && runnersQuery.data.online === 0
  const canShowRunBuild = hasProjects && canWriteBuilds
  const blockedBuilds = activeBuilds.filter(
    (build) => build.runner_policy_block_reason,
  )

  if (
    projectsQuery.isLoading ||
    recentBuildsQuery.isLoading ||
    runnersQuery.isLoading
  ) {
    return (
      <PageLayout width="wide">
        <PageHeader title="Home" />
        <div className="space-y-8" role="status" aria-label="Loading Home">
          <Skeleton className="h-48 w-full" />
          <Skeleton className="h-80 w-full" />
        </div>
      </PageLayout>
    )
  }

  const runBuildButton = (
    <Button disabled={noOnlineRunners}>
      <HugeiconsIcon icon={PlayIcon} data-icon="inline-start" />
      Run build
    </Button>
  )

  return (
    <PageLayout width="wide">
      <Stack gap={8}>
        <PageHeader
          title="Home"
          actions={
            !canShowRunBuild ? undefined : noOnlineRunners ? (
              <Tooltip>
                <TooltipTrigger render={<span tabIndex={0} />}>
                  {runBuildButton}
                </TooltipTrigger>
                <TooltipContent>
                  No runner is online, so a build would only wait.
                </TooltipContent>
              </Tooltip>
            ) : (
              <Suspense fallback={null}>
                <TriggerBuildDrawer
                  description="Choose a project, pipeline and branch to build."
                  onBuildCreated={(buildId) => {
                    void navigate({
                      to: '/builds/$buildId',
                      params: { buildId },
                    })
                  }}
                >
                  {runBuildButton}
                </TriggerBuildDrawer>
              </Suspense>
            )
          }
        />

        {!projectsQuery.isLoading &&
        !projectsQuery.error &&
        projects.length === 0 ? (
          <DashboardGettingStarted
            canWriteIntegrations={canWriteIntegrations}
            canWriteProjects={canWriteProjects}
            noConnectedSources={noConnectedSources}
            noOnlineRunners={noOnlineRunners}
            runtimeMode={runtimeMode}
          />
        ) : null}

        {hasProjects &&
        (guidedProjectId || recentBuildsQuery.data?.builds.length === 0) ? (
          <Suspense fallback={<Skeleton className="h-16 w-full" />}>
            <SetupChecklist
              projectId={guidedProjectId ?? projects[0].id}
              variant="summary"
            />
          </Suspense>
        ) : null}

        {projectsQuery.error ? (
          <Alert variant="destructive">
            <AlertDescription className="flex items-center justify-between gap-3">
              <span>Projects could not be loaded.</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void projectsQuery.refetch()}
              >
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : null}

        <DashboardBuildIncident
          items={
            hasProjects
              ? attention.items
              : // First run lists connecting a runner as a step already.
                attention.items.filter((item) => item.key !== 'no-runner')
          }
        />

        {hasProjects || projectsQuery.error ? (
          <DashboardBuildActivity
            activeBuilds={activeBuilds}
            blockedBuilds={blockedBuilds}
            error={recentBuildsQuery.error}
            isLoading={recentBuildsQuery.isLoading}
            onRetry={() => void recentBuildsQuery.refetch()}
            recentBuilds={recentCompletedBuilds}
          />
        ) : null}

        {hasProjects ? (
          <Suspense fallback={null}>
            <ReleaseBoard />
            <ReleaseActivity limit={3} showProject />
          </Suspense>
        ) : null}

        {hasProjects ? (
          <DashboardBuildHealth
            canApprove={canApprove}
            days={insightsDays}
            onDaysChange={setInsightsDays}
            onlineRunners={runnersQuery.data?.online}
          />
        ) : null}

        {hasProjects ? <DashboardStoreHealth /> : null}

        {hasProjects ? <DashboardProjectHealth days={insightsDays} /> : null}
      </Stack>
    </PageLayout>
  )
}
