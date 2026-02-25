import { createSignal, Show } from 'solid-js'
import { createForm } from '@tanstack/solid-form'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import z from 'zod'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useGitLabStart } from '@/hooks/use-integrations'
import { PageMeta } from '@/lib/seo'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { toast } from '@/components/ui/sonner'

type GitLabAuthMode = 'personal_token' | 'oauth_app'

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
  const preferences = useInstancePreferences()
  const remoteEnabled = () => preferences.data?.preferences.runtime_mode === 'remote'

  const [error, setError] = createSignal<string | null>(null)

  const schema = z
    .object({
      hostUrl: z
        .string()
        .trim()
        .min(1, 'Host URL is required.')
        .url('Host URL must be a valid URL.'),
      authMode: z.enum(['personal_token', 'oauth_app']),
      webhookSecret: z.string().trim().min(1, 'Webhook secret is required.'),
      accessToken: z.string().optional(),
      clientId: z.string().optional(),
      clientSecret: z.string().optional(),
    })
    .superRefine((value, context) => {
      if (value.authMode === 'personal_token' && !value.accessToken?.trim()) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Access token is required for Personal Access Token mode.',
          path: ['accessToken'],
        })
      }

      if (value.authMode === 'oauth_app' && !value.clientId?.trim()) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Client ID is required for OAuth mode.',
          path: ['clientId'],
        })
      }

      if (value.authMode === 'oauth_app' && !value.clientSecret?.trim()) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Client secret is required for OAuth mode.',
          path: ['clientSecret'],
        })
      }
    })

  const form = createForm(() => ({
    defaultValues: {
      hostUrl: 'https://gitlab.com',
      authMode: 'personal_token' as GitLabAuthMode,
      webhookSecret: '',
      accessToken: '',
      clientId: '',
      clientSecret: '',
    },
    validators: {
      onSubmit: ({ value }) => {
        const parsed = schema.safeParse(value)
        if (parsed.success) return undefined
        const fields = parsed.error.flatten().fieldErrors
        return {
          fields: {
            hostUrl: fields.hostUrl?.[0],
            webhookSecret: fields.webhookSecret?.[0],
            accessToken: fields.accessToken?.[0],
            clientId: fields.clientId?.[0],
            clientSecret: fields.clientSecret?.[0],
          },
        }
      },
    },
    onSubmit: ({ value }) => {
      if (!remoteEnabled()) return
      setError(null)

      startMutation.mutate(
        {
          host_url: value.hostUrl.trim(),
          auth_mode: value.authMode,
          webhook_secret: value.webhookSecret.trim(),
          access_token:
            value.authMode === 'personal_token'
              ? value.accessToken.trim()
              : undefined,
          client_id:
            value.authMode === 'oauth_app' ? value.clientId.trim() : undefined,
          client_secret:
            value.authMode === 'oauth_app'
              ? value.clientSecret.trim()
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
              return
            }

            toast.success(
              `Connected: ${response.integration.display_name ?? 'GitLab'}`,
            )
            void navigate({ to: '/settings/integrations' })
          },
          onError: (mutationError) => {
            setError(
              mutationError instanceof Error
                ? mutationError.message
                : 'Failed to connect GitLab',
            )
          },
        },
      )
    },
  }))

  const submissionAttempts = form.useStore((state) => state.submissionAttempts)
  const values = form.useStore((state) => state.values)

  const handleSubmit = (event: SubmitEvent) => {
    event.preventDefault()
    void form.handleSubmit()
  }

  return (
    <PageLayout class="space-y-4">
      <PageMeta title="Connect GitLab Source" noindex />
      <PageHeader
        title="Connect GitLab Source"
        description="Connect gitlab.com or a self-managed GitLab source for repositories and webhook events."
      />

      <Card>
        <CardHeader>
          <CardTitle class="text-base">GitLab connection</CardTitle>
        </CardHeader>
        <CardContent class="space-y-4">
          <form class="space-y-4" onSubmit={handleSubmit}>
          <Show when={!remoteEnabled()}>
            <Alert>
              <AlertDescription>
                GitLab source connections are disabled in Local Only mode.
                Enable External Access in Preferences to continue.
              </AlertDescription>
            </Alert>
          </Show>

          <Show when={error()}>
            <Alert variant="destructive">
              <AlertTitle>Invalid form</AlertTitle>
              <AlertDescription>{error()}</AlertDescription>
            </Alert>
          </Show>

            <form.Field name="hostUrl">
              {(field) => {
                const fieldError = () => {
                  if (
                    !field().state.meta.isTouched &&
                    submissionAttempts() === 0
                  ) {
                    return null
                  }
                  return (field().state.meta.errors[0] as string | undefined) ?? null
                }

                return (
                  <FormField label="GitLab host URL" error={fieldError()}>
                    <Input
                      value={field().state.value}
                      onInput={(event) =>
                        field().handleChange(event.currentTarget.value)
                      }
                      onBlur={field().handleBlur}
                      placeholder="https://gitlab.com"
                    />
                  </FormField>
                )
              }}
            </form.Field>

            <form.Field name="authMode">
              {(field) => (
                <FormField label="Authentication method">
                  <select
                    class="h-9 w-full border border-input bg-background px-3 text-sm"
                    value={field().state.value}
                    onChange={(event) =>
                      field().handleChange(event.currentTarget.value as GitLabAuthMode)
                    }
                    onBlur={field().handleBlur}
                  >
                    <option value="personal_token">Personal Access Token</option>
                    <option value="oauth_app">OAuth Application</option>
                  </select>
                </FormField>
              )}
            </form.Field>

            <form.Field name="webhookSecret">
              {(field) => {
                const fieldError = () => {
                  if (
                    !field().state.meta.isTouched &&
                    submissionAttempts() === 0
                  ) {
                    return null
                  }
                  return (field().state.meta.errors[0] as string | undefined) ?? null
                }

                return (
                  <FormField
                    label="Webhook secret"
                    error={fieldError()}
                    description="Use the same secret when creating the webhook in GitLab."
                  >
                    <Input
                      type="password"
                      value={field().state.value}
                      onInput={(event) =>
                        field().handleChange(event.currentTarget.value)
                      }
                      onBlur={field().handleBlur}
                      placeholder="Shared secret for GitLab webhook settings"
                    />
                  </FormField>
                )
              }}
            </form.Field>

            <Show when={values().authMode === 'personal_token'}>
              <form.Field name="accessToken">
                {(field) => {
                  const fieldError = () => {
                    if (
                      !field().state.meta.isTouched &&
                      submissionAttempts() === 0
                    ) {
                      return null
                    }
                    return (field().state.meta.errors[0] as string | undefined) ?? null
                  }

                  return (
                    <FormField label="Access token" error={fieldError()}>
                      <Input
                        type="password"
                        value={field().state.value}
                        onInput={(event) =>
                          field().handleChange(event.currentTarget.value)
                        }
                        onBlur={field().handleBlur}
                        placeholder="glpat-..."
                      />
                    </FormField>
                  )
                }}
              </form.Field>
            </Show>

            <Show when={values().authMode === 'oauth_app'}>
              <form.Field name="clientId">
                {(field) => {
                  const fieldError = () => {
                    if (
                      !field().state.meta.isTouched &&
                      submissionAttempts() === 0
                    ) {
                      return null
                    }
                    return (field().state.meta.errors[0] as string | undefined) ?? null
                  }

                  return (
                    <FormField label="OAuth Client ID" error={fieldError()}>
                      <Input
                        value={field().state.value}
                        onInput={(event) =>
                          field().handleChange(event.currentTarget.value)
                        }
                        onBlur={field().handleBlur}
                        placeholder="GitLab OAuth client ID"
                      />
                    </FormField>
                  )
                }}
              </form.Field>

              <form.Field name="clientSecret">
                {(field) => {
                  const fieldError = () => {
                    if (
                      !field().state.meta.isTouched &&
                      submissionAttempts() === 0
                    ) {
                      return null
                    }
                    return (field().state.meta.errors[0] as string | undefined) ?? null
                  }

                  return (
                    <FormField label="OAuth Client Secret" error={fieldError()}>
                      <Input
                        type="password"
                        value={field().state.value}
                        onInput={(event) =>
                          field().handleChange(event.currentTarget.value)
                        }
                        onBlur={field().handleBlur}
                        placeholder="GitLab OAuth client secret"
                      />
                    </FormField>
                  )
                }}
              </form.Field>
            </Show>

            <Button type="submit" disabled={startMutation.isPending || !remoteEnabled()}>
              {startMutation.isPending ? 'Saving...' : 'Connect GitLab'}
            </Button>
          </form>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
