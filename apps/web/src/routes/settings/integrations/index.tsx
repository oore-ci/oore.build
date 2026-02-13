import { useEffect } from 'react'
import { Link, createFileRoute, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
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
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

export const Route = createFileRoute('/settings/integrations/')({
  staticData: { breadcrumbLabel: 'Integrations' },
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
  const deleteMutation = useDeleteIntegration()

  useEffect(() => {
    if (search.github === 'success') {
      toast.success('GitHub App connected successfully')
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

  const integrations = data?.integrations ?? []

  return (
    <PageLayout width="wide">
      <PageMeta title="Integrations" noindex />
      <PageHeader
        title="Integrations"
        description="Provider connections for repository access and webhook triggers."
      />

      <section className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              GitHub
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-sm text-muted-foreground">
              Create and install a GitHub App to enable repository discovery and
              webhook events.
            </p>
            <Button render={<Link to="/settings/integrations/github" />}>
              <HugeiconsIcon icon={LinkSquare02Icon} size={16} />
              Connect GitHub
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              GitLab
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-sm text-muted-foreground">
              Connect gitlab.com or self-managed GitLab through OAuth or
              personal access token.
            </p>
            <Button
              variant="outline"
              render={<Link to="/settings/integrations/gitlab" />}
            >
              <HugeiconsIcon icon={LinkSquare02Icon} size={16} />
              Connect GitLab
            </Button>
          </CardContent>
        </Card>
      </section>

      {isLoading ? (
        <Card>
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
            Failed to load integrations: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Connected Integrations
            </CardTitle>
          </CardHeader>
          <CardContent>
            {integrations.length === 0 ? (
              <p className="py-6 text-sm text-muted-foreground">
                No integrations connected yet.
              </p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Name</TableHead>
                    <TableHead>Provider</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Host</TableHead>
                    <TableHead>Auth mode</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {integrations.map((integration) => (
                    <TableRow key={integration.id}>
                      <TableCell>
                        <div>
                          <p className="font-medium">
                            {integration.display_name ?? integration.provider}
                          </p>
                          <p className="font-mono text-xs text-muted-foreground">
                            {integration.id.slice(0, 8)}
                          </p>
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline">{integration.provider}</Badge>
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={getIntegrationStatusVariant(
                            integration.status,
                          )}
                        >
                          {integration.status}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {integration.host_url}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {integration.auth_mode}
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex justify-end gap-2">
                          <Button
                            variant="outline"
                            size="sm"
                            render={
                              <Link
                                to="/settings/integrations/$integrationId"
                                params={{ integrationId: integration.id }}
                              />
                            }
                          >
                            Open
                          </Button>
                          <AlertDialog>
                            <AlertDialogTrigger
                              render={
                                <Button variant="ghost" size="sm">
                                  <HugeiconsIcon
                                    icon={Delete02Icon}
                                    size={16}
                                  />
                                  Disconnect
                                </Button>
                              }
                            />
                            <AlertDialogContent>
                              <AlertDialogHeader>
                                <AlertDialogTitle>
                                  Disconnect integration?
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
                                >
                                  Disconnect
                                </AlertDialogAction>
                              </AlertDialogFooter>
                            </AlertDialogContent>
                          </AlertDialog>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      ) : null}
    </PageLayout>
  )
}
