import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { lazy, Suspense, useMemo, useRef, useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Plus as Add01Icon,
  ArrowRight as ArrowRight01Icon,
  LoaderCircle as Loading03Icon,
  Play as PlayIcon,
} from 'lucide-react'

import type { RuntimeMode } from '@/lib/types'
import { useIndexAuthGuard } from '@/hooks/use-index-auth-guard'
import { useMountEffect } from '@/hooks/use-mount-effect'
import ActiveBuildBanner from '@/components/active-build-banner'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import ProjectCard from '@/components/project-card'
import {
  DashboardGettingStarted,
  DashboardRecentBuilds,
} from '@/components/dashboard-sections'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { ItemGroup } from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Spinner } from '@/components/ui/spinner'
import { useBuilds } from '@/hooks/use-builds'
import { useIntegrations } from '@/hooks/use-integrations'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useRunners } from '@/hooks/use-runners'
import { useSetupStatus } from '@/hooks/use-setup'
import { getSetupStatus } from '@/lib/api'
import { selectDashboardBuilds, selectDashboardProjects } from '@/lib/dashboard'
import { PageMeta } from '@/lib/seo'
import { isManagedFrontend } from '@/lib/managed-frontend'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'

const loadQaReleasesPage = () => import('@/components/qa-releases-page')
const QaReleasesPage = lazy(loadQaReleasesPage)
const loadTriggerBuildDialog = () => import('@/components/trigger-build-dialog')
const TriggerBuildDialog = lazy(loadTriggerBuildDialog)

export const Route = createFileRoute('/')({
  staticData: { breadcrumb: {
   title: 'Dashboard' ,
 },},
  component: IndexPage,
})

const KNOWN_LOCAL_DAEMON_URLS = [
  'http://127.0.0.1:8787',
  'http://127.0.0.1:8788',
  'http://127.0.0.1:8790',
]

function normalizeUrl(value: string): string {
  return value.replace(/\/+$/, '')
}

function isLoopbackHostname(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '::1' ||
    hostname === '[::1]'
  )
}

async function getSetupStatusWithTimeout(baseUrl: string, timeoutMs: number) {
  return await Promise.race([
    getSetupStatus(baseUrl),
    new Promise<never>((_, reject) => {
      setTimeout(() => reject(new Error('timeout')), timeoutMs)
    }),
  ])
}

async function detectReachableLocalDaemonUrl(): Promise<string | null> {
  for (const candidate of KNOWN_LOCAL_DAEMON_URLS) {
    try {
      await getSetupStatusWithTimeout(candidate, 900)
      return candidate
    } catch {
      // try next candidate
    }
  }
  return null
}

function IndexPage() {
  const instance = useActiveInstance()
  const { data: status, isLoading, error } = useSetupStatus()
  const [showAddInstance, setShowAddInstance] = useState(false)
  const [isDetectingLocalInstance, setIsDetectingLocalInstance] =
    useState(false)
  const [isAutoSigningIn, setIsAutoSigningIn] = useState(false)
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
        const existingInstance = Object.values(store.instances).find(
          (candidate) =>
            normalizeUrl(candidate.url) === normalizeUrl(detectedUrl),
        )
        const instanceId =
          existingInstance?.id ?? store.addInstance('Local', detectedUrl)
        store.setActiveInstance(instanceId)
      })
      .catch(() => {
        // No reachable local daemon; keep manual add-instance path.
      })
      .finally(() => {
        setIsDetectingLocalInstance(false)
      })
  })

  useIndexAuthGuard(status, instance, setIsAutoSigningIn)

  if (!instance && isDetectingLocalInstance) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <PageMeta />
        <div className="flex items-center gap-3">
          <Spinner className="size-5" />
          <p className="text-sm text-muted-foreground">
            Detecting local daemon...
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
              Self-hosted mobile CI and app distribution platform.
              <br />
              Connect a backend instance to begin.
            </p>
          </div>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Instance registry
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Add a backend instance to start setup or connect to an
                already-configured daemon.
              </p>
              <Button
                onClick={() => setShowAddInstance(true)}
                className="w-full"
              >
                <DynamicLucideIcon icon={Add01Icon} />
                Add instance
              </Button>
            </CardContent>
          </Card>
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
          <p className="text-sm text-muted-foreground">
            Connecting to backend...
          </p>
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
        <ConfiguredDashboard
          userName={authUser?.email}
          runtimeMode={status.runtime_mode}
        />
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

