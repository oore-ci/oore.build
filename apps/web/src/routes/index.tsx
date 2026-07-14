import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { useMemo, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowRight01Icon,
  Loading03Icon,
  PlayIcon,
} from '@hugeicons/core-free-icons'

import type { RuntimeMode } from '@/lib/types'
import { useIndexAuthGuard } from '@/hooks/use-index-auth-guard'
import { useMountEffect } from '@/hooks/use-mount-effect'
import ActiveBuildBanner from '@/components/active-build-banner'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import ProjectCard from '@/components/project-card'
import TriggerBuildDialog from '@/components/trigger-build-dialog'
import {
  DashboardGettingStarted,
  DashboardRecentBuilds,
} from '@/components/dashboard-sections'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Spinner } from '@/components/ui/spinner'
import { useBuilds } from '@/hooks/use-builds'
import { useIntegrations } from '@/hooks/use-integrations'
import { useHasPermission } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useRunners } from '@/hooks/use-runners'
import { useSetupStatus } from '@/hooks/use-setup'
import { getSetupStatus } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import { isManagedFrontend } from '@/lib/managed-frontend'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'

export const Route = createFileRoute('/')({
  staticData: { breadcrumbLabel: 'Dashboard' },
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
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                Instance Registry
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
                <HugeiconsIcon icon={Add01Icon} />
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
  const [showWelcome, setShowWelcome] = useState(() => {
    try {
      return !localStorage.getItem('oore_welcomed')
    } catch {
      return false
    }
  })

  function dismissWelcome() {
    setShowWelcome(false)
    try {
      localStorage.setItem('oore_welcomed', '1')
    } catch {
      // ignore
    }
  }

  const canWriteIntegrations = useHasPermission('integrations', 'write')
  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteBuilds = useHasPermission('builds', 'write')

  const projectsQuery = useProjects({ limit: 50 })
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
  const activeBuilds = useMemo(
    () =>
      recentBuilds.filter(
        (build) => build.status === 'queued' || build.status === 'running',
      ),
    [recentBuilds],
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
  const canShowRunBuild = canWriteBuilds && hasProjects && !noOnlineRunners

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
            <Button onClick={handleGlobalTrigger}>
              <HugeiconsIcon icon={PlayIcon} />
              Run build
            </Button>
          ) : undefined
        }
      />

      {showWelcome ? (
        <Alert>
          <AlertTitle>
            Welcome to Oore CI
            {userName ? `, ${userName.split('@')[0]}` : ''}!
          </AlertTitle>
          <AlertDescription className="flex items-start justify-between gap-4">
            <span>
              {authUser?.role === 'qa_viewer'
                ? 'You have view-only access. Browse projects and download build artifacts from the Builds page.'
                : authUser?.role === 'developer'
                  ? 'You can create projects, configure pipelines, and trigger builds. Start by exploring the Projects page.'
                  : 'You have full admin access. Manage users, runners, and integrations from the sidebar.'}
            </span>
            <Button
              variant="outline"
              size="sm"
              className="shrink-0"
              onClick={dismissWelcome}
            >
              Dismiss
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

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
            <HugeiconsIcon
              icon={Loading03Icon}
              size={14}
              className="animate-spin text-info"
            />
            <h2 className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Active Builds
            </h2>
            <Badge variant="info">{activeBuilds.length}</Badge>
          </div>
          <div className="space-y-1">
            {activeBuilds.map((build) => (
              <ActiveBuildBanner key={build.id} build={build} />
            ))}
          </div>
        </section>
      ) : null}

      {/* Projects Grid */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Projects
          </h2>
          <Button
            variant="ghost"
            size="sm"
            render={<Link to="/projects" />}
            nativeButton={false}
          >
            View all
            <HugeiconsIcon icon={ArrowRight01Icon} />
          </Button>
        </div>

        {projectsQuery.isLoading ? (
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
            {projects.map((project) => (
              <ProjectCard
                key={project.id}
                project={project}
                lastBuildStatus={lastBuildByProject.get(project.id)}
                onTriggerBuild={handleTriggerForProject}
              />
            ))}
          </div>
        )}
      </section>

      <DashboardRecentBuilds
        builds={recentBuilds}
        isLoading={recentBuildsQuery.isLoading}
        onOpenBuild={(buildId) =>
          void navigate({
            to: '/builds/$buildId',
            params: { buildId },
          })
        }
        projects={projects}
      />


      <TriggerBuildDialog
        open={triggerOpen}
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
    </PageLayout>
  )
}
