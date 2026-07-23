import { createFileRoute } from '@tanstack/react-router'
import {
  ArrowRight as ArrowRight01Icon,
  Info as InformationCircleIcon,
  SquareArrowOutUpRight as LinkSquare02Icon,
} from 'lucide-react'
import { toast } from '@/lib/toast'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { usePreviewGitHubAppSetup } from '@/hooks/use-authorization-start'
import { PageMeta } from '@/lib/seo'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import SetupHint from '@/components/setup-hint'
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table'

export const Route = createFileRoute('/settings/integrations/github')({
  staticData: {
    breadcrumb: {
      title: 'GitHub',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'])
  },
  component: GitHubSetupPage,
})

function GitHubSetupPage() {
  const instance = useActiveInstance()
  const startMutation = usePreviewGitHubAppSetup()
  const { data: preferences, isLoading: preferencesLoading } =
    useInstancePreferences()
  const remoteEnabled = preferences?.runtime_mode === 'remote'

  const backendUrl = resolveInstanceApiBaseUrl(instance) ?? ''
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
      <PageMeta title="Connect GitHub source" noindex />
      <PageHeader
        title="Connect GitHub source"
        description="Generate and install a GitHub App source for repository access and webhook delivery."
      />

      <section className="grid gap-4 lg:grid-cols-2">
        <section className="min-w-0 border bg-card">
          <div className="border-b px-4 py-3">
            <h2 className="text-sm font-semibold">Connection flow</h2>
          </div>
          <div className="space-y-3 p-4">
            <p className="text-sm text-muted-foreground">
              oore creates a GitHub App manifest, redirects you to GitHub, then
              returns here after install.
            </p>
            <SetupHint
              title="Generated GitHub App access"
              items={[
                'Repository contents, metadata, and pull requests are read-only.',
                'Statuses and checks are writable so builds can report CI feedback.',
                'Webhook events are push, pull_request, check_run, and check_suite.',
              ]}
            />
            <Button
              onClick={handleConnect}
              disabled={
                startMutation.isPending || preferencesLoading || !remoteEnabled
              }
            >
              <LinkSquare02Icon />
              {preferencesLoading
                ? 'Checking access...'
                : !remoteEnabled
                  ? 'External access required'
                  : startMutation.isPending
                    ? 'Starting...'
                    : 'Create GitHub app'}
              {!startMutation.isPending ? <ArrowRight01Icon /> : null}
            </Button>
          </div>
        </section>

        <section className="min-w-0 border bg-card">
          <div className="border-b px-4 py-3">
            <h2 className="text-sm font-semibold">Generated endpoints</h2>
          </div>
          <div className="p-4">
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
          </div>
        </section>
      </section>

      <Alert>
        <InformationCircleIcon size={16} />
        <AlertDescription>
          {remoteEnabled
            ? 'After GitHub installation, you will return to Sources with the connection status updated.'
            : 'GitHub source connections require the backend to be in Remote mode. Update access policy in Preferences to continue.'}
        </AlertDescription>
      </Alert>
    </PageLayout>
  )
}
