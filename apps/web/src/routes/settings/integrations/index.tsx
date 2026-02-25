import { createEffect, For, Show } from 'solid-js'
import { Link, createFileRoute } from '@tanstack/solid-router'
import {
  Delete02Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useDeleteIntegration, useIntegrations } from '@/hooks/use-integrations'
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { HugeIcon } from '@/components/huge-icon'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/settings/integrations/')({
  staticData: { breadcrumbLabel: 'Sources' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: IntegrationsPage,
})

function IntegrationsPage() {
  const integrationsQuery = useIntegrations()
  const preferences = useInstancePreferences()
  const deleteMutation = useDeleteIntegration()
  const runtimeMode = preferences.data?.preferences.runtime_mode ?? 'local'
  const remoteEnabled = runtimeMode === 'remote'

  createEffect(() => {
    if (typeof window === 'undefined') return
    const params = new URLSearchParams(window.location.search)
    if (params.get('github') === 'success') {
      toast.success('GitHub App connected successfully')
      params.delete('github')
      const next = params.toString()
      window.history.replaceState(
        {},
        '',
        next ? `/settings/integrations?${next}` : '/settings/integrations',
      )
    }
  })

  const integrations = () => integrationsQuery.data?.integrations ?? []

  const handleDisconnect = (id: string, name: string) => {
    const confirmed = window.confirm(`Disconnect source "${name}"?`)
    if (!confirmed) return

    deleteMutation.mutate(id, {
      onSuccess: () => {
        toast.success(`Disconnected source: ${name}`)
      },
      onError: (error) => {
        toast.error(error instanceof Error ? error.message : 'Disconnect failed')
      },
    })
  }

  return (
    <PageLayout class="space-y-4">
      <PageMeta title="Sources" noindex />
      <PageHeader
        title="Sources"
        description="Source connections used to discover repositories and trigger builds."
      />

      <Show when={!remoteEnabled}>
        <Card>
          <CardHeader>
            <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              External Access Required
            </CardTitle>
          </CardHeader>
          <CardContent class="space-y-3">
            <p class="text-sm text-muted-foreground">
              Source connections (GitHub/GitLab) are available only when
              External Access is enabled. In Local Only mode, choose a local
              directory during project creation.
            </p>
            <div class="flex flex-wrap gap-2">
              <Link to="/settings/preferences">
                <Button variant="outline">Open Preferences</Button>
              </Link>
              <Link to="/projects">
                <Button>Go To Projects</Button>
              </Link>
            </div>
          </CardContent>
        </Card>
      </Show>

      <Show when={remoteEnabled}>
        <section class="grid gap-4 md:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                GitHub Source
              </CardTitle>
            </CardHeader>
            <CardContent class="space-y-3">
              <p class="text-sm text-muted-foreground">
                Create and install a GitHub App to enable repository discovery
                and webhook events.
              </p>
              <Link to="/settings/integrations/github">
                <Button>
                  <HugeIcon icon={LinkSquare02Icon} size={16} />
                  Connect GitHub
                </Button>
              </Link>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                GitLab Source
              </CardTitle>
            </CardHeader>
            <CardContent class="space-y-3">
              <p class="text-sm text-muted-foreground">
                Connect gitlab.com or self-managed GitLab through OAuth or
                personal access token.
              </p>
              <Link to="/settings/integrations/gitlab">
                <Button variant="outline">
                  <HugeIcon icon={LinkSquare02Icon} size={16} />
                  Connect GitLab
                </Button>
              </Link>
            </CardContent>
          </Card>
        </section>
      </Show>

      <Show when={!remoteEnabled}>
        <Alert>
          <HugeIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Access mode is <code>local only</code>. GitHub/GitLab sources are
            disabled until External Access is enabled from Preferences. Local
            directories are selected in project creation.
          </AlertDescription>
        </Alert>
      </Show>

      <Show when={integrationsQuery.isLoading}>
        <Card>
          <CardContent class="space-y-3 pt-6">
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
          </CardContent>
        </Card>
      </Show>

      <Show when={integrationsQuery.error}>
        <Alert variant="destructive">
          <HugeIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load sources: {integrationsQuery.error?.message}
          </AlertDescription>
        </Alert>
      </Show>

      <Show
        when={!integrationsQuery.isLoading && !integrationsQuery.error && remoteEnabled}
      >
        <Card>
          <CardHeader>
            <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Connected Sources
            </CardTitle>
          </CardHeader>
          <CardContent>
            <Show
              when={integrations().length > 0}
              fallback={
                <p class="py-6 text-sm text-muted-foreground">
                  No sources connected yet.
                </p>
              }
            >
              <div class="space-y-3">
                <p class="text-xs text-muted-foreground">
                  Click a source tile to open details.
                </p>
                <For each={integrations()}>
                  {(integration) => (
                    <div class="flex items-start justify-between gap-3 rounded-md border border-border/60 bg-card p-4 transition-colors hover:border-primary/30 hover:bg-primary/5">
                      <Link
                        to="/settings/integrations/$integrationId"
                        params={{ integrationId: integration.id }}
                        class="min-w-0 flex-1"
                      >
                        <div class="space-y-2">
                          <div>
                            <p class="font-medium">
                              {integration.display_name ?? integration.provider}
                            </p>
                            <p class="font-mono text-xs text-muted-foreground">
                              {integration.id.slice(0, 8)}
                            </p>
                          </div>
                          <div class="flex flex-wrap items-center gap-2">
                            <Badge variant="outline">{integration.provider}</Badge>
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
                          <p class="truncate text-xs text-muted-foreground">
                            {integration.host_url}
                          </p>
                        </div>
                      </Link>

                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={deleteMutation.isPending}
                        onClick={() =>
                          handleDisconnect(
                            integration.id,
                            integration.display_name ?? integration.provider,
                          )
                        }
                        aria-label="Disconnect source"
                      >
                        <HugeIcon icon={Delete02Icon} size={16} />
                      </Button>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </CardContent>
        </Card>
      </Show>
    </PageLayout>
  )
}
