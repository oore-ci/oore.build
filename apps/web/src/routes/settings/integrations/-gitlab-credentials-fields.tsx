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

type GitLabCredentialsFieldsProps = {
  form: UseFormReturn<GitLabSetupForm>
  authMode: GitLabSetupForm['auth_mode']
  hostUrl: string
  callbackUrl: string
}

export function GitLabCredentialsFields({
  form,
  authMode,
  hostUrl,
  callbackUrl,
}: GitLabCredentialsFieldsProps) {
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
                <span key={0}>
                  Select <code>read_user</code>, <code>read_api</code>, and{' '}
                  <code>read_repository</code>.
                </span>,
                <span key={1}>
                  Do not select full <code>api</code> unless you are testing a
                  future write-capable GitLab feature.
                </span>,
                <span key={2}>
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
  )
}
