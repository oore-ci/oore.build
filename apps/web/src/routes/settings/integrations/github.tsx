import { createFileRoute } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

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
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table'

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
  const { data: preferences } = useInstancePreferences()
  const remoteEnabled = preferences?.preferences.runtime_mode === 'remote'

  const backendUrl = instance?.url ?? ''
  const webhookUrl = `${backendUrl}/v1/webhooks/github`
  const redirectUrl = `${window.location.origin}/settings/integrations`

  function handleConnect() {
    if (!remoteEnabled) return
    startMutation.mutate(
      { webhook_url: webhookUrl, redirect_url: redirectUrl },
      {
        onSuccess: (data) => {
          window.location.href = data.create_url
        },
        onError: (err) => {
          toast.error(`Failed to start GitHub setup: ${err.message}`)
        },
      },
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Connect GitHub Source" noindex />
      <PageHeader
        title="Connect GitHub Source"
        description="Generate and install a GitHub App source for repository access and webhook delivery."
        back={{ to: '/settings/integrations', label: 'Sources' }}
      />

      <section className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Connection flow
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-sm text-muted-foreground">
              oore creates a GitHub App manifest, redirects you to GitHub, then
              returns here after install.
            </p>
            <Button
              onClick={handleConnect}
              disabled={startMutation.isPending || !remoteEnabled}
            >
              <HugeiconsIcon icon={LinkSquare02Icon} size={16} />
              {!remoteEnabled
                ? 'External Access Required'
                : startMutation.isPending
                  ? 'Starting...'
                  : 'Create GitHub App'}
              {!startMutation.isPending ? (
                <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
              ) : null}
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Generated endpoints
            </CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableBody>
                <TableRow>
                  <TableCell className="w-44 text-muted-foreground">
                    Webhook URL
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {webhookUrl}
                  </TableCell>
                </TableRow>
                <TableRow>
                  <TableCell className="text-muted-foreground">
                    Redirect URL
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {redirectUrl}
                  </TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </section>

      <Alert>
        <HugeiconsIcon icon={InformationCircleIcon} size={16} />
        <AlertDescription>
          {remoteEnabled
            ? 'After GitHub installation, you will return to Sources with the connection status updated.'
            : 'GitHub source connections are disabled in Local Only mode. Enable External Access in Preferences to continue.'}
        </AlertDescription>
      </Alert>
    </PageLayout>
  )
}
