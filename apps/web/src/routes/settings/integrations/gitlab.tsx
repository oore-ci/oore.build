import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'
import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Refresh01Icon } from '@hugeicons/core-free-icons'
import type { UseFormReturn } from 'react-hook-form'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import {
  useExternalAccessNetworkSettings,
  useInstancePreferences,
} from '@/hooks/use-artifact-storage'
import { useGitLabStart } from '@/hooks/use-integrations'
import { gitLabPublicEndpoints, normalizeGitLabHostUrl } from '@/lib/gitlab-url'
import { PageMeta } from '@/lib/seo'
import SetupHint from '@/components/setup-hint'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

const gitLabSetupSchema = z
  .object({
    host_url: z
      .string()
      .trim()
      .min(1, 'Host URL is required')
      .refine(
        (value) => normalizeGitLabHostUrl(value) !== null,
        'Use an HTTP(S) host URL only, without a path, query, or credentials.',
      ),
    auth_mode: z.enum(['personal_token', 'oauth_app']),
    webhook_secret: z.string().trim().min(1, 'Webhook secret is required'),
    access_token: z.string().optional(),
    client_id: z.string().optional(),
    client_secret: z.string().optional(),
  })
  .superRefine((value, ctx) => {
    if (value.auth_mode === 'personal_token' && !value.access_token?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Access token is required',
        path: ['access_token'],
      })
    }

    if (value.auth_mode === 'oauth_app' && !value.client_id?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Client ID is required',
        path: ['client_id'],
      })
    }

    if (value.auth_mode === 'oauth_app' && !value.client_secret?.trim()) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Client secret is required',
        path: ['client_secret'],
      })
    }
  })

type GitLabSetupForm = z.infer<typeof gitLabSetupSchema>
type GitLabHostKind = 'gitlab_com' | 'self_managed'

