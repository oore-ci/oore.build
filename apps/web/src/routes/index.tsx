import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { useMemo, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowRight01Icon,
  Loading03Icon,
  PlayIcon,
  Refresh01Icon,
  WifiDisconnected04Icon,
} from '@hugeicons/core-free-icons'

import type { RuntimeMode } from '@/lib/types'
import { useIndexAuthGuard } from '@/hooks/use-index-auth-guard'
import { useMountEffect } from '@/hooks/use-mount-effect'
import ActiveBuildBanner from '@/components/active-build-banner'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import ProjectCard from '@/components/project-card'
import TriggerBuildDialog from '@/components/trigger-build-dialog'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Spinner } from '@/components/ui/spinner'
import { useBuilds } from '@/hooks/use-builds'
import { useIntegrations } from '@/hooks/use-integrations'
import { useHasPermission } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { getSetupStatus } from '@/lib/api'
import { getStatusVariant } from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
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
  const [isAutoLocalSigningIn, setIsAutoLocalSigningIn] = useState(false)
  const autoDetectAttemptedRef = useRef(false)
  const authUser = useAuthStore((s) => s.user)

  useMountEffect(() => {
    if (instance || autoDetectAttemptedRef.current) return
    if (!isLoopbackHostname(window.location.hostname)) return

    autoDetectAttemptedRef.current = true
    setIsDetectingLocalInstance(true)

    void detectReachableLocalDaemonUrl()
      .then((detectedUrl) => {
        if (!detectedUrl) return
        const store = useInstanceStore.getState()
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

  useIndexAuthGuard(status, instance, setIsAutoLocalSigningIn)

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

  if (isAutoLocalSigningIn) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <PageMeta />
        <div className="flex items-center gap-3">
          <Spinner className="size-5" />
          <p className="text-sm text-muted-foreground">Signing in locally...</p>
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
                <HugeiconsIcon icon={Add01Icon} size={16} />
                Add Instance
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
    const queryClient = useQueryClient()
    return (
      <div className="flex flex-1 flex-col items-center justify-center p-6">
        <PageMeta title="Connection Failed" />
        <div className="w-full max-w-md space-y-6">
          <div className="space-y-2 text-center text-destructive">
            <HugeiconsIcon
              icon={WifiDisconnected04Icon}
              size={48}
              className="mx-auto"
            />
            <h1 className="text-2xl font-bold tracking-tight">
              Backend Unreachable
            </h1>
            <p className="text-sm text-muted-foreground">
              Oore UI cannot connect to the daemon at:
            </p>
            <code className="block bg-destructive/10 px-3 py-2 text-sm font-mono tracking-tight text-destructive">
              {instance.url}
            </code>
          </div>

          <Alert variant="destructive">
            <AlertTitle>Connection failed</AlertTitle>
            <AlertDescription className="space-y-3">
              <p>
                Unable to reach the oore daemon. Make sure{' '}
                <code className="bg-muted px-1 py-0.5 text-xs">oored</code> is
                running.
              </p>
              <div className="flex items-center gap-2 pt-1">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    void queryClient.invalidateQueries({
                      queryKey: [instance.id, 'setup-status'],
                    })
                  }}
                  className="bg-background text-foreground hover:bg-muted"
                >
                  <HugeiconsIcon icon={Refresh01Icon} size={14} />
                  Retry Connection
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setShowAddInstance(true)}
                >
                  Edit Instances
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        </div>

        <AddInstanceDialog
          open={showAddInstance}
          onOpenChange={setShowAddInstance}
        />
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

  const activeBuildsQuery = useBuilds({ limit: 10 })
  const activeBuilds = useMemo(() => {
    const all = activeBuildsQuery.data?.builds ?? []
    return all.filter((b) => b.status === 'queued' || b.status === 'running')
  }, [activeBuildsQuery.data?.builds])

  const recentBuildsQuery = useBuilds({ limit: 50 })
  const recentBuilds = useMemo(
    () => recentBuildsQuery.data?.builds ?? [],
    [recentBuildsQuery.data?.builds],
  )
  const hasProjects = projects.length > 0
  const integrationsResolved =
    !integrationsQuery.isLoading && !integrationsQuery.error
  const noConnectedSources =
    runtimeMode === 'remote' &&
    integrationsResolved &&
    activeIntegrationsCount === 0
  const integrationConnectTo = '/settings/integrations'
  const canShowRunBuild = canWriteBuilds && hasProjects

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
    setTriggerProjectId(projectId)
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
              <HugeiconsIcon icon={PlayIcon} size={16} />
              Run Build
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
            <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
          </Button>
        </div>

        {projectsQuery.isLoading ? (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <Skeleton className="h-32" />
            <Skeleton className="h-32" />
            <Skeleton className="h-32" />
          </div>
        ) : projects.length === 0 ? (
          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium">
                Getting Started
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                <ol className="space-y-3 text-sm">
                  {runtimeMode === 'remote' && noConnectedSources ? (
                    <li className="flex items-start gap-3">
                      <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
                        1
                      </span>
                      <div className="space-y-1.5">
                        <p className="font-medium">Connect a source</p>
                        <p className="text-xs text-muted-foreground">
                          Link GitHub or GitLab to import repositories and
                          enable webhook-triggered builds.
                        </p>
                        {canWriteIntegrations ? (
                          <Button
                            variant="outline"
                            size="sm"
                            render={<Link to={integrationConnectTo} />}
                            nativeButton={false}
                          >
                            Connect Source
                          </Button>
                        ) : (
                          <p className="text-xs text-muted-foreground">
                            Ask an admin to connect a source.
                          </p>
                        )}
                      </div>
                    </li>
                  ) : null}
                  <li className="flex items-start gap-3">
                    <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
                      {runtimeMode === 'remote' && noConnectedSources
                        ? '2'
                        : '1'}
                    </span>
                    <div className="space-y-1.5">
                      <p className="font-medium">Create a project</p>
                      <p className="text-xs text-muted-foreground">
                        {runtimeMode === 'local'
                          ? 'Point to a local Flutter repository to get started.'
                          : 'Pick a repository from a connected source or use a local path.'}
                      </p>
                      {canWriteProjects ? (
                        <Button
                          size="sm"
                          render={
                            <Link to="/projects" search={{ openCreate: '1' }} />
                          }
                          nativeButton={false}
                        >
                          <HugeiconsIcon icon={Add01Icon} size={14} />
                          Create Project
                        </Button>
                      ) : (
                        <p className="text-xs text-muted-foreground">
                          Ask a developer or admin to create a project.
                        </p>
                      )}
                    </div>
                  </li>
                  <li className="flex items-start gap-3">
                    <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
                      {runtimeMode === 'remote' && noConnectedSources
                        ? '3'
                        : '2'}
                    </span>
                    <div className="space-y-1.5">
                      <p className="font-medium">Add a pipeline</p>
                      <p className="text-xs text-muted-foreground">
                        Configure which platforms to build (Android, iOS, macOS)
                        and signing settings.
                      </p>
                    </div>
                  </li>
                  <li className="flex items-start gap-3">
                    <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
                      {runtimeMode === 'remote' && noConnectedSources
                        ? '4'
                        : '3'}
                    </span>
                    <div className="space-y-1.5">
                      <p className="font-medium">Run your first build</p>
                      <p className="text-xs text-muted-foreground">
                        Trigger a build manually or push to your repository to
                        start automatically.
                      </p>
                    </div>
                  </li>
                </ol>
              </div>
            </CardContent>
          </Card>
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

      {/* Recent Builds */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Recent Builds
          </h2>
          <Button
            variant="ghost"
            size="sm"
            render={<Link to="/builds" />}
            nativeButton={false}
          >
            View all
            <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
          </Button>
        </div>

        {recentBuildsQuery.isLoading ? (
          <Card>
            <CardContent className="space-y-3">
              <Skeleton className="h-8 w-full" />
              <Skeleton className="h-8 w-full" />
              <Skeleton className="h-8 w-full" />
            </CardContent>
          </Card>
        ) : recentBuilds.length === 0 ? (
          <Card>
            <CardContent>
              <div className="py-4 text-center">
                <p className="text-sm text-muted-foreground">No builds yet.</p>
              </div>
            </CardContent>
          </Card>
        ) : (
          <Card>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Build</TableHead>
                    <TableHead>Project</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Branch</TableHead>
                    <TableHead>Commit</TableHead>
                    <TableHead>When</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {recentBuilds.map((build) => {
                    const projectName =
                      projects.find((p) => p.id === build.project_id)?.name ??
                      build.project_id.slice(0, 8)
                    return (
                      <TableRow
                        key={build.id}
                        className="group cursor-pointer"
                        onClick={() =>
                          void navigate({
                            to: '/builds/$buildId',
                            params: { buildId: build.id },
                          })
                        }
                      >
                        <TableCell className="font-mono text-sm group-hover:underline">
                          #{build.build_number}
                        </TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {projectName}
                        </TableCell>
                        <TableCell>
                          <Badge variant={getStatusVariant(build.status)}>
                            {build.status}
                          </Badge>
                        </TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {build.branch ?? 'n/a'}
                        </TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {build.commit_sha
                            ? build.commit_sha.slice(0, 8)
                            : 'n/a'}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {relativeTime(build.created_at)}
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        )}
      </section>

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
