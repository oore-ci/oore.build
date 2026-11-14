import { Link } from '@tanstack/react-router'

import {
  DashboardList,
  DashboardSection,
  DashboardSectionError,
} from '@/components/dashboard-section'
import { ReleaseStateBadge } from '@/components/releases/release-rollout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { useReleaseEvents } from '@/hooks/use-releases'
import { relativeTime } from '@/lib/format-utils'
import {
  latestChangePerRelease,
  releaseAppName,
  releaseChangeDestination,
  releaseChangeShare,
  type ReleaseLatestChange,
} from '@/lib/releases'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

/**
 * Recent release and rollout changes, one row per app version with where
 * it stands now; each step is in the release's own history. Renders nothing
 * until there is something to show, so apps that never ship to a store see
 * no new section.
 */
export function ReleaseActivity({
  limit = 5,
  projectId,
  showProject = false,
  title = 'Release activity',
}: {
  limit?: number
  projectId?: string
  showProject?: boolean
  title?: string
}) {
  // Read enough changes that a busy rollout doesn't crowd other releases out.
  const eventsQuery = useReleaseEvents({
    project_id: projectId,
    limit: Math.min(200, Math.max(50, limit * 20)),
  })
  const events = latestChangePerRelease(eventsQuery.data?.events ?? [], limit)

  // Apps that never shipped stay quiet, once the server has said so.
  if (eventsQuery.isSuccess && events.length === 0) return null

  return (
    <DashboardSection title={title}>
      {eventsQuery.error ? (
        <DashboardSectionError
          label="Release activity"
          onRetry={() => void eventsQuery.refetch()}
        />
      ) : !eventsQuery.data ? (
        <Skeleton className="h-32 w-full" />
      ) : (
        <DashboardList>
          {events.map((change) => ({
            key: change.event.id,
            node: (
              <ReleaseActivityItem change={change} showProject={showProject} />
            ),
          }))}
        </DashboardList>
      )}
    </DashboardSection>
  )
}

function ReleaseActivityItem({
  change,
  showProject,
}: {
  change: ReleaseLatestChange
  showProject: boolean
}) {
  const { event, changes } = change
  const version = event.version_name ?? 'Unversioned'
  const share = releaseChangeShare(event)

  return (
    <Item
      size="sm"
      className="flex-nowrap rounded-none"
      render={
        <Link
          to="/releases/$releaseId"
          params={{ releaseId: event.release_id }}
        />
      }
    >
      <ItemContent className="min-w-0">
        <ItemTitle className="block w-full truncate">
          {showProject ? `${releaseAppName(event)} ` : ''}
          <span className="tabular-nums">{version}</span>
          {share ? (
            <span className="font-normal text-muted-foreground tabular-nums">
              {' '}
              · {share}
            </span>
          ) : null}
        </ItemTitle>
        <ItemDescription className="truncate">
          {releaseChangeDestination(change)}
          {changes > 1 ? ` · ${changes} changes` : ''}
        </ItemDescription>
      </ItemContent>
      <ItemActions className="shrink-0 flex-col items-end gap-1 sm:flex-row sm:items-center sm:gap-3">
        <ReleaseStateBadge
          release={{ state: event.to_state, awaiting_approval: false }}
        />
        <span
          className={cn(
            textVariants({ variant: 'caption' }),
            'whitespace-nowrap',
          )}
          title={new Date(event.at * 1000).toLocaleString()}
        >
          {relativeTime(event.at)}
        </span>
      </ItemActions>
    </Item>
  )
}
