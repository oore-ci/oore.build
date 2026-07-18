import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { zodResolver } from '@hookform/resolvers/zod'
import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { toast } from '@/lib/toast'

import { GitLabAuthStep } from './-gitlab-auth-step'
import { GitLabHostStep } from './-gitlab-host-step'
import { gitLabSetupSchema } from './-gitlab-setup'
import { GitLabVerificationStep } from './-gitlab-verification-step'
import type { GitLabHostKind, GitLabSetupForm } from './-gitlab-setup'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Form } from '@/components/ui/form'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  useExternalAccessNetworkSettings,
  useInstancePreferences,
} from '@/hooks/use-artifact-storage'
import { useGitLabStart } from '@/hooks/use-integrations'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { gitLabPublicEndpoints, normalizeGitLabHostUrl } from '@/lib/gitlab-url'
import { PageMeta } from '@/lib/seo'

export const Route = createFileRoute('/settings/integrations/gitlab')({
  staticData: {
    breadcrumb: {
      title: 'GitLab',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'])
  },
  component: GitLabSetupPage,
})

function GitLabSetupPage() {
  const navigate = useNavigate()
  const startMutation = useGitLabStart()
  const { data: preferences, isLoading: preferencesLoading } =
    useInstancePreferences()
  const { data: networkSettings } = useExternalAccessNetworkSettings()
  const remoteEnabled = preferences?.preferences.runtime_mode === 'remote'
  const [hostKind, setSelectedHostKind] = useState<GitLabHostKind>('gitlab_com')
  const form = useForm<GitLabSetupForm>({
    resolver: zodResolver(gitLabSetupSchema),
    mode: 'onBlur',
    defaultValues: {
      host_url: 'https://gitlab.com',
      auth_mode: 'personal_token',
      access_token: '',
      client_id: '',
      client_secret: '',
    },
  })
  const authMode = form.watch('auth_mode')
  const hostUrl = form.watch('host_url')
  const normalizedHostUrl =
    normalizeGitLabHostUrl(hostUrl) ?? 'https://gitlab.com'
  const { callbackUrl } = gitLabPublicEndpoints(
    networkSettings?.settings.public_url,
    window.location.origin,
  )

  function handleSubmit(data: GitLabSetupForm) {
    if (!remoteEnabled) return
    const submittedHostUrl = normalizeGitLabHostUrl(data.host_url)
    if (!submittedHostUrl) return

    startMutation.mutate(
      {
        host_url: submittedHostUrl,
        auth_mode: data.auth_mode,
        access_token:
          data.auth_mode === 'personal_token'
            ? data.access_token?.trim() || undefined
            : undefined,
        client_id:
          data.auth_mode === 'oauth_app'
            ? data.client_id?.trim() || undefined
            : undefined,
        client_secret:
          data.auth_mode === 'oauth_app'
            ? data.client_secret?.trim() || undefined
            : undefined,
      },
      {
        onSuccess: (response) => {
          const displayName = response.integration.display_name ?? 'GitLab'
          if (response.integration.status === 'inactive') {
            toast.message(
              `Host verified. Authorize ${displayName} to complete setup.`,
            )
          } else {
            toast.success(`Source verified: ${displayName}`)
          }
          void navigate({
            to: '/settings/integrations/$integrationId',
            params: { integrationId: response.integration.id },
          })
        },
        onError: (err) =>
          toast.error(`Failed to connect GitLab: ${err.message}`),
      },
    )
  }

  function normalizeHostUrl() {
    const normalized = normalizeGitLabHostUrl(form.getValues('host_url'))
    if (normalized)
      form.setValue('host_url', normalized, { shouldValidate: true })
  }

  function selectHostKind(value: GitLabHostKind | null) {
    if (!value) return
    setSelectedHostKind(() => value)
    if (value === 'gitlab_com') {
      form.setValue('host_url', 'https://gitlab.com', {
        shouldDirty: true,
        shouldValidate: true,
      })
    }
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Connect GitLab source" noindex />
      <PageHeader
        title="Connect GitLab source"
        description="Connect GitLab.com or a self-managed GitLab host for repository discovery and webhook-triggered builds."
      />
      <section className="border bg-card">
        <div className="border-b px-4 py-3">
          <h2 className="text-sm font-semibold">GitLab connection</h2>
        </div>
        <div className="p-4">
          {!remoteEnabled ? (
            <Alert>
              <AlertDescription>
                GitLab source connections require the backend to be in Remote
                mode. Update access policy in Preferences to continue.
              </AlertDescription>
            </Alert>
          ) : null}
          <Form {...form}>
            <form
              onSubmit={form.handleSubmit(handleSubmit)}
              className="space-y-6"
            >
              <GitLabHostStep
                form={form}
                hostKind={hostKind}
                onHostKindChange={selectHostKind}
                onHostUrlBlur={normalizeHostUrl}
              />
              <GitLabAuthStep
                form={form}
                authMode={authMode}
                hostUrl={normalizedHostUrl}
                callbackUrl={callbackUrl}
              />
              <GitLabVerificationStep authMode={authMode} />
              <section className="space-y-4 border-t border-border/60 pt-6">
                <div>
                  <p className="text-sm font-medium text-muted-foreground">
                    4. Finish repository setup
                  </p>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Save and sync the source, then generate a separate webhook
                    token for each GitLab project from the source details page.
                  </p>
                </div>
              </section>
              <Button
                type="submit"
                disabled={
                  startMutation.isPending ||
                  preferencesLoading ||
                  !remoteEnabled
                }
              >
                {preferencesLoading
                  ? 'Checking access...'
                  : !remoteEnabled
                    ? 'External access required'
                    : startMutation.isPending
                      ? 'Connecting...'
                      : authMode === 'personal_token'
                        ? 'Verify and save GitLab source'
                        : 'Save and authorize on GitLab'}
              </Button>
            </form>
          </Form>
        </div>
      </section>
    </PageLayout>
  )
}
