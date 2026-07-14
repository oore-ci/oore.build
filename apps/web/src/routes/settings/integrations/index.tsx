import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  InformationCircleIcon,
  Link04Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'
import { useMountEffect } from '@/hooks/use-mount-effect'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useDeleteIntegration, useIntegrations } from '@/hooks/use-integrations'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { PageMeta } from '@/lib/seo'
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
import SetupHint from '@/components/setup-hint'
import { Skeleton } from '@/components/ui/skeleton'

export const Route = createFileRoute('/settings/integrations/')({
  staticData: { breadcrumbLabel: 'Sources' },
  validateSearch: (
    search: Record<string, unknown>,
  ): { github?: string; integration_id?: string } => ({
    github: (search.github as string) || undefined,
    integration_id: (search.integration_id as string) || undefined,
  }),
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: IntegrationsPage,
})

function IntegrationsPage() {
  const search = useSearch({ from: '/settings/integrations/' })
  const { data, isLoading, error } = useIntegrations()
  const {
    data: preferences,
    isLoading: preferencesLoading,
    error: preferencesError,
  } = useInstancePreferences()
  const deleteMutation = useDeleteIntegration()
  const runtimeMode = preferences?.preferences.runtime_mode
  const remoteEnabled = runtimeMode === 'remote'

  useMountEffect(() => {
    if (search.github === 'success') {
      toast.success('GitHub App connected successfully')
      window.history.replaceState({}, '', '/settings/integrations')
    }
  })

  function handleDisconnect(id: string, name: string) {
    deleteMutation.mutate(id, {
      onSuccess: () => {
        toast.success(`Disconnected source: ${name}`)
      },
      onError: (err) => {
        toast.error(`Failed to disconnect: ${err.message}`)
      },
    })
  }

  const integrations = data?.integrations ?? []

  return (
    <PageLayout width="wide">
      <PageMeta title="Sources" noindex />
      <PageHeader
        title="Sources"
        description="Source connections used to discover repositories and trigger builds."
      />

      {preferencesLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-2/3" />
          </CardContent>
        </Card>
      ) : preferencesError ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load access policy: {preferencesError.message}
          </AlertDescription>
        </Alert>
      ) : !remoteEnabled ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              External Access Required
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-sm text-muted-foreground">
              Source connections (GitHub/GitLab) are available only when the
              backend is in Remote mode. In Local Only mode, choose a local
              repository during project creation.
            </p>
            <SetupHint
              title="Local only path"
              items={[
                'Use Projects to create a project from a repository path available on the runner host.',
                'Switch to Remote mode only when browser users or external webhooks need to reach the backend.',
              ]}
            />
            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                render={<Link to="/settings/preferences" />}
                nativeButton={false}
              >
                Open preferences
              </Button>
              <Button render={<Link to="/projects" />} nativeButton={false}>
                Go to projects
              </Button>
            </div>
          </CardContent>
        </Card>
      ) : (
        <section className="grid gap-4 md:grid-cols-2">
          <Card className="h-full">
            <CardHeader>
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                GitHub Source
              </CardTitle>
            </CardHeader>
            <CardContent className="flex flex-1 flex-col gap-3">
              <p className="text-sm text-muted-foreground">
                Create and install a GitHub App to enable repository discovery
                and webhook events.
              </p>
              <SetupHint
                title="What GitHub will ask for"
                items={[
                  'Repository contents and metadata read access for checkout and repository discovery.',
                  'Pull request read access plus statuses/checks write access for CI feedback.',
                  'Webhook events for pushes and pull requests.',
                ]}
              />
              <Button
                className="mt-auto self-start"
                render={<Link to="/settings/integrations/github" />}
                nativeButton={false}
              >
                <HugeiconsIcon icon={Link04Icon} />
                Connect GitHub
              </Button>
            </CardContent>
          </Card>

          <Card className="h-full">
            <CardHeader>
              <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                GitLab Source
              </CardTitle>
            </CardHeader>
            <CardContent className="flex flex-1 flex-col gap-3">
              <p className="text-sm text-muted-foreground">
                Connect GitLab.com or a self-managed GitLab host through a
                personal access token or OAuth application.
              </p>
              <SetupHint
                title="Personal access token scopes"
                items={[
                  <span>
                    Use <code>read_user</code>, <code>read_api</code>, and{' '}
                    <code>read_repository</code>.
                  </span>,
                  <span>
                    Avoid full <code>api</code> unless a future GitLab write
                    feature explicitly needs it.
                  </span>,
                ]}
              />
              <Button
                className="mt-auto self-start"
                render={<Link to="/settings/integrations/gitlab" />}
                nativeButton={false}
              >
                <HugeiconsIcon icon={Link04Icon} />
                Connect GitLab
              </Button>
            </CardContent>
          </Card>
        </section>
      )}

      {!preferencesLoading && !preferencesError && !remoteEnabled ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Access mode is <code>local only</code>. GitHub/GitLab sources are
            disabled until the backend is switched to Remote mode from
            Preferences. Local repositories are selected in project creation.
          </AlertDescription>
        </Alert>
      ) : null}

      {isLoading ? (
        <Card size="sm">
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load sources: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error && remoteEnabled ? (
        <Card size="sm">
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Connected Sources
            </CardTitle>
          </CardHeader>
          <CardContent>
            {integrations.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No connected sources yet. Connect GitHub or GitLab above.
              </p>
            ) : (
              <div className="space-y-3">
                <p className="text-xs text-muted-foreground">
                  Click a source tile to open details.
                </p>
                {integrations.map((integration) => (
                  <div
                    key={integration.id}
                    className="group flex flex-col border border-border/60 bg-card transition-colors hover:border-primary/30 hover:bg-primary/5 sm:flex-row sm:items-start sm:justify-between sm:gap-3"
                  >
                    <Link
                      to="/settings/integrations/$integrationId"
                      params={{ integrationId: integration.id }}
                      className="min-w-0 flex-1 p-4 outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
                    >
                      <div className="space-y-2">
                        <div>
                          <p className="font-medium">
                            {integration.display_name ?? integration.provider}
                          </p>
                          <p className="font-mono text-xs text-muted-foreground">
                            {integration.id.slice(0, 8)}
                          </p>
                        </div>

                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant="outline">
                            {integration.provider}
                          </Badge>
                          <Badge variant="outline">
                            {integration.provider === 'local_git'
                              ? 'Single repo'
                              : 'Multi repo'}
                          </Badge>
                          <Badge
                            variant={getIntegrationStatusVariant(
                              integration.status,
                            )}
                          >
                            {integration.status}
                          </Badge>
                          <Badge variant="outline">
                            {integration.auth_mode}
                          </Badge>
                        </div>

                        <p className="truncate text-xs text-muted-foreground">
                          {integration.host_url}
                        </p>
                      </div>
                    </Link>

                    <div className="p-4 pt-0 sm:pl-0 sm:pt-4">
                      <AlertDialog>
                        <AlertDialogTrigger
                          render={
                            <Button variant="ghost" size="sm">
                              <HugeiconsIcon icon={Delete02Icon} />
                              Disconnect
                            </Button>
                          }
                        />
                        <AlertDialogContent>
                          <AlertDialogHeader>
                            <AlertDialogTitle>
                              Disconnect source?
                            </AlertDialogTitle>
                            <AlertDialogDescription>
                              This removes credentials, installations,
                              repository links, and webhook behavior.
                            </AlertDialogDescription>
                          </AlertDialogHeader>
                          <AlertDialogFooter>
                            <AlertDialogCancel>Cancel</AlertDialogCancel>
                            <AlertDialogAction
                              onClick={() =>
                                handleDisconnect(
                                  integration.id,
                                  integration.display_name ??
                                    integration.provider,
                                )
                              }
                              disabled={deleteMutation.isPending}
                            >
                              {deleteMutation.isPending
                                ? 'Disconnecting...'
                                : 'Disconnect'}
                            </AlertDialogAction>
                          </AlertDialogFooter>
                        </AlertDialogContent>
                      </AlertDialog>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      ) : null}
    </PageLayout>
  )
}
