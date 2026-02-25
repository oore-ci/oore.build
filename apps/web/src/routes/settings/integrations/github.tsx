import { createFileRoute } from '@tanstack/solid-router'
import {
  ArrowRight01Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useGitHubAppStart } from '@/hooks/use-integrations'
import { PageMeta } from '@/lib/seo'
import { useActiveInstance } from '@/stores/instance-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table'
import { HugeIcon } from '@/components/huge-icon'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/settings/integrations/github')({
  staticData: { breadcrumbLabel: 'GitHub' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: GitHubSetupPage,
})

function GitHubSetupPage() {
  const instance = useActiveInstance()
  const startMutation = useGitHubAppStart()
  const preferences = useInstancePreferences()
  const remoteEnabled = preferences.data?.preferences.runtime_mode === 'remote'

  const backendUrl = () => instance()?.url ?? ''
  const webhookUrl = () => `${backendUrl()}/v1/webhooks/github`
  const redirectUrl = () =>
    `${window.location.origin}/settings/integrations`

  const handleConnect = () => {
    if (!remoteEnabled) return
    startMutation.mutate(
      { webhook_url: webhookUrl(), redirect_url: redirectUrl() },
      {
        onSuccess: (data) => {
          window.location.href = data.create_url
        },
        onError: (error) => {
          toast.error(
            error instanceof Error
              ? error.message
              : 'Failed to start GitHub setup',
          )
        },
      },
    )
  }

  return (
    <PageLayout class="space-y-4">
      <PageMeta title="Connect GitHub Source" noindex />
      <PageHeader
        title="Connect GitHub Source"
        description="Generate and install a GitHub App source for repository access and webhook delivery."
      />

      <section class="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle class="text-base">Connection flow</CardTitle>
          </CardHeader>
          <CardContent class="space-y-3">
            <p class="text-sm text-muted-foreground">
              Oore creates a GitHub App manifest, redirects you to GitHub, then
              returns here after install.
            </p>
            <Button
              onClick={handleConnect}
              disabled={startMutation.isPending || !remoteEnabled}
            >
              <HugeIcon icon={LinkSquare02Icon} size={16} />
              {!remoteEnabled
                ? 'External Access Required'
                : startMutation.isPending
                  ? 'Starting...'
                  : 'Create GitHub App'}
              {!startMutation.isPending ? (
                <HugeIcon icon={ArrowRight01Icon} size={14} />
              ) : null}
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle class="text-base">Generated endpoints</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableBody>
                <TableRow>
                  <TableCell class="w-44 text-muted-foreground">
                    Webhook URL
                  </TableCell>
                  <TableCell class="font-mono text-xs">
                    {webhookUrl()}
                  </TableCell>
                </TableRow>
                <TableRow>
                  <TableCell class="text-muted-foreground">
                    Redirect URL
                  </TableCell>
                  <TableCell class="font-mono text-xs">
                    {redirectUrl()}
                  </TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </section>

      <Alert>
        <HugeIcon icon={InformationCircleIcon} size={16} />
        <AlertDescription>
          {remoteEnabled
            ? 'After GitHub installation, you will return to Sources with the connection status updated.'
            : 'GitHub source connections are disabled in Local Only mode. Enable External Access in Preferences to continue.'}
        </AlertDescription>
      </Alert>
    </PageLayout>
  )
}
