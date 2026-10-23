import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { zodResolver } from '@hookform/resolvers/zod'
import { useForm, useWatch } from 'react-hook-form'
import { toast } from '@/lib/toast'

import { GitLabAuthStep } from './-gitlab-auth-step'
import { GitLabHostStep } from './-gitlab-host-step'
import { gitLabSetupSchema } from './-gitlab-setup'
import type { GitLabHostKind, GitLabSetupForm } from './-gitlab-setup'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Form } from '@/components/ui/form'
import { FormActions } from '@/components/form-actions'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  SettingsSection,
  SettingsSurface,
  SettingsSurfaceSeparator,
} from '@/components/settings/settings-section'
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
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

export const Route = createFileRoute('/settings/integrations/gitlab')({
  staticData: {
    breadcrumb: {
      title: 'GitLab',
    },
  },
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin'], context)
  },
  component: GitLabSetupPage,
})

function GitLabSetupPage() {
  const navigate = useNavigate()
  const startMutation = useGitLabStart()
  const { data: preferences, isLoading: preferencesLoading } =
    useInstancePreferences()
  const { data: networkSettings } = useExternalAccessNetworkSettings()
  const remoteEnabled = preferences?.runtime_mode === 'remote'
  const form = useForm<GitLabSetupForm>({
    resolver: zodResolver(gitLabSetupSchema),
    mode: 'onBlur',
    defaultValues: {
      host_kind: 'gitlab_com',
      host_url: 'https://gitlab.com',
      auth_mode: 'personal_token',
      access_token: '',
      client_id: '',
      client_secret: '',
    },
  })
  const hostKind = useWatch({ control: form.control, name: 'host_kind' })
  const authMode = useWatch({ control: form.control, name: 'auth_mode' })
  const hostUrl = useWatch({ control: form.control, name: 'host_url' })
  const normalizedHostUrl =
    normalizeGitLabHostUrl(hostUrl) ?? 'https://gitlab.com'
  const { callbackUrl } = gitLabPublicEndpoints(
    networkSettings?.public_url ?? undefined,
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
    form.setValue('host_kind', value, { shouldDirty: true })
    if (value === 'gitlab_com') {
      form.setValue('host_url', 'https://gitlab.com', {
        shouldDirty: true,
        shouldValidate: true,
      })
    }
  }

  return (
    <PageLayout>
      <PageMeta title="Connect GitLab source" noindex />
      <PageHeader title="Connect GitLab" />
      {!remoteEnabled && !preferencesLoading ? (
        <Alert>
          <AlertDescription>
            GitLab sources need external access. Turn it on in General settings
            first.
          </AlertDescription>
        </Alert>
      ) : null}
      <Form {...form}>
        <form
          onSubmit={form.handleSubmit(handleSubmit)}
          className="flex flex-col gap-8"
        >
          <SettingsSection title="Connection">
            <SettingsSurface className="flex flex-col gap-5">
              <GitLabHostStep
                form={form}
                hostKind={hostKind}
                onHostKindChange={selectHostKind}
                onHostUrlBlur={normalizeHostUrl}
              />
              <SettingsSurfaceSeparator />
              <GitLabAuthStep
                form={form}
                authMode={authMode}
                hostUrl={normalizedHostUrl}
                callbackUrl={callbackUrl}
              />
              <SettingsSurfaceSeparator />
              <Stack gap={3}>
                <div>
                  <p className="text-sm font-medium">3. Verify and finish</p>
                  <Text variant="muted" className="mt-1">
                    {authMode === 'personal_token'
                      ? 'Saving verifies the token against your GitLab account and discovers the projects it can reach. If that fails, create a new token with the listed read-only scopes.'
                      : 'Saving checks that the host is reachable, then sends you to GitLab to authorize the OAuth application. The source becomes active when GitLab returns you here.'}
                  </Text>
                  <Text variant="muted" className="mt-1">
                    After the source syncs, generate a webhook token for each
                    GitLab project from the source details page.
                  </Text>
                </div>
              </Stack>
            </SettingsSurface>
          </SettingsSection>
          <FormActions
            disabled={!remoteEnabled || preferencesLoading}
            disabledReason={
              preferencesLoading
                ? 'Checking access...'
                : 'External access is required.'
            }
            isPending={startMutation.isPending}
            onCancel={() => void navigate({ to: '/settings/integrations' })}
            pendingLabel="Connecting..."
            submitLabel={
              authMode === 'personal_token'
                ? 'Verify and save'
                : 'Save and authorize on GitLab'
            }
          />
        </form>
      </Form>
    </PageLayout>
  )
}
