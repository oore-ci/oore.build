import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useMemo, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowRight01Icon,
  Loading03Icon,
  PlayIcon,
} from '@hugeicons/core-free-icons'

import type { RuntimeMode } from '@/lib/types'
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
import { getSetupStatus, localLogin } from '@/lib/api'
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

function resolveBackendHostname(rawUrl: string): string {
  const trimmed = rawUrl.trim()
  if (!trimmed) return window.location.hostname
  try {
    return new URL(trimmed).hostname
  } catch {
    return ''
  }
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
  const navigate = useNavigate()
  const [showAddInstance, setShowAddInstance] = useState(false)
  const [isDetectingLocalInstance, setIsDetectingLocalInstance] =
    useState(false)
  const [isAutoLocalSigningIn, setIsAutoLocalSigningIn] = useState(false)
  const autoDetectAttemptedRef = useRef(false)
  const autoLocalLoginInstanceRef = useRef<string | null>(null)
  const authToken = useAuthStore((s) => s.token)
  const authExpiresAt = useAuthStore((s) => s.expiresAt)
  const authUser = useAuthStore((s) => s.user)
  const clearAuth = useAuthStore((s) => s.clearAuth)
  const setAuth = useAuthStore((s) => s.setAuth)

  useEffect(() => {
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
  }, [instance])

  useEffect(() => {
    if (!status || !instance) return

    if (status.setup_mode && status.runtime_mode !== 'local') {
      void navigate({ to: '/setup' })
      return
    }

    const now = Math.floor(Date.now() / 1000)
    const hasValidToken =
      !!authToken && authExpiresAt != null && authExpiresAt > now

    if (status.runtime_mode === 'local') {
      const uiIsLoopback = isLoopbackHostname(window.location.hostname)
      const backendIsLoopback = isLoopbackHostname(
        resolveBackendHostname(instance.url),
      )

      if (!uiIsLoopback || !backendIsLoopback) {
        if (!hasValidToken) {
          clearAuth()
          void navigate({ to: '/login' })
        }
        return
      }

      if (hasValidToken) return
      if (autoLocalLoginInstanceRef.current === instance.id) return

      autoLocalLoginInstanceRef.current = instance.id
      setIsAutoLocalSigningIn(true)
      clearAuth()
      void localLogin(instance.url, {})
        .then((response) => {
          if (!response.user.user_id || !response.user.role) {
            throw new Error('Incomplete user profile received from server')
          }
          setAuth(
            response.session_token,
            response.expires_at,
            {
              email: response.user.email,
              oidc_subject: response.user.oidc_subject,
              user_id: response.user.user_id,
              role: response.user.role,
              avatar_url: response.user.avatar_url,
            },
            'local',
          )
        })
        .catch(() => {
          autoLocalLoginInstanceRef.current = null
          clearAuth()
          void navigate({ to: '/login' })
        })
        .finally(() => {
          setIsAutoLocalSigningIn(false)
        })
      return
    }

    if (status.is_configured && !hasValidToken) {
      clearAuth()
      void navigate({ to: '/login' })
    }
  }, [status, instance, authToken, authExpiresAt, clearAuth, setAuth, navigate])

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
              <img
                src="/logo.svg"
                alt="Oore CI logo"
                className="size-full"
              />
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
            <CardContent>
              <div className="space-y-3 py-4 text-center">
              <p className="text-sm text-muted-foreground">
                {noConnectedSources
                  ? 'Create a project from a local repository path, or connect a source to pick from synced repositories.'
                  : 'No projects yet.'}
              </p>
              <div className="flex flex-col items-center justify-center gap-2 sm:flex-row">
                {canWriteProjects ? (
                  <Button
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
                    Owner/Admin/Developer required to create projects.
                  </p>
                )}

                {noConnectedSources ? (
                  canWriteIntegrations ? (
                    <Button
                      variant="outline"
                      render={<Link to={integrationConnectTo} />}
                      nativeButton={false}
                    >
                      Connect Source
                    </Button>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      Owner/Admin required to connect a source.
                    </p>
                  )
                ) : null}
              </div>
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
                    <TableHead>Status</TableHead>
                    <TableHead>Branch</TableHead>
                    <TableHead>Commit</TableHead>
                    <TableHead>When</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {recentBuilds.map((build) => (
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
                  ))}
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
