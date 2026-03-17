import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useGitLabStart } from '@/hooks/use-integrations'
import { PageMeta } from '@/lib/seo'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
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
    host_url: z.string().trim().min(1, 'Host URL is required'),
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
  const { data: preferences } = useInstancePreferences()
  const remoteEnabled = preferences?.preferences.runtime_mode === 'remote'

  const form = useForm<GitLabSetupForm>({
    resolver: zodResolver(gitLabSetupSchema),
    mode: 'onBlur',
    defaultValues: {
      host_url: 'https://gitlab.com',
      auth_mode: 'personal_token',
      webhook_secret: '',
      access_token: '',
      client_id: '',
      client_secret: '',
    },
  })

  const authMode = form.watch('auth_mode')
  const hostUrl = form.watch('host_url')

  function handleSubmit(data: GitLabSetupForm) {
    if (!remoteEnabled) return
    startMutation.mutate(
      {
        host_url: data.host_url.trim(),
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
              `Saved: ${response.integration.display_name ?? 'GitLab'} - authorize on GitLab to complete setup.`,
            )
            void navigate({
              to: '/settings/integrations/$integrationId',
              params: { integrationId: response.integration.id },
            })
          } else {
            toast.success(
              `Connected: ${response.integration.display_name ?? 'GitLab'}`,
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

  return (
    <PageLayout width="wide">
      <PageMeta title="Connect GitLab Source" noindex />
      <PageHeader
        title="Connect GitLab Source"
        description="Connect gitlab.com or a self-managed GitLab source for repositories and webhook events."
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
                GitLab source connections are disabled in Local Only mode.
                Enable External Access in Preferences to continue.
              </AlertDescription>
            </Alert>
          ) : null}
          <Form {...form}>
            <form
              onSubmit={form.handleSubmit(handleSubmit)}
              className="space-y-4"
            >
              <FormField
                control={form.control}
                name="host_url"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>GitLab host URL</FormLabel>
                    <FormControl>
                      <Input {...field} placeholder="https://gitlab.com" />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

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
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="webhook_secret"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Webhook secret</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        {...field}
                        placeholder="Shared secret for GitLab webhook settings"
                      />
                    </FormControl>
                    <p className="text-xs text-muted-foreground">
                      Use the same secret when creating the webhook in GitLab.
                    </p>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {authMode === 'personal_token' ? (
                <FormField
                  control={form.control}
                  name="access_token"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Access token</FormLabel>
                      <FormControl>
                        <Input
                          type="password"
                          {...field}
                          placeholder="glpat-..."
                        />
                      </FormControl>
                      <p className="text-xs text-muted-foreground">
                        Create a token with api scope at{' '}
                        {hostUrl || 'https://gitlab.com'}
                        /-/user_settings/personal_access_tokens.
                      </p>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ) : (
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
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                </>
              )}

              <Button
                type="submit"
                disabled={startMutation.isPending || !remoteEnabled}
              >
                {!remoteEnabled
                  ? 'External Access Required'
                  : startMutation.isPending
                    ? 'Connecting...'
                    : 'Connect GitLab'}
              </Button>
            </form>
          </Form>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
