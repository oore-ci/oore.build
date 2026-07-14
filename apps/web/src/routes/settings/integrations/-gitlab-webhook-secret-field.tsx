import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Refresh01Icon } from '@hugeicons/core-free-icons'
import type { UseFormReturn } from 'react-hook-form'

import type { GitLabSetupForm } from './-gitlab-setup'
import SetupHint from '@/components/setup-hint'
import { Button } from '@/components/ui/button'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'

type GitLabWebhookSecretFieldProps = {
  form: UseFormReturn<GitLabSetupForm>
  webhookUrl: string
  onCopy: () => void
  onRegenerate: () => void
}

export function GitLabWebhookSecretField({
  form,
  webhookUrl,
  onCopy,
  onRegenerate,
}: GitLabWebhookSecretFieldProps) {
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
