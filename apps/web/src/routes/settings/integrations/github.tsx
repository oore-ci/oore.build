import { createFileRoute } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Copy01Icon,
  InformationCircleIcon,
  LinkSquare02Icon,
} from '@hugeicons/core-free-icons'
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
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import SetupHint from '@/components/setup-hint'

function EndpointRow({ label, value }: { label: string; value: string }) {
  return (
    <Item size="sm">
      <ItemContent>
        <ItemTitle>{label}</ItemTitle>
        <ItemDescription className="line-clamp-none font-mono text-xs break-all">
          {value}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() =>
            void navigator.clipboard
              .writeText(value)
              .then(() => toast.success(`${label} copied`))
          }
        >
          <HugeiconsIcon icon={Copy01Icon} />
          Copy
        </Button>
      </ItemActions>
    </Item>
  )
}

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
    <PageLayout>
      <PageMeta title="Connect GitHub source" noindex />
      <PageHeader
        title="Connect GitHub"
        description="Oore creates a GitHub App for repository access and webhooks, sends you to GitHub to install it, and brings you back here."
      />

      {!remoteEnabled && !preferencesLoading ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            GitHub sources need external access. Turn it on in General settings
            first.
          </AlertDescription>
        </Alert>
      ) : null}

      <SettingsSection
        title="GitHub App"
        description="What the generated app can see and do."
        actions={
          <Button
            onClick={handleConnect}
            disabled={
              startMutation.isPending || preferencesLoading || !remoteEnabled
            }
          >
            <HugeiconsIcon icon={LinkSquare02Icon} />
            {preferencesLoading
              ? 'Checking access...'
              : startMutation.isPending
                ? 'Starting...'
                : 'Create GitHub app'}
          </Button>
        }
      >
        <SettingsSurface>
          <SetupHint
            title="Requested access"
            items={[
              'Repository contents, metadata, and pull requests are read-only.',
              'Statuses and checks are writable so builds can report CI feedback.',
              'Webhook events are push, pull_request, check_run, and check_suite.',
            ]}
          />
        </SettingsSurface>
      </SettingsSection>

      <SettingsSection
        title="Endpoints"
        description="Filled into the app manifest for you. Copy them if you register the app by hand."
      >
        <SettingsSurface inset={false}>
          <ItemGroup>
            <EndpointRow label="Webhook URL" value={webhookUrl} />
            <ItemSeparator />
            <EndpointRow label="Redirect URL" value={redirectUrl} />
          </ItemGroup>
        </SettingsSurface>
      </SettingsSection>
    </PageLayout>
  )
}
