import { createFileRoute, Link, Navigate } from '@tanstack/react-router'

import PageLayout from '@/components/page-layout'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useRelease } from '@/hooks/use-releases'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'

// Stable link target for a single release (notifications link here); the
// release itself lives on its build's Release tab.
export const Route = createFileRoute('/releases/$releaseId')({
  staticData: {
    breadcrumb: {
      title: 'Release',
    },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ReleaseRedirect,
})

function ReleaseRedirect() {
  const { releaseId } = Route.useParams()
  const { data, error, refetch } = useRelease(releaseId)

  if (data) {
    return (
      <Navigate
        to="/builds/$buildId"
        params={{ buildId: data.release.build_id }}
        search={{ tab: 'release', release: releaseId }}
        replace
      />
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
