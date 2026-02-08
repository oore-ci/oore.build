import { useEffect } from 'react'
import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Delete02Icon, InformationCircleIcon } from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { useDeleteIntegration, useIntegrations } from '@/hooks/use-integrations'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
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
import { getIntegrationStatusVariant } from '@/lib/status-variants'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/settings/integrations/')({
  staticData: { breadcrumbLabel: 'Integrations' },
  validateSearch: (search: Record<string, unknown>): { github?: string; integration_id?: string } => ({
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
  const deleteMutation = useDeleteIntegration()

  useEffect(() => {
    document.title = webPageTitle('Integrations')
  }, [])

  // Show success toast when redirected back from GitHub App creation
  useEffect(() => {
    if (search.github === 'success') {
      toast.success('GitHub App connected successfully')
      // Clean the URL params without navigation
      window.history.replaceState({}, '', '/settings/integrations')
    }
  }, [search.github])

  function handleDisconnect(id: string, name: string) {
    deleteMutation.mutate(id, {
      onSuccess: () => {
        toast.success(`Disconnected ${name}`)
      },
      onError: (err) => {
        toast.error(`Failed to disconnect: ${err.message}`)
      },
    })
  }

  return (
    <PageLayout>
      <PageHeader
        title="Integrations"
        description="Connect your Git providers to enable build triggers."
      />

      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>GitHub</CardTitle>
            <CardDescription>
              Connect a GitHub App to access repositories and receive webhooks.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button render={<Link to="/settings/integrations/github" />}>
              Add GitHub Integration
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>GitLab</CardTitle>
            <CardDescription>
              Connect a GitLab instance via OAuth or personal access token.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button render={<Link to="/settings/integrations/gitlab" />}>
              Add GitLab Integration
            </Button>
          </CardContent>
        </Card>
      </div>

      <div className="space-y-4">
        <h2 className="text-lg font-medium">Connected Integrations</h2>

        {isLoading && (
          <div className="space-y-3">
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
          </div>
        )}

        {error && (
          <Alert variant="destructive">
            <HugeiconsIcon icon={InformationCircleIcon} size={16} />
            <AlertDescription>
              Failed to load integrations: {error.message}
            </AlertDescription>
          </Alert>
        )}

        {data && data.integrations.length === 0 && (
          <p className="text-sm text-muted-foreground">
            No integrations connected yet.
          </p>
        )}

        {data?.integrations.map((integration) => (
          <Link
            key={integration.id}
            to="/settings/integrations/$integrationId"
            params={{ integrationId: integration.id }}
            className="group/integration block"
          >
            <Card size="sm" className="py-0 transition-colors group-hover/integration:bg-muted/50">
              <CardContent className="flex items-center justify-between py-3">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="font-medium group-hover/integration:text-foreground">
                      {integration.display_name ?? integration.provider}
                    </span>
                    <Badge variant={getIntegrationStatusVariant(integration.status)}>
                      {integration.status}
                    </Badge>
                    <Badge variant="outline">{integration.provider}</Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {integration.host_url} &middot; {integration.auth_mode}
                  </p>
                </div>
                <AlertDialog>
                  <AlertDialogTrigger render={
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={(e: React.MouseEvent) => e.preventDefault()}
                    >
                      <HugeiconsIcon icon={Delete02Icon} size={16} />
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
                      <AlertDialogAction
                        onClick={() =>
                          handleDisconnect(
                            integration.id,
                            integration.display_name ?? integration.provider,
                          )
                        }
                      >
                        Disconnect
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              </CardContent>
            </Card>
          </Link>
        ))}
      </div>
    </PageLayout>
  )
}
