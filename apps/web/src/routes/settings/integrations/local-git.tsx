import { Link, createFileRoute } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon } from '@hugeicons/core-free-icons'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { PageMeta } from '@/lib/seo'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'

export const Route = createFileRoute('/settings/integrations/local-git')({
  staticData: {
    breadcrumbLabel: 'Local Repositories',
    breadcrumbParent: { label: 'Sources', to: '/settings/integrations' },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: LocalGitPage,
})

function LocalGitPage() {
  const { data: preferences } = useInstancePreferences()
  const runtimeMode = preferences?.preferences.runtime_mode ?? 'local'

  return (
    <PageLayout width="wide">
      <PageMeta title="Local Repositories" noindex />
      <PageHeader
        title="Local Repositories"
        description="Local repository selection now happens directly during project creation."
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Use Project Creation
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-muted-foreground">
            Local repository selection happens directly during project creation.
            Enter an absolute path (or browse folders when available).
          </p>
          {runtimeMode === 'remote' ? (
            <p className="text-xs text-muted-foreground">
              In External Access mode, remote clients must enter paths manually.
              Folder browsing is limited to localhost for security.
            </p>
          ) : (
            <p className="text-xs text-muted-foreground">
              In Local Only mode, folder browsing is available from localhost.
            </p>
          )}
          <Button
            render={<Link to="/projects" search={{ openCreate: '1' }} />}
            nativeButton={false}
          >
            <HugeiconsIcon icon={Add01Icon} />
            Create project
          </Button>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
