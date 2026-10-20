import { Link } from '@tanstack/react-router'

import type { ReleaseActivityEvent } from '@oore/client/models'
import { ReleaseStateBadge } from '@/components/releases/release-rollout'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
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

  if (events.length === 0) return null

  return (
    <section aria-labelledby="release-activity" className="flex flex-col gap-3">
      <h2
        id="release-activity"
        className="text-sm font-medium text-muted-foreground"
      >
        Release activity
      </h2>
      <ItemGroup className="gap-2">
        {events.map((event) => (
          <ReleaseActivityItem
            key={event.id}
            event={event}
            showProject={showProject}
          />
        ))}
      </ItemGroup>
    </section>
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

  return (
    <Item
      variant="outline"
      size="sm"
      render={
        <Link
          to="/releases/$releaseId"
          params={{ releaseId: event.release_id }}
        />
      }
    >
      <ItemContent>
        <ItemTitle>
          {showProject ? `${releaseAppName(event)} ` : ''}
          <span className="font-mono text-xs tabular-nums">
            {event.version_name ?? 'Unversioned'}
          </span>
          <span className="text-muted-foreground">
            {' '}
            · {RELEASE_STORE_LABEL[event.store]} {event.track}
          </span>
        </ItemTitle>
        <ItemDescription>
          {transition ? `${transition} · ` : ''}
          {event.actor_email ?? 'Oore'}
          {event.reason ? ` · ${event.reason}` : ''}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        <ReleaseStateBadge
          release={{ state: event.to_state, awaiting_approval: false }}
        />
        <span
          className="text-xs text-muted-foreground"
          title={new Date(event.at * 1000).toLocaleString()}
        >
          {relativeTime(event.at)}
        </span>
      </ItemActions>
    </Item>
  )
}
