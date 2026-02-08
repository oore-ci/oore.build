import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { toast } from 'sonner'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useGitHubAppStart } from '@/hooks/use-integrations'
import { useActiveInstance } from '@/stores/instance-store'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { webPageTitle } from '@/lib/seo'

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

  const backendUrl = instance?.url ?? ''
  const webhookUrl = `${backendUrl}/v1/webhooks/github`
  const redirectUrl = `${window.location.origin}/settings/integrations`

  useEffect(() => {
    document.title = webPageTitle('Connect GitHub')
  }, [])

  function handleConnect() {
    startMutation.mutate(
      { webhook_url: webhookUrl, redirect_url: redirectUrl },
      {
        onSuccess: (data) => {
          // Navigate the current window to the backend's create page.
          // That page auto-POSTs the manifest to GitHub.
          // After app creation, GitHub redirects to the backend callback,
          // which exchanges the code and redirects back here.
          window.location.href = data.create_url
        },
        onError: (err) => {
          toast.error(`Failed to start GitHub setup: ${err.message}`)
        },
      },
    )
  }

  return (
    <PageLayout width="narrow">
      <PageHeader
        title="Connect GitHub"
        description="Create a GitHub App to access repositories and receive webhooks."
        back={{ to: '/settings/integrations', label: 'Back to Integrations' }}
      />

      <Card>
        <CardHeader>
          <CardTitle>GitHub App</CardTitle>
          <CardDescription>
            Click the button below to create a new GitHub App. You'll be
            redirected to GitHub to review and create the app, then
            automatically returned here when setup is complete.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button
            onClick={handleConnect}
            disabled={startMutation.isPending}
          >
            {startMutation.isPending ? 'Starting...' : 'Connect GitHub'}
          </Button>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
