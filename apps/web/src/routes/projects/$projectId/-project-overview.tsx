import { useEffect } from 'react'
import { Link } from '@tanstack/react-router'
import type { Build } from '@oore/client/models'
import { useProjectArtifacts } from '@/hooks/use-builds'
import { AppOutputs } from '@/components/build-details/app-outputs'
import { BuildItem } from '@/components/build-item'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'

export default function ProjectOverview({
  projectId,
  projectName,
  builds,
  loading,
  error,
  onRetry,
}: {
  projectId: string
  projectName: string
  builds: Build[]
  loading: boolean
  error: Error | null
  onRetry: () => void
}) {
  const artifacts = useProjectArtifacts(projectId)
  const { refetch } = artifacts
  const latest = builds[0]
  useEffect(() => {
    if (latest?.status === 'succeeded') void refetch()
  }, [latest?.id, latest?.status, refetch])
  return (
    <div className="space-y-6 pt-3">
      {artifacts.error ? (
        <Alert>
          <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
            App outputs could not be checked.
            <Button
              variant="outline"
              size="sm"
              onClick={() => void artifacts.refetch()}
            >
              Retry outputs
            </Button>
          </AlertDescription>
        </Alert>
      ) : artifacts.isLoading ? (
        <Skeleton className="h-40 w-full" />
      ) : (
        <AppOutputs
          artifacts={artifacts.data?.artifacts ?? []}
          projectName={projectName}
        />
      )}
      <section aria-label="Recent project activity" className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="font-medium">Recent activity</h2>
          <Button
            variant="ghost"
            size="sm"
            render={
              <Link
                to="/projects/$projectId"
                params={{ projectId }}
                search={{ tab: 'builds' }}
              />
            }
            nativeButton={false}
          >
            All builds
          </Button>
        </div>
        {error ? (
          <Alert>
            <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
              Recent activity could not be checked.
              <Button variant="outline" size="sm" onClick={onRetry}>
                Retry activity
              </Button>
            </AlertDescription>
          </Alert>
        ) : loading ? (
          <Skeleton className="h-40 w-full" />
        ) : builds.length ? (
          <div role="list" className="space-y-2">
            {builds.slice(0, 5).map((build) => (
              <BuildItem key={build.id} build={build} />
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            No builds yet. Set up a pipeline, then review and run your first
            build.
          </p>
        )}
      </section>
    </div>
  )
}