function generateWebhookSecret(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(24))
  return `oore_${Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')}`
}

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
  const { data: preferences, isLoading: preferencesLoading } =
    useInstancePreferences()
  const { data: networkSettings } = useExternalAccessNetworkSettings()
  const remoteEnabled = preferences?.preferences.runtime_mode === 'remote'
  const [webhookSecret] = useState(generateWebhookSecret)
  const [hostKind, setSelectedHostKind] = useState<GitLabHostKind>('gitlab_com')

  const form = useForm<GitLabSetupForm>({
    resolver: zodResolver(gitLabSetupSchema),
    mode: 'onBlur',
    defaultValues: {
      host_url: 'https://gitlab.com',
      auth_mode: 'personal_token',
      webhook_secret: webhookSecret,
      access_token: '',
      client_id: '',
      client_secret: '',
    },
  })

  const authMode = form.watch('auth_mode')
  const hostUrl = form.watch('host_url')
  const normalizedHostUrl =
    normalizeGitLabHostUrl(hostUrl) ?? 'https://gitlab.com'
  const { callbackUrl, webhookUrl } = gitLabPublicEndpoints(
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
        webhook_secret: data.webhook_secret.trim(),
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
          if (response.integration.status === 'inactive') {
            toast.message(
              `Host verified. Authorize ${response.integration.display_name ?? 'GitLab'} to complete setup.`,
            )
          } else {
            toast.success(
              `Source verified: ${response.integration.display_name ?? 'GitLab'}`,
            )
          }
          void navigate({
            to: '/settings/integrations/$integrationId',
            params: { integrationId: response.integration.id },
          })
        },
        onError: (err) => {
          toast.error(`Failed to connect GitLab: ${err.message}`)
        },
      },
    )
  }

  function normalizeHostUrl() {
    const normalized = normalizeGitLabHostUrl(form.getValues('host_url'))
    if (normalized) {
      form.setValue('host_url', normalized, { shouldValidate: true })
    }
  }

  function selectHostKind(value: GitLabHostKind | null) {
    if (!value) return
    setSelectedHostKind(value)
    if (value === 'gitlab_com') {
      form.setValue('host_url', 'https://gitlab.com', {
        shouldDirty: true,
        shouldValidate: true,
      })
    }
  }

  function replaceWebhookSecret() {
    form.setValue('webhook_secret', generateWebhookSecret(), {
      shouldDirty: true,
      shouldTouch: true,
      shouldValidate: true,
    })
  }

  function copyWebhookSecret() {
    void navigator.clipboard.writeText(form.getValues('webhook_secret')).then(
      () => toast.success('Webhook secret copied'),
      () => toast.error('Could not copy webhook secret'),
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Connect GitLab Source" noindex />
      <PageHeader
        title="Connect GitLab Source"
        description="Connect GitLab.com or a self-managed GitLab host for repository discovery and webhook-triggered builds."
        back={{ to: '/settings/integrations', label: 'Sources' }}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            GitLab connection
          </CardTitle>
        </CardHeader>
        <CardContent>
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
              <section className="space-y-4">
                <div>
                  <p className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                    1. Choose GitLab host
                  </p>
                  <p className="mt-1 text-sm text-muted-foreground">
                    GitLab.com and self-managed GitLab use the same connection
                    flow.
                  </p>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="gitlab-host">GitLab host</Label>
                  <Select
                    value={hostKind}
                    onValueChange={selectHostKind}
                    items={{
                      gitlab_com: 'GitLab.com',
                      self_managed: 'Self-managed GitLab',
                    }}
                  >
                    <SelectTrigger id="gitlab-host">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="gitlab_com">GitLab.com</SelectItem>
                      <SelectItem value="self_managed">
                        Self-managed GitLab
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                {hostKind === 'self_managed' ? (
                  <FormField
                    control={form.control}
                    name="host_url"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Self-managed host URL</FormLabel>
                        <FormControl>
                          <Input
                            {...field}
                            placeholder="https://gitlab.example.com"
                            onBlur={() => {
                              field.onBlur()
                              normalizeHostUrl()
                            }}
                          />
                        </FormControl>
                        <FormDescription>
                          Host origin only. Oore normalizes a trailing slash; do
                          not include <code>/api/v4</code> or a group path.
                        </FormDescription>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                ) : (
                  <SetupHint
                    title="GitLab.com selected"
                    items={[
                      'Oore will connect to https://gitlab.com. Choose self-managed only when your GitLab has a different host URL.',
                    ]}
                  />
                )}
              </section>

              <section className="space-y-4 border-t border-border/60 pt-6">
                <div>
                  <p className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                    2. Authenticate
                  </p>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Choose the credential model that fits this GitLab source.
                  </p>
                </div>
                <FormField
                  control={form.control}
                  name="auth_mode"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Authentication method</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                        items={{
                          personal_token: 'Personal Access Token',
                          oauth_app: 'OAuth Application',
                        }}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          <SelectItem value="personal_token">
                            Personal Access Token
                          </SelectItem>
                          <SelectItem value="oauth_app">
                            OAuth Application
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <FormDescription>
                        Personal access tokens are fastest for one account and
                        are verified before saving. OAuth keeps user
                        authorization in GitLab and is better for a shared
                        source; it requires one additional authorization after
                        saving.
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <GitLabCredentialsFields
                  form={form}
                  authMode={authMode}
                  hostUrl={normalizedHostUrl}
                  callbackUrl={callbackUrl}
                />
              </section>

              <section className="space-y-3 border-t border-border/60 pt-6">
                <div>
                  <p className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                    3. Verify source connection
                  </p>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Oore checks the selected GitLab host before it stores this
                    source.
                  </p>
                </div>
                <Alert>
                  <AlertDescription>
                    {authMode === 'personal_token'
                      ? 'Saving verifies this token against your GitLab account and discovers accessible projects. If it fails, create a new token with the listed read-only scopes.'
                      : 'Saving checks that this GitLab host is reachable. Then authorize the saved OAuth application on GitLab; the source becomes active only after the callback returns.'}
                  </AlertDescription>
                </Alert>
              </section>

              <section className="space-y-4 border-t border-border/60 pt-6">
                <div>
                  <p className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                    4. Set up webhooks
                  </p>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Save the source first, then add this endpoint and secret in
                    each GitLab project.
                  </p>
                </div>
                <GitLabWebhookSecretField
                  form={form}
                  webhookUrl={webhookUrl}
                  onCopy={copyWebhookSecret}
                  onRegenerate={replaceWebhookSecret}
                />
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
        </CardContent>
      </Card>
    </PageLayout>
  )
}

