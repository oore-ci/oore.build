import { Link, createFileRoute } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'

export const Route = createFileRoute('/settings/integrations/local-git')({
  staticData: { breadcrumbLabel: 'Local Repositories' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: LocalGitPage,
})

function LocalGitPage() {
  const { data: preferences } = useInstancePreferences()
  const runtimeMode = preferences?.preferences.runtime_mode ?? 'local'
  const isLocalMode = runtimeMode === 'local'

  return (
    <PageLayout width="wide">
      <PageMeta title="Local Repositories" noindex />
      <PageHeader
        title="Local Repositories"
        description="Local repository selection now happens directly during project creation."
        back={{ to: '/settings/integrations', label: 'Sources' }}
      />

      {isLocalMode ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Use Project Creation
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-sm text-muted-foreground">
              To keep Local Only mode simple, pick your local directory when
              creating a project. There is no separate local source setup step.
            </p>
            <Button render={<Link to="/projects" />} nativeButton={false}>
              Go To Projects
            </Button>
          </CardContent>
        </Card>
      ) : (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Access mode is <code>external access</code>. Local repository setup
            is available only in <code>local only</code> mode.
          </AlertDescription>
        </Alert>
      )}
    </PageLayout>
  )
}
