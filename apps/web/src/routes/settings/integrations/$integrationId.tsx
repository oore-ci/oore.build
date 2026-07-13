import { createFileRoute, useNavigate, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Copy01Icon,
  Delete02Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
  Refresh01Icon,
  Setting07Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'
import { useMountEffect } from '@/hooks/use-mount-effect'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBreadcrumbStore } from '@/stores/breadcrumb-store'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import {
  useDeleteIntegration,
  useGitLabAuthorize,
  useInstallations,
  useIntegration,
  useIntegrationRepos,
  useSyncInstallations,
} from '@/hooks/use-integrations'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { useExternalAccessNetworkSettings } from '@/hooks/use-artifact-storage'
import { gitLabPublicEndpoints } from '@/lib/gitlab-url'
import { PageMeta } from '@/lib/seo'
import RepositoryAvatar from '@/components/repository-avatar'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

export const Route = createFileRoute('/settings/integrations/$integrationId')({
  staticData: {
    breadcrumbLabel: 'Details',
    breadcrumbParent: { label: 'Sources', to: '/settings/integrations' },
  },
  validateSearch: (
    search: Record<string, unknown>,
  ): { installed?: string; gitlab?: string } => ({
    installed: (search.installed as string) || undefined,
    gitlab: (search.gitlab as string) || undefined,
  }),
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: IntegrationDetailPage,
})

function humanizeAuthMode(mode: string): string {
  const labels: Record<string, string> = {
    github_app_manifest: 'GitHub App (Manifest)',
    github_app: 'GitHub App',
    oauth_app: 'OAuth App',
    pat: 'Personal Access Token',
    personal_token: 'Personal Access Token',
  }
  return (
    labels[mode] ??
    mode.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
  )
}

