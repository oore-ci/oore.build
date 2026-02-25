import { Link, createFileRoute } from '@tanstack/solid-router'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { PageMeta } from '@/lib/seo'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'

export const Route = createFileRoute('/settings/integrations/local-git')({
  staticData: { breadcrumbLabel: 'Local Repositories' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: LocalGitPage,
})

function LocalGitPage() {
  const preferences = useInstancePreferences()
  const runtimeMode = preferences.data?.preferences.runtime_mode ?? 'local'

  return (
    <PageLayout>
      <PageMeta title="Local Repositories" noindex />
      <PageHeader
        title="Local Repositories"
        description="Local repository selection happens directly during project creation."
      />

      <Card>
        <CardHeader>
          <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Use Project Creation
          </CardTitle>
        </CardHeader>
        <CardContent class="space-y-3">
          <p class="text-sm text-muted-foreground">
            Local repository selection happens directly during project creation.
            Enter an absolute path (or browse folders when available).
          </p>
          {runtimeMode === 'remote' ? (
            <p class="text-xs text-muted-foreground">
              In External Access mode, remote clients must enter paths manually.
              Folder browsing is limited to localhost for security.
            </p>
          ) : (
            <p class="text-xs text-muted-foreground">
              In Local Only mode, folder browsing is available from localhost.
            </p>
          )}

          <Link to="/projects" search={{ openCreate: '1' }}>
            <Button>Create Project</Button>
          </Link>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
