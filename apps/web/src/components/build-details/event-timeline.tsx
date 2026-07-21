import type { BuildEvent } from '@/lib/types'
import { Card, CardContent } from '@/components/ui/card'
import { relativeTime } from '@/lib/format-utils'

export function EventTimeline({ events }: { events: Array<BuildEvent> }) {
  return (
    <Card className="overflow-hidden">
      <div className="border-b px-4 py-3">
        <p className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/60">
          Event Timeline
        </p>
      </div>
      <CardContent className="p-4">
        {events.length === 0 ? (
          <p className="text-xs text-muted-foreground">No events yet.</p>
        ) : (
          <div className="relative space-y-0">
            {events.map((event, i) => (
              <div key={event.id} className="relative flex gap-3 pb-4 last:pb-0">
                {i < events.length - 1 ? (
                  <div className="absolute left-[5px] top-3 bottom-0 w-px bg-border" />
                ) : null}
                <div className="relative mt-1 size-[11px] shrink-0 rounded-sm border-2 border-primary bg-background" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-2">
                    <p className="text-xs font-medium">
                      {event.from_status ? (
                        <span className="text-muted-foreground">
                          {event.from_status} &rarr;{' '}
                        </span>
                      ) : null}
                      {event.to_status}
                    </p>
                    <span
                      className="shrink-0 font-mono text-[10px] text-muted-foreground"
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
