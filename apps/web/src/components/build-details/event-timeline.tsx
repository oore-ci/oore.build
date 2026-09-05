import type { BuildEvent } from '@oore/client/models'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { relativeTime } from '@/lib/format-utils'

function EventReason({ reason }: { reason: string }) {
  const marker = ' Repair: '
  const markerIndex = reason.indexOf(marker)
  if (markerIndex < 0) return reason

  const label = reason.slice(0, markerIndex)
  const repairUrl = reason.slice(markerIndex + marker.length)
  return (
    <>
      {label}{' '}
      <a className="font-medium text-foreground underline" href={repairUrl}>
        Repair source
      </a>
    </>
  )
}

export function EventTimeline({ events }: { events: Array<BuildEvent> }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Event timeline</CardTitle>
      </CardHeader>
      <CardContent>
        {events.length === 0 ? (
          <p className="text-xs text-muted-foreground">No events yet.</p>
        ) : (
          <div className="relative space-y-0">
            {events.map((event, i) => (
              <div
                key={event.id}
                className="relative flex gap-3 pb-4 last:pb-0"
              >
                {i < events.length - 1 ? (
                  <div className="absolute top-3 bottom-0 left-1.25 w-px bg-border" />
                ) : null}
                <div className="relative mt-1 size-2.75 shrink-0 rounded-full border-2 border-primary bg-background" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-2">
                    <p className="text-xs font-medium">
                      {event.from_status ? (
                        <span className="text-muted-foreground">
                          {event.from_status} →{' '}
                        </span>
                      ) : null}
                      {event.to_status}
                    </p>
                    <span
                      className="shrink-0 text-xs text-muted-foreground"
                      title={new Date(event.created_at * 1000).toLocaleString()}
                    >
                      {relativeTime(event.created_at)}
                    </span>
                  </div>
                  {event.reason ? (
                    <p className="mt-0.5 text-sm text-muted-foreground">
                      <EventReason reason={event.reason} />
                    </p>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