function GitLabWebhookSecretField({
  form,
  webhookUrl,
  onCopy,
  onRegenerate,
}: {
  form: UseFormReturn<GitLabSetupForm>
  webhookUrl: string
  onCopy: () => void
  onRegenerate: () => void
}) {
  return (
    <FormField
      control={form.control}
      name="webhook_secret"
      render={({ field }) => (
        <FormItem>
          <FormLabel>Webhook secret</FormLabel>
          <div className="flex gap-2">
            <FormControl>
              <Input type="password" {...field} />
            </FormControl>
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={onCopy}
              aria-label="Copy webhook secret"
              title="Copy webhook secret"
            >
              <HugeiconsIcon icon={Copy01Icon} />
            </Button>
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={onRegenerate}
              aria-label="Generate a new webhook secret"
              title="Generate a new webhook secret"
            >
              <HugeiconsIcon icon={Refresh01Icon} />
            </Button>
          </div>
          <FormDescription>
            Generated securely in this browser. Copy it into GitLab; Oore
            encrypts it when this source is saved.
          </FormDescription>
          <SetupHint
            title="Webhook setup in GitLab"
            items={[
              <span>
                URL: <code>{webhookUrl}</code>
              </span>,
              'Project Settings -> Webhooks -> Secret token. Enable Push events (and Merge request events if your pipeline uses them).',
            ]}
          />
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

function GitLabCredentialsFields({
  form,
  authMode,
  hostUrl,
  callbackUrl,
}: {
  form: UseFormReturn<GitLabSetupForm>
  authMode: GitLabSetupForm['auth_mode']
  hostUrl: string
  callbackUrl: string
}) {
  if (authMode === 'personal_token') {
    return (
      <FormField
        control={form.control}
        name="access_token"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Access token</FormLabel>
            <FormControl>
              <Input type="password" {...field} placeholder="glpat-..." />
            </FormControl>
            <SetupHint
              title="Required GitLab PAT scopes"
              items={[
                <span>
                  Select <code>read_user</code>, <code>read_api</code>, and{' '}
                  <code>read_repository</code>.
                </span>,
                <span>
                  Do not select full <code>api</code> unless you are testing a
                  future write-capable GitLab feature.
                </span>,
                <span>
                  Create it at{' '}
                  <code>{hostUrl}/-/user_settings/personal_access_tokens</code>.
                </span>,
              ]}
            />
            <FormMessage />
          </FormItem>
        )}
      />
    )
  }

  return (
    <>
      <FormField
        control={form.control}
        name="client_id"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Client ID</FormLabel>
            <FormControl>
              <Input {...field} placeholder="Application ID" />
            </FormControl>
            <FormDescription>
              Create an OAuth application on {hostUrl} and paste its Application
              ID here.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="client_secret"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Client secret</FormLabel>
            <FormControl>
              <Input
                type="password"
                {...field}
                placeholder="Application secret"
              />
            </FormControl>
            <FormDescription>
              Use the Secret from the same GitLab OAuth application.
            </FormDescription>
            <SetupHint
              title="OAuth callback"
              items={[
                <span>
                  Register this redirect URI in GitLab:{' '}
                  <code>{callbackUrl}</code>
                </span>,
                <span>
                  Request only <code>read_api</code> and{' '}
                  <code>read_repository</code>; Oore does not request write
                  scopes for this source.
                </span>,
                'Save this source, then choose Authorize on GitLab from its source details page.',
              ]}
            />
            <FormMessage />
          </FormItem>
        )}
      />
    </>
  )
}