function ConfiguredDashboard({
  userName,
  runtimeMode,
}: {
  userName?: string
  runtimeMode: RuntimeMode
}) {
  const navigate = useNavigate()
  const [triggerOpen, setTriggerOpen] = useState(false)
  const [triggerProjectId, setTriggerProjectId] = useState<string | undefined>()
  const authUser = useAuthStore((s) => s.user)
  const canWriteIntegrations = useHasPermission('integrations', 'write')
  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteBuilds = useHasPermission('builds', 'write')

  const projectsQuery = useProjects({ limit: 6 })
  const projects = useMemo(
    () => projectsQuery.data?.projects ?? [],
    [projectsQuery.data?.projects],
  )
  const integrationsQuery = useIntegrations()
  const runnersQuery = useRunners()
  const integrations = useMemo(
    () => integrationsQuery.data?.integrations ?? [],
    [integrationsQuery.data?.integrations],
  )
  const activeIntegrationsCount = useMemo(
    () =>
      integrations.filter((integration) => integration.status === 'active')
        .length,
    [integrations],
  )

  const recentBuildsQuery = useBuilds({ limit: 50 })
  const recentBuilds = useMemo(
    () => recentBuildsQuery.data?.builds ?? [],
    [recentBuildsQuery.data?.builds],
  )
  const { active: activeBuilds, recentCompleted: recentCompletedBuilds } =
    useMemo(() => selectDashboardBuilds(recentBuilds), [recentBuilds])
  const recentProjects = useMemo(
    () => selectDashboardProjects(projects),
    [projects],
  )
  const hasProjects = projects.length > 0
  const integrationsResolved =
    !integrationsQuery.isLoading && !integrationsQuery.error
  const noConnectedSources =
    runtimeMode === 'remote' &&
    integrationsResolved &&
    activeIntegrationsCount === 0
  const integrationConnectTo = '/settings/integrations'
  const noOnlineRunners =
    !!runnersQuery.data &&
    !runnersQuery.data.runners.some(
      (runner) => runner.status === 'online' || runner.status === 'busy',
    )
  const canActOnEveryProject =
    authUser?.role === 'owner' || authUser?.role === 'admin'
  const canTriggerProject = (projectId: string) => {
    const project = projects.find((candidate) => candidate.id === projectId)
    return (
      canWriteBuilds &&
      (canActOnEveryProject ||
        hasProjectPermission(project?.current_user_role, 'builds', 'write'))
    )
  }
  const canManageProject = (projectId: string) => {
    const project = projects.find((candidate) => candidate.id === projectId)
    return (
      canActOnEveryProject ||
      hasProjectPermission(project?.current_user_role, 'projects', 'write')
    )
  }
  const canShowRunBuild = hasProjects && !noOnlineRunners && canWriteBuilds

  // Derive last build status per project from recent builds
  const lastBuildByProject = useMemo(() => {
    const map = new Map<string, string>()
    for (const build of recentBuilds) {
      if (!map.has(build.project_id)) {
        map.set(build.project_id, build.status)
      }
    }
    return map
  }, [recentBuilds])

  function handleTriggerForProject(projectId: string) {
    setTriggerProjectId(() => projectId)
    setTriggerOpen(true)
  }

  function handleGlobalTrigger() {
    setTriggerProjectId(undefined)
    setTriggerOpen(true)
  }

  return (
    <PageLayout width="wide">
      <PageHeader
        title={userName ? `Welcome, ${userName.split('@')[0]}` : 'Dashboard'}
        description="Project overview and build activity."
        actions={
          canShowRunBuild ? (
            <Button
              onMouseEnter={() => void loadTriggerBuildDialog()}
              onFocus={() => void loadTriggerBuildDialog()}
              onClick={handleGlobalTrigger}
            >
              <DynamicLucideIcon icon={PlayIcon} />
              Run build
            </Button>
          ) : undefined
        }
      />

      {noOnlineRunners ? (
        <Alert variant="destructive">
          <AlertTitle>No runner is available</AlertTitle>
          <AlertDescription>
            Builds cannot run until a runner checks in. Verify that the Oore
            daemon is running on the runner host.
          </AlertDescription>
        </Alert>
      ) : null}

      {/* Active Builds */}
      {activeBuilds.length > 0 ? (
        <section className="space-y-2">
          <div className="flex items-center gap-2">
            <DynamicLucideIcon
              icon={Loading03Icon}
              size={14}
              className="animate-spin text-info"
            />
            <h2 className="text-sm font-medium text-muted-foreground">
              Active builds
            </h2>
            <Badge variant="secondary">{activeBuilds.length}</Badge>
          </div>
          <ItemGroup>
            {activeBuilds.map((build) => (
              <ActiveBuildBanner key={build.id} build={build} />
            ))}
          </ItemGroup>
        </section>
      ) : null}

      {/* Projects Grid */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-medium text-muted-foreground">
            Projects
          </h2>
          <Button
            variant="ghost"
            size="sm"
            render={<Link to="/projects" />}
            nativeButton={false}
          >
            View all
            <DynamicLucideIcon icon={ArrowRight01Icon} />
          </Button>
        </div>

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
        ) : projectsQuery.isLoading ? (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <Skeleton className="h-32" />
            <Skeleton className="h-32" />
            <Skeleton className="h-32" />
          </div>
        ) : projects.length === 0 ? (
          <DashboardGettingStarted
            canWriteIntegrations={canWriteIntegrations}
            canWriteProjects={canWriteProjects}
            integrationConnectTo={integrationConnectTo}
            noConnectedSources={noConnectedSources}
            runtimeMode={runtimeMode}
          />
        ) : (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {recentProjects.map((project) => (
              <ProjectCard
                key={project.id}
                canOpenSettings={canManageProject(project.id)}
                canTriggerBuild={
                  !noOnlineRunners && canTriggerProject(project.id)
                }
                project={project}
                lastBuildStatus={lastBuildByProject.get(project.id)}
                onPreloadTriggerBuild={() => void loadTriggerBuildDialog()}
                onTriggerBuild={handleTriggerForProject}
              />
            ))}
          </div>
        )}
      </section>

      <DashboardRecentBuilds
        builds={recentCompletedBuilds}
        error={recentBuildsQuery.error}
        isLoading={recentBuildsQuery.isLoading}
        onRetry={() => void recentBuildsQuery.refetch()}
        projects={projects}
      />

      {triggerOpen ? (
        <Suspense fallback={null}>
          <TriggerBuildDialog
            open
            onOpenChange={setTriggerOpen}
            fixedProjectId={triggerProjectId}
            description="Choose a project and pipeline to run a manual build."
            onBuildCreated={(buildId) => {
              void navigate({
                to: '/builds/$buildId',
                params: { buildId },
              })
            }}
          />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
