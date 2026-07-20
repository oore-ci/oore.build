import type { BuildEvent } from '@/lib/types'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { relativeTime } from '@/lib/format-utils'

export function EventTimeline({ events }: { events: Array<BuildEvent> }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm font-medium text-muted-foreground">
          Event timeline
        </CardTitle>
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
                      className="shrink-0 text-[10px] text-muted-foreground"
                      title={new Date(event.created_at * 1000).toLocaleString()}
                    >
                      {relativeTime(event.created_at)}
                    </span>
                  </div>
                  {event.reason ? (
                    <p className="mt-0.5 text-[11px] text-muted-foreground">
                      {event.reason}
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
