import { useState } from 'react'
import type { BuildEvent, BuildFeedback } from '@oore/client/models'
import { useBuildFeedbackScreenshot } from '@/hooks/use-build-feedback'
import { useObjectUrl } from '@/hooks/use-object-url'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Skeleton } from '@/components/ui/skeleton'
import { formatEnumLabel, relativeTime } from '@/lib/format-utils'
import { getStatusPresentation } from '@/lib/status-variants'
import { cn } from '@/lib/utils'
import { Text } from '@/components/layout/text'
import { focusRingVariants, textVariants } from '@/lib/recipes'

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

function FeedbackScreenshot({ feedback }: { feedback: BuildFeedback }) {
  const [open, setOpen] = useState(false)
  const { data, isError } = useBuildFeedbackScreenshot(
    feedback.build_id,
    feedback.id,
    feedback.has_screenshot,
  )
  const url = useObjectUrl(data instanceof Blob ? data : undefined)
  if (isError)
    return (
      <Text variant="caption" className="mt-1">
        The screenshot could not be loaded.
      </Text>
    )
  if (!url) return <Skeleton className="mt-2 h-24 w-16 rounded-lg" />

  return (
    <>
      <button
        type="button"
        className={cn(
          focusRingVariants(),
          'mt-2 block overflow-hidden rounded-lg border',
        )}
        aria-label="Open screenshot"
        onClick={() => setOpen(true)}
      >
        <img src={url} alt="" className="h-24 w-auto max-w-40 object-cover" />
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>Screenshot</DialogTitle>
          </DialogHeader>
          <img
            src={url}
            alt="Screenshot attached to the feedback"
            className="max-h-[70vh] w-full rounded-lg object-contain"
          />
        </DialogContent>
      </Dialog>
    </>
  )
}

function FeedbackEntry({ feedback }: { feedback: BuildFeedback }) {
  const facts = [
    feedback.device,
    feedback.os_version,
    feedback.app_version ? `App ${feedback.app_version}` : null,
  ].filter(Boolean)

  return (
    <>
      <div className="flex flex-wrap items-baseline justify-between gap-x-2">
        <p className="text-xs font-medium">
          Tester feedback
          <span
            className="font-normal text-muted-foreground"
            title={feedback.author_email}
          >
            {' '}
            from {feedback.author_name ?? feedback.author_email}
          </span>
        </p>
        <EventTime createdAt={feedback.created_at} />
      </div>
      <p className="mt-0.5 text-sm wrap-break-word whitespace-pre-wrap">
        {feedback.message}
      </p>
      {facts.length > 0 ? (
        <Text variant="caption" className="mt-0.5">
          {facts.join(' · ')}
        </Text>
      ) : null}
      {feedback.has_screenshot ? (
        <FeedbackScreenshot feedback={feedback} />
      ) : null}
    </>
  )
}

function EventTime({ createdAt }: { createdAt: number }) {
  return (
    <span
      className={cn(
        textVariants({ variant: 'caption' }),
        'shrink-0 tabular-nums',
      )}
      title={new Date(createdAt * 1000).toLocaleString()}
    >
      {formatClock(createdAt)} · {relativeTime(createdAt)}
    </span>
  )
}

type TimelineEntry =
  | { kind: 'event'; id: string; createdAt: number; event: BuildEvent }
  | {
      kind: 'feedback'
      id: string
      createdAt: number
      feedback: BuildFeedback
    }

function timelineEntries(
  events: Array<BuildEvent>,
  feedback: Array<BuildFeedback>,
): Array<TimelineEntry> {
  return [
    ...events.map((event): TimelineEntry => ({
      kind: 'event',
      id: event.id,
      createdAt: event.created_at,
      event,
    })),
    ...feedback.map((item): TimelineEntry => ({
      kind: 'feedback',
      id: `feedback-${item.id}`,
      createdAt: item.created_at,
      feedback: item,
    })),
  ].sort((left, right) => left.createdAt - right.createdAt)
}

export function EventTimeline({
  events,
  feedback = [],
}: {
  events: Array<BuildEvent>
  feedback?: Array<BuildFeedback>
}) {
  const entries = timelineEntries(events, feedback)
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Event timeline</CardTitle>
      </CardHeader>
      <CardContent>
        {entries.length === 0 ? (
          <Text variant="caption">No events yet.</Text>
        ) : (
          <div className="relative space-y-0">
            {entries.map((entry, i) => (
              <div
                key={entry.id}
                className="relative flex gap-3 pb-4 last:pb-0"
              >
                {i < entries.length - 1 ? (
                  <div className="absolute top-3 bottom-0 left-1.25 w-px bg-border" />
                ) : null}
                <div
                  className={cn(
                    'relative mt-1 size-2.75 shrink-0 rounded-full border-2',
                    entry.kind === 'feedback'
                      ? 'border-primary bg-primary'
                      : dotTone(entry.event.to_status),
                  )}
                />
                <div className="min-w-0 flex-1">
                  {entry.kind === 'feedback' ? (
                    <FeedbackEntry feedback={entry.feedback} />
                  ) : (
                    <>
                      <div className="flex flex-wrap items-baseline justify-between gap-x-2">
                        <p className="text-xs font-medium">
                          {entry.event.from_status ? (
                            <span className="text-muted-foreground">
                              {formatEnumLabel(entry.event.from_status)} →{' '}
                            </span>
                          ) : null}
                          {formatEnumLabel(entry.event.to_status)}
                          {entry.event.actor ? (
                            <span className="font-normal text-muted-foreground">
                              {' '}
                              by {entry.event.actor}
                            </span>
                          ) : null}
                        </p>
                        <EventTime createdAt={entry.event.created_at} />
                      </div>
                      {entry.event.reason ? (
                        <Text variant="muted" className="mt-0.5">
                          <EventReason reason={entry.event.reason} />
                        </Text>
                      ) : null}
                    </>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
