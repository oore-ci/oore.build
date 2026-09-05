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
  DashboardBuildOverview,
  DashboardGettingStarted,
} from '@/components/dashboard-sections'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Spinner } from '@/components/ui/spinner'
import { useBuilds } from '@/hooks/use-builds'
import { useIntegrations } from '@/hooks/use-integrations'
import {
  useMarkOperatorIncidentRead,
  useOperatorIncidents,
} from '@/hooks/use-operator-incidents'
import { useHasPermissions } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useRunners } from '@/hooks/use-runners'
import { useSetupStatus } from '@/hooks/use-setup'
import { getSetupStatus } from '@oore/client/operations'
import { isLoopbackHostname } from '@/lib/connectivity'
import { PageMeta } from '@/lib/seo'
import { isManagedFrontend } from '@/lib/managed-frontend'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'
import { OperatorIncidentAlert } from '@/components/operator-incident-alert'
import { createWebOoreClient } from '@/lib/api-client/client'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'

const FirstAppProgress = lazy(() => import('@/components/first-app-progress'))

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
    completedCount: completed.length,
    recentCompleted: completed.slice(0, 6),
    successfulCount: completed.filter((build) => build.status === 'succeeded')
      .length,
  }
}

function selectBuildTotal({ total }: ListBuildsResponse): number {
  return total
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
        <div className="flex items-center gap-3">
          <Spinner className="size-5" />
          <p className="text-sm text-muted-foreground">
            Looking for Oore on this Mac...
          </p>
        </div>
      </div>
    )
  }

  if (isAutoSigningIn) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <PageMeta />
        <div className="flex items-center gap-3">
          <Spinner className="size-5" />
          <p className="text-sm text-muted-foreground">Signing in...</p>
        </div>
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
            <p className="text-sm text-muted-foreground">
              Build your mobile app on a Mac you control. Share an install link
              with your team.
            </p>
          </div>

          <Card size="sm">
            <CardHeader>
              <CardTitle>Join your team</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Have an Oore address? Connect and sign in to find your team’s
                projects and builds.
              </p>
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
            <p className="text-sm text-muted-foreground">
              Install Oore and start the service on macOS. Keep the Mac running
              while it builds your apps. The browser provides the interface.
            </p>
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
              Open setup guide
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
        <div className="flex items-center gap-3">
          <Spinner className="size-5" />
          <p className="text-sm text-muted-foreground">Connecting to Oore...</p>
        </div>
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
            <Button onClick={() => void refetch()}>Retry connection</Button>
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
      <div className="flex items-center gap-3">
        <Spinner className="size-5" />
        <p className="text-sm text-muted-foreground">Loading...</p>
      </div>
    </div>
  )
}

function ConfiguredDashboard({ runtimeMode }: { runtimeMode: RuntimeMode }) {
  const navigate = useNavigate()
  const [canWriteIntegrations, canWriteProjects, canWriteBuilds] =
    useHasPermissions(['integrations:write', 'projects:write', 'builds:write'])
  const incidentsQuery = useOperatorIncidents({
    enabled: canWriteIntegrations,
  })
  const markIncidentRead = useMarkOperatorIncidentRead()

  const scope = useFirstAppScope()
  const progress = useFirstAppStore((state) => state.progress[scope])
  const updateProgress = useFirstAppStore((state) => state.update)
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
  const runningBuildsQuery = useBuilds(
    { status: 'running', limit: 1 },
    { select: selectBuildTotal },
  )
  const waitingBuildsQuery = useBuilds(
    { status: 'queued,scheduled,assigned', limit: 1 },
    { select: selectBuildTotal },
  )
  const activeBuilds = recentBuildsQuery.data?.active ?? []
  const recentCompletedBuilds = recentBuildsQuery.data?.recentCompleted ?? []
  const completedBuilds = recentBuildsQuery.data?.completedCount ?? 0
  const successfulBuilds = recentBuildsQuery.data?.successfulCount ?? 0
  const hasProjects = projects.length > 0
  const integrationsResolved =
    !integrationsQuery.isLoading && !integrationsQuery.error
  const noConnectedSources =
    runtimeMode === 'remote' &&
    integrationsResolved &&
    integrationsQuery.data === false
  const onlineRunners = runnersQuery.data?.online ?? 0
  const totalRunners = runnersQuery.data?.total ?? 0
  const noOnlineRunners = !!runnersQuery.data && runnersQuery.data.online === 0
  const canShowRunBuild = hasProjects && !noOnlineRunners && canWriteBuilds
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

  return (
    <PageLayout width="wide">
      <div className="flex flex-col gap-8">
        <PageHeader
          title="Home"
          actions={
            canShowRunBuild ? (
              <Suspense fallback={null}>
                <TriggerBuildDrawer
                  description="Choose a project and pipeline to run a manual build."
                  onBuildCreated={(buildId) => {
                    void navigate({
                      to: '/builds/$buildId',
                      params: { buildId },
                    })
                  }}
                >
                  <Button>
                    <HugeiconsIcon icon={PlayIcon} data-icon="inline-start" />
                    Run build
                  </Button>
                </TriggerBuildDrawer>
              </Suspense>
            ) : undefined
          }
        />

        {incidentsQuery.data?.incidents.map((incident) => (
          <OperatorIncidentAlert
            incident={incident}
            key={incident.id}
            onRead={() => markIncidentRead.mutate(incident.id)}
          />
        ))}

        {!projectsQuery.isLoading &&
        !projectsQuery.error &&
        projects.length === 0 ? (
          <DashboardGettingStarted
            canWriteIntegrations={canWriteIntegrations}
            canWriteProjects={canWriteProjects}
            noConnectedSources={noConnectedSources}
            runtimeMode={runtimeMode}
          />
        ) : null}

        {hasProjects &&
        (progress?.projectId || recentBuildsQuery.data?.builds.length === 0) ? (
          <Suspense fallback={<Skeleton className="h-40 w-full" />}>
            <FirstAppProgress
              projectId={progress?.projectId ?? projects[0].id}
            />
          </Suspense>
        ) : hasProjects ? (
          <Button
            variant="ghost"
            size="sm"
            className="self-start"
            onClick={() =>
              updateProgress(scope, {
                projectId: projects[0].id,
                hidden: false,
              })
            }
          >
            Set up an app
          </Button>
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

        {hasProjects || projectsQuery.isLoading || projectsQuery.error ? (
          <DashboardBuildOverview
            activeBuilds={activeBuilds}
            blockedBuilds={blockedBuilds}
            completedBuilds={completedBuilds}
            error={recentBuildsQuery.error}
            isLoading={recentBuildsQuery.isLoading}
            noOnlineRunners={hasProjects && noOnlineRunners}
            onlineRunners={onlineRunners}
            onRetry={() => void recentBuildsQuery.refetch()}
            recentBuilds={recentCompletedBuilds}
            runnersError={!!runnersQuery.error}
            runnersLoading={runnersQuery.isLoading}
            runningBuilds={runningBuildsQuery.data ?? 0}
            statusCountsError={
              !!runningBuildsQuery.error || !!waitingBuildsQuery.error
            }
            statusCountsLoading={
              runningBuildsQuery.isLoading || waitingBuildsQuery.isLoading
            }
            successfulBuilds={successfulBuilds}
            totalRunners={totalRunners}
            waitingBuilds={waitingBuildsQuery.data ?? 0}
          />
        ) : null}
      </div>
    </PageLayout>
  )
}
