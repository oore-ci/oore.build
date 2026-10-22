import type { BuildEvent } from '@oore/client/models'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { formatEnumLabel, relativeTime } from '@/lib/format-utils'
import { getStatusPresentation } from '@/lib/status-variants'
import { cn } from '@/lib/utils'

// Dot colour follows the status the event moved the build into.
function dotTone(status: string) {
  switch (getStatusPresentation('build', status).variant) {
    case 'success':
      return 'border-success bg-success'
    case 'destructive':
      return 'border-destructive bg-destructive'
    case 'info':
      return 'border-info bg-background'
    case 'secondary':
      return 'border-muted-foreground bg-muted-foreground'
    default:
      return 'border-muted-foreground bg-background'
  }
}

function sentence(text: string) {
  const trimmed = text.trim()
  return trimmed.charAt(0).toUpperCase() + trimmed.slice(1)
}

function formatClock(epochSecs: number) {
  return new Date(epochSecs * 1000).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function EventReason({ reason }: { reason: string }) {
  const marker = ' Repair: '
  const markerIndex = reason.indexOf(marker)
  if (markerIndex < 0) return sentence(reason)

  const label = sentence(reason.slice(0, markerIndex))
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
                <div
                  className={cn(
                    'relative mt-1 size-2.75 shrink-0 rounded-full border-2',
                    dotTone(event.to_status),
                  )}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-baseline justify-between gap-x-2">
                    <p className="text-xs font-medium">
                      {event.from_status ? (
                        <span className="text-muted-foreground">
                          {formatEnumLabel(event.from_status)} →{' '}
                        </span>
                      ) : null}
                      {formatEnumLabel(event.to_status)}
                      {event.actor ? (
                        <span className="font-normal text-muted-foreground">
                          {' '}
                          by {event.actor}
                        </span>
                      ) : null}
                    </p>
                    <span
                      className="shrink-0 text-xs text-muted-foreground tabular-nums"
                      title={new Date(event.created_at * 1000).toLocaleString()}
                    >
                      {formatClock(event.created_at)} ·{' '}
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
