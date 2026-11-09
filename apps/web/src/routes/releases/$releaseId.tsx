import { createFileRoute, Link } from '@tanstack/react-router'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { ReleasePanel } from '@/components/releases/release-panel'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useRelease } from '@/hooks/use-releases'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { releaseAppName, releaseDestinationLabel } from '@/lib/releases'

// Every release has its own page (notifications link here). A release made
// by a build also shows on that build's Release tab.
export const Route = createFileRoute('/releases/$releaseId')({
  staticData: {
    breadcrumb: {
      title: 'Release',
    },
  },
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id, context)
  },
  component: ReleaseRedirect,
})

function ReleaseRedirect() {
  const { releaseId } = Route.useParams()
  const { data, error, refetch } = useRelease(releaseId)

  if (data) {
    const release = data.release
    const appName = releaseAppName(release)
    return (
      <PageLayout>
        <PageMeta
          title={`${appName} ${release.version_name ?? 'release'}`}
          noindex
        />
        <PageHeader
          title={`${appName} ${release.version_name ?? ''}`.trim()}
          description={releaseDestinationLabel(release)}
          meta={
            <>
              <Link
                to="/projects/$projectId"
                params={{ projectId: release.project_id }}
                search={{ tab: 'releases' }}
                className="underline-offset-4 hover:text-foreground hover:underline"
              >
                All releases of {appName}
              </Link>
              {release.build_id ? (
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: release.build_id }}
                  className="underline-offset-4 hover:text-foreground hover:underline"
                >
                  Built by #{release.build_number ?? 'build'}
                </Link>
              ) : null}
            </>
          }
        />
        <ReleasePanel
          error={null}
          isLoading={false}
          onRetry={() => void refetch()}
          releases={[release]}
        />
      </PageLayout>
    )
  }

  return (
    <PageLayout>
      <PageMeta title="Release" noindex />
      {error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex items-center justify-between gap-3">
            <span>This release could not be loaded.</span>
            <span className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                render={<Link to="/" />}
                nativeButton={false}
              >
                Home
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void refetch()}
              >
                Retry
              </Button>
            </span>
          </AlertDescription>
        </Alert>
      ) : (
        <Skeleton className="h-32 w-full" />
      )}
    </PageLayout>
  )
}
