import type { UseFormReturn } from 'react-hook-form'

import type { GitLabSetupForm } from './-gitlab-setup'
import SetupHint from '@/components/setup-hint'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'

type GitLabAuthStepProps = {
  form: UseFormReturn<GitLabSetupForm>
  authMode: GitLabSetupForm['auth_mode']
  hostUrl: string
  callbackUrl: string
}

export function GitLabAuthStep({
  form,
  authMode,
  hostUrl,
  callbackUrl,
}: GitLabAuthStepProps) {
  return (
    <section className="space-y-4">
      <Separator />
      <div>
        <p className="text-sm font-medium">2. Authenticate</p>
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
                personal_token: 'Personal access token',
                oauth_app: 'OAuth application',
              }}
            >
              <FormControl>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                <SelectItem value="personal_token">
                  Personal access token
                </SelectItem>
                <SelectItem value="oauth_app">OAuth application</SelectItem>
              </SelectContent>
            </Select>
            <FormDescription>
              Personal access tokens are fastest for one account and are
              verified before saving. OAuth keeps user authorization in GitLab
              and is better for a shared source; it requires one additional
              authorization after saving.
            </FormDescription>
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
                <Input type="password" {...field} placeholder="glpat-..." />
              </FormControl>
              <SetupHint
                title="Required GitLab PAT scopes"
                items={[
                  <span key={0}>
                    Select <code>read_user</code>, <code>read_api</code>, and{' '}
                    <code>read_repository</code>.
                  </span>,
                  <span key={1}>
                    To show build statuses on merge requests, select{' '}
                    <code>api</code> instead of <code>read_api</code>.
                  </span>,
                  <span key={2}>
                    Create it at{' '}
                    <code>
                      {hostUrl}/-/user_settings/personal_access_tokens
                    </code>
                    .
                  </span>,
                ]}
              />
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
                <FormDescription>
                  Create an OAuth application on {hostUrl} and paste its
                  Application ID here.
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
                    <span key={3}>
                      Register this redirect URI in GitLab:{' '}
                      <code>{callbackUrl}</code>
                    </span>,
                    <span key={4}>
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
      )}
    </section>
  )
}
