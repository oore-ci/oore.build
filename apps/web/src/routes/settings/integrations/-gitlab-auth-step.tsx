import { GitLabCredentialsFields } from './-gitlab-credentials-fields'
import type { UseFormReturn } from 'react-hook-form'

import type { GitLabSetupForm } from './-gitlab-setup'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
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
        <p className="text-sm font-medium text-muted-foreground">
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
      <GitLabCredentialsFields
        form={form}
        authMode={authMode}
        hostUrl={hostUrl}
        callbackUrl={callbackUrl}
      />
    </section>
  )
}
