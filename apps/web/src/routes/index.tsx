import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowRight01Icon,
  CheckmarkCircle02Icon,
  Folder02Icon,
  GitBranchIcon,
  InformationCircleIcon,
  Setting07Icon,
} from '@hugeicons/core-free-icons'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Spinner } from '@/components/ui/spinner'
import { useSetupStatus } from '@/hooks/use-setup'
import { webPageTitle } from '@/lib/seo'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'
import AddInstanceDialog from '@/components/AddInstanceDialog'

export const Route = createFileRoute('/')({
  staticData: { breadcrumbLabel: 'Dashboard' },
  component: IndexPage,
})

function IndexPage() {
  const instance = useActiveInstance()
  const { data: status, isLoading, error } = useSetupStatus()
  const navigate = useNavigate()
  const [showAddInstance, setShowAddInstance] = useState(false)
  const authToken = useAuthStore((s) => s.token)
  const authExpiresAt = useAuthStore((s) => s.expiresAt)
  const authUser = useAuthStore((s) => s.user)
  const clearAuth = useAuthStore((s) => s.clearAuth)

  useEffect(() => {
    document.title = webPageTitle()
  }, [])

  useEffect(() => {
    if (status?.setup_mode) {
      void navigate({ to: '/setup' })
    }
  }, [status?.setup_mode, navigate])

  useEffect(() => {
    if (status?.is_configured) {
      const now = Math.floor(Date.now() / 1000)
      const valid = !!authToken && authExpiresAt != null && authExpiresAt > now
      if (!valid) {
        clearAuth()
        void navigate({ to: '/login' })
      }
    }
  }, [status?.is_configured, authToken, authExpiresAt, clearAuth, navigate])

  if (!instance) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center p-6">
        <div className="w-full max-w-md space-y-8">
          <div className="space-y-3 text-center">
            <div className="mx-auto flex size-14 items-center justify-center border-2 border-primary/20 bg-primary/5">
              <img src="/logo.svg" alt="oore.build logo" className="size-7" />
            </div>
            <h1 className="text-3xl font-bold tracking-tight">
              oore.build
            </h1>
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
                Add a backend instance to start setup or connect to an already-configured daemon.
              </p>
              <Button onClick={() => setShowAddInstance(true)} className="w-full">
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
        <div className="flex items-center gap-3">
          <Spinner className="size-5" />
          <p className="text-sm text-muted-foreground">Connecting to backend...</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <div className="w-full max-w-md">
          <Alert variant="destructive">
            <AlertTitle>Connection failed</AlertTitle>
            <AlertDescription>
              Unable to reach the oore daemon. Make sure{' '}
              <code className="bg-muted px-1 py-0.5 text-xs">oored</code> is running.
            </AlertDescription>
          </Alert>
        </div>
      </div>
    )
  }

  if (status?.is_configured) {
    const isAdmin = authUser?.role === 'owner' || authUser?.role === 'admin'

    return (
      <PageLayout width="wide">
        <PageHeader
          title="Dashboard"
          description="Operational overview for this connected instance."
          meta={
            <>
              <Badge variant="success" className="gap-1">
                <HugeiconsIcon icon={CheckmarkCircle02Icon} size={12} />
                Instance ready
              </Badge>
              <span className="font-mono text-[11px]">{status.instance_id}</span>
              {authUser?.role ? <Badge variant="outline">{authUser.role}</Badge> : null}
            </>
          }
          actions={
            <>
              <Button variant="outline" render={<Link to="/projects" />}>
                Projects
              </Button>
              <Button render={<Link to="/builds" />}>
                Build Queue
              </Button>
            </>
          }
        />

        <section className="grid gap-4 md:grid-cols-3">
          <Card>
            <CardContent>
              <div className="flex items-center justify-between">
                <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Daemon</p>
                <Badge variant="success">online</Badge>
              </div>
              <p className="mt-3 text-2xl font-bold tracking-tight">Connected</p>
              <p className="mt-1 text-xs text-muted-foreground">Authenticated and responding</p>
            </CardContent>
          </Card>
          <Card>
            <CardContent>
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Setup state</p>
              <p className="mt-3 text-2xl font-bold tracking-tight">Ready</p>
              <p className="mt-1 text-xs text-muted-foreground">OIDC and owner bootstrap complete</p>
            </CardContent>
          </Card>
          <Card>
            <CardContent>
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Operator</p>
              <p className="mt-3 truncate text-2xl font-bold tracking-tight">{authUser?.email ?? 'Unknown'}</p>
              <p className="mt-1 text-xs text-muted-foreground">Role-based actions enforced per route</p>
            </CardContent>
          </Card>
        </section>

        <section className="grid gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">Quick Actions</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <QuickAction
                to="/projects"
                icon={Folder02Icon}
                title="Manage projects"
                description="Create and maintain build-ready repositories and pipeline configs."
              />
              <QuickAction
                to="/builds"
                icon={GitBranchIcon}
                title="Inspect builds"
                description="Open run details, stream logs, and download generated artifacts."
              />
              {isAdmin ? (
                <QuickAction
                  to="/settings/runners"
                  icon={Setting07Icon}
                  title="Review runners"
                  description="Track runner health and rename non-embedded runners."
                />
              ) : null}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">System Notes</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <Alert>
                <HugeiconsIcon icon={InformationCircleIcon} size={16} />
                <AlertDescription>
                  Build execution starts only after a runner heartbeats online and claims queued work.
                </AlertDescription>
              </Alert>
              {isAdmin ? (
                <Button variant="outline" className="w-full" render={<Link to="/settings/integrations" />}>
                  <HugeiconsIcon icon={Setting07Icon} size={16} />
                  Configure integrations
                </Button>
              ) : null}
            </CardContent>
          </Card>
        </section>
      </PageLayout>
    )
  }

  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="flex items-center gap-3">
        <Spinner className="size-5" />
        <p className="text-sm text-muted-foreground">Loading...</p>
      </div>
    </div>
  )
}

function QuickAction({
  to,
  icon,
  title,
  description,
}: {
  to: '/projects' | '/builds' | '/settings/runners'
  icon: typeof Folder02Icon
  title: string
  description: string
}) {
  return (
    <Link
      to={to}
      className="group flex items-center justify-between gap-4 border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/30 hover:bg-primary/5"
    >
      <div className="flex items-center gap-4">
        <div className="flex size-9 shrink-0 items-center justify-center border bg-muted/40 text-muted-foreground transition-colors group-hover:border-primary/30 group-hover:text-primary">
          <HugeiconsIcon icon={icon} size={16} />
        </div>
        <div className="min-w-0">
          <p className="text-sm font-medium">{title}</p>
          <p className="text-xs text-muted-foreground">{description}</p>
        </div>
      </div>
      <HugeiconsIcon icon={ArrowRight01Icon} size={16} className="shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5" />
    </Link>
  )
}
