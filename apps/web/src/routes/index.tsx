import { For, Match, Show, Switch, createEffect, createMemo, createSignal } from 'solid-js'
import { Link, createFileRoute, useNavigate } from '@tanstack/solid-router'
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
import { HugeIcon } from '@/components/huge-icon'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
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
import { Spinner } from '@/components/ui/spinner'
import { useBuilds } from '@/hooks/use-builds'
import { useIntegrations } from '@/hooks/use-integrations'
import { useHasPermission } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { getSetupStatus, localLogin } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'
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

function relativeTime(epochSeconds: number): string {
  const diff = Math.floor(Date.now() / 1000) - epochSeconds
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

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
  const navigate = useNavigate()
  const instance = useActiveInstance()
  const setupStatus = useSetupStatus()

  const token = useAuthStore((state) => state.token)
  const expiresAt = useAuthStore((state) => state.expiresAt)
  const authUser = useAuthStore((state) => state.user)
  const clearAuth = useAuthStore((state) => state.clearAuth)
  const setAuth = useAuthStore((state) => state.setAuth)

  const [showAddInstance, setShowAddInstance] = createSignal(false)
  const [isDetectingLocalInstance, setIsDetectingLocalInstance] =
    createSignal(false)
  const [isAutoLocalSigningIn, setIsAutoLocalSigningIn] = createSignal(false)
  const [autoDetectAttempted, setAutoDetectAttempted] = createSignal(false)
  const [autoLocalLoginInstanceId, setAutoLocalLoginInstanceId] =
    createSignal<string | null>(null)

  createEffect(() => {
    if (instance() || autoDetectAttempted()) return
    if (!isLoopbackHostname(window.location.hostname)) return

    setAutoDetectAttempted(true)
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
      .finally(() => {
        setIsDetectingLocalInstance(false)
      })
  })

  createEffect(() => {
    const status = setupStatus.data
    const activeInstance = instance()
    if (!status || !activeInstance) return

    if (status.setup_mode && status.runtime_mode !== 'local') {
      void navigate({ to: '/setup' })
      return
    }

    const now = Math.floor(Date.now() / 1000)
    const hasValidToken =
      !!token() && expiresAt() != null && (expiresAt() as number) > now

    if (status.runtime_mode === 'local') {
      const uiIsLoopback = isLoopbackHostname(window.location.hostname)
      const backendIsLoopback = isLoopbackHostname(
        resolveBackendHostname(activeInstance.url),
      )

      if (!uiIsLoopback || !backendIsLoopback) {
        if (!hasValidToken) {
          clearAuth()()
          void navigate({ to: '/login' })
        }
        return
      }

      if (hasValidToken) return
      if (autoLocalLoginInstanceId() === activeInstance.id) return

      setAutoLocalLoginInstanceId(activeInstance.id)
      setIsAutoLocalSigningIn(true)
      clearAuth()()

      void localLogin(activeInstance.url, {})
        .then((response) => {
          if (!response.user.user_id || !response.user.role) {
            throw new Error('Incomplete user profile received from server')
          }

          setAuth()(
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
          setAutoLocalLoginInstanceId(null)
          clearAuth()()
          void navigate({ to: '/login' })
        })
        .finally(() => {
          setIsAutoLocalSigningIn(false)
        })
      return
    }

    if (status.is_configured && !hasValidToken) {
      clearAuth()()
      void navigate({ to: '/login' })
    }
  })

  return (
    <Switch>
      <Match when={!instance() && isDetectingLocalInstance()}>
        <div class="flex flex-1 items-center justify-center">
          <PageMeta />
          <div class="flex items-center gap-3">
            <Spinner class="size-5" />
            <p class="text-sm text-muted-foreground">Detecting local daemon...</p>
          </div>
        </div>
      </Match>

      <Match when={isAutoLocalSigningIn()}>
        <div class="flex flex-1 items-center justify-center">
          <PageMeta />
          <div class="flex items-center gap-3">
            <Spinner class="size-5" />
            <p class="text-sm text-muted-foreground">Signing in locally...</p>
          </div>
        </div>
      </Match>

      <Match when={!instance()}>
        <div class="flex flex-1 flex-col items-center justify-center p-6">
          <PageMeta />
          <div class="w-full max-w-md space-y-8">
            <div class="space-y-3 text-center">
              <div class="mx-auto flex size-14 items-center justify-center">
                <img src="/logo.svg" alt="Oore CI logo" class="size-full" />
              </div>
              <h1 class="text-3xl font-bold tracking-tight">Oore CI</h1>
              <p class="text-sm text-muted-foreground">
                Self-hosted mobile CI and app distribution platform.
                <br />
                Connect a backend instance to begin.
              </p>
            </div>

            <Card>
              <CardHeader>
                <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                  Instance Registry
                </CardTitle>
              </CardHeader>
              <CardContent class="space-y-4">
                <p class="text-sm text-muted-foreground">
                  Add a backend instance to start setup or connect to an
                  already-configured daemon.
                </p>
                <Button
                  onClick={() => setShowAddInstance(true)}
                  class="w-full"
                >
                  <HugeIcon icon={Add01Icon} size={16} />
                  Add Instance
                </Button>
              </CardContent>
            </Card>
          </div>

          <AddInstanceDialog
            open={showAddInstance()}
            onOpenChange={setShowAddInstance}
          />
        </div>
      </Match>

      <Match when={!!instance() && setupStatus.isLoading}>
        <div class="flex flex-1 items-center justify-center">
          <PageMeta />
          <div class="flex items-center gap-3">
            <Spinner class="size-5" />
            <p class="text-sm text-muted-foreground">Connecting to backend...</p>
          </div>
        </div>
      </Match>

      <Match when={!!instance() && !!setupStatus.error}>
        <div class="flex flex-1 items-center justify-center p-6">
          <PageMeta />
          <div class="w-full max-w-md">
            <Alert variant="destructive">
              <AlertTitle>Connection failed</AlertTitle>
              <AlertDescription>
                Unable to reach the oore daemon. Make sure{' '}
                <code class="bg-muted px-1 py-0.5 text-xs">oored</code> is
                running.
              </AlertDescription>
            </Alert>
          </div>
        </div>
      </Match>

      <Match when={!!setupStatus.data?.is_configured}>
        <>
          <PageMeta />
          <ConfiguredDashboard
            userName={authUser()?.email}
            runtimeMode={setupStatus.data?.runtime_mode ?? 'local'}
          />
        </>
      </Match>

      <Match when>
        <div class="flex flex-1 items-center justify-center">
          <PageMeta />
          <div class="flex items-center gap-3">
            <Spinner class="size-5" />
            <p class="text-sm text-muted-foreground">Loading...</p>
          </div>
        </div>
      </Match>
    </Switch>
  )
}

function ConfiguredDashboard(props: {
  userName?: string
  runtimeMode: RuntimeMode
}) {
  const navigate = useNavigate()
  const canWriteIntegrations = useHasPermission('integrations', 'write')
  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteBuilds = useHasPermission('builds', 'write')

  const projectsQuery = useProjects({ limit: 50 })
  const integrationsQuery = useIntegrations()
  const activeBuildsQuery = useBuilds({ limit: 10 })
  const recentBuildsQuery = useBuilds({ limit: 50 })

  const projects = createMemo(() => projectsQuery.data?.projects ?? [])
  const integrations = createMemo(
    () => integrationsQuery.data?.integrations ?? [],
  )
  const activeBuilds = createMemo(() => {
    const all = activeBuildsQuery.data?.builds ?? []
    return all.filter((build) => build.status === 'queued' || build.status === 'running')
  })
  const recentBuilds = createMemo(() => recentBuildsQuery.data?.builds ?? [])

  const activeIntegrationsCount = createMemo(
    () => integrations().filter((integration) => integration.status === 'active').length,
  )

  const hasProjects = createMemo(() => projects().length > 0)
  const integrationsResolved = createMemo(
    () => !integrationsQuery.isLoading && !integrationsQuery.error,
  )
  const noConnectedSources = createMemo(
    () =>
      props.runtimeMode === 'remote' &&
      integrationsResolved() &&
      activeIntegrationsCount() === 0,
  )

  const canShowRunBuild = createMemo(() => canWriteBuilds && hasProjects())

  const lastBuildByProject = createMemo(() => {
    const map = new Map<string, string>()
    for (const build of recentBuilds()) {
      if (!map.has(build.project_id)) {
        map.set(build.project_id, build.status)
      }
    }
    return map
  })

  function handleTriggerForProject(_projectId: string) {
    void navigate({ to: '/builds' })
  }

  function handleGlobalTrigger() {
    void navigate({ to: '/builds' })
  }

  return (
    <PageLayout width="wide">
      <PageHeader
        title={
          props.userName
            ? `Welcome, ${props.userName.split('@')[0]}`
            : 'Dashboard'
        }
        description="Project overview and build activity."
        actions={
          canShowRunBuild() ? (
            <Button onClick={handleGlobalTrigger}>
              <HugeIcon icon={PlayIcon} size={16} />
              Run Build
            </Button>
          ) : undefined
        }
      />

      <Show when={activeBuilds().length > 0}>
        <section class="space-y-2">
          <div class="flex items-center gap-2">
            <HugeIcon
              icon={Loading03Icon}
              size={14}
              class="animate-spin text-info"
            />
            <h2 class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Active Builds
            </h2>
            <Badge variant="info">{activeBuilds().length}</Badge>
          </div>

          <div class="space-y-1">
            <For each={activeBuilds()}>
              {(build) => <ActiveBuildBanner build={build} />}
            </For>
          </div>
        </section>
      </Show>

      <section class="space-y-3">
        <div class="flex items-center justify-between">
          <h2 class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Projects
          </h2>
          <Link to="/projects">
            <Button variant="ghost" size="sm">
              View all
              <HugeIcon icon={ArrowRight01Icon} size={14} />
            </Button>
          </Link>
        </div>

        <Show
          when={!projectsQuery.isLoading}
          fallback={
            <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              <Skeleton class="h-32" />
              <Skeleton class="h-32" />
              <Skeleton class="h-32" />
            </div>
          }
        >
          <Show
            when={projects().length > 0}
            fallback={
              <Card>
                <CardContent class="space-y-3 py-8 text-center">
                  <p class="text-sm text-muted-foreground">
                    {noConnectedSources()
                      ? 'Create a project from a local repository path, or connect a source to pick from synced repositories.'
                      : 'No projects yet.'}
                  </p>
                  <div class="flex flex-col items-center justify-center gap-2 sm:flex-row">
                    <Show
                      when={canWriteProjects}
                      fallback={
                        <p class="text-xs text-muted-foreground">
                          Owner/Admin/Developer required to create projects.
                        </p>
                      }
                    >
                      <Link to="/projects" search={{ openCreate: '1' }}>
                        <Button>
                          <HugeIcon icon={Add01Icon} size={14} />
                          Create Project
                        </Button>
                      </Link>
                    </Show>

                    <Show when={noConnectedSources()}>
                      <Show
                        when={canWriteIntegrations}
                        fallback={
                          <p class="text-xs text-muted-foreground">
                            Owner/Admin required to connect a source.
                          </p>
                        }
                      >
                        <Link to="/settings/integrations">
                          <Button variant="outline">Connect Source</Button>
                        </Link>
                      </Show>
                    </Show>
                  </div>
                </CardContent>
              </Card>
            }
          >
            <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              <For each={projects()}>
                {(project) => (
                  <ProjectCard
                    project={project}
                    lastBuildStatus={lastBuildByProject().get(project.id)}
                    onTriggerBuild={handleTriggerForProject}
                  />
                )}
              </For>
            </div>
          </Show>
        </Show>
      </section>

      <section class="space-y-3">
        <div class="flex items-center justify-between">
          <h2 class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Recent Builds
          </h2>
          <Link to="/builds">
            <Button variant="ghost" size="sm">
              View all
              <HugeIcon icon={ArrowRight01Icon} size={14} />
            </Button>
          </Link>
        </div>

        <Show
          when={!recentBuildsQuery.isLoading}
          fallback={
            <Card>
              <CardContent class="space-y-3">
                <Skeleton class="h-8 w-full" />
                <Skeleton class="h-8 w-full" />
                <Skeleton class="h-8 w-full" />
              </CardContent>
            </Card>
          }
        >
          <Show
            when={recentBuilds().length > 0}
            fallback={
              <Card>
                <CardContent class="py-8 text-center">
                  <p class="text-sm text-muted-foreground">No builds yet.</p>
                </CardContent>
              </Card>
            }
          >
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
                    <For each={recentBuilds()}>
                      {(build) => (
                        <TableRow>
                          <TableCell class="font-mono text-sm">
                            <Link
                              to="/builds/$buildId"
                              params={{ buildId: build.id }}
                              class="hover:underline"
                            >
                              #{build.build_number}
                            </Link>
                          </TableCell>
                          <TableCell>
                            <Badge variant={getStatusVariant(build.status)}>
                              {build.status}
                            </Badge>
                          </TableCell>
                          <TableCell class="font-mono text-xs text-muted-foreground">
                            {build.branch ?? 'n/a'}
                          </TableCell>
                          <TableCell class="font-mono text-xs text-muted-foreground">
                            {build.commit_sha
                              ? build.commit_sha.slice(0, 8)
                              : 'n/a'}
                          </TableCell>
                          <TableCell class="text-xs text-muted-foreground">
                            {relativeTime(build.created_at)}
                          </TableCell>
                        </TableRow>
                      )}
                    </For>
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </Show>
        </Show>
      </section>
    </PageLayout>
  )
}
