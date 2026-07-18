import type { GitLabSetupForm } from './-gitlab-setup'
import { Alert, AlertDescription } from '@/components/ui/alert'

export function GitLabVerificationStep({
  authMode,
}: {
  authMode: GitLabSetupForm['auth_mode']
}) {
  return (
    <section className="space-y-3 border-t border-border/60 pt-6">
      <div>
        <p className="text-sm font-medium text-muted-foreground">
          3. Verify source connection
        </p>
        <p className="mt-1 text-sm text-muted-foreground">
          Oore checks the selected GitLab host before it stores this source.
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
  )
}
