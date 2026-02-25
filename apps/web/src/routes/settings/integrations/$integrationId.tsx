import { Match, Switch, createEffect, Show } from 'solid-js'
import { Link, createFileRoute, useNavigate } from '@tanstack/solid-router'
import {
  Delete02Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
  Refresh01Icon,
  Setting07Icon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBreadcrumbStore } from '@/stores/breadcrumb-store'
import {
  useDeleteIntegration,
  useGitLabAuthorize,
  useInstallations,
  useIntegration,
  useIntegrationRepos,
  useSyncInstallations,
} from '@/hooks/use-integrations'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { HugeIcon } from '@/components/huge-icon'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/settings/integrations/$integrationId')({
  staticData: { breadcrumbLabel: 'Details' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: IntegrationDetailPage,
})

function IntegrationDetailPage() {
  const params = Route.useParams()
  const navigate = useNavigate()
  const integrationId = () => params().integrationId

  const detailQuery = useIntegration(integrationId())
  const installationsQuery = useInstallations(integrationId())
  const reposQuery = useIntegrationRepos(integrationId())
  const syncMutation = useSyncInstallations()
  const deleteMutation = useDeleteIntegration()
  const gitlabAuthorizeMutation = useGitLabAuthorize()

  const setLabel = useBreadcrumbStore((state) => state.setLabel)

  const label = () =>
    detailQuery.data?.integration.display_name ??
    detailQuery.data?.integration.provider ??
    'Source Details'

  createEffect(() => {
    const integration = detailQuery.data?.integration
    if (!integration) return
    setLabel()(
      '/settings/integrations/$integrationId',
      integration.display_name ?? integration.provider,
    )
  })

  createEffect(() => {
    if (typeof window === 'undefined') return
    const search = new URLSearchParams(window.location.search)
    let changed = false
    if (search.get('installed') === 'true') {
      toast.success('GitHub App installed successfully')
      search.delete('installed')
      changed = true
    }
    if (search.get('gitlab') === 'success') {
      toast.success('GitLab OAuth authorization completed')
      search.delete('gitlab')
      changed = true
    }
    if (changed) {
      const next = search.toString()
      window.history.replaceState(
        {},
        '',
        next
          ? `/settings/integrations/${integrationId()}?${next}`
          : `/settings/integrations/${integrationId()}`,
      )
    }
  })

  const handleSync = () => {
    syncMutation.mutate(integrationId(), {
      onSuccess: () => toast.success('Installations synced'),
      onError: (error) =>
        toast.error(error instanceof Error ? error.message : 'Sync failed'),
    })
  }

  const handleDisconnect = () => {
    const integration = detailQuery.data?.integration
    const name = integration?.display_name ?? integration?.provider ?? 'source'
    const confirmed = window.confirm(`Disconnect source "${name}"?`)
    if (!confirmed) return

    deleteMutation.mutate(integrationId(), {
      onSuccess: () => {
        toast.success(`Disconnected source: ${name}`)
        void navigate({ to: '/settings/integrations' })
      },
      onError: (error) =>
        toast.error(
          error instanceof Error ? error.message : 'Failed to disconnect',
        ),
    })
  }

  const integration = () => detailQuery.data?.integration
  const installations = () => installationsQuery.data?.installations ?? []
  const repositories = () => reposQuery.data?.repositories ?? []
  const canSyncInstallations = () => {
    const provider = integration()?.provider
    return provider === 'github' || provider === 'gitlab'
  }

  return (
    <PageLayout class="space-y-4">
      <PageMeta title={label()} noindex />

      <Switch>
        <Match when={detailQuery.isLoading}>
          <Skeleton class="h-8 w-56" />
          <Skeleton class="h-24 w-full" />
          <Skeleton class="h-56 w-full" />
        </Match>

        <Match when={detailQuery.error}>
          <Alert variant="destructive">
            <HugeIcon icon={InformationCircleIcon} size={16} />
            <AlertDescription>
              Failed to load source: {detailQuery.error?.message}
            </AlertDescription>
          </Alert>
        </Match>

        <Match when>
          <PageHeader
            title={integration()?.display_name ?? integration()?.provider ?? 'Source'}
            description="Installation and repository link state for this source connection."
            actions={
              <div class="flex items-center gap-2">
                <Badge
                  variant={getIntegrationStatusVariant(integration()?.status ?? '')}
                >
                  {integration()?.status}
                </Badge>
                <Badge variant="outline">{integration()?.provider}</Badge>
              </div>
            }
          />

          <div>
            <Link to="/settings/integrations">
              <Button variant="outline" size="sm">
                Back to Sources
              </Button>
            </Link>
          </div>

          <section class="grid gap-4 md:grid-cols-3">
            <Card>
              <CardHeader>
                <CardTitle class="text-sm font-medium">Installations</CardTitle>
              </CardHeader>
              <CardContent>
                <p class="text-2xl font-semibold tracking-tight">
                  {installations().length}
                </p>
                <p class="text-xs text-muted-foreground">Connected accounts</p>
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle class="text-sm font-medium">Repositories</CardTitle>
              </CardHeader>
              <CardContent>
                <p class="text-2xl font-semibold tracking-tight">
                  {repositories().length}
                </p>
                <p class="text-xs text-muted-foreground">Synced repositories</p>
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle class="text-sm font-medium">Auth mode</CardTitle>
              </CardHeader>
              <CardContent>
                <p class="text-sm font-medium">{integration()?.auth_mode}</p>
                <p class="text-xs text-muted-foreground">
                  Host: {integration()?.host_url}
                </p>
              </CardContent>
            </Card>
          </section>

          <Card>
            <CardHeader>
              <CardTitle class="text-base">Actions</CardTitle>
            </CardHeader>
            <CardContent class="flex flex-wrap gap-2">
              <Show
                when={
                  integration()?.provider === 'gitlab' &&
                  integration()?.auth_mode === 'oauth_app' &&
                  integration()?.status === 'inactive'
                }
              >
                <Button
                  variant="outline"
                  onClick={() =>
                    gitlabAuthorizeMutation.mutate({
                      integration_id: integrationId(),
                      redirect_url: window.location.href,
                    })
                  }
                  disabled={gitlabAuthorizeMutation.isPending}
                >
                  <HugeIcon icon={LinkSquare02Icon} size={16} />
                  {gitlabAuthorizeMutation.isPending
                    ? 'Redirecting...'
                    : 'Authorize on GitLab'}
                </Button>
              </Show>

              <Show when={integration()?.provider === 'github' && integration()?.app_slug}>
                <a
                  href={
                    installations().length > 0
                      ? `https://github.com/apps/${integration()?.app_slug}/installations/select_target`
                      : `https://github.com/apps/${integration()?.app_slug}/installations/new`
                  }
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  <Button variant="outline">
                    <HugeIcon icon={Setting07Icon} size={16} />
                    {installations().length > 0
                      ? 'Manage on GitHub'
                      : 'Install on GitHub'}
                  </Button>
                </a>
              </Show>

              <Show when={canSyncInstallations()}>
                <Button
                  variant="outline"
                  onClick={handleSync}
                  disabled={syncMutation.isPending}
                >
                  <HugeIcon icon={Refresh01Icon} size={16} />
                  {syncMutation.isPending ? 'Syncing...' : 'Sync Installations'}
                </Button>
              </Show>

              <Button
                variant="destructive"
                disabled={deleteMutation.isPending}
                onClick={handleDisconnect}
              >
                <HugeIcon icon={Delete02Icon} size={16} />
                Disconnect
              </Button>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle class="text-base">
                Installations ({installations().length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Show
                when={installations().length > 0}
                fallback={
                  <p class="text-sm text-muted-foreground">
                    No installations yet.
                  </p>
                }
              >
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Account</TableHead>
                      <TableHead>Type</TableHead>
                      <TableHead>External ID</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {installations().map((installation) => (
                      <TableRow>
                        <TableCell>{installation.account_name}</TableCell>
                        <TableCell>{installation.account_type ?? '—'}</TableCell>
                        <TableCell class="font-mono text-xs text-muted-foreground">
                          {installation.external_id}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </Show>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle class="text-base">
                Repositories ({repositories().length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Show
                when={repositories().length > 0}
                fallback={
                  <p class="text-sm text-muted-foreground">
                    No repositories synced yet.
                  </p>
                }
              >
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Repository</TableHead>
                      <TableHead>Default branch</TableHead>
                      <TableHead>Visibility</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {repositories().map((repo) => (
                      <TableRow>
                        <TableCell>{repo.full_name}</TableCell>
                        <TableCell class="font-mono text-xs text-muted-foreground">
                          {repo.default_branch ?? '—'}
                        </TableCell>
                        <TableCell>
                          <Badge variant={repo.is_private ? 'secondary' : 'outline'}>
                            {repo.is_private ? 'private' : 'public'}
                          </Badge>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </Show>
            </CardContent>
          </Card>
        </Match>
      </Switch>
    </PageLayout>
  )
}
