import { Link, createFileRoute } from '@tanstack/react-router'
import { Plus as Add01Icon } from 'lucide-react'
import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useHasPermission } from '@/hooks/use-permissions'
import { PageMeta } from '@/lib/seo'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'

export const Route = createFileRoute('/settings/integrations/local-git')({
  staticData: {
    breadcrumb: {
      title: 'Local repositories',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: LocalGitPage,
})

function LocalGitPage() {
  const canCreateProjects = useHasPermission('projects', 'write')
  const canReadPreferences = useHasPermission('instance_settings', 'read')
  const { data: preferences } = useInstancePreferences({
    enabled: canReadPreferences,
  })
  const runtimeMode = preferences?.runtime_mode

  return (
    <PageLayout width="wide">
      <PageMeta title="Local repositories" noindex />
      <PageHeader
        title="Local repositories"
        description="Local repository selection now happens directly during project creation."
      />

      <Card size="sm" aria-labelledby="local-project-title">
        <CardHeader>
          <CardTitle id="local-project-title">Use project creation</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-muted-foreground">
            Owners and admins link local repositories while creating a project
            or from the project&apos;s Source repository setting.
          </p>
          {canReadPreferences && runtimeMode === 'remote' ? (
            <p className="text-xs text-muted-foreground">
              In External Access mode, remote clients must enter paths manually.
              Folder browsing is limited to localhost for security.
            </p>
          ) : canReadPreferences && runtimeMode === 'local' ? (
            <p className="text-xs text-muted-foreground">
              In Local Only mode, folder browsing is available from localhost.
            </p>
          ) : !canCreateProjects ? (
            <p className="text-xs text-muted-foreground">
              You can use local repositories already linked to projects you can
              access. Ask an owner or admin to add or change a source.
            </p>
          ) : null}
          {canCreateProjects ? (
            <Button
              render={<Link to="/projects" search={{ openCreate: '1' }} />}
              nativeButton={false}
            >
              <Add01Icon />
              Create project
            </Button>
          ) : null}
        </CardContent>
      </Card>
    </PageLayout>
  )
}
