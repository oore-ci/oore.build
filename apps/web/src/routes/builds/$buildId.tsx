import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { useBuild, useCancelBuild } from '@/hooks/use-builds'
import { getStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Alert, AlertDescription } from '@/components/ui/alert'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/builds/$buildId')({
  staticData: { breadcrumbLabel: 'Details' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: BuildDetailPage,
})

const TERMINAL_STATUSES = new Set([
  'succeeded',
  'failed',
  'canceled',
  'timed_out',
  'expired',
])

function BuildDetailPage() {
  const { buildId } = Route.useParams()
  const { data, isLoading, error } = useBuild(buildId)
  const cancelMutation = useCancelBuild()

  useEffect(() => {
    const label = data?.build?.build_number
      ? `Build #${data.build.build_number}`
      : 'Build Details'
    document.title = webPageTitle(label)
  }, [data?.build?.build_number])

  function handleCancel() {
    cancelMutation.mutate(buildId, {
      onSuccess: () => {
        toast.success('Build canceled')
      },
      onError: (err) => {
        toast.error(`Failed to cancel: ${err.message}`)
      },
    })
  }

  if (isLoading) {
    return (
      <PageLayout>
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-32 w-full" />
        <Skeleton className="h-48 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    return (
      <PageLayout>
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load build: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { build, events } = data
  const canCancel = !TERMINAL_STATUSES.has(build.status)

  return (
    <PageLayout>
      <PageHeader
        title={`Build #${build.build_number}`}
        meta={
          <>
            <Badge variant={getStatusVariant(build.status)}>
              {build.status}
            </Badge>
            <Badge variant="outline">{build.trigger_type}</Badge>
          </>
        }
        actions={
          canCancel ? (
            <Button
              variant="destructive"
              onClick={handleCancel}
              disabled={cancelMutation.isPending}
            >
              {cancelMutation.isPending ? 'Canceling...' : 'Cancel Build'}
            </Button>
          ) : undefined
        }
      />

      <Card>
        <CardHeader>
          <CardTitle>Details</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
            {build.branch && (
              <>
                <dt className="text-muted-foreground">Branch</dt>
                <dd>{build.branch}</dd>
              </>
            )}
            {build.commit_sha && (
              <>
                <dt className="text-muted-foreground">Commit</dt>
                <dd className="font-mono">{build.commit_sha}</dd>
              </>
            )}
            {build.trigger_actor && (
              <>
                <dt className="text-muted-foreground">Actor</dt>
                <dd>{build.trigger_actor}</dd>
              </>
            )}
            <dt className="text-muted-foreground">Queued</dt>
            <dd>{new Date(build.queued_at * 1000).toLocaleString()}</dd>
            {build.started_at && (
              <>
                <dt className="text-muted-foreground">Started</dt>
                <dd>{new Date(build.started_at * 1000).toLocaleString()}</dd>
              </>
            )}
            {build.finished_at && (
              <>
                <dt className="text-muted-foreground">Finished</dt>
                <dd>{new Date(build.finished_at * 1000).toLocaleString()}</dd>
              </>
            )}
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Events</CardTitle>
        </CardHeader>
        <CardContent>
          {events.length === 0 ? (
            <p className="text-sm text-muted-foreground">No events yet.</p>
          ) : (
            <div className="space-y-3">
              {events.map((event) => (
                <div
                  key={event.id}
                  className="flex items-start gap-3 text-sm"
                >
                  <span className="text-xs text-muted-foreground whitespace-nowrap">
                    {new Date(event.created_at * 1000).toLocaleTimeString()}
                  </span>
                  <div>
                    <span>
                      {event.from_status && (
                        <span className="text-muted-foreground">
                          {event.from_status} &rarr;{' '}
                        </span>
                      )}
                      <span className="font-medium">{event.to_status}</span>
                    </span>
                    {event.reason && (
                      <p className="text-xs text-muted-foreground">
                        {event.reason}
                      </p>
                    )}
                    {event.actor && (
                      <p className="text-xs text-muted-foreground">
                        by {event.actor}
                      </p>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </PageLayout>
  )
}
