import { Link, createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'

import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { getStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import {
  Card,
  CardContent,
} from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Alert, AlertDescription } from '@/components/ui/alert'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/builds/')({
  staticData: { breadcrumbLabel: 'Builds' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildsListPage,
})

function BuildsListPage() {
  const { data, isLoading, error } = useBuilds({ limit: 50 })

  useEffect(() => {
    document.title = webPageTitle('Builds')
  }, [])

  return (
    <PageLayout>
      <PageHeader
        title="Builds"
        description="View and manage build history across all projects."
      />

      {isLoading && (
        <div className="space-y-3">
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
        </div>
      )}

      {error && (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load builds: {error.message}
          </AlertDescription>
        </Alert>
      )}

      {data && data.builds.length === 0 && (
        <p className="text-sm text-muted-foreground">No builds yet.</p>
      )}

      <div className="space-y-3">
        {data?.builds.map((build) => (
          <Link key={build.id} to="/builds/$buildId" params={{ buildId: build.id }}>
            <Card className="hover:bg-muted/50 transition-colors">
              <CardContent className="flex items-center justify-between py-3">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-sm">
                      #{build.build_number}
                    </span>
                    <Badge variant={getStatusVariant(build.status)}>
                      {build.status}
                    </Badge>
                    <Badge variant="outline">{build.trigger_type}</Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {build.branch && <span>{build.branch}</span>}
                    {build.commit_sha && (
                      <span className="ml-2 font-mono">
                        {build.commit_sha.slice(0, 8)}
                      </span>
                    )}
                    {build.trigger_actor && (
                      <span className="ml-2">by {build.trigger_actor}</span>
                    )}
                  </p>
                </div>
                <span className="text-xs text-muted-foreground">
                  {new Date(build.created_at * 1000).toLocaleString()}
                </span>
              </CardContent>
            </Card>
          </Link>
        ))}
      </div>
    </PageLayout>
  )
}
