import { Link } from '@tanstack/react-router'

import type { ReleaseActivityEvent } from '@oore/client/models'
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
  RELEASE_STORE_LABEL,
  releaseAppName,
  releaseTransitionLabel,
} from '@/lib/releases'

/**
 * Recent release and rollout changes. Renders nothing until there is
 * something to show, so apps that never ship to a store see no new section.
 */
export function ReleaseActivity({
  limit = 5,
  projectId,
  showProject = false,
}: {
  limit?: number
  projectId?: string
  showProject?: boolean
}) {
  const eventsQuery = useReleaseEvents({ project_id: projectId, limit })
  const events = eventsQuery.data?.events ?? []

  // Apps that never shipped stay quiet, once the server has said so.
  if (eventsQuery.isSuccess && events.length === 0) return null

  return (
    <DashboardSection title="Release activity">
      {eventsQuery.error ? (
        <DashboardSectionError
          label="Release activity"
          onRetry={() => void eventsQuery.refetch()}
        />
      ) : !eventsQuery.data ? (
        <Skeleton className="h-32 w-full" />
      ) : (
        <DashboardList>
          {events.map((event) => ({
            key: event.id,
            node: (
              <ReleaseActivityItem event={event} showProject={showProject} />
            ),
          }))}
        </DashboardList>
      )}
    </DashboardSection>
  )
}

function ReleaseActivityItem({
  event,
  showProject,
}: {
  event: ReleaseActivityEvent
  showProject: boolean
}) {
  const transition = releaseTransitionLabel(event)
  const version = event.version_name ?? 'Unversioned'

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
          {transition ? (
            <span className="font-normal text-muted-foreground">
              {' '}
              · {transition}
            </span>
          ) : null}
        </ItemTitle>
        <ItemDescription className="truncate">
          {RELEASE_STORE_LABEL[event.store]} {event.track} ·{' '}
          {event.actor_email ?? 'Oore'}
          {event.reason ? ` · ${event.reason}` : ''}
        </ItemDescription>
      </ItemContent>
      <ItemActions className="shrink-0 flex-col items-end gap-1 sm:flex-row sm:items-center sm:gap-3">
        <ReleaseStateBadge
          release={{ state: event.to_state, awaiting_approval: false }}
        />
        <span
          className="text-xs whitespace-nowrap text-muted-foreground"
          title={new Date(event.at * 1000).toLocaleString()}
        >
          {relativeTime(event.at)}
        </span>
      </ItemActions>
    </Item>
  )
}
