import { useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { toast } from 'sonner'
import { useEffect } from 'react'

import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { useGitLabStart } from '@/hooks/use-integrations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/settings/integrations/gitlab')({
  staticData: { breadcrumbLabel: 'GitLab' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: GitLabSetupPage,
})

function GitLabSetupPage() {
  const navigate = useNavigate()
  const startMutation = useGitLabStart()

  const [hostUrl, setHostUrl] = useState('https://gitlab.com')
  const [authMode, setAuthMode] = useState<'personal_token' | 'oauth_app'>(
    'personal_token',
  )
  const [accessToken, setAccessToken] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [webhookSecret, setWebhookSecret] = useState('')

  useEffect(() => {
    document.title = webPageTitle('Connect GitLab')
  }, [])

  function handleSubmit() {
    startMutation.mutate(
      {
        host_url: hostUrl,
        auth_mode: authMode,
        webhook_secret: webhookSecret,
        access_token: authMode === 'personal_token' ? accessToken : undefined,
        client_id: authMode === 'oauth_app' ? clientId : undefined,
        client_secret: authMode === 'oauth_app' ? clientSecret : undefined,
      },
      {
        onSuccess: (data) => {
          if (data.integration.status === 'inactive') {
            toast.message(
              `Saved: ${data.integration.display_name ?? 'GitLab'} — authorize on GitLab to complete setup.`,
            )
            void navigate({
              to: '/settings/integrations/$integrationId',
              params: { integrationId: data.integration.id },
            })
          } else {
            toast.success(
              `Connected: ${data.integration.display_name ?? 'GitLab'}`,
            )
            void navigate({ to: '/settings/integrations' })
          }
        },
        onError: (err) => {
          toast.error(`Failed to connect GitLab: ${err.message}`)
        },
      },
    )
  }

  const isValid =
    hostUrl.trim() !== '' &&
    webhookSecret.trim() !== '' &&
    (authMode === 'personal_token'
      ? accessToken.trim() !== ''
      : clientId.trim() !== '' && clientSecret.trim() !== '')

  return (
    <PageLayout width="narrow">
      <PageHeader
        title="Connect GitLab"
        description="Connect to gitlab.com or a self-managed GitLab instance."
        back={{ to: '/settings/integrations', label: 'Back to Integrations' }}
      />

      <Card>
        <CardHeader>
          <CardTitle>GitLab Connection</CardTitle>
          <CardDescription>
            Choose your GitLab host and authentication method.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label>GitLab Host URL</Label>
            <Input
              value={hostUrl}
              onChange={(e) => setHostUrl(e.target.value)}
              placeholder="https://gitlab.com"
            />
          </div>

          <div className="space-y-2">
            <Label>Authentication Method</Label>
            <Select
              value={authMode}
              onValueChange={(v) =>
                setAuthMode(v as 'personal_token' | 'oauth_app')
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="personal_token">
                  Personal Access Token
                </SelectItem>
                <SelectItem value="oauth_app">OAuth Application</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label>Webhook Secret</Label>
            <Input
              type="password"
              value={webhookSecret}
              onChange={(e) => setWebhookSecret(e.target.value)}
              placeholder="Shared secret used in GitLab webhook settings"
            />
            <p className="text-xs text-muted-foreground">
              Use the exact same secret when creating the GitLab webhook for this instance.
            </p>
          </div>

          {authMode === 'personal_token' && (
            <div className="space-y-2">
              <Label>Access Token</Label>
              <Input
                type="password"
                value={accessToken}
                onChange={(e) => setAccessToken(e.target.value)}
                placeholder="glpat-..."
              />
              <p className="text-xs text-muted-foreground">
                Create a token with api scope at{' '}
                {hostUrl}/-/user_settings/personal_access_tokens
              </p>
            </div>
          )}

          {authMode === 'oauth_app' && (
            <>
              <div className="space-y-2">
                <Label>Client ID</Label>
                <Input
                  value={clientId}
                  onChange={(e) => setClientId(e.target.value)}
                  placeholder="Application ID"
                />
              </div>
              <div className="space-y-2">
                <Label>Client Secret</Label>
                <Input
                  type="password"
                  value={clientSecret}
                  onChange={(e) => setClientSecret(e.target.value)}
                  placeholder="Application secret"
                />
              </div>
            </>
          )}

          <Button
            onClick={handleSubmit}
            disabled={startMutation.isPending || !isValid}
          >
            {startMutation.isPending ? 'Connecting...' : 'Connect GitLab'}
          </Button>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
