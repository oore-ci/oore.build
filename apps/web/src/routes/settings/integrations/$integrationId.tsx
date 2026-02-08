import { useEffect } from 'react'
import { createFileRoute, useNavigate, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
  Refresh01Icon,
  Setting07Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import {
  useDeleteIntegration,
  useGitLabAuthorize,
  useInstallations,
  useIntegration,
  useIntegrationRepos,
  useSyncInstallations,
} from '@/hooks/use-integrations'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
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
import { Skeleton } from '@/components/ui/skeleton'
import { Alert, AlertDescription } from '@/components/ui/alert'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/settings/integrations/$integrationId')({
  staticData: { breadcrumbLabel: 'Details' },
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

function IntegrationDetailPage() {
  const { integrationId } = Route.useParams()
  const search = useSearch({ from: '/settings/integrations/$integrationId' })
  const navigate = useNavigate()

  const { data: detail, isLoading, error } = useIntegration(integrationId)
  const { data: installationsData } = useInstallations(integrationId)
  const { data: reposData } = useIntegrationRepos(integrationId)
  const syncMutation = useSyncInstallations()
  const deleteMutation = useDeleteIntegration()
  const gitlabAuthorizeMutation = useGitLabAuthorize()

  useEffect(() => {
    const label = detail?.integration?.display_name
      ?? detail?.integration?.provider
      ?? 'Integration Details'
    document.title = webPageTitle(label)
  }, [detail?.integration?.display_name, detail?.integration?.provider])

  useEffect(() => {
    if (search.installed === 'true') {
      toast.success('GitHub App installed successfully')
      window.history.replaceState({}, '', `/settings/integrations/${integrationId}`)
    }
    if (search.gitlab === 'success') {
      toast.success('GitLab OAuth authorization completed')
      window.history.replaceState({}, '', `/settings/integrations/${integrationId}`)
    }
  }, [search.installed, search.gitlab, integrationId])

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
    const name = detail?.integration.display_name ?? detail?.integration.provider ?? 'integration'
    deleteMutation.mutate(integrationId, {
      onSuccess: () => {
        toast.success(`Disconnected ${name}`)
        void navigate({ to: '/settings/integrations' })
      },
      onError: (err) => {
        toast.error(`Failed to disconnect: ${err.message}`)
      },
    })
  }

  if (isLoading) {
    return (
      <PageLayout>
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-32 w-full" />
        <Skeleton className="h-48 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    return (
      <PageLayout>
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load integration: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!detail) return null

  const { integration } = detail
  const installations = installationsData?.installations ?? []
  const repositories = reposData?.repositories ?? []

  return (
    <PageLayout>
      <PageHeader
        title={integration.display_name ?? integration.provider}
        back={{ to: '/settings/integrations', label: 'Back to Integrations' }}
        meta={
          <>
            <Badge variant={getIntegrationStatusVariant(integration.status)}>
              {integration.status}
            </Badge>
            <Badge variant="outline">{integration.provider}</Badge>
          </>
        }
      />

      <Card>
        <CardHeader>
          <CardTitle>App Info</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
            <dt className="text-muted-foreground">Provider</dt>
            <dd>{integration.provider}</dd>
            <dt className="text-muted-foreground">Host URL</dt>
            <dd>{integration.host_url}</dd>
            <dt className="text-muted-foreground">Auth Mode</dt>
            <dd>{integration.auth_mode}</dd>
            {integration.app_id && (
              <>
                <dt className="text-muted-foreground">App ID</dt>
                <dd className="font-mono">{integration.app_id}</dd>
              </>
            )}
            <dt className="text-muted-foreground">Created</dt>
            <dd>{new Date(integration.created_at * 1000).toLocaleString()}</dd>
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Actions</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-3">
          {integration.provider === 'gitlab' &&
            integration.auth_mode === 'oauth_app' &&
            integration.status === 'inactive' && (
              <Button
                variant="outline"
                onClick={() =>
                  gitlabAuthorizeMutation.mutate({
                    integration_id: integrationId,
                    redirect_url: window.location.href,
                  })
                }
                disabled={gitlabAuthorizeMutation.isPending}
              >
                <HugeiconsIcon icon={LinkSquare02Icon} size={16} />
                {gitlabAuthorizeMutation.isPending
                  ? 'Redirecting...'
                  : 'Authorize on GitLab'}
              </Button>
            )}
          {integration.provider === 'github' && integration.app_slug && (
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
            >
              <HugeiconsIcon icon={Setting07Icon} size={16} />
              {installations.length > 0 ? 'Manage on GitHub' : 'Install on GitHub'}
            </Button>
          )}
          <Button
            variant="outline"
            onClick={handleSync}
            disabled={syncMutation.isPending}
          >
            <HugeiconsIcon icon={Refresh01Icon} size={16} />
            {syncMutation.isPending ? 'Syncing...' : 'Sync Installations'}
          </Button>
          <AlertDialog>
            <AlertDialogTrigger render={
              <Button variant="destructive">
                <HugeiconsIcon icon={Delete02Icon} size={16} />
                Disconnect
              </Button>
            } />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Disconnect integration?</AlertDialogTitle>
                <AlertDialogDescription>
                  This will remove the integration, all credentials, installations,
                  and repository links. Webhooks will stop working.
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
          <CardTitle>Installations ({installations.length})</CardTitle>
        </CardHeader>
        <CardContent>
          {installations.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No installations yet
              {integration.provider === 'github' && integration.app_slug
                ? ' — install your GitHub App to get started.'
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
                    <TableCell className="font-mono text-muted-foreground">{inst.external_id}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Repositories ({repositories.length})</CardTitle>
        </CardHeader>
        <CardContent>
          {repositories.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No repositories synced yet.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Repository</TableHead>
                  <TableHead>Default Branch</TableHead>
                  <TableHead>Visibility</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {repositories.map((repo) => (
                  <TableRow key={repo.id}>
                    <TableCell>{repo.full_name}</TableCell>
                    <TableCell className="font-mono text-muted-foreground">
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
          )}
        </CardContent>
      </Card>
    </PageLayout>
  )
}