function IntegrationDetailPage() {
  const { integrationId } = Route.useParams()
  const search = useSearch({ from: '/settings/integrations/$integrationId' })
  const navigate = useNavigate()

  const { data: detail, isLoading, error } = useIntegration(integrationId)
  const { data: networkSettings } = useExternalAccessNetworkSettings()
  const { data: installationsData } = useInstallations(integrationId)
  const { data: reposData } = useIntegrationRepos(integrationId)
  const syncMutation = useSyncInstallations()
  const deleteMutation = useDeleteIntegration()
  const gitlabAuthorizeMutation = useGitLabAuthorize()

  const setLabel = useBreadcrumbStore((s) => s.setLabel)

  const label =
    detail?.integration.display_name ??
    detail?.integration.provider ??
    'Source Details'

  useBreadcrumbLabel(
    setLabel,
    '/settings/integrations/$integrationId',
    detail?.integration.display_name ?? detail?.integration.provider,
  )

  useMountEffect(() => {
    if (search.installed === 'true') {
      toast.success('GitHub App installed successfully')
      window.history.replaceState(
        {},
        '',
        `/settings/integrations/${integrationId}`,
      )
    }
    if (search.gitlab === 'success') {
      toast.success('GitLab OAuth authorization completed')
      window.history.replaceState(
        {},
        '',
        `/settings/integrations/${integrationId}`,
      )
    }
  })

  function handleSync() {
    syncMutation.mutate(integrationId, {
      onSuccess: () => {
        toast.success('Installations synced')
      },
      onError: (err) => {
        toast.error(`Sync failed: ${err.message}`)
      },
    })
  }

  function handleDisconnect() {
    const name =
      detail?.integration.display_name ??
      detail?.integration.provider ??
      'source'
    deleteMutation.mutate(integrationId, {
      onSuccess: () => {
        toast.success(`Disconnected source: ${name}`)
        void navigate({ to: '/settings/integrations' })
      },
      onError: (err) => {
        toast.error(`Failed to disconnect: ${err.message}`)
      },
    })
  }

  if (isLoading) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-24 w-full" />
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
            Failed to load source: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!detail) return null

  const { integration } = detail
  const installations = installationsData?.installations ?? []
  const repositories = reposData?.repositories ?? []
  const providerLabel = integration.provider === 'gitlab' ? 'GitLab' : 'GitHub'
  const sourceDescription =
    integration.provider === 'gitlab'
      ? `GitLab source at ${integration.host_url}. Authorize, then sync projects to make them available in Oore.`
      : 'GitHub App installation and repository link state for this source connection.'
  const installationsLabel =
    integration.provider === 'gitlab' ? 'GitLab accounts' : 'Installations'
  const repositoriesLabel =
    integration.provider === 'gitlab' ? 'GitLab projects' : 'Repositories'
  const syncLabel =
    integration.provider === 'gitlab'
      ? 'Sync GitLab projects'
      : 'Sync installations'
  const { webhookUrl: gitLabWebhookUrl } = gitLabPublicEndpoints(
    networkSettings?.settings.public_url,
    window.location.origin,
  )
  const canSyncInstallations =
    integration.provider === 'github' || integration.provider === 'gitlab'

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
      <PageHeader
        title={integration.display_name ?? integration.provider}
        description={sourceDescription}
        meta={
          <>
            <Badge variant={getIntegrationStatusVariant(integration.status)}>
              {integration.status}
            </Badge>
            <Badge variant="outline">{providerLabel}</Badge>
            <span className="font-mono">{integration.id.slice(0, 8)}</span>
          </>
        }
      />

      <section className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Installations
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {installations.length}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Connected accounts
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Repositories
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {repositories.length}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Synced repositories
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Auth mode
            </p>
            <p className="mt-3 text-2xl font-bold tracking-tight">
              {humanizeAuthMode(integration.auth_mode)}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              Host: {integration.host_url}
            </p>
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Connection details
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableBody>
              <TableRow>
                <TableCell className="w-56 text-muted-foreground">
                  Provider
                </TableCell>
                <TableCell>{integration.provider}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">
                  Host URL
                </TableCell>
                <TableCell>{integration.host_url}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">
                  Auth mode
                </TableCell>
                <TableCell>{humanizeAuthMode(integration.auth_mode)}</TableCell>
              </TableRow>
              {integration.provider === 'gitlab' ? (
                <>
                  <TableRow>
                    <TableCell className="text-muted-foreground">
                      Webhook URL
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <code className="font-mono text-xs">
                          {gitLabWebhookUrl}
                        </code>
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label="Copy GitLab webhook URL"
                          title="Copy GitLab webhook URL"
                          onClick={() => {
                            void navigator.clipboard
                              .writeText(gitLabWebhookUrl)
                              .then(
                                () => toast.success('Webhook URL copied'),
                                () => toast.error('Could not copy webhook URL'),
                              )
                          }}
                        >
                          <HugeiconsIcon icon={Copy01Icon} />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell className="text-muted-foreground">
                      Last webhook delivery
                    </TableCell>
                    <TableCell>
                      {detail.last_webhook_at
                        ? new Date(
                            detail.last_webhook_at * 1000,
                          ).toLocaleString()
                        : 'Waiting for a test delivery'}
                    </TableCell>
                  </TableRow>
                </>
              ) : null}
              {integration.app_id ? (
                <TableRow>
                  <TableCell className="text-muted-foreground">
                    App ID
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {integration.app_id}
                  </TableCell>
                </TableRow>
              ) : null}
              <TableRow>
                <TableCell className="text-muted-foreground">Created</TableCell>
                <TableCell>
                  {new Date(integration.created_at * 1000).toLocaleString()}
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {integration.provider === 'gitlab' && !detail.last_webhook_at ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Webhook readiness is pending. In GitLab, add the copied URL and
            secret to each project, enable Push events, then send a test
            delivery. If it fails, confirm the backend URL is reachable from
            GitLab and that the secret still matches before refreshing this
            page. Reconnect the source if the saved secret has changed.
          </AlertDescription>
        </Alert>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Actions
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-2">
          {integration.provider === 'gitlab' &&
          integration.auth_mode === 'oauth_app' &&
          integration.status === 'inactive' ? (
            <Button
              onClick={() =>
                gitlabAuthorizeMutation.mutate(
                  {
                    integration_id: integrationId,
                    redirect_url: window.location.href,
                  },
                  {
                    onError: (authorizationError) =>
                      toast.error(
                        `GitLab authorization failed: ${authorizationError.message}`,
                      ),
                  },
                )
              }
              disabled={gitlabAuthorizeMutation.isPending}
            >
              <HugeiconsIcon icon={LinkSquare02Icon} size={16} />
              {gitlabAuthorizeMutation.isPending
                ? 'Redirecting...'
                : 'Authorize on GitLab'}
            </Button>
          ) : null}

          {integration.provider === 'github' && integration.app_slug ? (
            <Button
              variant="outline"
              render={
                <a
                  href={
                    installations.length > 0
                      ? `https://github.com/apps/${integration.app_slug}/installations/select_target`
                      : `https://github.com/apps/${integration.app_slug}/installations/new`
                  }
                  target="_blank"
                  rel="noopener noreferrer"
                />
              }
              nativeButton={false}
            >
              <HugeiconsIcon icon={Setting07Icon} />
              {installations.length > 0
                ? 'Manage on GitHub'
                : 'Install on GitHub'}
            </Button>
          ) : null}

          {integration.provider === 'gitlab' ? (
            <Button
              variant="outline"
              render={
                <a
                  href={integration.host_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label="Open GitLab in a new tab"
                />
              }
              nativeButton={false}
            >
              <HugeiconsIcon icon={Setting07Icon} />
              Open GitLab
            </Button>
          ) : null}

          {canSyncInstallations ? (
            <Button
              variant="outline"
              onClick={handleSync}
              disabled={syncMutation.isPending}
            >
              <HugeiconsIcon icon={Refresh01Icon} />
              {syncMutation.isPending ? 'Syncing...' : syncLabel}
            </Button>
          ) : null}

          <AlertDialog>
            <AlertDialogTrigger
              render={
                <Button variant="destructive">
                  <HugeiconsIcon icon={Delete02Icon} />
                  Disconnect
                </Button>
              }
            />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Disconnect source?</AlertDialogTitle>
                <AlertDialogDescription>
                  This removes credentials, installations, repository links, and
                  webhook behavior.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Cancel</AlertDialogCancel>
                <AlertDialogAction onClick={handleDisconnect}>
                  Disconnect
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            {installationsLabel} ({installations.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {installations.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No{' '}
              {integration.provider === 'gitlab'
                ? 'GitLab account'
                : 'installation'}{' '}
              yet
              {integration.provider === 'github' && integration.app_slug
                ? ' - install your GitHub App to get started.'
                : '.'}
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Account</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>External ID</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {installations.map((inst) => (
                  <TableRow key={inst.id}>
                    <TableCell>{inst.account_name}</TableCell>
                    <TableCell>{inst.account_type ?? '—'}</TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {inst.external_id}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            {repositoriesLabel} ({repositories.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          {repositories.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No{' '}
              {integration.provider === 'gitlab'
                ? 'GitLab projects'
                : 'repositories'}{' '}
              synced yet.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Repository</TableHead>
                  <TableHead>Default branch</TableHead>
                  <TableHead>Visibility</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {repositories.map((repo) => (
                  <TableRow key={repo.id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <RepositoryAvatar
                          fullName={repo.full_name}
                          avatarUrl={repo.avatar_url}
                        />
                        <span>{repo.full_name}</span>
                      </div>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {repo.default_branch ?? '—'}
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={repo.is_private ? 'secondary' : 'outline'}
                      >
                        {repo.is_private ? 'private' : 'public'}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </PageLayout>
  )
}
