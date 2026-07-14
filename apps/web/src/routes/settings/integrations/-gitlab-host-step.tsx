import type { UseFormReturn } from 'react-hook-form'

import type { GitLabHostKind, GitLabSetupForm } from './-gitlab-setup'
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
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

type GitLabHostStepProps = {
  form: UseFormReturn<GitLabSetupForm>
  hostKind: GitLabHostKind
  onHostKindChange: (value: GitLabHostKind | null) => void
  onHostUrlBlur: () => void
}

export function GitLabHostStep({
  form,
  hostKind,
  onHostKindChange,
  onHostUrlBlur,
}: GitLabHostStepProps) {
  return (
    <section className="space-y-4">
      <div>
        <p className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          1. Choose GitLab host
        </p>
        <p className="mt-1 text-sm text-muted-foreground">
          GitLab.com and self-managed GitLab use the same connection flow.
        </p>
      </div>
      <div className="space-y-2">
        <Label htmlFor="gitlab-host">GitLab host</Label>
        <Select
          value={hostKind}
          onValueChange={onHostKindChange}
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
            <SelectItem value="self_managed">Self-managed GitLab</SelectItem>
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
                    onHostUrlBlur()
                  }}
                />
              </FormControl>
              <FormDescription>
                Host origin only. Oore normalizes a trailing slash; do not
                include <code>/api/v4</code> or a group path.
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
  )
}
